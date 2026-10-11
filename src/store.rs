//! Snapshots on disk, in the state directory:
//! - `snapshots/<UTC timestamp>.json`, one file per save;
//! - `latest.json`, a symlink to the latest snapshot.

use std::ffi::OsStr;
use std::path::PathBuf;

use crate::error::{Context, Result};
use crate::retention::Retention;
use crate::snapshot::Snapshot;
use crate::state::{self, StateDir};
use crate::timestamp::Timestamp;

const SNAPSHOTS_DIR: &str = "snapshots";
const LATEST: &str = "latest.json";

pub struct SnapshotStore {
    snapshots: PathBuf,
    latest: PathBuf,
}

impl SnapshotStore {
    pub fn new(state: &StateDir) -> SnapshotStore {
        SnapshotStore {
            snapshots: state.join(SNAPSHOTS_DIR),
            latest: state.join(LATEST),
        }
    }

    /// Writes `snapshot` as a new timestamped file, points "latest" at it,
    /// then removes the snapshots `retention` doesn't keep. "latest" always
    /// points at a kept one.
    pub fn write(
        &self,
        snapshot: &Snapshot,
        taken_at: Timestamp,
        retention: Retention,
    ) -> Result<PathBuf> {
        let name = format!("{}.json", taken_at.compact());
        let path = self.snapshots.join(&name);
        let json = serde_json::to_vec_pretty(snapshot).expect("snapshots serialize");
        state::write_file(&path, &json)?;
        self.point_latest_at(&name)?;
        retention
            .prune(&self.snapshots, OsStr::new(&name), is_snapshot_file)
            .with_context(|| "removing old snapshots")?;
        Ok(path)
    }

    /// The latest snapshot, or `None` before the first save.
    pub fn read_latest(&self) -> Result<Option<Snapshot>> {
        let Some(bytes) = state::read_file(&self.latest)? else {
            return Ok(None);
        };
        let snapshot = serde_json::from_slice(&bytes)
            .with_context(|| format!("reading the snapshot {}", self.latest.display()))?;
        Ok(Some(snapshot))
    }

    fn point_latest_at(&self, name: &str) -> Result<()> {
        state::write_symlink(&self.latest, &PathBuf::from(SNAPSHOTS_DIR).join(name))
    }
}

/// A snapshot's file name: `<timestamp>.json`. Not a hidden file, such as
/// [`state::write_file`]'s temp files.
fn is_snapshot_file(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    name.ends_with(".json") && !name.starts_with('.')
}
