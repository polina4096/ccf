mod cache;
mod content;
mod indexer;
mod parse;

use camino::Utf8PathBuf;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use self::content::Content;
pub(crate) use self::{
  indexer::{Indexer, cached_sessions, rebuild},
  parse::{Role, Transcript, transcript},
};

/// One chat as listed and searched. `content` is the mapped conversation text
/// and lives beside the serialized index rather than inside it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Session {
  pub(crate) id: String,
  pub(crate) path: Utf8PathBuf,
  pub(crate) cwd: Utf8PathBuf,
  pub(crate) branch: Option<String>,

  /// Custom title, else Claude's own title, else the first prompt.
  pub(crate) title: String,

  #[serde(skip)]
  pub(crate) content: Content,

  pub(crate) started: Timestamp,
  pub(crate) updated: Timestamp,

  /// Prompts a person typed, and replies to them.
  pub(crate) turns: u32,
  pub(crate) replies: u32,
  pub(crate) bytes: u64,

  /// Last model that answered, e.g. "opus 4.1".
  pub(crate) model: Option<String>,

  /// Context size at the last reply, and output tokens over the whole chat.
  pub(crate) ctx_tokens: u64,
  pub(crate) out_tokens: u64,

  pub(crate) agents: u32,
}

impl Session {
  /// The first eight characters of the id, enough to tell chats apart.
  pub(crate) fn short_id(&self) -> &str {
    return self.id.get(.. 8).unwrap_or(&self.id);
  }

  /// The working directory with the home prefix folded to `~`.
  pub(crate) fn short_cwd(&self) -> String {
    return dirs::home_dir()
      .and_then(|home| self.cwd.strip_prefix(home).ok())
      .map(|rest| if rest.as_str().is_empty() { "~".to_string() } else { format!("~/{rest}") })
      .unwrap_or_else(|| self.cwd.to_string());
  }

  /// Last path component, or `~` for the home directory itself.
  pub(crate) fn project(&self) -> String {
    return match self.short_cwd().as_str() {
      "~" => "~".to_string(),
      _ => self.cwd.file_name().unwrap_or("/").to_string(),
    };
  }
}
