//! `rex-resurrect autosave install|uninstall`: run `save` on a timer.
//!
//! The scheduler (launchd today) is behind [`crate::scheduler::Scheduler`].

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::error::{Context, Error, Result};
use crate::rex;
use crate::scheduler::{self, Interval, Job};
use crate::state::{STATE_DIR_VAR, StateDir};
use crate::workdir;

#[derive(Debug)]
pub enum AutosaveCommand {
    Install(InstallOptions),
    Uninstall,
}

#[derive(Debug)]
pub struct InstallOptions {
    pub interval: Interval,
}

impl Default for InstallOptions {
    fn default() -> Self {
        InstallOptions {
            interval: Interval::DEFAULT,
        }
    }
}

pub fn run(command: &AutosaveCommand) -> Result<ExitCode> {
    match command {
        AutosaveCommand::Install(options) => install(options),
        AutosaveCommand::Uninstall => uninstall(),
    }
}

fn install(options: &InstallOptions) -> Result<ExitCode> {
    let program = std::env::current_exe().with_context(|| "finding rex-resurrect's own path")?;
    let job = Job {
        program: utf8(program.as_os_str())?,
        args: vec!["save".to_string()],
        env: job_env()?,
        interval: options.interval,
    };
    let scheduler = scheduler::for_this_platform()?;
    scheduler.install(&job)?;
    println!(
        "autosave installed: `rex-resurrect save` runs every {} as {}",
        job.interval,
        scheduler.describe()
    );
    Ok(ExitCode::SUCCESS)
}

/// The scheduled save's environment. A scheduler starts its jobs with
/// almost none (launchd: no `REX_SERVER`, a bare `PATH`, nothing from the
/// user's shell), so the job gets what `save` depends on, as install finds
/// it:
/// - `REX_SERVER`: the server install was aimed at, else the Rex app's
///   server. Save passes it to `rex` with `-S`, so the job can't end up on
///   another server: not rex's default, which `XDG_RUNTIME_DIR` changes,
///   and not a fallback when the server isn't running.
/// - the state directory, as resolved here, so autosave shares snapshots,
///   lineage and recorder files with manual runs and the zsh recorder;
/// - `PATH`: `rex`'s directory first, then the system's.
fn job_env() -> Result<BTreeMap<String, String>> {
    let server = match std::env::var_os(rex::SERVER_VAR).filter(|s| !s.is_empty()) {
        Some(server) => utf8(&server)?,
        None => rex::app_server_endpoint(&workdir::home()?),
    };
    let state = StateDir::from_env()?;
    let mut env = BTreeMap::new();
    env.insert(rex::SERVER_VAR.to_string(), server);
    env.insert(STATE_DIR_VAR.to_string(), utf8(state.path().as_os_str())?);
    env.insert("PATH".to_string(), job_path()?);
    Ok(env)
}

/// `PATH` for the job: the directory of the `rex` on this `PATH`, then the
/// system directories launchd uses by default.
fn job_path() -> Result<String> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let rex_dir = std::env::split_paths(&path)
        .find(|dir| is_executable(&dir.join("rex")))
        .ok_or_else(|| {
            Error::new(
                "can't find `rex` on PATH, and autosave needs it: run this where `rex` is on \
                 your PATH (it is in every Rex block)",
            )
        })?;
    let dirs = std::iter::once(rex_dir.clone()).chain(SYSTEM_PATH.iter().map(PathBuf::from));
    let joined = std::env::join_paths(dirs)
        .with_context(|| format!("putting {} on autosave's PATH", rex_dir.display()))?;
    utf8(&joined)
}

const SYSTEM_PATH: &[&str] = &["/usr/bin", "/bin", "/usr/sbin", "/sbin"];

fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn utf8(value: &OsStr) -> Result<String> {
    value
        .to_str()
        .map(str::to_string)
        .ok_or_else(|| Error::new(format!("{value:?} isn't valid UTF-8")))
}

/// Where the README explains auto-restore, including the line to remove.
const AUTO_RESTORE_DOCS: &str = "https://github.com/cemcatik/rex-resurrect#auto-restore";

fn uninstall() -> Result<ExitCode> {
    let scheduler = scheduler::for_this_platform()?;
    if scheduler.uninstall()? {
        println!("autosave uninstalled: removed {}", scheduler.describe());
    } else {
        println!(
            "autosave wasn't installed: there was no {}",
            scheduler.describe()
        );
    }
    println!(
        "\
To back out of rex-resurrect completely:
- zsh recorder: remove the line that sources rex-resurrect.plugin.zsh from ~/.zshrc
  (or `rex-resurrect` from oh-my-zsh's plugins=(...)), then open new shells.
- auto-restore: remove the rex-resurrect line from Rex's init.lua; see
  {AUTO_RESTORE_DOCS}"
    );
    if let Ok(state) = StateDir::from_env() {
        println!(
            "- snapshots: they stay in {}; delete it if you don't need them.",
            state.path().display()
        );
    }
    Ok(ExitCode::SUCCESS)
}
