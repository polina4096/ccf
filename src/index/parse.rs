use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};
use jiff::Timestamp;
use serde::Deserialize;
use serde_json::value::RawValue;

use super::{
  Session,
  content::{self, Content},
};

pub(crate) struct ParseOutput {
  /// `None` when nobody typed anything readable in the file.
  pub(crate) session: Option<Session>,

  /// Records that were skipped as malformed: `path:line: what`.
  pub(crate) warnings: Vec<String>,
}

#[derive(Deserialize)]
struct UserRecord<'a> {
  #[serde(borrow)]
  message: Message<'a>,
  timestamp: Option<String>,
  cwd: Option<Utf8PathBuf>,
  #[serde(rename = "gitBranch")]
  git_branch: Option<String>,
  #[serde(rename = "isSidechain", default)]
  is_sidechain: bool,
}

#[derive(Deserialize)]
struct AssistantRecord<'a> {
  #[serde(borrow)]
  message: AssistantMessage<'a>,
  timestamp: Option<String>,
  #[serde(rename = "isSidechain", default)]
  is_sidechain: bool,
}

#[derive(Deserialize)]
struct Message<'a> {
  #[serde(borrow)]
  content: &'a RawValue,
}

#[derive(Deserialize)]
struct AssistantMessage<'a> {
  model: Option<String>,
  usage: Option<Usage>,
  #[serde(borrow)]
  content: &'a RawValue,
}

#[derive(Deserialize, Default)]
struct Usage {
  #[serde(default)]
  input_tokens: u64,
  #[serde(default)]
  cache_read_input_tokens: u64,
  #[serde(default)]
  cache_creation_input_tokens: u64,
  #[serde(default)]
  output_tokens: u64,
}

/// A content block; only `{"type":"text","text":"…"}` carries prose.
#[derive(Deserialize)]
struct Block {
  #[serde(rename = "type")]
  kind: String,
  #[serde(default)]
  text: String,
}

#[derive(Deserialize)]
struct TitleRecord {
  #[serde(rename = "aiTitle", alias = "customTitle")]
  text: String,
}

const KNOWN: [&[u8]; 4] = [b"user", b"assistant", b"ai-title", b"custom-title"];

/// The record's top-level type, found without parsing the line. An unescaped
/// `"type":"assistant"` cannot occur inside a JSON string, so the first known
/// value is the record's own even when content blocks carry `"type"` keys.
fn record_type(line: &[u8]) -> Option<&[u8]> {
  return memchr::memmem::find_iter(line, b"\"type\":\"")
    .filter_map(|i| {
      let start = i + 8;
      let end = memchr::memchr(b'"', &line[start ..])?;

      return Some(&line[start .. start + end]);
    })
    .find(|t| KNOWN.contains(t));
}

fn text_blocks(raw: &str) -> Option<String> {
  let blocks: Vec<Block> = serde_json::from_str(raw).ok()?;
  let texts: Vec<String> = blocks.into_iter().filter(|b| b.kind == "text").map(|b| b.text).collect();
  let joined = texts.join("\n");

  return (!joined.trim().is_empty()).then_some(joined);
}

/// Text a person typed; `None` for tool results, slash-command expansions,
/// interruption notices and other machine-made user records.
fn typed_text(raw: &RawValue) -> Option<String> {
  let s = raw.get();
  let text = if s.starts_with('"') { serde_json::from_str::<String>(s).ok()? } else { text_blocks(s)? };
  let text = text.trim();

  let machine_made = text.starts_with('<') || text.starts_with("Caveat:") || text.starts_with("[Request interrupted");

  if text.is_empty() || machine_made {
    return None;
  }

  return Some(text.to_string());
}

/// `claude-opus-4-1-20250805` → `opus 4.1`; the date suffix is dropped.
fn short_model(id: &str) -> String {
  let parts: Vec<&str> = id.trim_start_matches("claude-").split('-').collect();
  let family = parts.iter().find(|p| p.chars().all(|c| c.is_ascii_alphabetic())).copied().unwrap_or(id);
  let version: Vec<&str> =
    parts.iter().copied().filter(|p| p.len() < 8 && p.chars().all(|c| c.is_ascii_digit())).collect();

  if version.is_empty() {
    return family.to_string();
  }

  return format!("{family} {}", version.join("."));
}

