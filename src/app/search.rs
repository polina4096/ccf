//! The ranked search over sessions: exact title words, then words anywhere in
//! the conversation, then fuzzy title matches as a last resort.

use std::{
  cell::RefCell,
  collections::{HashMap, HashSet},
  ops::Range,
  sync::Arc,
};

use memchr::memmem;
use nucleo::{
  Config, Matcher, Nucleo, Utf32String,
  pattern::{CaseMatching, Normalization},
};
use rayon::prelude::*;

use crate::index::Session;

/// The query and everything that answers it.
pub(crate) struct SessionSearch {
  query: String,

  /// Exact title matches, then fuzzy title matches.
  tiers: [Tier; 2],
  /// Scratch memory for `match_indices`, which is why it may be asked through `&self`.
  highlighter: RefCell<Matcher>,

  /// Where the query first occurs in each session's content: session index →
  /// byte range of the earliest matched word.
  hits: HashMap<usize, Range<usize>>,
}

impl SessionSearch {
  pub(crate) fn new() -> Self {
    return Self {
      query: String::new(),
      tiers: [Tier::new(true), Tier::new(false)],
      highlighter: RefCell::new(Matcher::new(Config::DEFAULT)),
      hits: HashMap::new(),
    };
  }

  pub(crate) fn is_empty(&self) -> bool {
    return self.query.is_empty();
  }

  /// Re-runs the query over a new session list.
  pub(crate) fn set_sessions(&mut self, sessions: &[Session]) {
    for tier in &mut self.tiers {
      tier.set_sessions(sessions);
    }

    self.reparse(sessions, false);
  }

  pub(crate) fn set_query(&mut self, sessions: &[Session], query: &str) {
    let append = query.starts_with(&self.query);
    self.query = query.to_string();

    self.reparse(sessions, append);
  }

  fn reparse(&mut self, sessions: &[Session], append: bool) {
    for tier in &mut self.tiers {
      tier.reparse(&self.query, append);
    }

    self.hits = content_search(sessions, &self.query);
  }

  /// Exact title hits, then content hits newest first, then fuzzy title hits.
  /// Nucleo matches asynchronously, so the answer settles over a few frames.
  pub(crate) fn ranked(&mut self) -> Vec<usize> {
    let by_title = self.tiers[0].matches();
    let mut by_content: Vec<usize> = self.hits.keys().copied().collect();
    by_content.sort_unstable();

    let by_fuzzy_title = self.tiers[1].matches();

    return first_occurrences(by_title.into_iter().chain(by_content).chain(by_fuzzy_title));
  }

  /// About `width` cells of one line of text around the place the query first
  /// occurs in the content of `sessions[index]`, with the matched word's byte
  /// range inside that text.
  pub(crate) fn snippet(&self, sessions: &[Session], index: usize, width: usize) -> Option<(String, Range<usize>)> {
    let hit = self.hits.get(&index)?;
    let text = String::from_utf8_lossy(sessions[index].content.as_bytes());

    let start = text.floor_char_boundary(hit.start.saturating_sub(width / 3));
    let end = text.ceil_char_boundary((hit.end + width).min(text.len()));
    let line = text[start .. end].replace(char::is_whitespace, " ");

    return Some((line, hit.start - start .. hit.end - start));
  }

  /// Character positions of `text` the query matched, for highlighting.
  pub(crate) fn match_indices(&self, text: &str) -> Vec<u32> {
    if self.query.is_empty() {
      return vec![];
    }

    return self.tiers[1].indices(text, &mut self.highlighter.borrow_mut());
  }
}

/// `order` with every repeat after the first dropped.
fn first_occurrences(order: impl IntoIterator<Item = usize>) -> Vec<usize> {
  let mut seen = HashSet::new();

  return order.into_iter().filter(|i| seen.insert(*i)).collect();
}

/// A nucleo matcher over `title project branch`, exact or fuzzy.
struct Tier {
  nucleo: Nucleo<usize>,
  exact: bool,
}

impl Tier {
  fn new(exact: bool) -> Self {
    return Self {
      nucleo: Nucleo::new(Config::DEFAULT, Arc::new(|| {}), None, 1),
      exact,
    };
  }

  fn set_sessions(&mut self, sessions: &[Session]) {
    self.nucleo.restart(true);

    let injector = self.nucleo.injector();
    for (i, s) in sessions.iter().enumerate() {
      let text = format!("{} {} {}", s.title, s.short_cwd(), s.branch.as_deref().unwrap_or(""));
      injector.push(i, |_, columns| columns[0] = Utf32String::from(text.as_str()));
    }
  }

  /// `append` is the hint nucleo uses to keep its previous results when the
  /// query only grew.
  fn reparse(&mut self, query: &str, append: bool) {
    let pattern = if self.exact { exact_pattern(query) } else { query.to_string() };
    self.nucleo.pattern.reparse(0, &pattern, CaseMatching::Smart, Normalization::Smart, append);
  }

  /// Best matches first.
  fn matches(&mut self) -> Vec<usize> {
    self.nucleo.tick(10);

    let snapshot = self.nucleo.snapshot();
    return snapshot.matched_items(0 .. snapshot.matched_item_count()).map(|item| *item.data).collect();
  }

  /// Character positions of `text` the fuzzy pattern matched, sorted.
  fn indices(&self, text: &str, matcher: &mut Matcher) -> Vec<u32> {
    let haystack = Utf32String::from(text);

    let mut indices = vec![];
    self.nucleo.pattern.column_pattern(0).indices(haystack.slice(..), matcher, &mut indices);
    indices.sort_unstable();
    indices.dedup();

    return indices;
  }
}

/// Every word becomes a `'substring` atom; nucleo's own `!neg` and `^prefix`
/// atoms pass through.
fn exact_pattern(query: &str) -> String {
  return query
    .split_whitespace()
    .map(|w| if w.starts_with(['\'', '!', '^']) { w.to_string() } else { format!("'{w}") })
    .collect::<Vec<_>>()
    .join(" ");
}

/// A query word for content search: lowercased, fzf-style prefixes stripped.
struct Word {
  negated: bool,
  finder: memmem::Finder<'static>,
}

fn words(query: &str) -> Vec<Word> {
  return query
    .split_whitespace()
    .map(|w| (w.starts_with('!'), w.trim_start_matches(['!', '\'', '^']).to_lowercase()))
    .filter(|(_, w)| !w.is_empty())
    .map(|(negated, w)| {
      Word {
        negated,
        finder: memmem::Finder::new(w.as_bytes()).into_owned(),
      }
    })
    .collect();
}

/// Sessions where every positive word occurs and no negated word does, with
/// the byte range of the earliest occurrence. Runs on every keystroke: the
/// text is a few tens of megabytes and the scan is SIMD substring search.
fn content_search(sessions: &[Session], query: &str) -> HashMap<usize, Range<usize>> {
  let words = words(query);
  if words.iter().all(|w| w.negated) {
    return HashMap::new();
  }

  return sessions
    .par_iter()
    .enumerate()
    .filter_map(|(i, session)| {
      let content = session.content.as_bytes();
      let mut earliest: Option<Range<usize>> = None;
      for word in &words {
        match (word.finder.find(content), word.negated) {
          (Some(_), true) | (None, false) => return None,
          (None, true) => {}
          (Some(at), false) => {
            let found = at .. at + word.finder.needle().len();
            earliest = Some(earliest.filter(|e| e.start <= found.start).unwrap_or(found));
          }
        }
      }

      return earliest.map(|range| (i, range));
    })
    .collect();
}
