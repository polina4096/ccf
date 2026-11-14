mod search;
mod transcripts;

use anyhow::Result;
use camino::Utf8PathBuf;

use self::{search::SessionSearch, transcripts::TranscriptCache};
use crate::index::{Indexer, Session, Transcript};

pub(crate) struct App {
  transcripts: TranscriptCache,

  /// Newest first.
  pub(crate) sessions: Vec<Session>,
  pub(crate) search: SessionSearch,

  /// Indices into `sessions` that answer the query, best first; every one,
  /// newest first, when the query is empty. Narrowed by `cwd_only`.
  pub(crate) matches: Vec<usize>,
  pub(crate) indexer: Indexer,

  /// The directory the program was started in.
  pub(crate) cwd: Utf8PathBuf,

  /// Whether only chats from `cwd` and below are matched.
  pub(crate) cwd_only: bool,
}

impl App {
  pub(crate) fn new(sessions: Vec<Session>, indexer: Indexer, cwd: Utf8PathBuf) -> Self {
    let mut app = Self {
      transcripts: TranscriptCache::default(),
      sessions: vec![],
      search: SessionSearch::new(),
      matches: vec![],
      indexer,
      cwd,
      cwd_only: false,
    };

    app.set_sessions(sessions);

    return app;
  }

  fn set_sessions(&mut self, sessions: Vec<Session>) {
    self.sessions = sessions;
    self.search.set_sessions(&self.sessions);
    self.update_matches();
  }

  fn update_matches(&mut self) {
    let answers = if self.search.is_empty() { (0 .. self.sessions.len()).collect() } else { self.search.ranked() };

    self.matches = answers
      .into_iter()
      .filter(|&i| !self.cwd_only || self.sessions[i].cwd.starts_with(&self.cwd))
      .collect();
  }

  pub(crate) fn toggle_cwd_only(&mut self) {
    self.cwd_only = !self.cwd_only;
    self.update_matches();
  }

  /// Takes in what the background has finished: search results and rescanned
  /// sessions. True when the sessions were replaced, after which an index
  /// held from before points at another chat.
  pub(crate) fn refresh(&mut self) -> bool {
    let Some(sessions) = self.indexer.poll()
    else {
      self.update_matches();
      return false;
    };

    self.set_sessions(sessions);

    return true;
  }

  pub(crate) fn set_query(&mut self, query: &str) {
    self.search.set_query(&self.sessions, query);
    self.update_matches();
  }

  /// Reads the transcript of `sessions[index]` in the background.
  pub(crate) fn load_transcript(&mut self, index: usize) {
    self.transcripts.load(&self.sessions[index]);
  }

  /// The transcript of `sessions[index]`; `None` until `load_transcript` has read it.
  pub(crate) fn transcript(&self, index: usize) -> Option<&Result<Transcript>> {
    return self.transcripts.get(&self.sessions[index]);
  }

  /// Whether background results are still expected.
  pub(crate) fn is_busy(&self) -> bool {
    return self.indexer.progress.is_some() || self.transcripts.is_reading();
  }
}
