mod app;
mod config;
mod index;
mod ui;
mod utils;

use std::time::Duration;

use anyhow::Result;
use camino::Utf8PathBuf;
use clap::Parser;

use crate::{app::App, config::Config};

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

  /// Open the configuration file in the default text editor.
  #[arg(long)]
  open_config: bool,
}

fn main() -> Result<()> {
  let cli = Cli::parse();

  if cli.verify {
    return utils::verify_index();
  }

  Config::ensure_exists()?;

  if cli.open_config {
    edit::edit_file(Config::path()?)?;

    return Ok(());
  }

  let config = Config::load()?;

  let indexer = index::Indexer::start(cli.reindex, Duration::from_secs(config.rescan_interval));
  let sessions = if cli.reindex { vec![] } else { index::cached_sessions()? };
  let cwd = Utf8PathBuf::try_from(std::env::current_dir()?)?;
  let mut app = App::new(sessions, indexer, cwd);
  if config.cwd_only {
    app.toggle_cwd_only();
  }

  match ui::run(&mut app, config)? {
    Some(session) if cli.print => println!("{}", session.id),
    Some(session) => utils::resume(&session)?,
    None => {}
  }

  return Ok(());
}
