//! Turning a live session (its `session.view`) into a saved session.

use crate::error::{Error, Result};
use crate::label;
use crate::lineage::Lineage;
use crate::rex::Rex;
use crate::rex::api::{BlockView, LayerView, LayoutView, SessionView, WindowView};
use crate::snapshot::{SavedBlock, SavedLayout, SavedSession, SavedSplit, SavedWindow};
use crate::workdir;

pub fn saved_session(rex: &Rex, view: &SessionView, lineage: Lineage) -> Result<SavedSession> {
    let mut windows = Vec::new();
    for window in &view.windows {
        if let Some(saved) = saved_window(rex, view, window)? {
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
        label: label::user_set_window_label(window).map(str::to_owned),
        layout: saved_layout(rex, session, tiled, layout)?,
    }))
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
        label: label::user_set_block_label(block).map(str::to_owned),
        working_directory: workdir::working_directory(rex, &session.session_id, &block.block_id)?,
    })
}
