//! `rex-resurrect restore`: recreate the latest snapshot's sessions.

use std::collections::HashMap;
use std::io::Write;
use std::process::ExitCode;

use crate::create;
use crate::error::{Context, Result};
use crate::instance::RestoreLock;
use crate::lineage::{Lineage, LineageMap};
use crate::rex::Rex;
use crate::state::StateDir;
use crate::store::SnapshotStore;

#[derive(Debug, Default)]
pub struct RestoreOptions {}

/// What happened to one saved session.
#[derive(Debug)]
pub enum Outcome {
    Created {
        label: String,
        session_id: String,
    },
    /// A session of its lineage is already live.
    Skipped {
        label: String,
        live_session_id: String,
    },
    Failed {
        label: String,
        error: String,
    },
}

#[derive(Debug, Default)]
pub struct RestoreReport {
    /// False when there was no snapshot to restore from.
    pub had_snapshot: bool,
    pub outcomes: Vec<Outcome>,
}

impl RestoreReport {
    pub fn failed(&self) -> usize {
        self.count(|o| matches!(o, Outcome::Failed { .. }))
    }

    fn count(&self, pred: impl Fn(&Outcome) -> bool) -> usize {
        self.outcomes.iter().filter(|o| pred(o)).count()
    }

    pub fn print(&self, out: &mut impl Write) -> std::io::Result<()> {
        if !self.had_snapshot {
            return writeln!(out, "nothing to restore: no snapshot yet");
        }
        for outcome in &self.outcomes {
            match outcome {
                Outcome::Created { label, session_id } => {
                    writeln!(out, "created  {label}  {session_id}")?
                }
                Outcome::Skipped {
                    label,
                    live_session_id,
                } => writeln!(out, "skipped  {label}  (live as {live_session_id})")?,
                Outcome::Failed { label, error } => writeln!(out, "failed   {label}: {error}")?,
            }
        }
        writeln!(
            out,
            "restore: {} created, {} skipped, {} failed",
            self.count(|o| matches!(o, Outcome::Created { .. })),
            self.count(|o| matches!(o, Outcome::Skipped { .. })),
            self.failed(),
        )
    }
}

pub fn run(options: &RestoreOptions) -> Result<ExitCode> {
    let state = StateDir::from_env()?;
    let lock = RestoreLock::acquire(&state)?;
    let report = restore(&Rex::from_env(), &state, &lock, options)?;
    let _ = report.print(&mut std::io::stdout().lock());
    Ok(if report.failed() > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// Restores into the server's current instance, unless rex-resurrect already
/// has: `None` then. This is how `save` treats an unrestored server instance.
pub fn restore_unrestored_instance(
    rex: &Rex,
    state: &StateDir,
    lock: &RestoreLock,
) -> Result<Option<RestoreReport>> {
    let instance_id = rex.server_status()?.instance_id;
    if lock.last_restored()?.as_deref() == Some(instance_id.as_str()) {
        return Ok(None);
    }
    restore(rex, state, lock, &RestoreOptions::default()).map(Some)
}

/// Restores the latest snapshot and, once that has finished, records the
/// server instance it restored into. Sessions that failed to create don't
/// stop it from finishing.
pub fn restore(
    rex: &Rex,
    state: &StateDir,
    lock: &RestoreLock,
    options: &RestoreOptions,
) -> Result<RestoreReport> {
    let instance_id = rex.server_status()?.instance_id;
    let report = restore_sessions(rex, state, options)?;
    lock.record_restored(&instance_id)?;
    Ok(report)
}

/// Creates each saved session whose lineage has no live session (ADR-0001)
/// and records the new session's lineage. A session that fails to create
/// doesn't stop the others; errors that stop the whole restore (no server,
/// unreadable state) are returned.
fn restore_sessions(
    rex: &Rex,
    state: &StateDir,
    _options: &RestoreOptions,
) -> Result<RestoreReport> {
    let Some(snapshot) = SnapshotStore::new(state).read_latest()? else {
        return Ok(RestoreReport::default());
    };
    let live_sessions = rex.list_sessions()?;
    let mut lineages = LineageMap::load(state)?;
    let mut live: HashMap<Lineage, String> = live_sessions
        .iter()
        .map(|s| (lineages.lineage_of(&s.session_id), s.session_id.clone()))
        .collect();
    lineages.retain_live(|id| live_sessions.iter().any(|s| s.session_id == id));

    let mut report = RestoreReport {
        had_snapshot: true,
        outcomes: Vec::new(),
    };
    for saved in &snapshot.sessions {
        let label = saved.label.clone();
        if let Some(live_session_id) = live.get(&saved.lineage) {
            report.outcomes.push(Outcome::Skipped {
                label,
                live_session_id: live_session_id.clone(),
            });
            continue;
        }
        match rex.create_session(&create::create_request(saved)) {
            Ok(created) => {
                lineages.record(&created.session_id, saved.lineage.clone());
                lineages.save().with_context(|| {
                    format!(
                        "recording the lineage of restored session {}",
                        created.session_id
                    )
                })?;
                live.insert(saved.lineage.clone(), created.session_id.clone());
                report.outcomes.push(Outcome::Created {
                    label,
                    session_id: created.session_id,
                });
            }
            Err(e) => report.outcomes.push(Outcome::Failed {
                label,
                error: e.to_string(),
            }),
        }
    }
    Ok(report)
}
