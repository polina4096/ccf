//! The preview pane: a compact header for the selected chat and its transcript.

use std::ops::Range;

use anyhow::Result;
use jiff::{SpanRound, Unit, Zoned};
use ratatui::{
  Frame,
  layout::{Constraint, Layout, Rect},
  style::{Modifier, Style, Stylize},
  text::{Line, Span},
  widgets::Paragraph,
};

use super::{
  ACCENT, BAR, chrome,
  text::{compact_size, compact_tokens, fit, plural},
};
use crate::{
  app::App,
  config::{Config, MessageCut, MessageOrder},
  index::{Role, Session, Transcript},
};

/// Whether the pane is shown and how far its transcript is scrolled.
pub(crate) struct PreviewPane {
  pub(crate) show: bool,

  /// Whether the movement keys scroll this pane instead of moving the list
  /// cursor.
  pub(crate) focused: bool,

  /// Where the pane was last drawn; empty while it is not on screen.
  pub(crate) area: Rect,

  /// First transcript line on screen. Drawing pulls a value past the last
  /// screenful back to it.
  scroll: u16,
}

impl PreviewPane {
  pub(crate) fn new(show: bool) -> Self {
    return Self {
      show,
      focused: false,
      area: Rect::default(),
      scroll: 0,
    };
  }

  /// Focusing a hidden pane shows it.
  pub(crate) fn toggle_focus(&mut self) {
    self.focused = !self.focused;
    self.show |= self.focused;
  }

  pub(crate) fn scroll_by(&mut self, delta: i16) {
    self.scroll = self.scroll.saturating_add_signed(delta);
  }

  pub(crate) fn scroll_to_top(&mut self) {
    self.scroll = 0;
  }

  pub(crate) fn scroll_to_end(&mut self) {
    self.scroll = u16::MAX;
  }

  /// Jumps to the end, or back to the top when already away from it.
  pub(crate) fn toggle_end(&mut self) {
    self.scroll = if self.scroll == 0 { u16::MAX } else { 0 };
  }
}

/// Bar, role label and a space: the indent every transcript line hangs from.
const GUTTER: usize = 9;

/// Draws the chat at `index` of the app's sessions.
pub(crate) fn draw(
  frame: &mut Frame,
  app: &App,
  index: usize,
  pane: &mut PreviewPane,
  config: &Config,
  area: Rect,
  rule_style: Style,
) {
  let session = &app.sessions[index];
  let snippet = app.search.snippet(&app.sessions, index, (area.width as usize).saturating_sub(6));

  let mut header = header_lines(session, area.width as usize);
  header.extend(snippet.map(snippet_line));

  let [head, rule, body] = Layout::vertical([
    Constraint::Length(header.len() as u16),
    Constraint::Length(1),
    Constraint::Fill(1),
  ])
  .areas(area);

  frame.render_widget(Paragraph::new(header), head);
  chrome::hrule(
    frame.buffer_mut(),
    Rect {
      x: rule.x.saturating_sub(1),
      width: rule.width + 1,
      ..rule
    },
    None,
    rule_style,
  );

  let transcript = app.transcript(index);
  let lines = transcript_lines(transcript, pane, config, body);
  if matches!(transcript, Some(Ok(_))) {
    pane.scroll = pane.scroll.min((lines.len() as u16).saturating_sub(body.height));
  }
  frame.render_widget(Paragraph::new(lines).scroll((pane.scroll, 0)), body);
}

fn header_lines(s: &Session, width: usize) -> Vec<Line<'static>> {
  let dot = || "  ·  ".dim();

  let branch = s.branch.as_ref().map(|b| format!("  ⎇ {b}")).unwrap_or_default();
  let missing = if s.cwd.is_dir() { "" } else { "  (directory gone)" };

  let when = [Some(time_span(s)), Some(duration(s)), s.model.clone()];
  let stats = [
    Some(plural(s.turns, "turn", "turns")),
    Some(plural(s.replies, "reply", "replies")),
    (s.agents > 0).then(|| plural(s.agents, "agent", "agents")),
    (s.ctx_tokens > 0).then(|| format!("ctx {}", compact_tokens(s.ctx_tokens))),
    (s.out_tokens > 0).then(|| format!("out {}", compact_tokens(s.out_tokens))),
    Some(compact_size(s.bytes)),
    Some(s.short_id().to_string()),
  ];
  let dotted = |parts: &[Option<String>]| {
    let spans: Vec<Span> = parts.iter().flatten().flat_map(|part| [dot(), part.clone().into()]).skip(1).collect();

    return Line::from(spans);
  };

  return vec![
    Line::from(fit(&s.title, width).trim_end().to_string().bold()),
    Line::from(vec![s.short_cwd().fg(ACCENT), branch.dim(), missing.dim().italic()]),
    dotted(&when),
    dotted(&stats),
  ];
}

