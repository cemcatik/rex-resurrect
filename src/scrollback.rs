//! Scrollback: a shell block's earlier output, saved with a snapshot and
//! printed back above the new prompt when restore recreates the block.
//!
//! Saving (prototype #13):
//! - Rex's terminal `format` method renders the block's scrollback, not just
//!   its screen. It's saved as VT (text plus SGR styles) with soft wraps
//!   joined, so it reflows at whatever width the restored block has.
//! - A block on the alternate screen is skipped: it's showing a full-screen
//!   program (`nvim`, `less`, `top`), and its capture would be a frozen
//!   picture of that program rather than history. Rex reports the alternate
//!   screen itself, which is more reliable than guessing from the foreground
//!   process: a block running `tail -f` keeps its scrollback, and one left
//!   on the alternate screen by a killed program loses it.
//!
//! On disk, in the state directory:
//! - `scrollback/<snapshot ID>/<block ID>.vt`: one directory per snapshot,
//!   named like the snapshot (see `store`), holding one file per block;
//! - each saved block records its file's path relative to the state
//!   directory;
//! - every file goes through `state::write_file`, like snapshots;
//! - retention: a snapshot's directory is removed once the snapshot is gone
//!   (see [`ScrollbackStore::remove_orphans`]), so scrollback is kept
//!   exactly as long as the snapshot it belongs to.
//!
//! Restoring: see [`Replay`].

use std::path::{Component, Path, PathBuf};

use crate::account::Account;
use crate::error::{Context, Result};
use crate::rex::Rex;
use crate::rex::api::{FormatArgs, TerminalOptions};
use crate::snapshot::SavedBlock;
use crate::state::{self, StateDir};
use crate::timestamp::Timestamp;

const DIR: &str = "scrollback";

/// What the terminal `format` method's `extra_modes` prefix holds while the
/// block is on the alternate screen (xterm's three ways to switch to it).
const ALTERNATE_SCREEN: [&str; 3] = ["\x1b[?1049h", "\x1b[?1047h", "\x1b[?47h"];

/// Every snapshot's scrollback.
pub struct ScrollbackStore {
    state: StateDir,
}

impl ScrollbackStore {
    pub fn new(state: &StateDir) -> ScrollbackStore {
        ScrollbackStore {
            state: state.clone(),
        }
    }

    /// Where the save that writes the snapshot taken at `taken_at` puts its
    /// blocks' scrollback: a directory named like that snapshot.
    pub fn for_snapshot(&self, taken_at: Timestamp) -> SnapshotScrollback {
        let relative = Path::new(DIR).join(taken_at.compact());
        SnapshotScrollback {
            dir: self.state.join(&relative),
            relative,
        }
    }

