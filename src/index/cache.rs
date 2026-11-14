//! The persisted index: one entry per session file, keyed by the file's
//! fingerprint.

use std::{cmp::Reverse, collections::HashMap, time::UNIX_EPOCH};

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use super::{
  Session,
  content::{self, Content},
  parse::{self, ParseOutput},
};

pub(crate) struct Paths {
  projects: Utf8PathBuf,
  pub(crate) index: Utf8PathBuf,
  pub(crate) content: Utf8PathBuf,
}

impl Paths {
  pub(crate) fn locate() -> Result<Self> {
    let home: Utf8PathBuf = dirs::home_dir().context("no home directory")?.try_into()?;
    let cache: Utf8PathBuf = dirs::cache_dir().context("no cache directory")?.try_into()?;
    let cache = cache.join("cff");

    return Ok(Self {
      projects: home.join(".claude/projects"),
      index: cache.join("index-v4.json"),
      content: cache.join("content"),
    });
  }

  /// Top-level session files, `projects/<encoded cwd>/<id>.jsonl`. Subagent
  /// transcripts live one level deeper and belong to their parent.
  pub(crate) fn session_files(&self) -> Result<HashMap<Utf8PathBuf, Fingerprint>> {
    let mut files = HashMap::new();
    for project in fs_err::read_dir(self.projects.as_std_path())? {
      let project = project?.path();
      if !project.is_dir() {
        continue;
      }

      for entry in fs_err::read_dir(&project)? {
        let path = Utf8PathBuf::try_from(entry?.path())?;
        if path.extension() == Some("jsonl") {
          let fingerprint = Fingerprint::of(&path)?;
          files.insert(path, fingerprint);
        }
      }
    }

    return Ok(files);
  }
}

/// What must be unchanged for a cached entry to still describe its file.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Fingerprint {
  mtime: u128,
  size: u64,
}

impl Fingerprint {
  fn of(path: &Utf8Path) -> Result<Self> {
    let meta = fs_err::metadata(path.as_std_path())?;
    let mtime = meta.modified()?.duration_since(UNIX_EPOCH)?.as_nanos();

    return Ok(Self { mtime, size: meta.len() });
  }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Entry {
  fingerprint: Fingerprint,

  /// `None` when the file holds no readable conversation.
  session: Option<Session>,

  warnings: Vec<String>,
}

impl Entry {
  /// A file that cannot be read at all still gets an entry, with the reason
  /// as its warning.
  pub(crate) fn parse(file: &Utf8Path, fingerprint: Fingerprint, content_dir: &Utf8Path) -> Self {
    let output = parse::parse_file(file, content_dir).unwrap_or_else(|e| {
      return ParseOutput {
        session: None,
        warnings: vec![format!("{file}: {e:#}")],
      };
    });

    return Self {
      fingerprint,
      session: output.session,
      warnings: output.warnings,
    };
  }
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Cache {
  pub(crate) entries: HashMap<Utf8PathBuf, Entry>,
}

impl Cache {
  pub(crate) fn load(path: &Utf8Path) -> Result<Self> {
    let bytes = match fs_err::read(path.as_std_path()) {
      Ok(bytes) => bytes,
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
      Err(e) => return Err(e.into()),
    };

    let cache = serde_json::from_slice(&bytes).with_context(|| format!("{path} is unreadable; run `cff --reindex`"))?;

    return Ok(cache);
  }

  /// Written through a sibling temp file and a rename, so a reader never
  /// sees a half-written index.
  pub(crate) fn save(&self, path: &Utf8Path) -> Result<()> {
    fs_err::create_dir_all(path.parent().context("index path has no parent")?.as_std_path())?;
    let temp = path.with_extension("tmp");
    fs_err::write(temp.as_std_path(), serde_json::to_vec(self)?)?;
    fs_err::rename(temp.as_std_path(), path.as_std_path())?;

    return Ok(());
  }

  /// Drops every entry whose file is gone or no longer matches its fingerprint.
  pub(crate) fn retain_current(&mut self, files: &HashMap<Utf8PathBuf, Fingerprint>) {
    self.entries.retain(|file, entry| files.get(file) == Some(&entry.fingerprint));
  }

  /// The sessions with their content mapped, newest first.
  pub(crate) fn sessions(&self, content_dir: &Utf8Path) -> Result<Vec<Session>> {
    let mut sessions = self
      .entries
      .values()
      .filter_map(|entry| entry.session.clone())
      .map(|mut session| {
        let content = Content::open(&content::path(content_dir, &session.id)).context("run `cff --reindex`")?;
        session.content = content;

        return Ok(session);
      })
      .collect::<Result<Vec<Session>>>()?;

    sessions.sort_by_key(|session| Reverse(session.updated));

    return Ok(sessions);
  }

  pub(crate) fn warnings(&self) -> Vec<String> {
    return self.entries.values().flat_map(|entry| entry.warnings.iter().cloned()).collect();
  }
}
