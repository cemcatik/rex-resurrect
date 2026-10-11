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
        window_label: None,
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

/// Every block comes back as a plain shell block in its saved directory,
/// with its last command typed at the prompt.
fn create_block(block: &SavedBlock) -> CreateBlock {
    CreateBlock {
        flavor: SHELL_FLAVOR.to_string(),
        label: None,
        options: TerminalOptions {
            cwd: Some(block.working_directory.clone()),
            initial_input: typed_command(block).map(str::to_string),
        },
    }
}

/// The command restore types at a block's prompt: its last command, only if
/// it's a single line without control characters. Rex writes `initial_input`
/// to the terminal as if typed, so a line break would act as Enter and run
/// what came before it (other control characters can be editor keys too).
/// Recorders already skip such commands; this check doesn't trust them, or
/// the snapshot.
pub fn typed_command(block: &SavedBlock) -> Option<&str> {
    block
        .last_command
        .as_deref()
        .filter(|command| !command.is_empty() && !command.chars().any(char::is_control))
}
