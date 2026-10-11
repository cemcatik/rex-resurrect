//! Telling user-set labels from default labels.
//!
//! Rex generates a default label for every window and block nobody labeled.
//! Only user-set labels are saved: a default one is left out so Rex generates
//! a fresh one on restore, instead of a stale `Window 1a2b3c4d` that looks
//! deliberate.

use crate::rex::api::{BlockView, WindowView};

/// The window's label if the user set it; `None` for its default label,
/// `Window ` plus the last 8 hex characters of its ID.
pub fn user_set_window_label(window: &WindowView) -> Option<&str> {
    let default = window
        .label
        .strip_prefix("Window ")
        .is_some_and(|suffix| suffix == id_suffix(&window.window_id));
    (!default).then_some(window.label.as_str())
}

/// The block's label if the user set it; `None` for the default label a new
/// shell block gets, `shell`.
pub fn user_set_block_label(block: &BlockView) -> Option<&str> {
    (block.label != "shell").then_some(block.label.as_str())
}

/// The last 8 characters of a Rex ID (`window:<uuid>`, `block:<uuid>`): hex,
/// from the UUID's last group.
fn id_suffix(id: &str) -> &str {
    &id[id.len().saturating_sub(8)..]
}
