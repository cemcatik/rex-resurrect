//! rex-resurrect's state directory: snapshots, the lineage map and anything
//! else rex-resurrect has to remember between runs.
//!
//! Location, first match wins:
//! 1. `$REX_RESURRECT_STATE_DIR`
//! 2. `$XDG_STATE_HOME/rex-resurrect` (only if `XDG_STATE_HOME` is absolute)
//! 3. `$HOME/.local/state/rex-resurrect`
//!
//! Each module owns the names of its files inside it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::error::{Context, Error, Result};

pub const STATE_DIR_VAR: &str = "REX_RESURRECT_STATE_DIR";

#[derive(Debug, Clone)]
pub struct StateDir {
    root: PathBuf,
}

impl StateDir {
    pub fn from_env() -> Result<StateDir> {
        let var = |name| std::env::var_os(name).filter(|v: &OsString| !v.is_empty());
        resolve(var(STATE_DIR_VAR), var("XDG_STATE_HOME"), var("HOME"))
            .map(|root| StateDir { root })
            .ok_or_else(|| {
                Error::new(format!(
                    "can't find a state directory: set {STATE_DIR_VAR}, XDG_STATE_HOME or HOME"
                ))
            })
    }

    /// A path inside the state directory.
    pub fn join(&self, name: impl AsRef<Path>) -> PathBuf {
        self.root.join(name)
    }
}

fn resolve(
    override_dir: Option<OsString>,
    xdg_state_home: Option<OsString>,
    home: Option<OsString>,
) -> Option<PathBuf> {
    if let Some(dir) = override_dir {
        return Some(PathBuf::from(dir));
    }
    if let Some(xdg) = xdg_state_home
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return Some(xdg.join("rex-resurrect"));
    }
    home.map(|home| PathBuf::from(home).join(".local/state/rex-resurrect"))
}

/// Writes a state file, creating its parent directories. Every state file is
/// written through here.
pub fn write_file(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
}

/// Reads a state file; `None` if it doesn't exist.
pub fn read_file(path: &Path) -> Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}
