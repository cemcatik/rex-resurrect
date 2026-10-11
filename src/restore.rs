//! `rex-resurrect restore`: recreate a snapshot's sessions, the latest by default.

use std::collections::HashMap;
use std::io::Write;
use std::process::ExitCode;

use crate::create;
use crate::error::{Context, Error, Result};
use crate::instance::RestoreLock;
use crate::lineage::{Lineage, LineageMap};
use crate::recorder::RecorderFiles;
use crate::rex::Rex;
use crate::rex::api::CreatedSession;
use crate::scrollback::Replay;
use crate::snapshot::{SavedBlock, SavedLayout, SavedSession, Snapshot};
use crate::state::StateDir;
use crate::store::SnapshotStore;
use crate::workdir;

#[derive(Debug, Default)]
pub struct RestoreOptions {
    /// Restore from this snapshot (an identifier `list` shows) instead of the latest.
    pub snapshot: Option<String>,
    /// Restore only the saved sessions with this label (every one of them).
    pub session: Option<String>,
    /// Print what would be created and skipped, and create nothing.
    pub dry_run: bool,
}

impl RestoreOptions {
    /// Whether this restore counts as restoring the server instance. A dry
    /// run creates nothing and a single-session restore leaves the other
    /// sessions out, so a save after either still restores first. Restoring
    /// a whole older snapshot counts.
    fn restores_whole_snapshot(&self) -> bool {
        !self.dry_run && self.session.is_none()
    }
}

/// What happened to one saved session.
#[derive(Debug)]
pub enum Outcome {
    Created {
        label: String,
        session_id: String,
    },
    /// A dry run would create it.
    WouldCreate {
        label: String,
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
    pub dry_run: bool,
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
                Outcome::WouldCreate { label } => writeln!(out, "would create  {label}")?,
                Outcome::Skipped {
                    label,
                    live_session_id,
                } => writeln!(out, "skipped  {label}  (live as {live_session_id})")?,
                Outcome::Failed { label, error } => writeln!(out, "failed   {label}: {error}")?,
            }
        }
        if self.dry_run {
            return writeln!(
                out,
                "dry run: {} would be created, {} skipped; nothing was changed",
                self.count(|o| matches!(o, Outcome::WouldCreate { .. })),
                self.count(|o| matches!(o, Outcome::Skipped { .. })),
            );
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
/// has: `None` then. `save` runs this first; auto-restore, once the server is
/// ready, needs nothing else.
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

/// Restores a snapshot and, once that has finished, records the server
/// instance it restored into, unless `options` restore less than a whole
/// snapshot. Sessions that failed to create don't stop it from finishing.
pub fn restore(
    rex: &Rex,
    state: &StateDir,
    lock: &RestoreLock,
    options: &RestoreOptions,
) -> Result<RestoreReport> {
    let instance_id = rex.server_status()?.instance_id;
    let report = restore_sessions(rex, state, options)?;
    if options.restores_whole_snapshot() {
        lock.record_restored(&instance_id)?;
    }
    Ok(report)
}

/// Creates each saved session whose lineage has no live session (ADR-0001)
/// and records the new session's lineage; a dry run only reports what it
/// would create. A session that fails to create doesn't stop the others;
/// errors that stop the whole restore (no server, unreadable state) are
/// returned.
fn restore_sessions(
    rex: &Rex,
    state: &StateDir,
    options: &RestoreOptions,
) -> Result<RestoreReport> {
    let Some(snapshot) = chosen_snapshot(state, options)? else {
        return Ok(RestoreReport::default());
    };
    let selected = selected_sessions(&snapshot, options)?;
    let home = workdir::home()?;
    let live_sessions = rex.list_sessions()?;
    let mut lineages = LineageMap::load(state)?;
    let mut live: HashMap<Lineage, String> = live_sessions
        .iter()
        .map(|s| (lineages.lineage_of(&s.session_id), s.session_id.clone()))
        .collect();
    lineages.retain_live(|id| live_sessions.iter().any(|s| s.session_id == id));
    let recorder = RecorderFiles::new(state);
    let replay = Replay::new(state);

    let mut report = RestoreReport {
        had_snapshot: true,
        dry_run: options.dry_run,
        outcomes: Vec::new(),
    };
    for saved in selected {
        let label = saved.label.clone();
        if let Some(live_session_id) = live.get(&saved.lineage) {
            report.outcomes.push(Outcome::Skipped {
                label,
                live_session_id: live_session_id.clone(),
            });
            continue;
        }
        if options.dry_run {
            report.outcomes.push(Outcome::WouldCreate { label });
            continue;
        }
        match rex.create_session(&create::create_request(saved, &home, &replay)) {
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

/// The snapshot to restore from: the one asked for, else the latest
/// (`None` before the first save).
fn chosen_snapshot(state: &StateDir, options: &RestoreOptions) -> Result<Option<Snapshot>> {
    let store = SnapshotStore::new(state);
    let Some(id) = &options.snapshot else {
        return store.read_latest();
    };
    match store.read(id)? {
        Some(snapshot) => Ok(Some(snapshot)),
        None => Err(Error::new(format!(
            "no snapshot {id:?}: `rex-resurrect list` shows the snapshots"
        ))),
    }
}

/// The saved sessions to restore: every one, or every one with the label
/// asked for. Asking for a label the snapshot doesn't hold is an error.
fn selected_sessions<'a>(
    snapshot: &'a Snapshot,
    options: &RestoreOptions,
) -> Result<Vec<&'a SavedSession>> {
    let Some(label) = &options.session else {
        return Ok(snapshot.sessions.iter().collect());
    };
    let selected: Vec<_> = snapshot
        .sessions
        .iter()
        .filter(|s| s.label == *label)
        .collect();
    if selected.is_empty() {
        let which = match &options.snapshot {
            Some(id) => format!("snapshot {id}"),
            None => "the latest snapshot".to_string(),
        };
        let labels: Vec<&str> = snapshot.sessions.iter().map(|s| s.label.as_str()).collect();
        return Err(Error::new(format!(
            "no saved session labelled {label:?} in {which}, which holds: {}",
            labels.join(", ")
        )));
    }
    Ok(selected)
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
