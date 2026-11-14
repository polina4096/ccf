mod input;
mod view;

use std::{
  ops::ControlFlow::{self, Break, Continue},
  time::Duration,
};

use anyhow::Result;
use ratatui::{
  DefaultTerminal,
  crossterm::event::{self, Event},
};
use tui_input::{Input, InputRequest};

use self::{input::Command, view::PreviewPane};
use crate::{app::App, index::Session};

#[derive(Default)]
pub(crate) struct Ui {
  /// The search line: the query's text and the cursor in it.
  query: Input,

  /// Cursor position within the app's matches.
  selected: usize,

  /// First list line on screen.
  list_scroll: u16,
  preview: PreviewPane,

  /// A one-off note for the footer.
  status: Option<String>,
}

/// Runs the picker on the terminal until a chat is chosen (`Some`) or the user quits (`None`).
pub(crate) fn run(app: &mut App) -> Result<Option<Session>> {
  let mut terminal = ratatui::init();
  let output = Ui::default().run(&mut terminal, app);
  ratatui::restore();

  return output;
}

impl Ui {
  fn run(mut self, terminal: &mut DefaultTerminal, app: &mut App) -> Result<Option<Session>> {
    loop {
      self.update(app);
      terminal.draw(|frame| view::draw(frame, app, &mut self))?;

      let timeout = Duration::from_millis(if app.is_busy() { 30 } else { 250 });

      if !event::poll(timeout)? {
        continue;
      }

      let Event::Key(key) = event::read()?
      else {
        continue;
      };

      let Some(command) = input::command(key)
      else {
        continue;
      };

      if let Break(session) = self.apply(command, app) {
        return Ok(session);
      }
    }
  }

  fn update(&mut self, app: &mut App) {
    let followed = self.selected_index(app).filter(|_| self.selected > 0).map(|i| app.sessions[i].id.clone());
    let replaced = app.refresh();

    let position = followed
      .filter(|_| replaced)
      .and_then(|id| app.matches.iter().position(|&i| app.sessions[i].id == id));

    self.select(app, position.unwrap_or(self.selected));

    if let Some(index) = self.selected_index(app) {
      app.load_transcript(index);
    }
  }

  /// The index into the app's sessions under the cursor.
  fn selected_index(&self, app: &App) -> Option<usize> {
    return app.matches.get(self.selected).copied();
  }

  /// Puts the cursor on `position`, or on the last row when that is past it.
  fn select(&mut self, app: &App, position: usize) {
    self.selected = position.min(app.matches.len().saturating_sub(1));
  }
}

/// Commands from the keyboard.
impl Ui {
  /// Breaks with the chat to resume, or with `None` on quit.
  fn apply(&mut self, command: Command, app: &mut App) -> ControlFlow<Option<Session>> {
    let selection = self.selected_index(app);

    match command {
      Command::Cancel if !app.search.is_empty() => self.edit_query(app, InputRequest::DeleteLine),
      Command::Cancel | Command::Quit => return Break(None),
      Command::Choose => {
        if let Some(index) = selection {
          return Break(Some(app.sessions[index].clone()));
        }
      }
      Command::TogglePreview => self.preview.show = !self.preview.show,

      Command::Move(delta) => self.select(app, self.selected.saturating_add_signed(delta)),
      Command::First => self.select(app, 0),
      Command::Last => self.select(app, usize::MAX),
      Command::ScrollPreview(delta) => self.preview.scroll_by(delta),
      Command::TogglePreviewEnd => self.preview.toggle_end(),

      Command::EditQuery(request) => self.edit_query(app, request),

      Command::Copy => self.status = selection.map(|index| copy_resume_command(&app.sessions[index])),
      Command::Rescan => app.indexer.rescan(),
    }

    if self.selected_index(app) != selection {
      self.preview.scroll_to_top();
    }

    return Continue(());
  }

  /// A changed text is a new query, whose best match takes the cursor.
  fn edit_query(&mut self, app: &mut App, request: InputRequest) {
    let text_changed = self.query.handle(request).is_some_and(|changed| changed.value);
    if text_changed {
      app.set_query(self.query.value());
      self.selected = 0;
    }
  }
}

/// Puts `claude --resume <id>` on the clipboard and says how that went.
fn copy_resume_command(session: &Session) -> String {
  let command = format!("claude --resume {}", session.id);
  let copied = arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(command));

  return match copied {
    Ok(()) => "copied".to_string(),
    Err(e) => format!("copy failed: {e}"),
  };
}
