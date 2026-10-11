//! `rex-resurrect save`: snapshot every session on the server.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::capture;
use crate::error::{Context, Result};
use crate::instance::RestoreLock;
use crate::lineage::LineageMap;
use crate::restore;
use crate::retention::Retention;
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

/// Restores first if rex-resurrect hasn't restored into this server instance
/// yet, so that a save on a freshly started server can't replace the latest
/// snapshot with a near-empty one. Still saves if some sessions failed to
/// restore, but then exits non-zero.
///
/// Holds the restore lock throughout: a save that finds a restore running
/// waits for it rather than saving a half-restored server.
pub fn run(options: &SaveOptions) -> Result<ExitCode> {
    // Configuration first: a bad setting fails before anything is restored.
    let retention = Retention::from_env()?;
    let rex = Rex::from_env();
    let state = StateDir::from_env()?;
    let lock = RestoreLock::acquire(&state)?;
    let restored_first = restore::restore_unrestored_instance(&rex, &state, &lock)?;
    if let Some(report) = &restored_first {
        println!("restored first: this server instance hadn't been restored into yet");
        let _ = report.print(&mut std::io::stdout().lock());
    }
    match save(&rex, &state, retention, options)? {
        Saved::Snapshot { path, sessions } => {
            println!("saved {sessions} session(s) to {}", path.display());
        }
        Saved::EmptyServer => {
            println!("nothing to save: no sessions worth saving; the latest snapshot is unchanged");
        }
    }
    Ok(match restored_first {
        Some(report) if report.failed() > 0 => ExitCode::FAILURE,
        _ => ExitCode::SUCCESS,
    })
}

pub fn save(
    rex: &Rex,
    state: &StateDir,
    retention: Retention,
    _options: &SaveOptions,
) -> Result<Saved> {
    let lineages = LineageMap::load(state)?;
    let local = capture::Local::read(rex, state)?;
    let mut sessions = Vec::new();
    for summary in rex.list_sessions()? {
        let view = rex.view_session(&summary.session_id)?;
        let lineage = lineages.lineage_of(&view.session_id);
        let saved = capture::saved_session(rex, &local, &view, lineage)
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
    snapshot
        .validate()
        .with_context(|| "the snapshot failed validation, so nothing was written")?;
    let path = SnapshotStore::new(state).write(&snapshot, taken_at, retention)?;
    Ok(Saved::Snapshot {
        path,
        sessions: snapshot.sessions.len(),
    })
}

/// Whether a live session has anything to restore. A server where no
/// session does is an empty server, and saving from it writes nothing.
///
/// A session that saved no windows has nothing to restore. Rex keeps a
/// session live, with no windows, after its last shell exits.
fn worth_saving(session: &SavedSession) -> bool {
    !session.windows.is_empty()
}
