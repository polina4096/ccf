mod input;
mod view;

use std::{
  io,
  ops::ControlFlow::{self, Break, Continue},
  time::Duration,
};

use anyhow::Result;
use ratatui::{
  DefaultTerminal,
  crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, MouseButton, MouseEvent, MouseEventKind},
    execute,
  },
  layout::Position,
};
use tui_input::{Input, InputRequest};

use self::{
  input::Command,
  view::{ListPane, PreviewPane},
};
use crate::{app::App, config::Config, index::Session};

pub(crate) struct Ui {
  /// The search line: the query's text and the cursor in it.
  query: Input,

  /// Cursor position within the app's matches.
  selected: usize,

  list: ListPane,
  preview: PreviewPane,

  /// A one-off note for the footer.
  status: Option<String>,
  config: Config,
}

/// Runs the picker on the terminal until a chat is chosen (`Some`) or the user quits (`None`).
pub(crate) fn run(app: &mut App, config: Config) -> Result<Option<Session>> {
  let mut terminal = ratatui::init();
  let ui = Ui {
    query: Input::default(),
    selected: 0,
    list: ListPane::new(),
    preview: PreviewPane::new(config.show_preview),
    status: None,
    config,
  };
  let output = ui.run(&mut terminal, app);
  let mouse_released = execute!(io::stdout(), DisableMouseCapture);
  ratatui::restore();
  mouse_released?;

  return output;
}

impl Ui {
  fn run(mut self, terminal: &mut DefaultTerminal, app: &mut App) -> Result<Option<Session>> {
    execute!(io::stdout(), EnableMouseCapture)?;

    loop {
      self.update(app);
      terminal.draw(|frame| view::draw(frame, app, &mut self))?;

      let timeout = Duration::from_millis(if app.is_busy() { 30 } else { 250 });

      if !event::poll(timeout)? {
        continue;
      }

      match event::read()? {
        Event::Key(key) => {
          if let Some(command) = input::command(key)
            && let Break(session) = self.apply(command, app)
          {
            return Ok(session);
          }
        }

        Event::Mouse(mouse) => self.apply_mouse(mouse, app),

        _ => {}
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

  /// Like `select`, and the list scrolls to keep the cursor in sight.
  fn move_cursor(&mut self, app: &App, position: usize) {
    self.select(app, position);
    self.list.follows_cursor = true;
  }
}

/// Commands from the keyboard.
impl Ui {
  /// Breaks with the chat to resume, or with `None` on quit.
  fn apply(&mut self, command: Command, app: &mut App) -> ControlFlow<Option<Session>> {
    let selection = self.selected_index(app);

    match command {
      Command::Cancel if self.preview.focused => self.preview.focused = false,
      Command::Cancel if !app.search.is_empty() => self.edit_query(app, InputRequest::DeleteLine),
      Command::Cancel | Command::Quit => return Break(None),
      Command::Choose => {
        if let Some(index) = selection {
          return Break(Some(app.sessions[index].clone()));
        }
      }
      Command::TogglePreview => self.preview.show = !self.preview.show,
      Command::TogglePreviewFocus => self.preview.toggle_focus(),

      Command::Move(delta) if self.preview.focused => self.preview.scroll_by(delta),
      Command::First if self.preview.focused => self.preview.scroll_to_top(),
      Command::Last if self.preview.focused => self.preview.scroll_to_end(),

      Command::Move(delta) => self.move_cursor(app, self.selected.saturating_add_signed(isize::from(delta))),
      Command::First => self.move_cursor(app, 0),
      Command::Last => self.move_cursor(app, usize::MAX),
      Command::ScrollPreview(delta) => self.preview.scroll_by(delta),
      Command::TogglePreviewEnd => self.preview.toggle_end(),

      Command::EditQuery(request) => self.edit_query(app, request),

      Command::Copy => self.status = selection.map(|index| copy_resume_command(&app.sessions[index])),
      Command::Rescan => app.indexer.rescan(),
      Command::ToggleCwdOnly => {
        app.toggle_cwd_only();
        self.move_cursor(app, 0);
      }
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
      self.move_cursor(app, 0);
    }
  }
}

/// Mouse input.
impl Ui {
  const WHEEL_LINES: i16 = 3;

  /// A click focuses the pane under it and, on a chat in the list, moves the cursor
  /// there. The wheel scrolls the pane under the pointer, whichever has the focus.
  fn apply_mouse(&mut self, mouse: MouseEvent, app: &App) {
    let pointer = Position::new(mouse.column, mouse.row);
    let over_list = self.list.area.contains(pointer);
    let over_preview = self.preview.area.contains(pointer);
    let wheel = match mouse.kind {
      MouseEventKind::ScrollUp => Some(-Self::WHEEL_LINES),
      MouseEventKind::ScrollDown => Some(Self::WHEEL_LINES),
      _ => None,
    };

    match (mouse.kind, wheel) {
      (MouseEventKind::Down(MouseButton::Left), _) if over_list => {
        let position = self
          .list
          .session_at(mouse.row)
          .and_then(|index| app.matches.iter().position(|&i| i == index))
          .filter(|&position| position != self.selected);

        if let Some(position) = position {
          self.select(app, position);
          self.preview.scroll_to_top();
        }

        self.preview.focused = false;
      }

      (MouseEventKind::Down(MouseButton::Left), _) if over_preview => self.preview.focused = true,
      (_, Some(lines)) if over_list => self.list.scroll_by(lines),
      (_, Some(lines)) if over_preview => self.preview.scroll_by(lines),

      _ => {}
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
