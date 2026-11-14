//! Age labels for the list: group headers and the short stamp per row.

use jiff::{SpanRound, Timestamp, Unit, Zoned};

/// The header a chat is listed under when the list is unfiltered.
pub(crate) fn age_group(at: &Timestamp, now: &Zoned) -> &'static str {
  let then = at.to_zoned(now.time_zone().clone());
  let days = then.date().until(now.date()).map_or(0, |span| span.get_days());

  return match days {
    ..= 0 => "today",
    1 => "yesterday",
    2 ..= 6 => "this week",
    7 ..= 13 => "last week",
    14 ..= 30 => "this month",
    31 ..= 61 => "last month",
    62 ..= 364 => "this year",
    _ => "older",
  };
}

/// `now`, `3m`, `2h`, `5d`, `Sep 12`, `Jul 1 '25`: what a six-cell column can hold.
pub(crate) fn age_label(at: &Timestamp, now: &Zoned) -> String {
  let span = now.timestamp().since(*at).unwrap_or_default();
  let secs = span.get_seconds();
  let rounded = |unit| span.round(SpanRound::new().largest(unit).smallest(unit)).unwrap_or_default();

  if secs < 60 {
    return "now".to_string();
  }
  if secs < 3600 {
    return format!("{}m", rounded(Unit::Minute).get_minutes());
  }
  if secs < 86_400 {
    return format!("{}h", rounded(Unit::Hour).get_hours());
  }
  if secs < 7 * 86_400 {
    return format!("{}d", rounded(Unit::Hour).get_hours() / 24);
  }

  let then = at.to_zoned(now.time_zone().clone());
  if then.year() == now.year() {
    return then.strftime("%b %-d").to_string();
  }

  return then.strftime("%b %-d '%y").to_string();
}