/// `-Users-polina4096-dev-foo` → `/Users/polina4096/dev/foo`. Best effort for
/// files whose records never name their directory: dashes in names are lost.
fn decode_folder(path: &Utf8Path) -> Utf8PathBuf {
  let name = path.parent().and_then(|p| p.file_name()).unwrap_or("");

  return Utf8PathBuf::from(name.replace('-', "/"));
}

fn is_continuation(prompt: &str) -> bool {
  return prompt.starts_with("This session is being continued");
}

/// Everything gathered from a file's records before it becomes a `Session`.
#[derive(Default)]
struct Draft {
  cwd: Option<Utf8PathBuf>,
  branch: Option<String>,
  ai_title: Option<String>,
  custom_title: Option<String>,
  prompts: Vec<String>,
  content: String,
  first: Option<Timestamp>,
  last: Option<Timestamp>,
  replies: u32,
  model: Option<String>,
  ctx_tokens: u64,
  out_tokens: u64,
  warnings: Vec<String>,
}

impl Draft {
  fn feed(&mut self, line: &[u8], at: &str) {
    match record_type(line) {
      Some(b"user") => {
        if let Some(record) = self.record::<UserRecord>(line, at, "user") {
          self.user(record, at);
        }
      }

      Some(b"assistant") => {
        if let Some(record) = self.record::<AssistantRecord>(line, at, "assistant") {
          self.assistant(record, at);
        }
      }

      Some(b"ai-title") => {
        if let Some(record) = self.record::<TitleRecord>(line, at, "title") {
          self.ai_title = Some(record.text);
        }
      }

      Some(b"custom-title") => {
        if let Some(record) = self.record::<TitleRecord>(line, at, "title") {
          self.custom_title = Some(record.text);
        }
      }
      _ => {}
    }
  }

  /// The line as a `T`, or a warning naming `what` failed.
  fn record<'a, T: Deserialize<'a>>(&mut self, line: &'a [u8], at: &str, what: &str) -> Option<T> {
    return match serde_json::from_slice(line) {
      Ok(record) => Some(record),
      Err(e) => {
        self.warnings.push(format!("{at}: {what} record: {e}"));
        None
      }
    };
  }

  fn user(&mut self, record: UserRecord, at: &str) {
    if record.is_sidechain {
      return;
    }

    if self.cwd.is_none() {
      self.cwd = record.cwd.filter(|c| !c.as_str().is_empty());
    }

    if self.branch.is_none() {
      self.branch = record.git_branch.filter(|b| !b.is_empty() && b != "HEAD");
    }

    let Some(text) = typed_text(record.message.content)
    else {
      return;
    };
    self.stamp(record.timestamp.as_deref(), at);

    // A resumed or retried turn repeats the prompt verbatim.
    if self.prompts.last() == Some(&text) {
      return;
    }

    self.content.push_str(&text);
    self.content.push('\n');
    self.prompts.push(text);
  }

  fn assistant(&mut self, record: AssistantRecord, at: &str) {
    if record.is_sidechain {
      return;
    }

    self.replies += 1;
    self.stamp(record.timestamp.as_deref(), at);
    if let Some(text) = text_blocks(record.message.content.get()) {
      self.content.push_str(&text);
      self.content.push('\n');
    }

    if let Some(model) = record.message.model {
      self.model = Some(model);
    }

    let Some(usage) = record.message.usage
    else {
      return;
    };

    let ctx = usage.input_tokens + usage.cache_read_input_tokens + usage.cache_creation_input_tokens;
    if ctx > 0 {
      self.ctx_tokens = ctx;
    }

    self.out_tokens += usage.output_tokens;
  }

  fn stamp(&mut self, timestamp: Option<&str>, at: &str) {
    let Some(timestamp) = timestamp
    else {
      return;
    };

    match timestamp.parse::<Timestamp>() {
      Ok(t) => {
        self.first.get_or_insert(t);
        self.last = Some(self.last.map_or(t, |l| l.max(t)));
      }

      Err(e) => self.warnings.push(format!("{at}: timestamp {timestamp:?}: {e}")),
    }
  }