/// `Fri 25 Sep 2026  20:54–20:57` within a day, else `11 Aug 2026 15:22 → 25 Sep 2026 21:27`.
fn time_span(s: &Session) -> String {
  let tz = Zoned::now().time_zone().clone();
  let started = s.started.to_zoned(tz.clone());
  let updated = s.updated.to_zoned(tz);

  if started.date() == updated.date() {
    return format!(
      "{}  {}–{}",
      started.strftime("%a %-d %b %Y"),
      started.strftime("%H:%M"),
      updated.strftime("%H:%M")
    );
  }

  return format!("{} → {}", started.strftime("%-d %b %Y %H:%M"), updated.strftime("%-d %b %Y %H:%M"));
}

/// `45m`, `2h 05m`, `45d 6h`.
fn duration(s: &Session) -> String {
  let span = s.updated.since(s.started).unwrap_or_default();
  if span.get_seconds() < 60 {
    return "under a minute".to_string();
  }

  let rounded = span.round(SpanRound::new().largest(Unit::Hour).smallest(Unit::Minute)).unwrap_or_default();

  return match (rounded.get_hours(), rounded.get_minutes()) {
    (0, m) => format!("{m}m"),
    (h, _) if h >= 48 => format!("{}d {}h", h / 24, h % 24),
    (h, m) => format!("{h}h {m:02}m"),
  };
}

/// Where the query first occurs in the conversation, for a chat whose title
/// did not explain the match.
fn snippet_line((text, hit): (String, Range<usize>)) -> Line<'static> {
  return Line::from(vec![
    "⌕ ".fg(ACCENT),
    "…".dim(),
    text[.. hit.start].to_string().dim().italic(),
    text[hit.clone()].to_string().fg(ACCENT).underlined(),
    text[hit.end ..].to_string().dim().italic(),
  ]);
}

/// The transcript as screen lines, built only as far as the screen shows.
fn transcript_lines(
  transcript: Option<&Result<Transcript>>,
  pane: &PreviewPane,
  config: &Config,
  body: Rect,
) -> Vec<Line<'static>> {
  let budget = pane.scroll as usize + body.height as usize;
  let width = (body.width as usize).saturating_sub(GUTTER).max(10);
  let note = |text: String| vec![Line::from(text.dim().italic())];

  let transcript = match transcript {
    None => return note("loading…".to_string()),
    Some(Err(e)) => return note(format!("{e:#}")),
    Some(Ok(transcript)) => transcript,
  };

  let messages: Box<dyn Iterator<Item = &(Role, String)>> = match config.preview_order {
    MessageOrder::OldestFirst => Box::new(transcript.messages.iter()),
    MessageOrder::NewestFirst => Box::new(transcript.messages.iter().rev()),
  };

  return messages
    .flat_map(|(role, text)| message_lines(*role, text, width, config).into_iter().chain([Line::default()]))
    .take(budget)
    .collect();
}

/// One message as gutter lines, the role label on the first. A message longer
/// than the configured limit has a `…` line where it was cut.
fn message_lines(role: Role, text: &str, width: usize, config: &Config) -> Vec<Line<'static>> {
  let (tag, bar_style, text_style, cut) = match role {
    Role::User => ("you", Style::new().fg(ACCENT), Style::new(), MessageCut::End),
    Role::Assistant => ("claude", Style::new().dim(), Style::new().dim(), config.preview_claude_cut),
  };

  let limit = config.preview_message_lines;
  let wrapped = text
    .lines()
    .filter(|line| !line.trim().is_empty())
    .flat_map(|line| textwrap::wrap(line.trim_end(), width))
    .map(String::from);

  // `None` is the line standing for what was cut.
  let rows: Vec<Option<String>> = match cut {
    MessageCut::End => wrapped.take(limit + 1).enumerate().map(|(i, line)| (i < limit).then_some(line)).collect(),

    // The tail is only known once the whole message is wrapped.
    MessageCut::Middle => {
      let lines: Vec<String> = wrapped.collect();
      if lines.len() <= limit {
        lines.into_iter().map(Some).collect()
      }
      else {
        let head = lines.iter().take(limit.div_ceil(2)).cloned().map(Some);
        let tail = lines.iter().skip(lines.len() - limit / 2).cloned().map(Some);

        head.chain([None]).chain(tail).collect()
      }
    }
  };

  return rows
    .into_iter()
    .enumerate()
    .map(|(i, row)| {
      match row {
        Some(line) => gutter_line((i == 0).then_some(tag), bar_style, Span::styled(line, text_style)),
        None => gutter_line(None, bar_style, "…".dim()),
      }
    })
    .collect();
}

fn gutter_line(label: Option<&str>, bar_style: Style, body: Span<'static>) -> Line<'static> {
  let label = format!(" {:<width$}", label.unwrap_or(""), width = GUTTER - 2);

  return Line::from(vec![
    Span::styled(BAR, bar_style),
    Span::styled(label, bar_style.add_modifier(Modifier::BOLD)),
    body,
  ]);
}
