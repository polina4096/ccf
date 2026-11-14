mod app;
mod index;
mod ui;
mod utils;

use anyhow::Result;
use camino::Utf8PathBuf;
use clap::Parser;

use crate::app::App;

/// Search, browse and resume local Claude Code chats.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
  /// Print the chosen session id instead of resuming it.
  #[arg(long)]
  print: bool,

  /// Ignore the cache and re-read every session file.
  #[arg(long)]
  reindex: bool,

  /// Rebuild the index from every session file, list each record that had to be skipped, and exit.
  #[arg(long)]
  verify: bool,
}

fn main() -> Result<()> {
  let cli = Cli::parse();

  if cli.verify {
    return utils::verify_index();
  }

  let indexer = index::Indexer::start(cli.reindex);
  let sessions = if cli.reindex { vec![] } else { index::cached_sessions()? };
  let cwd = Utf8PathBuf::try_from(std::env::current_dir()?)?;
  let mut app = App::new(sessions, indexer, cwd);

  match ui::run(&mut app)? {
    Some(session) if cli.print => println!("{}", session.id),
    Some(session) => utils::resume(&session)?,
    None => {}
  }

  return Ok(());
}
