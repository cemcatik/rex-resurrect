//! Snapshots on disk, in the state directory:
//! - `snapshots/<UTC timestamp>.json`, one file per save;
//! - `latest.json`, a symlink to the latest snapshot.
//!
//! A snapshot's identifier (what `list` shows and `restore --snapshot`
//! takes) is its file name without `.json`. Identifiers sort in time order.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

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
        read_snapshot(&self.latest)
    }

    /// Every snapshot's identifier, newest first.
    pub fn list(&self) -> Result<Vec<String>> {
        let entries = match std::fs::read_dir(&self.snapshots) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(e).with_context(|| format!("listing {}", self.snapshots.display()));
            }
        };
        let mut ids = Vec::new();
        for entry in entries {
            let entry = entry.with_context(|| format!("listing {}", self.snapshots.display()))?;
            let name = entry.file_name();
            let id = name.to_str().and_then(|name| name.strip_suffix(".json"));
            if let Some(id) = id.filter(|id| !id.is_empty() && !id.starts_with('.')) {
                ids.push(id.to_string());
            }
        }
        ids.sort_unstable_by(|a, b| b.cmp(a));
        Ok(ids)
    }

    /// The snapshot `list` shows as `id`, or `None` if there's no such snapshot.
    pub fn read(&self, id: &str) -> Result<Option<Snapshot>> {
        if id.is_empty() || id.starts_with('.') || id.contains('/') {
            return Ok(None);
        }
        read_snapshot(&self.snapshots.join(format!("{id}.json")))
    }

    /// Removes the temporary files that crashed snapshot writes left behind
    /// (see [`state::remove_stale_temp_files`]).
    pub fn remove_stale_temp_files(&self) -> Result<()> {
        state::remove_stale_temp_files(&self.snapshots, state::is_temp_name)
    }

    fn point_latest_at(&self, name: &str) -> Result<()> {
        state::write_symlink(&self.latest, &PathBuf::from(SNAPSHOTS_DIR).join(name))
    }
}

/// A snapshot's file name, as `list` picks them: `<timestamp>.json`, not a
/// hidden file such as [`state::write_file`]'s temp files.
fn is_snapshot_file(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    name.ends_with(".json") && !name.starts_with('.')
}

fn read_snapshot(path: &Path) -> Result<Option<Snapshot>> {
    let Some(bytes) = state::read_file(path)? else {
        return Ok(None);
    };
    let snapshot = serde_json::from_slice(&bytes)
        .with_context(|| format!("reading the snapshot {}", path.display()))?;
    Ok(Some(snapshot))
}
