//! Cell-exact text shaping shared by the list and the preview.

use ratatui::{
  style::{Modifier, Style},
  text::Span,
};
use unicode_truncate::UnicodeTruncateStr;
use unicode_width::UnicodeWidthStr;

use super::ACCENT;

/// Pads or truncates (with `…`) to exactly `width` cells, whitespace collapsed.
pub(crate) fn fit(text: &str, width: usize) -> String {
  let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
  if text.width() <= width {
    return format!("{text}{}", " ".repeat(width - text.width()));
  }

  let (kept, kept_width) = text.unicode_truncate(width.saturating_sub(1));

  return format!("{kept}…{}", " ".repeat(width.saturating_sub(kept_width + 1)));
}

/// The last `width` cells of `text`, with a leading `…` when it was cut.
pub(crate) fn fit_tail(text: &str, width: usize) -> String {
  if text.width() <= width {
    return text.to_string();
  }

  let (kept, _) = text.unicode_truncate_start(width.saturating_sub(1));

  return format!("…{kept}");
}

/// `text` as spans, with the characters at `hits` (sorted positions) accented.
pub(crate) fn highlight(text: &str, hits: &[u32], base: Style) -> Vec<Span<'static>> {
  let hit = base.fg(ACCENT).add_modifier(Modifier::UNDERLINED);
  let chars: Vec<(bool, char)> =
    text.chars().enumerate().map(|(i, c)| (hits.binary_search(&(i as u32)).is_ok(), c)).collect();

  return chars
    .chunk_by(|a, b| a.0 == b.0)
    .map(|run| {
      let text: String = run.iter().map(|(_, c)| c).collect();

      return Span::styled(text, if run[0].0 { hit } else { base });
    })
    .collect();
}

pub(crate) fn plural(n: u32, one: &str, many: &str) -> String {
  return format!("{n} {}", if n == 1 { one } else { many });
}

/// `312`, `4.2k`, `305k`, `1.3M`.
pub(crate) fn compact_tokens(n: u64) -> String {
  return match n {
    n if n < 1_000 => n.to_string(),
    n if n < 100_000 => format!("{:.1}k", n as f64 / 1e3),
    n if n < 1_000_000 => format!("{}k", n / 1_000),
    n => format!("{:.1}M", n as f64 / 1e6),
  };
}

/// `917K`, `5.7M`: five cells at most.
pub(crate) fn compact_size(bytes: u64) -> String {
  return match bytes {
    b if b < 1 << 10 => format!("{b}B"),
    b if b < 1 << 20 => format!("{}K", b >> 10),
    b => format!("{:.1}M", b as f64 / (1u64 << 20) as f64),
  };
}
