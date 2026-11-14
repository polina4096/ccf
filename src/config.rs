use anyhow::{Context as _, Result, ensure};
use camino::Utf8PathBuf;
use documented::DocumentedFields;
use etcetera::BaseStrategy as _;
use figment2::{
  Figment,
  providers::{Env, Format as _, Toml},
};
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use toml_edit::DocumentMut;

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum MessageOrder {
  OldestFirst,
  NewestFirst,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum MessageCut {
  End,
  Middle,
}

#[derive(Clone, SmartDefault, Deserialize, Serialize, DocumentedFields)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Config {
  /// Whether to start with only the chats from the current directory and below; ctrl-t switches.
  pub(crate) cwd_only: bool,

  /// Whether to start with the preview pane shown; shift-tab switches.
  #[default = true]
  pub(crate) show_preview: bool,

  /// Terminal width in columns below which the preview pane is hidden.
  #[default = 110]
  pub(crate) preview_min_width: u16,

  /// How many lines of each message the preview shows before cutting it.
  #[default = 14]
  pub(crate) preview_message_lines: usize,

  /// Which message the preview starts with: "oldest-first" reads down as the chat went, "newest-first" puts the latest message on top.
  #[default(MessageOrder::OldestFirst)]
  pub(crate) preview_order: MessageOrder,

  /// Where a long message from Claude is cut in the preview: "end" keeps its first lines, "middle" keeps its first and last.
  #[default(MessageCut::End)]
  pub(crate) preview_claude_cut: MessageCut,

  /// How often to look for new and changed chats, in seconds.
  #[default = 5]
  pub(crate) rescan_interval: u64,
}

impl Config {
  pub(crate) fn path() -> Result<Utf8PathBuf> {
    let dirs = etcetera::base_strategy::Xdg::new()?;
    let config_dir = Utf8PathBuf::try_from(dirs.config_dir())?;

    return Ok(config_dir.join("ccf").join("config.toml"));
  }

  /// The defaults as TOML, each key under its doc comment.
  fn default_toml() -> Result<String> {
    let toml = toml_edit::ser::to_string_pretty(&Self::default())?;
    let mut doc: DocumentMut = toml.parse()?;

    for (i, comment) in Self::FIELD_DOCS.iter().enumerate() {
      let prefix = if i == 0 { format!("# {comment}\n") } else { format!("\n# {comment}\n") };
      let name = Self::FIELD_NAMES[i];
      let mut key = doc.key_mut(name).with_context(|| format!("{name} is missing from the serialized config"))?;
      key.leaf_decor_mut().set_prefix(prefix);
    }

    return Ok(doc.to_string());
  }

  /// Writes the default config unless one exists.
  pub(crate) fn ensure_exists() -> Result<()> {
    let path = Self::path()?;
    if path.exists() {
      return Ok(());
    }

    if let Some(parent) = path.parent() {
      fs_err::create_dir_all(parent)?;
    }
    fs_err::write(&path, Self::default_toml()?)?;

    return Ok(());
  }

  /// The config file, with `CCF_CONFIG_<KEY>` environment variables over it.
  pub(crate) fn load() -> Result<Self> {
    let path = Self::path()?;
    let config: Self = Figment::new()
      .merge(Toml::file(&path))
      .merge(Env::prefixed("CCF_CONFIG_"))
      .extract()
      .with_context(|| format!("invalid config {path}"))?;

    ensure!(config.rescan_interval > 0, "invalid config {path}: rescan_interval must be at least 1");

    return Ok(config);
  }
}
