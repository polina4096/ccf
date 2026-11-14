use std::{
  sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError},
  },
  time::Duration,
};

use anyhow::Result;
use camino::Utf8PathBuf;
use rayon::prelude::*;

use super::{
  Session,
  cache::{Cache, Entry, Paths},
};

const RESCAN_EVERY: Duration = Duration::from_secs(5);

/// A completed pass over the session files.
#[derive(Default)]
pub(crate) struct ScanOutput {
  /// Newest first.
  pub(crate) sessions: Vec<Session>,

  /// Files indexed with parts skipped, or not indexed at all: `path: what`.
  pub(crate) warnings: Vec<String>,
}

enum Report {
  Scanned { done: usize, total: usize },
  Done(ScanOutput),
  Failed(String),
}

/// The picker's handle to the indexer thread.
pub(crate) struct Indexer {
  reports: Receiver<Report>,
  rebuilds: Sender<()>,

  /// Files scanned so far and files to scan, while a pass is running.
  pub(crate) progress: Option<(usize, usize)>,

  /// From the last completed pass.
  pub(crate) warnings: Vec<String>,

  /// Why the thread stopped; nothing is indexed after that.
  pub(crate) failure: Option<String>,
}

impl Indexer {
  pub(crate) fn start(rebuild: bool) -> Self {
    let (reports_tx, reports) = mpsc::channel();
    let (rebuilds, rebuilds_rx) = mpsc::channel();

    std::thread::spawn(move || {
      if let Err(e) = watch(&reports_tx, &rebuilds_rx, rebuild) {
        let _ = reports_tx.send(Report::Failed(format!("{e:#}")));
      }
    });

    return Self {
      reports,
      rebuilds,
      progress: None,
      warnings: vec![],
      failure: None,
    };
  }

  /// Takes in everything reported since the last poll, without waiting. The
  /// sessions of a pass that completed in that time are returned.
  pub(crate) fn poll(&mut self) -> Option<Vec<Session>> {
    let mut sessions = None;

    loop {
      match self.reports.try_recv() {
        Ok(Report::Scanned { done, total }) => self.progress = Some((done, total)),

        Ok(Report::Done(output)) => {
          self.progress = None;
          self.warnings = output.warnings;
          sessions = Some(output.sessions);
        }

        Ok(Report::Failed(why)) => self.failure = Some(why),
        Err(TryRecvError::Empty) => break,

        // A thread that ended without a report panicked.
        Err(TryRecvError::Disconnected) => {
          self.progress = None;
          self.failure.get_or_insert_with(|| "crashed".to_string());
          break;
        }
      }
    }

    return sessions;
  }

  /// Asks for a full rebuild unless a pass is already running. A stopped
  /// thread shows as `failure`, so a request it never receives is not an error.
  pub(crate) fn rescan(&self) {
    if self.progress.is_none() {
      let _ = self.rebuilds.send(());
    }
  }
}

/// A pass at once, then one every few seconds or on request.
fn watch(reports: &Sender<Report>, rebuilds: &Receiver<()>, mut rebuild: bool) -> Result<()> {
  let paths = Paths::locate()?;
  let on_progress = |done, total| {
    let _ = reports.send(Report::Scanned { done, total });
  };

  loop {
    if let Some(output) = scan(&paths, rebuild, on_progress)?
      && reports.send(Report::Done(output)).is_err()
    {
      return Ok(());
    }

    rebuild = match rebuilds.recv_timeout(RESCAN_EVERY) {
      Ok(()) => true,
      Err(RecvTimeoutError::Timeout) => false,
      Err(RecvTimeoutError::Disconnected) => return Ok(()),
    };
  }
}

/// Sessions whose files are unchanged since the index was written, for an
/// instant first paint before the first scan completes.
pub(crate) fn cached_sessions() -> Result<Vec<Session>> {
  let paths = Paths::locate()?;

  let mut cache = Cache::load(&paths.index)?;
  cache.retain_current(&paths.session_files()?);

  return cache.sessions(&paths.content);
}

/// One scan from scratch.
pub(crate) fn rebuild() -> Result<ScanOutput> {
  let output = scan(&Paths::locate()?, true, |_, _| {})?;

  return Ok(output.unwrap_or_default());
}

/// Re-reads every new or changed file in parallel and rewrites the index.
/// `None` when the index was already current.
fn scan(paths: &Paths, rebuild: bool, on_progress: impl Fn(usize, usize) + Sync) -> Result<Option<ScanOutput>> {
  let mut cache = if rebuild { Cache::default() } else { Cache::load(&paths.index)? };
  let known = cache.entries.len();

  let files = paths.session_files()?;
  cache.retain_current(&files);

  let stale: Vec<_> = files.iter().filter(|(file, _)| !cache.entries.contains_key(*file)).collect();
  if stale.is_empty() && cache.entries.len() == known {
    return Ok(None);
  }

  fs_err::create_dir_all(paths.content.as_std_path())?;
  let done = AtomicUsize::new(0);
  on_progress(0, stale.len());

  let parsed: Vec<(Utf8PathBuf, Entry)> = stale
    .par_iter()
    .map(|(file, fingerprint)| {
      let entry = Entry::parse(file, **fingerprint, &paths.content);
      on_progress(done.fetch_add(1, Ordering::Relaxed) + 1, stale.len());

      return ((*file).clone(), entry);
    })
    .collect();

  cache.entries.extend(parsed);
  cache.save(&paths.index)?;

  return Ok(Some(ScanOutput {
    sessions: cache.sessions(&paths.content)?,
    warnings: cache.warnings(),
  }));
}
