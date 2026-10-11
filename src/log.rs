//! rex-resurrect's log: `rex-resurrect.log` in the state directory.
//!
//! `save` and `restore` append their failures here, one timestamped line
//! each, so a failed autosave (or auto-restore), which nobody watches, can
//! still be diagnosed. Successful runs write nothing.

use std::fmt::Display;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use crate::error::{Context, Result};
use crate::state::StateDir;
use crate::timestamp::Timestamp;

const LOG: &str = "rex-resurrect.log";

#[derive(Debug)]
pub struct Log {
    state: StateDir,
    path: PathBuf,
}

impl Log {
    pub fn new(state: &StateDir) -> Log {
        Log {
            state: state.clone(),
            path: state.join(LOG),
        }
    }

    /// Appends `<UTC timestamp> <message>` as one line: line breaks in
    /// `message` become `; `. Creates the log (`0600`) and the state
    /// directory (private) if needed.
    pub fn append(&self, message: &str) -> Result<()> {
        self.state.make_private()?;
        let message: Vec<&str> = message
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        let entry = format!("{} {}\n", Timestamp::now().rfc3339(), message.join("; "));
        OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .open(&self.path)
            .and_then(|mut log| log.write_all(entry.as_bytes()))
            .with_context(|| format!("appending to {}", self.path.display()))
    }
}

/// Appends "`<command>` failed: `<failure>`" to the log in `state`. Best
/// effort: if the log can't be written, says so on stderr and carries on,
/// so a broken log never hides the failure itself.
pub fn log_failure(state: &StateDir, command: &str, failure: &dyn Display) {
    if let Err(e) = Log::new(state).append(&format!("{command} failed: {failure}")) {
        eprintln!("rex-resurrect: couldn't log a failure: {e}");
    }
}
