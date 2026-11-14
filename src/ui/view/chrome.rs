use ratatui::{
  Frame,
  buffer::Buffer,
  layout::{Position, Rect},
  style::{Style, Stylize},
  text::{Line, Span},
  widgets::{Paragraph, Widget},
};
use tui_input::Input;
use unicode_width::UnicodeWidthStr;

use super::{ACCENT, RULE, text::fit_tail};
use crate::{
  app::App,
  index::{Session, short_path},
  ui::Ui,
};

/// A horizontal rule across `area`, optionally headed by an accent label.
pub(crate) fn hrule(buf: &mut Buffer, area: Rect, label: Option<&str>, style: Style) {
  let mut spans: Vec<Span> = vec![" ".into()];
  let mut used = 1;
  if let Some(label) = label {
    spans.push(label.fg(ACCENT));
    spans.push(" ".into());
    used += label.width() + 1;
  }

  spans.push(Span::styled(RULE.repeat((area.width as usize).saturating_sub(used + 1)), style));

  Line::from(spans).render(area, buf);
}

pub(crate) fn vrule(buf: &mut Buffer, area: Rect, symbol: &str, style: Style) {
  for y in area.y .. area.bottom() {
    buf[(area.x, y)].set_symbol(symbol).set_style(style);
  }
}

pub(crate) fn search(frame: &mut Frame, app: &App, query: &Input, area: Rect) {
  let right = match app.indexer.progress {
    Some((done, total)) => Line::from(vec![meter(done, total, 8).fg(ACCENT), format!(" {done}/{total} ").dim()]),
    None => {
      let scope = if app.cwd_only { format!("in {}  ", fit_tail(&short_path(&app.cwd), 32)) } else { String::new() };

      Line::from(vec![
        scope.fg(ACCENT),
        app.matches.len().to_string().into(),
        format!("/{} ", app.sessions.len()).dim(),
      ])
    }
  };

  let right_width = right.width() as u16;
  frame.render_widget(right.right_aligned(), area);

  let prompt = " ⌕ ";
  frame.render_widget(prompt.fg(ACCENT).bold(), area);

  // The count always survives; a long query scrolls sideways before it, far
  // enough to keep its cursor on screen.
  let text = Rect {
    x: area.x + prompt.width() as u16,
    width: area.width.saturating_sub(prompt.width() as u16 + right_width + 1),
    ..area
  };

  let scroll = query.visual_scroll((text.width as usize).saturating_sub(1));
  frame.render_widget(Paragraph::new(query.value()).scroll((0, scroll as u16)), text);
  frame.set_cursor_position(Position::new(text.x + (query.visual_cursor() - scroll) as u16, area.y));
}

/// `▰▰▰▱▱▱▱▱`: progress in `cells` cells.
fn meter(done: usize, total: usize, cells: usize) -> String {
  let filled = (done * cells).checked_div(total).unwrap_or(0);

  return format!("{}{}", "▰".repeat(filled), "▱".repeat(cells - filled));
}

/// Key hints on the left; on the right the most pressing note: the status,
/// a stopped indexer, skipped records, or `selection` when one is given.
pub(crate) fn footer(frame: &mut Frame, app: &App, ui: &Ui, area: Rect, selection: Option<&Session>) {
  let keys: [(&str, &str); 9] = [
    ("⏎", "select"),
    ("⇥", if ui.preview.focused { "list" } else { "chat" }),
    ("⇧⇥", "preview"),
    ("^d ^e", "scroll"),
    ("^g", "end"),
    ("^y", "copy"),
    ("^r", "rescan"),
    ("^t", if app.cwd_only { "everywhere" } else { "here" }),
    ("esc", "quit"),
  ];

  let mut spans: Vec<Span> = vec![" ".into()];
  for (key, what) in keys {
    spans.push(key.fg(ACCENT));
    spans.push(format!(" {what}   ").dim());
  }

  let hints = Line::from(spans);
  let hints_width = hints.width() as u16;
  frame.render_widget(hints, area);

  let accent = Style::new().fg(ACCENT).italic();
  let dim = Style::new().dim();
  let warnings = app.indexer.warnings.len();
  let note = None
    .or(ui.status.clone().map(|status| (status, accent)))
    .or(app.indexer.failure.as_ref().map(|why| (format!("indexer stopped: {why}"), accent)))
    .or((warnings > 0).then(|| (format!("{warnings} files with skipped records, see ccf --verify"), dim)))
    .or(selection.map(|s| (format!("{}  {}", s.short_cwd(), s.short_id()), dim)));

  let Some((note, style)) = note
  else {
    return;
  };

  // The note gets only the room the hints leave, shortened from its start so
  // the end of a path stays readable, and never paints over the hints.
  let room = Rect {
    x: area.x + hints_width,
    width: area.width.saturating_sub(hints_width + 1),
    ..area
  };

  if room.width < 8 {
    return;
  }

  let note = fit_tail(&note, room.width as usize);
  frame.render_widget(Line::from(Span::styled(note, style)).right_aligned(), room);
}
