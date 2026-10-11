//! Upkeep after a save: forgetting blocks and sessions that aren't live.
//!
//! Recorder files are keyed by block ID and lineage entries by session ID,
//! and Rex never reuses either, so once a block or session is gone its file
//! or entry can't matter again. Save removes them only after it has written
//! a snapshot, which holds every live block's last command and every live
//! session's lineage. A save that fails, or writes nothing, removes nothing.

use std::collections::HashSet;

use crate::error::Result;
use crate::recorder::RecorderFiles;
use crate::rex::api::SessionView;
use crate::state::StateDir;

/// The blocks live on the server, as save read it.
#[derive(Debug, Default)]
pub struct Live {
    blocks: HashSet<String>,
}

impl Live {
    /// Adds every block of a live session, tiled or floating.
    pub fn add(&mut self, view: &SessionView) {
        let blocks = view.windows.iter().flat_map(|w| &w.layers).flat_map(|l| &l.blocks);
        self.blocks.extend(blocks.map(|b| b.block_id.clone()));
    }
}

/// Removes the recorder files of blocks that aren't `live`.
pub fn forget_what_is_not_live(state: &StateDir, live: &Live) -> Result<()> {
    RecorderFiles::new(state).retain_live(|block_id| live.blocks.contains(block_id))
}
