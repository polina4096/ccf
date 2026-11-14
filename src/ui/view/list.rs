use jiff::Zoned;
use ratatui::{
  Frame,
  buffer::Buffer,
  layout::Rect,
  style::{Modifier, Style, Stylize},
  text::{Line, Span},
  widgets::Widget,
};

use super::{
  ACCENT, BAR, chrome,
  text::{compact_size, fit, highlight},
  time::{age_group, age_label},
};
use crate::{app::App, index::Session};

/// Cell widths of the columns; zero hides a column.
pub(crate) struct Columns {
  title: usize,
  project: usize,
  model: usize,
  size: usize,
}

impl Columns {
  const TURNS_AND_AGE: usize = 5 + 1 + 6;

  pub(crate) fn for_width(width: u16) -> Self {
    let width = width as usize;
    let project = match width {
      90 .. => 24,
      60 .. => 16,
      _ => 0,
    };
    let model = if width >= 78 { 10 } else { 0 };
    let size = if width >= 100 { 5 } else { 0 };

    let shown = [project, model, size].iter().filter(|&&c| c > 0).count();
    let margins = 3 + shown + 1 + 1;
    let fixed = margins + project + model + size + Self::TURNS_AND_AGE;

    return Self {
      title: width.saturating_sub(fixed).max(8),
      project,
      model,
      size,
    };
  }

  /// The cells after the title: each shown column fitted to its width behind
  /// a one-cell gap, in the style of the span it was given.
  fn cells(&self, project: Span, model: Span, size: Span, turns_and_age: Span) -> Vec<Span<'static>> {
    let left = |width: usize, cell: Span| Span::styled(fit(&cell.content, width), cell.style);
    let right = |width: usize, cell: Span| Span::styled(format!("{:>width$}", cell.content), cell.style);

    return [
      (self.project > 0).then(|| left(self.project, project)),
      (self.model > 0).then(|| left(self.model, model)),
      (self.size > 0).then(|| right(self.size, size)),
      Some(right(Self::TURNS_AND_AGE, turns_and_age)),
    ]
    .into_iter()
    .flatten()
    .flat_map(|cell| [Span::raw(" "), cell])
    .collect();
  }
}

pub(crate) fn column_labels(buf: &mut Buffer, area: Rect, columns: &Columns) {
  let mut spans: Vec<Span> = vec!["   ".into(), fit("title", columns.title).dim()];
  spans.extend(columns.cells(
    "project".dim(),
    "model".dim(),
    "size".dim(),
    format!("{:>5} {:>6}", "turns", "age").dim(),
  ));

  Line::from(spans).render(area, buf);
}

/// One screen line of the list.
enum Slot {
  Gap,
  Header(&'static str),
  Item(usize),
}

/// The matched sessions as screen lines. `grouped` puts them under age headers,
/// each header after the first below a gap.
fn slots(app: &App, grouped: bool, now: &Zoned) -> Vec<Slot> {
  if !grouped {
    return app.matches.iter().map(|&i| Slot::Item(i)).collect();
  }

  let group = |i: &usize| age_group(&app.sessions[*i].updated, now);

  return app
    .matches
    .chunk_by(|a, b| group(a) == group(b))
    .enumerate()
    .flat_map(|(n, chunk)| {
      let gap = (n > 0).then_some(Slot::Gap);
      let items = chunk.iter().map(|&i| Slot::Item(i));

      return gap.into_iter().chain([Slot::Header(group(&chunk[0]))]).chain(items);
    })
    .collect();
}

pub(crate) fn draw(frame: &mut Frame, app: &App, selected: usize, scroll: &mut u16, area: Rect, columns: &Columns) {
  let now = Zoned::now();
  let slots = slots(app, app.search.is_empty(), &now);
  if slots.is_empty() {
    let message = match () {
      () if app.sessions.is_empty() => "no chats yet",
      () if app.search.is_empty() => "no chats in this directory",
      () => "nothing matches",
    };

    frame.render_widget(Line::from(message.dim().italic()).centered(), Rect { y: area.y + 1, height: 1, ..area });
    return;
  }

  let current = app.matches.get(selected).copied();
  let cursor = slots.iter().position(|slot| matches!(slot, Slot::Item(i) if Some(*i) == current)).unwrap_or(0);
  // The first chat brings the header above it along.
  let from = if selected == 0 { 0 } else { *scroll };

  *scroll = scroll_to_show(from, cursor as u16, area.height, slots.len() as u16);
  let scroll = *scroll as usize;

  let list = Rect {
    width: area.width.saturating_sub(1),
    ..area
  };

  for (index, slot) in slots.iter().enumerate().skip(scroll).take(area.height as usize) {
    let at = Rect::new(list.x, area.y + (index - scroll) as u16, list.width, 1);
    match slot {
      Slot::Gap => {}
      Slot::Header(label) => chrome::hrule(frame.buffer_mut(), at, Some(label)),
      Slot::Item(i) => {
        let session = &app.sessions[*i];
        let hits = app.search.match_indices(&session.title);
        frame.render_widget(item_line(session, index == cursor, &hits, columns, &now), at);
      }
    }
  }

  scrollbar(
    frame.buffer_mut(),
    Rect { x: area.right() - 1, width: 1, ..area },
    scroll as u16,
    slots.len() as u16,
  );
}

/// `scroll` moved only as far as keeps `cursor` inside a two-line margin, so
/// the list holds still while the cursor travels inside it.
fn scroll_to_show(mut scroll: u16, cursor: u16, height: u16, total: u16) -> u16 {
  let margin = 2.min(height / 4);
  if cursor + 1 + margin > scroll + height {
    scroll = cursor + 1 + margin - height;
  }

  if cursor < scroll + margin {
    scroll = cursor.saturating_sub(margin);
  }

  return scroll.min(total.saturating_sub(height));
}

fn scrollbar(buf: &mut Buffer, area: Rect, scroll: u16, total: u16) {
  if total <= area.height || area.height == 0 {
    return;
  }

  let track = area.height as u32;
  let thumb = ((track * track) / total as u32).max(1);
  let top = (scroll as u32 * (track - thumb)) / (total - area.height) as u32;
  for i in 0 .. track {
    let on = i >= top && i < top + thumb;
    let style = if on { Style::new().fg(ACCENT) } else { Style::new().dim() };
    buf[(area.x, area.y + i as u16)].set_symbol(if on { "┃" } else { "│" }).set_style(style);
  }
}

fn item_line(s: &Session, selected: bool, hits: &[u32], columns: &Columns, now: &Zoned) -> Line<'static> {
  let (bar, title_style, meta) = if selected {
    (BAR.fg(ACCENT).bold(), Style::new().fg(ACCENT).bold(), Style::new())
  }
  else {
    (" ".into(), Style::new(), Style::new().dim())
  };

  // A project whose directory is gone cannot be resumed in place.
  let project_style = if s.cwd.is_dir() { meta } else { meta.add_modifier(Modifier::CROSSED_OUT) };
  let project = match &s.branch {
    Some(branch) => format!("{} ⎇ {branch}", s.project()),
    None => s.project(),
  };

  let mut spans: Vec<Span> = vec![" ".into(), bar, " ".into()];
  spans.extend(highlight(&fit(&s.title, columns.title), hits, title_style));
  spans.extend(columns.cells(
    Span::styled(project, project_style),
    Span::styled(s.model.clone().unwrap_or_default(), meta),
    Span::styled(compact_size(s.bytes), meta),
    Span::styled(format!("{:>5} {:>6}", s.turns, age_label(&s.updated, now)), meta),
  ));

  return Line::from(spans);
}
