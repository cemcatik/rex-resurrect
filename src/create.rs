//! Turning a saved session into a `session.create` request.

use std::path::Path;

use crate::rex::api::{
    CreateBlock, CreateLayout, CreateSession, CreateSplit, CreateWindow, SHELL_FLAVOR,
    TerminalOptions,
};
use crate::snapshot::{SavedBlock, SavedLayout, SavedSession, SavedWindow};

/// The request that recreates `saved`. `home` is where a block goes whose
/// saved directory no longer exists.
pub fn create_request(saved: &SavedSession, home: &Path) -> CreateSession {
    CreateSession {
        label: saved.label.clone(),
        initial_windows: saved
            .windows
            .iter()
            .map(|window| create_window(window, home))
            .collect(),
    }
}

fn create_window(window: &SavedWindow, home: &Path) -> CreateWindow {
    CreateWindow {
        window_label: window.label.clone(),
        layout: create_layout(&window.layout, home),
    }
}

fn create_layout(layout: &SavedLayout, home: &Path) -> CreateLayout {
    match layout {
        SavedLayout::Block(block) => CreateLayout::Block(create_block(block, home)),
        SavedLayout::Split(split) => CreateLayout::Split(CreateSplit {
            direction: split.direction,
            ratio: split.ratio,
            before: Box::new(create_layout(&split.before, home)),
            after: Box::new(create_layout(&split.after, home)),
        }),
    }
}

/// Every block comes back as a plain shell block in its saved directory, or
/// in `home` if that directory no longer exists, with its last command typed
/// at the prompt. Rex would quietly start the shell in its own home anyway,
/// but rex-resurrect doesn't rely on that.
fn create_block(block: &SavedBlock, home: &Path) -> CreateBlock {
    let cwd = if block.working_directory.is_dir() {
        block.working_directory.clone()
    } else {
        home.to_path_buf()
    };
    CreateBlock {
        flavor: SHELL_FLAVOR.to_string(),
        label: block.label.clone(),
        options: TerminalOptions {
            cwd: Some(cwd),
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
