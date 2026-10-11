//! Turning a saved session into a `session.create` request.

use crate::rex::api::{
    CreateBlock, CreateLayout, CreateSession, CreateSplit, CreateWindow, SHELL_FLAVOR,
    TerminalOptions,
};
use crate::snapshot::{SavedBlock, SavedLayout, SavedSession, SavedWindow};

pub fn create_request(saved: &SavedSession) -> CreateSession {
    CreateSession {
        label: saved.label.clone(),
        initial_windows: saved.windows.iter().map(create_window).collect(),
    }
}

fn create_window(window: &SavedWindow) -> CreateWindow {
    CreateWindow {
        window_label: window.label.clone(),
        layout: create_layout(&window.layout),
    }
}

fn create_layout(layout: &SavedLayout) -> CreateLayout {
    match layout {
        SavedLayout::Block(block) => CreateLayout::Block(create_block(block)),
        SavedLayout::Split(split) => CreateLayout::Split(CreateSplit {
            direction: split.direction,
            ratio: split.ratio,
            before: Box::new(create_layout(&split.before)),
            after: Box::new(create_layout(&split.after)),
        }),
    }
}

/// Every block comes back as a plain shell block in its saved directory.
fn create_block(block: &SavedBlock) -> CreateBlock {
    CreateBlock {
        flavor: SHELL_FLAVOR.to_string(),
        label: block.label.clone(),
        options: TerminalOptions {
            cwd: Some(block.working_directory.clone()),
            initial_input: None,
        },
    }
}
