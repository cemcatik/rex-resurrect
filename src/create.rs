//! Turning a saved session into a `session.create` request.

use std::path::{Path, PathBuf};

use crate::rex::api::{
    CreateBlock, CreateLayout, CreateSession, CreateSplit, CreateWindow, SHELL_FLAVOR,
    TerminalOptions,
};
use crate::scrollback::Replay;
use crate::snapshot::{SavedBlock, SavedLayout, SavedSession, SavedWindow};

/// The request that recreates `saved`. `home` is where a block goes whose
/// saved directory no longer exists; `replay` prints each block's saved
/// scrollback above its new prompt.
pub fn create_request(saved: &SavedSession, home: &Path, replay: &Replay) -> CreateSession {
    CreateSession {
        label: saved.label.clone(),
        initial_windows: saved
            .windows
            .iter()
            .map(|window| create_window(window, home, replay))
            .collect(),
    }
}

fn create_window(window: &SavedWindow, home: &Path, replay: &Replay) -> CreateWindow {
    CreateWindow {
        window_label: window.label.clone(),
        layout: create_layout(&window.layout, home, replay),
    }
}

fn create_layout(layout: &SavedLayout, home: &Path, replay: &Replay) -> CreateLayout {
    match layout {
        SavedLayout::Block(block) => CreateLayout::Block(create_block(block, home, replay)),
        SavedLayout::Split(split) => CreateLayout::Split(CreateSplit {
            direction: split.direction,
            ratio: split.ratio,
            before: Box::new(create_layout(&split.before, home, replay)),
            after: Box::new(create_layout(&split.after, home, replay)),
        }),
    }
}

/// Every block comes back as a plain shell block in its saved directory, or
/// in `home` if that directory no longer exists, with its saved scrollback
/// above the prompt and its last command typed at it. Rex would quietly
/// start the shell in its own home anyway, but rex-resurrect doesn't rely on
/// that.
fn create_block(block: &SavedBlock, home: &Path, replay: &Replay) -> CreateBlock {
    let mut options = TerminalOptions {
        cwd: Some(working_directory(block, home)),
        initial_input: typed_command(block).map(str::to_string),
        ..TerminalOptions::default()
    };
    replay.apply(block, &mut options);
    CreateBlock {
        flavor: SHELL_FLAVOR.to_string(),
        label: block.label.clone(),
        options,
    }
}

/// The directory restore starts a block's shell in: its saved working
/// directory, or `home` if that's no longer a directory.
pub fn working_directory(block: &SavedBlock, home: &Path) -> PathBuf {
    if block.working_directory.is_dir() {
        block.working_directory.clone()
    } else {
        home.to_path_buf()
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
