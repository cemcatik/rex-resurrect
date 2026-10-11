//! Snapshots on disk, in the state directory:
//! - `snapshots/<UTC timestamp>.json`, one file per save;
//! - `latest.json`, a symlink to the latest snapshot.
//!
//! A snapshot's identifier (what `list` shows and `restore --snapshot`
//! takes) is its file name without `.json`. Identifiers sort in time order.

use std::path::{Path, PathBuf};

use crate::error::{Context, Result};
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

    /// Writes `snapshot` as a new timestamped file and points "latest" at it.
    pub fn write(&self, snapshot: &Snapshot, taken_at: Timestamp) -> Result<PathBuf> {
        let name = format!("{}.json", taken_at.compact());
        let path = self.snapshots.join(&name);
        let json = serde_json::to_vec_pretty(snapshot).expect("snapshots serialize");
        state::write_file(&path, &json)?;
        self.point_latest_at(&name)?;
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

    fn point_latest_at(&self, name: &str) -> Result<()> {
        let target = PathBuf::from(SNAPSHOTS_DIR).join(name);
        match std::fs::remove_file(&self.latest) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("removing {}", self.latest.display())),
        }
        std::os::unix::fs::symlink(&target, &self.latest)
            .with_context(|| format!("pointing {} at {}", self.latest.display(), target.display()))
    }
}

fn read_snapshot(path: &Path) -> Result<Option<Snapshot>> {
    let Some(bytes) = state::read_file(path)? else {
        return Ok(None);
    };
    let snapshot = serde_json::from_slice(&bytes)
        .with_context(|| format!("reading the snapshot {}", path.display()))?;
    Ok(Some(snapshot))
}
