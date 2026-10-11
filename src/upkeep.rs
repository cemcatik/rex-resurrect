//! Upkeep after a save: forgetting blocks and sessions that aren't live.
//!
//! Recorder files are keyed by block ID and lineage entries by session ID,
//! and Rex never reuses either, so once a block or session is gone its file
//! or entry can't matter again. Save removes them only once it has written a
//! snapshot. A save that fails, or finds an empty server and writes nothing,
//! removes nothing: what it read can't be trusted to say what's gone.

use std::collections::HashSet;
use std::time::SystemTime;

use crate::error::Result;
use crate::lineage::LineageMap;
use crate::recorder::RecorderFiles;
use crate::rex::api::SessionView;
use crate::state::StateDir;

/// The sessions and blocks live on the server, as save read it.
#[derive(Debug)]
pub struct Live {
    /// When save started reading the server. A block that appeared after
    /// that is live but missing here.
    read_at: SystemTime,
    sessions: HashSet<String>,
    blocks: HashSet<String>,
}

impl Live {
    /// Call just before reading the server's sessions.
    pub fn new() -> Live {
        Live {
            read_at: SystemTime::now(),
            sessions: HashSet::new(),
            blocks: HashSet::new(),
        }
    }

    /// Adds a live session and every block in it, tiled or floating.
    pub fn add(&mut self, view: &SessionView) {
        self.sessions.insert(view.session_id.clone());
        let blocks = view
            .windows
            .iter()
            .flat_map(|w| &w.layers)
            .flat_map(|l| &l.blocks);
        self.blocks.extend(blocks.map(|b| b.block_id.clone()));
    }
}

/// Removes the recorder files of blocks, and the lineage entries of
/// sessions, that aren't `live`. A recorder file changed since save started
/// reading the server stays: it may belong to a block that appeared since.
/// Lineage entries need no such care: only restore adds them, and it can't
/// run during a save.
pub fn forget_what_is_not_live(state: &StateDir, live: &Live) -> Result<()> {
    RecorderFiles::new(state)
        .retain_live(|block_id| live.blocks.contains(block_id), live.read_at)?;
    let mut lineages = LineageMap::load(state)?;
    if lineages.retain_live(|session_id| live.sessions.contains(session_id)) {
        lineages.save()?;
    }
    Ok(())
}
