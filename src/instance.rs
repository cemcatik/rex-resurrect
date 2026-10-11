//! Server instances: each run of the Rex server, told apart by the
//! `instance_id` in `server.status`.
//!
//! rex-resurrect remembers the last instance it restored into, so it can tell
//! a freshly started server (an unrestored server instance) from one the user
//! has been working in.

use crate::error::Result;
use crate::state::{self, StateDir};

const LAST_RESTORED: &str = "last-restored-instance";

/// The server instance a restore last finished in, if any.
pub fn last_restored(state: &StateDir) -> Result<Option<String>> {
    let bytes = state::read_file(&state.join(LAST_RESTORED))?;
    Ok(bytes.map(|bytes| String::from_utf8_lossy(&bytes).trim().to_string()))
}

/// Records that a restore into `instance_id` finished.
pub fn record_restored(state: &StateDir, instance_id: &str) -> Result<()> {
    state::write_file(
        &state.join(LAST_RESTORED),
        format!("{instance_id}\n").as_bytes(),
    )
}
