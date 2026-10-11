//! `rex-resurrect restore`: recreate the latest snapshot's sessions.

use std::collections::HashMap;
use std::io::Write;
use std::process::ExitCode;

use crate::create;
use crate::error::{Context, Error, Result};
use crate::lineage::{Lineage, LineageMap};
use crate::recorder::RecorderFiles;
use crate::rex::Rex;
use crate::rex::api::CreatedSession;
use crate::snapshot::{SavedBlock, SavedLayout, SavedSession};
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
    let report = restore(&Rex::from_env(), &StateDir::from_env()?, options)?;
    let _ = report.print(&mut std::io::stdout().lock());
    Ok(if report.failed() > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// Creates each saved session whose lineage has no live session (ADR-0001)
/// and records the new session's lineage. A session that fails to create
/// doesn't stop the others; errors that stop the whole restore (no server,
/// unreadable state) are returned.
pub fn restore(rex: &Rex, state: &StateDir, _options: &RestoreOptions) -> Result<RestoreReport> {
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
    let recorder = RecorderFiles::new(state);

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
                carry_forward(&recorder, saved, &created).with_context(|| {
                    format!(
                        "carrying forward the last commands of restored session {}",
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

/// Seeds each new block's recorder file with the command restore typed at its
/// prompt. Typed input never reaches the recorder (nothing ran), so without
/// this the next save would find no last command. With it, the command
/// carries forward through any number of restores until the block's shell
/// records a new one.
///
/// `session.create` returns each window's block IDs in layout order
/// (depth-first, `before` then `after`), the order `layout_blocks` walks the
/// saved layout in.
fn carry_forward(
    recorder: &RecorderFiles,
    saved: &SavedSession,
    created: &CreatedSession,
) -> Result<()> {
    if created.initial_windows.len() != saved.windows.len() {
        return Err(Error::new(format!(
            "session.create returned {} windows for {} saved ones",
            created.initial_windows.len(),
            saved.windows.len()
        )));
    }
    for (i, (window, created_window)) in saved
        .windows
        .iter()
        .zip(&created.initial_windows)
        .enumerate()
    {
        let blocks = layout_blocks(&window.layout);
        if blocks.len() != created_window.block_ids.len() {
            return Err(Error::new(format!(
                "session.create returned {} block IDs for window {i}, which has {} saved blocks",
                created_window.block_ids.len(),
                blocks.len()
            )));
        }
        for (block, block_id) in blocks.into_iter().zip(&created_window.block_ids) {
            if let Some(command) = create::typed_command(block) {
                recorder.record(block_id, command)?;
            }
        }
    }
    Ok(())
}

/// A layout's blocks in depth-first, `before`-then-`after` order.
fn layout_blocks(layout: &SavedLayout) -> Vec<&SavedBlock> {
    match layout {
        SavedLayout::Block(block) => vec![block],
        SavedLayout::Split(split) => {
            let mut blocks = layout_blocks(&split.before);
            blocks.extend(layout_blocks(&split.after));
            blocks
        }
    }
}
