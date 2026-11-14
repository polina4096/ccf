//! Transcripts read off the UI thread, so cursor moves never wait on disk.

use std::{
  collections::HashMap,
  sync::mpsc::{self, Receiver, TryRecvError},
};

use anyhow::{Result, anyhow};

use crate::index::{self, Session, Transcript};

/// A session's id and size: a chat that grew is read again.
type Key = (String, u64);

#[derive(Default)]
pub(crate) struct TranscriptCache {
  /// Failed reads are kept too, so a broken file is reported, not retried.
  read: HashMap<Key, Result<Transcript>>,
  reading: Option<(Key, Receiver<Result<Transcript>>)>,
}

impl TranscriptCache {
  /// `None` until `load` has taken the transcript of `session` in.
  pub(crate) fn get(&self, session: &Session) -> Option<&Result<Transcript>> {
    return self.read.get(&(session.id.clone(), session.bytes));
  }

  /// Starts a thread reading the transcript of `session`, or takes in what
  /// that thread has read. Loading another session abandons a read still in
  /// flight.
  pub(crate) fn load(&mut self, session: &Session) {
    let key = (session.id.clone(), session.bytes);
    if self.read.contains_key(&key) {
      self.reading = None;
      return;
    }

    let arriving = match &self.reading {
      Some((reading, arriving)) if *reading == key => arriving,

      _ => {
        let (tx, rx) = mpsc::channel();
        let path = session.path.clone();
        std::thread::spawn(move || tx.send(index::transcript(&path)));
        self.reading = Some((key, rx));

        return;
      }
    };

    let transcript = match arriving.try_recv() {
      Ok(transcript) => transcript,
      Err(TryRecvError::Empty) => return,
      Err(TryRecvError::Disconnected) => Err(anyhow!("the transcript reader crashed")),
    };

    self.reading = None;
    self.read.insert(key, transcript);
  }

  pub(crate) fn is_reading(&self) -> bool {
    return self.reading.is_some();
  }
}
