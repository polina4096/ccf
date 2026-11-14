//! The lowercased text of a conversation, one file per chat beside the index,
//! memory-mapped for search so the process owns none of it.

use std::sync::Arc;

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};

/// A read-only mapping of a chat's text; `None` for an empty file, which cannot be mapped.
#[derive(Clone, Default, Debug)]
pub(crate) struct Content(Option<Arc<memmap2::Mmap>>);

impl Content {
  pub(crate) fn open(path: &Utf8Path) -> Result<Self> {
    let file = fs_err::File::open(path.as_std_path())?;
    if file.metadata()?.len() == 0 {
      return Ok(Self(None));
    }

    // SAFETY: content files are replaced by rename and never rewritten in
    // place, so a mapping only ever sees the bytes present when it was made.
    let map = unsafe { memmap2::Mmap::map(&file) }.with_context(|| format!("mapping {path}"))?;

    return Ok(Self(Some(Arc::new(map))));
  }

  pub(crate) fn as_bytes(&self) -> &[u8] {
    return self.0.as_deref().map_or(&[], |map| &map[..]);
  }
}

pub(crate) fn path(dir: &Utf8Path, id: &str) -> Utf8PathBuf {
  return dir.join(format!("{id}.txt"));
}

/// Writes the text for `id` through a sibling temp file and a rename, so an
/// existing mapping of the old text stays valid.
pub(crate) fn store(dir: &Utf8Path, id: &str, text: &str) -> Result<()> {
  let temp = dir.join(format!("{id}.tmp"));
  fs_err::write(temp.as_std_path(), text)?;
  fs_err::rename(temp.as_std_path(), path(dir, id).as_std_path())?;

  return Ok(());
}