  /// Custom title, else Claude's own, else the first real prompt. A continued
  /// session opens with a boilerplate summary, which never names it.
  fn title(&self) -> Option<String> {
    let first_prompt = self.prompts.iter().find(|p| !is_continuation(p)).or(self.prompts.first())?;
    if let Some(title) = self.custom_title.clone().or(self.ai_title.clone()) {
      return Some(title);
    }

    if is_continuation(first_prompt) {
      return Some("↪ continued session".to_string());
    }

    return Some(first_prompt.lines().next().unwrap_or("").chars().take(100).collect());
  }
}

pub(crate) fn parse_file(path: &Utf8Path, content_dir: &Utf8Path) -> Result<ParseOutput> {
  let data = fs_err::read(path.as_std_path())?;

  let mut draft = Draft::default();
  for (n, line) in data.split(|&c| c == b'\n').enumerate() {
    if line.is_empty() {
      continue;
    }

    draft.feed(line, &format!("{path}:{}", n + 1));
  }

  let Some(title) = draft.title()
  else {
    return Ok(ParseOutput { session: None, warnings: draft.warnings });
  };

  let id = path.file_stem().unwrap_or_default().to_string();
  content::store(content_dir, &id, &draft.content.to_lowercase())?;

  let modified = Timestamp::try_from(fs_err::metadata(path.as_std_path())?.modified()?)?;
  let updated = draft.last.unwrap_or(modified);
  let cwd = draft.cwd.unwrap_or_else(|| decode_folder(path));
  let session = Session {
    id,
    path: path.to_path_buf(),
    cwd,
    branch: draft.branch,
    title,
    content: Content::default(),
    started: draft.first.unwrap_or(updated),
    updated,
    turns: draft.prompts.len() as u32,
    replies: draft.replies,
    bytes: data.len() as u64,
    model: draft.model.as_deref().map(short_model),
    ctx_tokens: draft.ctx_tokens,
    out_tokens: draft.out_tokens,
    agents: count_agents(path)?,
  };

  return Ok(ParseOutput {
    session: Some(session),
    warnings: draft.warnings,
  });
}

/// Subagent transcripts sit in `<id>/subagents/*.jsonl` beside the file.
fn count_agents(path: &Utf8Path) -> Result<u32> {
  let dir = path.with_extension("").join("subagents");
  let entries = match fs_err::read_dir(dir.as_std_path()) {
    Ok(entries) => entries,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
    Err(e) => return Err(e.into()),
  };

  let mut count = 0;
  for entry in entries {
    if entry?.path().extension().is_some_and(|e| e == "jsonl") {
      count += 1;
    }
  }

  return Ok(count);
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
  User,
  Assistant,
}

/// The readable messages of a chat, in order.
pub(crate) struct Transcript {
  pub(crate) messages: Vec<(Role, String)>,
}

/// Loaded when a chat is selected. Records that were malformed at index
/// time are skipped again here without comment.
pub(crate) fn transcript(path: &Utf8Path) -> Result<Transcript> {
  let data = fs_err::read(path.as_std_path())?;

  let mut messages: Vec<(Role, String)> = vec![];
  for line in data.split(|&c| c == b'\n').filter(|l| !l.is_empty()) {
    let Some((role, text)) = message(line)
    else {
      continue;
    };

    if messages.last().is_some_and(|(r, t)| *r == role && *t == text) {
      continue;
    }

    match messages.last_mut() {
      // Consecutive assistant chunks are one reply.
      Some((Role::Assistant, previous)) if role == Role::Assistant => {
        previous.push('\n');
        previous.push_str(&text);
      }
      _ => messages.push((role, text)),
    }
  }

  return Ok(Transcript { messages });
}

fn message(line: &[u8]) -> Option<(Role, String)> {
  match record_type(line)? {
    b"user" => {
      let record = serde_json::from_slice::<UserRecord>(line).ok()?;
      if record.is_sidechain {
        return None;
      }

      return Some((Role::User, typed_text(record.message.content)?));
    }

    b"assistant" => {
      let record = serde_json::from_slice::<AssistantRecord>(line).ok()?;
      if record.is_sidechain {
        return None;
      }

      return Some((Role::Assistant, text_blocks(record.message.content.get())?));
    }

    _ => return None,
  }
}
