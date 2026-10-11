//! `rex-resurrect save`: snapshot every session on the server.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::capture;
use crate::error::{Context, Result};
use crate::lineage::LineageMap;
use crate::rex::Rex;
use crate::snapshot::{SavedSession, Snapshot};
use crate::state::StateDir;
use crate::store::SnapshotStore;
use crate::timestamp::Timestamp;

#[derive(Debug, Default)]
pub struct SaveOptions {}

/// What a save did.
pub enum Saved {
    /// A new snapshot, now the latest.
    Snapshot { path: PathBuf, sessions: usize },
    /// The server is empty: nothing was written and the latest snapshot is
    /// unchanged.
    EmptyServer,
}

pub fn run(options: &SaveOptions) -> Result<ExitCode> {
    match save(&Rex::from_env(), &StateDir::from_env()?, options)? {
        Saved::Snapshot { path, sessions } => {
            println!("saved {sessions} session(s) to {}", path.display());
        }
        Saved::EmptyServer => {
            println!("nothing to save: no sessions worth saving; the latest snapshot is unchanged");
        }
    }
    Ok(ExitCode::SUCCESS)
}

pub fn save(rex: &Rex, state: &StateDir, _options: &SaveOptions) -> Result<Saved> {
    let lineages = LineageMap::load(state)?;
    let mut sessions = Vec::new();
    for summary in rex.list_sessions()? {
        let view = rex.view_session(&summary.session_id)?;
        let lineage = lineages.lineage_of(&view.session_id);
        let saved = capture::saved_session(rex, &view, lineage)
            .with_context(|| format!("saving session {:?} ({})", view.label, view.session_id))?;
        if worth_saving(&saved) {
            sessions.push(saved);
        }
    }
    if sessions.is_empty() {
        return Ok(Saved::EmptyServer);
    }
    let taken_at = Timestamp::now();
    let snapshot = Snapshot::new(taken_at, sessions);
    let path = SnapshotStore::new(state).write(&snapshot, taken_at)?;
    Ok(Saved::Snapshot {
        path,
        sessions: snapshot.sessions.len(),
    })
}

/// Whether a live session has anything to restore. A server where no
/// session does is an empty server, and saving from it writes nothing.
fn worth_saving(_session: &SavedSession) -> bool {
    true
}
