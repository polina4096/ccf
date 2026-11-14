mod chrome;
mod list;
mod preview;
mod text;
mod time;

use ratatui::{
  Frame,
  layout::{Constraint, Layout, Rect},
  style::{Color, Style},
};

pub(crate) use self::{list::ListPane, preview::PreviewPane};
use super::Ui;
use crate::app::App;

pub(crate) const ACCENT: Color = Color::Rgb(217, 119, 87);
pub(crate) const RULE: &str = "─";
pub(crate) const BAR: &str = "▎";

pub(crate) fn draw(frame: &mut Frame, app: &App, ui: &mut Ui) {
  let [search, header, body, foot_rule, footer] = Layout::vertical([
    Constraint::Length(1),
    Constraint::Length(1),
    Constraint::Fill(1),
    Constraint::Length(1),
    Constraint::Length(1),
  ])
  .areas(frame.area());

  let with_preview = ui.preview.show && body.width >= ui.config.preview_min_width;
  let [list, preview] = if with_preview {
    Layout::horizontal([Constraint::Percentage(50), Constraint::Fill(1)]).areas(body)
  }
  else {
    Layout::horizontal([Constraint::Fill(1), Constraint::Length(0)]).areas(body)
  };

  ui.preview.area = preview;
  if !with_preview {
    ui.preview.focused = false;
  }

  // A focused preview is framed in the accent colour and the list beside it fades.
  let dim = Style::new().dim();
  let (vrule, frame_style) = if ui.preview.focused { ("┃", Style::new().fg(ACCENT)) } else { ("│", dim) };

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
    frame_style,
  );

  list::draw(frame, app, ui.selected, &mut ui.list, list, &columns);

  if ui.preview.focused {
    frame.buffer_mut().set_style(Rect { y: header.y, height: header.height + list.height, ..list }, dim);
  }

  let selection = ui.selected_index(app);

  if with_preview {
    chrome::vrule(frame.buffer_mut(), Rect { x: preview.x + 1, width: 1, ..preview }, vrule, frame_style);
    let pane = Rect {
      x: preview.x + 3,
      width: preview.width.saturating_sub(4),
      ..preview
    };

    if let Some(index) = selection {
      preview::draw(frame, app, index, &mut ui.preview, &ui.config, pane, frame_style);
    }
  }

  chrome::hrule(frame.buffer_mut(), foot_rule, None, dim);

  let selection = selection.filter(|_| !with_preview).map(|index| &app.sessions[index]);

  chrome::footer(frame, app, ui, footer, selection);
}
