//! `rex-resurrect save`: snapshot every session on the server.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::capture;
use crate::error::{Context, Result};
use crate::instance::RestoreLock;
use crate::lineage::LineageMap;
use crate::restore;
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

/// Restores first if rex-resurrect hasn't restored into this server instance
/// yet, so that a save on a freshly started server can't replace the latest
/// snapshot with a near-empty one. Still saves if some sessions failed to
/// restore, but then exits non-zero.
///
/// Holds the restore lock throughout: a save that finds a restore running
/// waits for it rather than saving a half-restored server.
pub fn run(options: &SaveOptions) -> Result<ExitCode> {
    let rex = Rex::from_env();
    let state = StateDir::from_env()?;
    let lock = RestoreLock::acquire(&state)?;
    let restored_first = restore::restore_unrestored_instance(&rex, &state, &lock)?;
    if let Some(report) = &restored_first {
        println!("restoring first: this server instance hasn't been restored yet");
        let _ = report.print(&mut std::io::stdout().lock());
    }
    let saved = save(&rex, &state, options)?;
    println!(
        "saved {} session(s) to {}",
        saved.sessions,
        saved.path.display()
    );
    Ok(match restored_first {
        Some(report) if report.failed() > 0 => ExitCode::FAILURE,
        _ => ExitCode::SUCCESS,
    })
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
