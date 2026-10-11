//! Turning a live session (its `session.view`) into a saved session.

use std::path::PathBuf;

use crate::error::{Error, Result};
use crate::label;
use crate::lineage::Lineage;
use crate::recorder::RecorderFiles;
use crate::rex::Rex;
use crate::rex::api::{BlockView, LayerView, LayoutView, SHELL_FLAVOR, SessionView, WindowView};
use crate::scrollback::{ScrollbackStore, SnapshotScrollback};
use crate::snapshot::{SavedBlock, SavedLayout, SavedSession, SavedSplit, SavedWindow};
use crate::state::StateDir;
use crate::timestamp::Timestamp;
use crate::workdir;

/// What save compares blocks against and reads about them on this machine:
/// the local server, the home directory and the recorder files; and where
/// it saves their scrollback.
pub struct Local {
    /// The local server's host. A block with a different `host_id` is a
    /// remote block.
    pub host_id: String,
    /// Where a block with no working directory to save comes back.
    pub home: PathBuf,
    /// Each local shell block's last command.
    pub recorder: RecorderFiles,
    /// Where the snapshot this save writes keeps its blocks' scrollback.
    pub scrollback: SnapshotScrollback,
}

impl Local {
    /// `taken_at`: the time of the snapshot this save writes.
    pub fn read(rex: &Rex, state: &StateDir, taken_at: Timestamp) -> Result<Local> {
        Ok(Local {
            host_id: rex.server_status()?.host_id,
            home: workdir::home()?,
            recorder: RecorderFiles::new(state),
            scrollback: ScrollbackStore::new(state).for_snapshot(taken_at),
        })
    }

    /// A shell block on the local server: the only kind of block whose
    /// working directory save can read. A block without a `host_id` counts
    /// as local.
    fn is_local_shell(&self, block: &BlockView) -> bool {
        block.flavor == SHELL_FLAVOR
            && block
                .host_id
                .as_deref()
                .is_none_or(|host_id| host_id == self.host_id)
    }

    /// A plain shell block (default label) in the home directory: what a
    /// block that can't be saved as it is comes back as, so the layout around
    /// it stays intact.
    fn plain_shell(&self) -> SavedBlock {
        SavedBlock {
            label: None,
            working_directory: self.home.clone(),
            last_command: None,
            scrollback: None,
        }
    }
}

pub fn saved_session(
    rex: &Rex,
    local: &Local,
    view: &SessionView,
    lineage: Lineage,
) -> Result<SavedSession> {
    let windows = view
        .windows
        .iter()
        .map(|window| saved_window(rex, local, view, window))
        .collect::<Result<_>>()?;
    Ok(SavedSession {
        lineage,
        label: view.label.clone(),
        windows,
    })
}

/// A window's tiled layout. Floating layers aren't saved. A window whose
/// blocks have all floated away has nothing tiled; it comes back holding one
/// plain shell, so it keeps its place among the session's windows.
fn saved_window(
    rex: &Rex,
    local: &Local,
    session: &SessionView,
    window: &WindowView,
) -> Result<SavedWindow> {
    let tiled = window.tiled_layer().ok_or_else(|| {
        Error::new(format!(
            "session.view: window {} has no tiled layer",
            window.window_id
        ))
    })?;
    let layout = match &tiled.layout {
        Some(layout) => saved_layout(rex, local, session, tiled, layout)?,
        None => SavedLayout::Block(local.plain_shell()),
    };
    Ok(SavedWindow {
        label: label::user_set_window_label(window).map(str::to_owned),
        layout,
    })
}

fn saved_layout(
    rex: &Rex,
    local: &Local,
    session: &SessionView,
    layer: &LayerView,
    node: &LayoutView,
) -> Result<SavedLayout> {
    match node {
        LayoutView::Block { block_id } => {
            let block = layer.block(block_id).ok_or_else(|| {
                Error::new(format!(
                    "session.view: block {block_id} is in a layout but not in its layer's blocks"
                ))
            })?;
            Ok(SavedLayout::Block(saved_block(rex, local, session, block)?))
        }
        LayoutView::Split(split) => Ok(SavedLayout::Split(SavedSplit {
            direction: split.direction,
            ratio: split.ratio,
            before: Box::new(saved_layout(rex, local, session, layer, &split.before)?),
            after: Box::new(saved_layout(rex, local, session, layer, &split.after)?),
        })),
    }
}

/// A local shell block in its working directory, with its recorder file as
/// the source of truth for its last command, and its scrollback. A
/// non-terminal block has no
/// working directory, and a remote block's is on another host, so either is
/// saved as a plain shell in the home directory.
fn saved_block(
    rex: &Rex,
    local: &Local,
    session: &SessionView,
    block: &BlockView,
) -> Result<SavedBlock> {
    if !local.is_local_shell(block) {
        return Ok(local.plain_shell());
    }
    Ok(SavedBlock {
        label: label::user_set_block_label(block).map(str::to_owned),
        working_directory: workdir::working_directory(rex, &session.session_id, &block.block_id)?,
        last_command: local.recorder.last_command(&block.block_id)?,
        scrollback: local
            .scrollback
            .save(rex, &session.session_id, &block.block_id)?,
    })
}
