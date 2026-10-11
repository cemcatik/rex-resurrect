//! `rex-resurrect restore`: recreate the latest snapshot's sessions.

use std::io::Write;
use std::process::ExitCode;

use crate::create;
use crate::error::Result;
use crate::rex::Rex;
use crate::state::StateDir;
use crate::store::SnapshotStore;

#[derive(Debug, Default)]
pub struct RestoreOptions {}

/// What happened to one saved session.
#[derive(Debug)]
pub enum Outcome {
    Created { label: String, session_id: String },
    Failed { label: String, error: String },
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
                Outcome::Failed { label, error } => writeln!(out, "failed   {label}: {error}")?,
            }
        }
        writeln!(
            out,
            "restore: {} created, {} failed",
            self.count(|o| matches!(o, Outcome::Created { .. })),
            self.failed(),
        )
    }
}

pub fn run(options: &RestoreOptions) -> Result<ExitCode> {
    let report = restore(&Rex::from_env(), &StateDir::from_env()?, options)?;
    let _ = report.print(&mut std::io::stdout().lock());
    Ok(if report.failed() > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

pub fn restore(rex: &Rex, state: &StateDir, _options: &RestoreOptions) -> Result<RestoreReport> {
    let Some(snapshot) = SnapshotStore::new(state).read_latest()? else {
        return Ok(RestoreReport::default());
    };
    let mut report = RestoreReport {
        had_snapshot: true,
        outcomes: Vec::new(),
    };
    for saved in &snapshot.sessions {
        let label = saved.label.clone();
        report.outcomes.push(match rex.create_session(&create::create_request(saved)) {
            Ok(created) => Outcome::Created {
                label,
                session_id: created.session_id,
            },
            Err(e) => Outcome::Failed {
                label,
                error: e.to_string(),
            },
        });
    }
    Ok(report)
}
