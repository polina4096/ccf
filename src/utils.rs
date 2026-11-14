use std::{os::unix::process::CommandExt, process::Command};

use anyhow::{Context, Result, anyhow};

use crate::index::{self, Session};

/// Replace the process with `claude --resume` in the chat directory.
pub(crate) fn resume(session: &Session) -> Result<!> {
  if !session.cwd.is_dir() {
    return Err(anyhow!("{} no longer exists", session.cwd));
  }

  let error = Command::new("claude").arg("--resume").arg(&session.id).current_dir(&session.cwd).exec();

  return Err(error).context("launching claude");
}

pub(crate) fn verify_index() -> Result<()> {
  let output = index::rebuild()?;

  println!("{} chats indexed", output.sessions.len());

  for warning in &output.warnings {
    println!("{warning}");
  }

  if !output.warnings.is_empty() {
    return Err(anyhow!("{} records skipped", output.warnings.len()));
  }

  return Ok(());
}
