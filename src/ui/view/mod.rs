mod chrome;
mod list;
mod preview;
mod text;
mod time;

use ratatui::{
  Frame,
  layout::{Constraint, Layout, Rect},
  style::Color,
};

pub(crate) use self::preview::PreviewPane;
use super::Ui;
use crate::app::App;

pub(crate) const ACCENT: Color = Color::Rgb(217, 119, 87);
pub(crate) const RULE: &str = "─";
pub(crate) const BAR: &str = "▎";

/// Below this many columns the preview pane is not worth its space.
const PREVIEW_MIN_WIDTH: u16 = 110;

pub(crate) fn draw(frame: &mut Frame, app: &App, ui: &mut Ui) {
  let [search, header, body, foot_rule, footer] = Layout::vertical([
    Constraint::Length(1),
    Constraint::Length(1),
    Constraint::Fill(1),
    Constraint::Length(1),
    Constraint::Length(1),
  ])
  .areas(frame.area());

  let with_preview = ui.preview.show && body.width >= PREVIEW_MIN_WIDTH;
  let [list, preview] = if with_preview {
    Layout::horizontal([Constraint::Percentage(50), Constraint::Fill(1)]).areas(body)
  }
  else {
    Layout::horizontal([Constraint::Fill(1), Constraint::Length(0)]).areas(body)
  };

  let columns = list::Columns::for_width(list.width.saturating_sub(1));

  chrome::search(frame, app, &ui.query, search);

  list::column_labels(
    frame.buffer_mut(),
    Rect {
      width: list.width.saturating_sub(1),
      ..header
    },
    &columns,
  );

  chrome::hrule(
    frame.buffer_mut(),
    Rect {
      x: list.right() - 1,
      width: header.width - list.width + 1,
      ..header
    },
    None,
  );

  list::draw(frame, app, ui.selected, &mut ui.list_scroll, list, &columns);

  let selection = ui.selected_index(app);

  if with_preview {
    chrome::vrule(frame.buffer_mut(), Rect { x: preview.x + 1, width: 1, ..preview });
    let pane = Rect {
      x: preview.x + 3,
      width: preview.width.saturating_sub(4),
      ..preview
    };

    if let Some(index) = selection {
      preview::draw(frame, app, index, &mut ui.preview, pane);
    }
  }

  chrome::hrule(frame.buffer_mut(), foot_rule, None);

  let selection = selection.filter(|_| !with_preview).map(|index| &app.sessions[index]);

  chrome::footer(frame, app, ui.status.as_deref(), footer, selection);
}
