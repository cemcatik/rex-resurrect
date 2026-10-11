//! The snapshot model: what `save` writes and `restore` reads.
//!
//! Only rex-resurrect reads this format (tests never look inside), so it can
//! change freely. Optional fields are skipped when empty so that fields added
//! later don't break older snapshots.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::lineage::Lineage;
use crate::rex::api::Direction;
use crate::timestamp::Timestamp;

/// Bumped on incompatible changes to this format.
pub const FORMAT: u32 = 1;

/// Every saved session at one point in time.
#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub format: u32,
    /// RFC 3339, UTC.
    pub taken_at: String,
    pub sessions: Vec<SavedSession>,
}

impl Snapshot {
    pub fn new(taken_at: Timestamp, sessions: Vec<SavedSession>) -> Snapshot {
        Snapshot {
            format: FORMAT,
            taken_at: taken_at.rfc3339(),
            sessions,
        }
    }
}

/// A session as recorded in a snapshot.
#[derive(Debug, Serialize, Deserialize)]
pub struct SavedSession {
    /// Which lineage this session belongs to (ADR-0001). Restore skips it
    /// while a session of the same lineage is live.
    pub lineage: Lineage,
    /// Always saved: Rex's generated session labels can't be told apart
    /// from user-set ones.
    pub label: String,
    /// In window (tab) order.
    pub windows: Vec<SavedWindow>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedWindow {
    /// Only a user-set label; omitted so Rex generates a fresh default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The window's tiled layout.
    pub layout: SavedLayout,
}

/// A layout node: exactly one of a block or a split, by construction.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SavedLayout {
    Block(SavedBlock),
    Split(SavedSplit),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedSplit {
    pub direction: Direction,
    /// `before`'s share, in (0, 1).
    pub ratio: f64,
    pub before: Box<SavedLayout>,
    pub after: Box<SavedLayout>,
}

/// A shell block.
#[derive(Debug, Serialize, Deserialize)]
pub struct SavedBlock {
    pub working_directory: PathBuf,
}
