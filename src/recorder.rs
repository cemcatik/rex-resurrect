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

use std::path::PathBuf;

use crate::error::{Error, Result};
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
        // Through a hidden temp file and a rename, into a private directory,
        // as the format above asks.
        state::write_file(&path, format!("{command}\n").as_bytes())
    }

    /// The block's recorder file. `None` for an ID that isn't a plain file
    /// name, so no ID can reach outside the recorder directory.
    fn file(&self, block_id: &str) -> Option<PathBuf> {
        let plain =
            !block_id.is_empty() && !block_id.starts_with('.') && !block_id.contains(['/', '\0']);
        plain.then(|| self.dir.join(block_id))
    }
}
