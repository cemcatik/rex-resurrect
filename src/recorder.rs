//! Recorder files: each block's last command, as recorded by the shell
//! recorder (`rex-resurrect.plugin.zsh` at the repo root) and seeded by
//! restore.
//!
//! The format is shared by every recorder, whatever its shell:
//! - one file per block, `recorder/<block ID>` in the state directory, named
//!   with the block ID exactly as Rex gives it (`block:<uuid>`, the value of
//!   `REX_BLOCK` in the block's shell);
//! - its contents: the block's last command, UTF-8 text on one line, ending
//!   in a newline (accepted without one);
//! - written to a temporary file whose name starts with `.` in the same
//!   directory, then renamed over the record;
//! - the directory is private (0700): commands can hold secrets.
//!
//! Recorders only record single-line commands, but restore doesn't rely on
//! that (see `create::typed_command`).

use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::error::{Context, Error, Result};
use crate::state::{self, StateDir};

const DIR: &str = "recorder";

pub struct RecorderFiles {
    dir: PathBuf,
}

impl RecorderFiles {
    pub fn new(state: &StateDir) -> RecorderFiles {
        RecorderFiles {
            dir: state.join(DIR),
        }
    }

    /// The block's last command, or `None` if nothing was ever recorded for it.
    pub fn last_command(&self, block_id: &str) -> Result<Option<String>> {
        let Some(path) = self.file(block_id) else {
            return Ok(None);
        };
        let Some(bytes) = state::read_file(&path)? else {
            return Ok(None);
        };
        let Ok(text) = String::from_utf8(bytes) else {
            return Ok(None);
        };
        let command = text.strip_suffix('\n').unwrap_or(&text);
        Ok((!command.is_empty()).then(|| command.to_string()))
    }

    /// Records `command` as the block's last command, as its recorder would.
    pub fn record(&self, block_id: &str, command: &str) -> Result<()> {
        let path = self
            .file(block_id)
            .ok_or_else(|| Error::new(format!("block ID {block_id:?} can't name a file")))?;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.dir)
            .with_context(|| format!("creating {}", self.dir.display()))?;
        let tmp = self.dir.join(format!(".{block_id}.{}", std::process::id()));
        state::write_file(&tmp, format!("{command}\n").as_bytes())?;
        std::fs::rename(&tmp, &path).with_context(|| format!("writing {}", path.display()))
    }

    /// Removes the records of blocks `is_live` says aren't live, unless they
    /// changed at or after `kept_since`. Leaves alone temporary files (named
    /// `.…`), which a recorder may be about to rename over a record, and
    /// anything that isn't a file.
    pub fn retain_live(
        &self,
        is_live: impl Fn(&str) -> bool,
        kept_since: SystemTime,
    ) -> Result<()> {
        let listing = || format!("listing {}", self.dir.display());
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e).with_context(listing),
        };
        for entry in entries {
            let entry = entry.with_context(listing)?;
            let name = entry.file_name();
            let Some(block_id) = name.to_str() else {
                continue;
            };
            let Some(path) = self.file(block_id) else {
                continue;
            };
            if is_live(block_id) {
                continue;
            }
            let metadata = entry.metadata().with_context(listing)?;
            if !metadata.is_file() || metadata.modified().with_context(listing)? >= kept_since {
                continue;
            }
            match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(e).with_context(|| format!("removing {}", path.display()));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// The block's recorder file. `None` for an ID that isn't a plain file
    /// name, so no ID can reach outside the recorder directory.
    fn file(&self, block_id: &str) -> Option<PathBuf> {
        let plain =
            !block_id.is_empty() && !block_id.starts_with('.') && !block_id.contains(['/', '\0']);
        plain.then(|| self.dir.join(block_id))
    }
}
