//! `rex-resurrect save`: snapshot every session on the server.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::capture;
use crate::error::{Context, Result};
use crate::lineage::LineageMap;
use crate::rex::Rex;
use crate::snapshot::Snapshot;
use crate::state::StateDir;
use crate::store::SnapshotStore;
use crate::timestamp::Timestamp;

#[derive(Debug, Default)]
pub struct SaveOptions {}

/// What a save wrote.
pub struct Saved {
    pub path: PathBuf,
    pub sessions: usize,
}

pub fn run(options: &SaveOptions) -> Result<ExitCode> {
    let saved = save(&Rex::from_env(), &StateDir::from_env()?, options)?;
    println!(
        "saved {} session(s) to {}",
        saved.sessions,
        saved.path.display()
    );
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
        sessions.push(saved);
    }
    let taken_at = Timestamp::now();
    let snapshot = Snapshot::new(taken_at, sessions);
    let path = SnapshotStore::new(state).write(&snapshot, taken_at)?;
    Ok(Saved {
        path,
        sessions: snapshot.sessions.len(),
    })
}