    /// Removes the scrollback of every snapshot not in `snapshot_ids` (the
    /// snapshots still on disk), and anything a save left behind without
    /// writing its snapshot.
    pub fn remove_orphans(&self, snapshot_ids: &[String]) -> Result<()> {
        let dir = self.state.join(DIR);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e).with_context(|| format!("listing {}", dir.display())),
        };
        for entry in entries {
            let entry = entry.with_context(|| format!("listing {}", dir.display()))?;
            let name = entry.file_name();
            if snapshot_ids.iter().any(|id| name == id.as_str()) {
                continue;
            }
            let path = entry.path();
            let removed = if entry.file_type().is_ok_and(|t| t.is_dir()) {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            match removed {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(e).with_context(|| format!("removing {}", path.display()));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// The file holding a saved block's scrollback, if it has one and the
    /// file is still there. A recorded path that would reach outside the
    /// state directory counts as none.
    fn file(&self, block: &SavedBlock) -> Option<PathBuf> {
        let relative = block.scrollback.as_deref()?;
        let inside = relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
        let path = self.state.join(relative);
        (inside && path.is_file()).then_some(path)
    }
}

/// One save's scrollback directory.
#[derive(Debug)]
pub struct SnapshotScrollback {
    dir: PathBuf,
    /// `dir` relative to the state directory.
    relative: PathBuf,
}

impl SnapshotScrollback {
    /// Saves a shell block's scrollback and returns the path its saved block
    /// records. `None`, and nothing saved, for a block on the alternate
    /// screen, a block with nothing in it, or a block ID that can't name a
    /// file.
    pub fn save(&self, rex: &Rex, session_id: &str, block_id: &str) -> Result<Option<PathBuf>> {
        let plain =
            !block_id.is_empty() && !block_id.starts_with('.') && !block_id.contains(['/', '\0']);
        if !plain {
            return Ok(None);
        }
        let modes = rex.terminal_format(
            session_id,
            block_id,
            &FormatArgs {
                format: "vt",
                extra_modes: true,
                ..FormatArgs::default()
            },
        )?;
        if ALTERNATE_SCREEN
            .iter()
            .any(|mode| modes.content.contains(mode))
        {
            return Ok(None);
        }
        let mut content = rex
            .terminal_format(
                session_id,
                block_id,
                &FormatArgs {
                    format: "vt",
                    unwrap: true,
                    ..FormatArgs::default()
                },
            )?
            .content;
        if content.trim().is_empty() {
            return Ok(None);
        }
        // The last line is usually the old prompt: end it, so what's printed
        // after the replay starts on a line of its own.
        if !content.ends_with('\n') {
            content.push_str("\r\n");
        }
        let name = format!("{block_id}.vt");
        state::write_file(&self.dir.join(&name), content.as_bytes())?;
        Ok(Some(self.relative.join(name)))
    }
}

/// How restore prints a block's saved scrollback above its new prompt.
///
/// A block with a scrollback file doesn't start the user's shell directly:
/// Rex runs [`REPLAY_SCRIPT`] instead (`shell: "none"` and `login: false`
/// make Rex run the command as given, with no shell of its own around it,
/// so no startup file runs twice). The script prints the file, then starts
/// the shell exactly as Rex would. `initial_input` is unchanged: the last
/// command is typed, never run.
pub struct Replay {
    store: ScrollbackStore,
    account: Option<Account>,
}

/// Run by `/bin/bash` with the scrollback file as `$1`, the user as `$2`,
/// [`LOGIN_SHELL_EXEC`] as `$3` and the login shell as `$4`. They're all
/// arguments, so nothing is ever interpolated into shell code.
///
/// 1. Erases the screen and scrollback. Rex has already written
///    `initial_input` into the new terminal, which echoed it; erasing keeps
///    that echo from being glued to the first replayed line. The typed input
///    itself is still waiting in the terminal for the shell to read.
/// 2. Prints the file (one that has gone since restore checked is skipped,
///    and the shell still starts), then resets styles.
/// 3. Execs Rex's own launch chain for a block's shell, copied from `ps` of
///    a default block: login(1), whose `Last login:` banner separates the old
///    output from the new (`-l` keeps the working directory, `-p` the
///    environment), then the shell as a login shell (`-zsh`). The shell reads
///    its startup files once, like any block's, and its first prompt picks
///    up the typed last command.
///
/// If Rex changes its own chain, restored blocks won't follow; the
/// scrollback round-trip test checks the shell still starts as `-zsh` under
/// `login`.
const REPLAY_SCRIPT: &str = r#"printf "\033[H\033[2J\033[3J"; cat -- "$1" 2>/dev/null; printf "\033[0m"; exec /usr/bin/login -flp "$2" /bin/bash --noprofile --norc -c "$3" rex-login "${4##*/}" "$4""#;

/// The last link of Rex's launch chain: execs the shell with `-<name>` as
/// its argv0, which makes it a login shell.
const LOGIN_SHELL_EXEC: &str = r#"name=$1; shift; exec -a "-$name" -- "$@""#;

impl Replay {
    pub fn new(state: &StateDir) -> Replay {
        Replay {
            store: ScrollbackStore::new(state),
            account: Account::current(),
        }
    }

    /// Makes `options` start the block by printing `block`'s saved
    /// scrollback. Leaves them alone if the block has no scrollback file
    /// (any more), or there's no account to start the shell as.
    pub fn apply(&self, block: &SavedBlock, options: &mut TerminalOptions) {
        let Some(account) = &self.account else {
            return;
        };
        let Some(file) = self.store.file(block) else {
            return;
        };
        let (Some(file), Some(shell)) = (file.to_str(), account.shell.to_str()) else {
            return;
        };
        let argv = [
            "/bin/bash",
            "--noprofile",
            "--norc",
            "-c",
            REPLAY_SCRIPT,
            "rex-resurrect",
            file,
            &account.user,
            LOGIN_SHELL_EXEC,
            shell,
        ];
        options.command = Some(argv.map(str::to_string).to_vec());
        options.shell = Some("none".to_string());
        options.login = Some(false);
    }
}
