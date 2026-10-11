//! Turning a live session (its `session.view`) into a saved session.

use crate::error::{Error, Result};
use crate::lineage::Lineage;
use crate::recorder::RecorderFiles;
use crate::rex::Rex;
use crate::rex::api::{BlockView, LayerView, LayoutView, SessionView, WindowView};
use crate::snapshot::{SavedBlock, SavedLayout, SavedSession, SavedSplit, SavedWindow};
use crate::workdir;

pub fn saved_session(
    rex: &Rex,
    recorder: &RecorderFiles,
    view: &SessionView,
    lineage: Lineage,
) -> Result<SavedSession> {
    let mut windows = Vec::new();
    for window in &view.windows {
        if let Some(saved) = saved_window(rex, recorder, view, window)? {
            windows.push(saved);
        }
    }
    Ok(SavedSession {
        lineage,
        label: view.label.clone(),
        windows,
    })
}

/// A window's tiled layout. `None` for a window with nothing tiled (every
/// block floated). Floating layers aren't saved.
fn saved_window(
    rex: &Rex,
    recorder: &RecorderFiles,
    session: &SessionView,
    window: &WindowView,
) -> Result<Option<SavedWindow>> {
    let tiled = window.tiled_layer().ok_or_else(|| {
        Error::new(format!(
            "session.view: window {} has no tiled layer",
            window.window_id
        ))
    })?;
    let Some(layout) = &tiled.layout else {
        return Ok(None);
    };
    Ok(Some(SavedWindow {
        layout: saved_layout(rex, recorder, session, tiled, layout)?,
    }))
}

fn saved_layout(
    rex: &Rex,
    recorder: &RecorderFiles,
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
            Ok(SavedLayout::Block(saved_block(
                rex, recorder, session, block,
            )?))
        }
        LayoutView::Split(split) => Ok(SavedLayout::Split(SavedSplit {
            direction: split.direction,
            ratio: split.ratio,
            before: Box::new(saved_layout(rex, recorder, session, layer, &split.before)?),
            after: Box::new(saved_layout(rex, recorder, session, layer, &split.after)?),
        })),
    }
}

/// The block's recorder file is the source of truth for its last command.
fn saved_block(
    rex: &Rex,
    recorder: &RecorderFiles,
    session: &SessionView,
    block: &BlockView,
) -> Result<SavedBlock> {
    Ok(SavedBlock {
        working_directory: workdir::working_directory(rex, &session.session_id, &block.block_id)?,
        last_command: recorder.last_command(&block.block_id)?,
    })
}
