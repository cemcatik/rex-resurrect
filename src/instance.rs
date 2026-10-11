//! Server instances and the restore lock.
//!
//! A server instance is one run of the Rex server, told apart by the
//! `instance_id` in `server.status`. rex-resurrect remembers the last instance
//! it restored into, so it can tell a freshly started server (an unrestored
//! server instance) from one the user has been working in.
//!
//! The restore lock lets one restore run at a time. Without it, two restores
//! (auto-restore and the restore a save starts first, say) could both create
//! the same session before either records its lineage (ADR-0001). A save
//! takes it too, so it never captures a half-restored server. The record of
//! the last restored instance is only read and written through the lock, so
//! "is this instance restored? if not, restore it and record it" can't
//! interleave with another process doing the same.

use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use crate::error::{Context, Result};
use crate::state::{self, StateDir};

const LOCK: &str = "restore.lock";
const LAST_RESTORED: &str = "last-restored-instance";

/// The restore lock, held until dropped (or until the process exits, however
/// it exits: it's an advisory `flock` on a file in the state directory).
#[derive(Debug)]
pub struct RestoreLock {
    _file: File,
    last_restored: PathBuf,
}

impl RestoreLock {
    /// Takes the restore lock, waiting for as long as another rex-resurrect
    /// process holds it. Save and restore both start here, so this is also
    /// where the state directory is created, or made, private.
    pub fn acquire(state: &StateDir) -> Result<RestoreLock> {
        state.make_private()?;
        let path = state.join(LOCK);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(&path)
            .with_context(|| format!("opening the restore lock {}", path.display()))?;
        file.lock()
            .with_context(|| format!("taking the restore lock {}", path.display()))?;
        Ok(RestoreLock {
            _file: file,
            last_restored: state.join(LAST_RESTORED),
        })
    }

    /// The server instance a restore last finished in, if any.
    pub fn last_restored(&self) -> Result<Option<String>> {
        let bytes = state::read_file(&self.last_restored)?;
        Ok(bytes.map(|bytes| String::from_utf8_lossy(&bytes).trim().to_string()))
    }

    /// Records that a restore into `instance_id` finished.
    pub fn record_restored(&self, instance_id: &str) -> Result<()> {
        state::write_file(&self.last_restored, format!("{instance_id}\n").as_bytes())
    }
}
