//! Turning a live session (its `session.view`) into a saved session.

use std::path::PathBuf;

use crate::error::{Error, Result};
use crate::lineage::Lineage;
use crate::rex::Rex;
use crate::rex::api::{BlockView, LayerView, LayoutView, SessionView, WindowView};
use crate::snapshot::{SavedBlock, SavedLayout, SavedSession, SavedSplit, SavedWindow};
use crate::workdir;

/// What save needs to know about the machine it runs on.
pub struct Local {
    /// Where a block with no working directory to save comes back.
    pub home: PathBuf,
}

impl Local {
    pub fn read() -> Result<Local> {
        Ok(Local {
            home: workdir::home()?,
        })
    }

    /// A plain shell block in the home directory: what a block that can't be
    /// saved as it is comes back as, so the layout around it stays intact.
    fn plain_shell(&self) -> SavedBlock {
        SavedBlock {
            working_directory: self.home.clone(),
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
        Some(layout) => saved_layout(rex, session, tiled, layout)?,
        None => SavedLayout::Block(local.plain_shell()),
    };
    Ok(SavedWindow { layout })
}

fn saved_layout(
    rex: &Rex,
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
            Ok(SavedLayout::Block(saved_block(rex, session, block)?))
        }
        LayoutView::Split(split) => Ok(SavedLayout::Split(SavedSplit {
            direction: split.direction,
            ratio: split.ratio,
            before: Box::new(saved_layout(rex, session, layer, &split.before)?),
            after: Box::new(saved_layout(rex, session, layer, &split.after)?),
        })),
    }
}

fn saved_block(rex: &Rex, session: &SessionView, block: &BlockView) -> Result<SavedBlock> {
    Ok(SavedBlock {
        working_directory: workdir::working_directory(rex, &session.session_id, &block.block_id)?,
    })
}
