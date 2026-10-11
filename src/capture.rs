//! Turning a live session (its `session.view`) into a saved session.

use std::path::PathBuf;

use crate::error::{Error, Result};
use crate::foreground;
use crate::label;
use crate::lineage::Lineage;
use crate::recorder::{Recorded, RecorderFiles};
use crate::rex::api::{BlockView, LayerView, LayoutView, SHELL_FLAVOR, SessionView, WindowView};
use crate::rex::{BlockRef, Rex};
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

/// A live session as save captured it: the saved session, still without its
/// blocks' scrollback, and where that scrollback is to come from. Save only
/// saves the scrollback of a session it keeps
/// ([`CapturedSession::with_scrollback`]), so a session it leaves out (an
/// untouched one, say) leaves nothing behind.
pub struct CapturedSession {
    pub saved: SavedSession,
    /// One entry per block of `saved`, window by window in layout order: the
    /// block whose scrollback to save, or `None` for a block that gets none.
    scrollback: Vec<Option<BlockRef>>,
}

impl CapturedSession {
    /// The saved session, with its blocks' scrollback saved for the snapshot
    /// `store` belongs to.
    pub fn with_scrollback(
        mut self,
        rex: &Rex,
        store: &SnapshotScrollback,
    ) -> Result<SavedSession> {
        let blocks = self
            .saved
            .windows
            .iter_mut()
            .flat_map(|window| window.layout.blocks_mut());
        for (block, source) in blocks.zip(self.scrollback) {
            if let Some(source) = source {
                block.scrollback = store.save(rex, &source.session_id, &source.block_id)?;
            }
        }
        Ok(self.saved)
    }
}

pub fn saved_session(
    rex: &Rex,
    local: &Local,
    view: &SessionView,
    lineage: Lineage,
) -> Result<CapturedSession> {
    let mut capture = Capture {
        rex,
        local,
        session: view,
        scrollback: Vec::new(),
    };
    let windows = view
        .windows
        .iter()
        .map(|window| capture.saved_window(window))
        .collect::<Result<_>>()?;
    Ok(CapturedSession {
        saved: SavedSession {
            lineage,
            label: view.label.clone(),
            windows,
        },
        scrollback: capture.scrollback,
    })
}

/// One session's capture, under way.
struct Capture<'a> {
    rex: &'a Rex,
    local: &'a Local,
    session: &'a SessionView,
    /// [`CapturedSession::scrollback`] so far.
    scrollback: Vec<Option<BlockRef>>,
}

impl Capture<'_> {
    /// A window's tiled layout. Floating layers aren't saved. A window whose
    /// blocks have all floated away has nothing tiled; it comes back holding
    /// one plain shell, so it keeps its place among the session's windows.
    fn saved_window(&mut self, window: &WindowView) -> Result<SavedWindow> {
        let tiled = window.tiled_layer().ok_or_else(|| {
            Error::new(format!(
                "session.view: window {} has no tiled layer",
                window.window_id
            ))
        })?;
        let layout = match &tiled.layout {
            Some(layout) => self.saved_layout(tiled, layout)?,
            None => SavedLayout::Block(self.plain_shell()),
        };
        Ok(SavedWindow {
            label: label::user_set_window_label(window).map(str::to_owned),
            layout,
        })
    }

    fn saved_layout(&mut self, layer: &LayerView, node: &LayoutView) -> Result<SavedLayout> {
        match node {
            LayoutView::Block { block_id } => {
                let block = layer.block(block_id).ok_or_else(|| {
                    Error::new(format!(
                        "session.view: block {block_id} is in a layout but not in its layer's blocks"
                    ))
                })?;
                Ok(SavedLayout::Block(self.saved_block(block)?))
            }
            LayoutView::Split(split) => Ok(SavedLayout::Split(SavedSplit {
                direction: split.direction,
                ratio: split.ratio,
                before: Box::new(self.saved_layout(layer, &split.before)?),
                after: Box::new(self.saved_layout(layer, &split.after)?),
            })),
        }
    }

    /// A local shell block in its working directory, with its recorder file
    /// as the source of truth for its last command, including a record that
    /// it has none. A block without one (its shell started before the
    /// recorder was installed) falls back to the program in its foreground.
    /// Its scrollback is saved later, if save keeps the session. A
    /// non-terminal block has no working directory, and a remote block's is
    /// on another host, so either is saved as a plain shell in the home
    /// directory.
    fn saved_block(&mut self, block: &BlockView) -> Result<SavedBlock> {
        if !self.local.is_local_shell(block) {
            return Ok(self.plain_shell());
        }
        let (rex, session_id, block_id) = (self.rex, &self.session.session_id, &block.block_id);
        let last_command = match self.local.recorder.read(block_id)? {
            Recorded::LastCommand(command) => Some(command),
            Recorded::NoLastCommand => None,
            Recorded::Nothing => foreground::foreground_command(rex, session_id, block_id)?,
        };
        let saved = SavedBlock {
            label: label::user_set_block_label(block).map(str::to_owned),
            working_directory: workdir::working_directory(rex, session_id, block_id)?,
            last_command,
            scrollback: None,
        };
        self.scrollback.push(Some(BlockRef {
            session_id: session_id.clone(),
            block_id: block_id.clone(),
        }));
        Ok(saved)
    }

    /// [`Local::plain_shell`]: no scrollback.
    fn plain_shell(&mut self) -> SavedBlock {
        self.scrollback.push(None);
        self.local.plain_shell()
    }
}
