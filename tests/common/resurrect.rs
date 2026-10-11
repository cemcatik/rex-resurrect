//! Running the built `rex-resurrect` binary.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};

use super::env::prepend_path;

/// The binary under test.
pub fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_rex-resurrect")
}

/// One `rex-resurrect` invocation, with the harness's scrubbed environment:
/// `REX_SERVER` is the target server, the state directory override points at
/// the test's temp dir, and `REX_BLOCK`/`REX_SESSION` are absent. Get one from
/// [`super::TestEnv::resurrect`].
#[must_use]
pub struct ResurrectCmd {
    args: Vec<OsString>,
    env: BTreeMap<OsString, OsString>,
    path_prepend: Vec<PathBuf>,
}

impl ResurrectCmd {
    pub(crate) fn new(env: BTreeMap<OsString, OsString>) -> Self {
        ResurrectCmd {
            args: Vec::new(),
            env,
            path_prepend: Vec::new(),
        }
    }

    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|a| a.as_ref().to_os_string()));
        self
    }

    pub fn env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.env
            .insert(key.as_ref().to_os_string(), value.as_ref().to_os_string());
        self
    }

    pub fn env_remove(mut self, key: impl AsRef<OsStr>) -> Self {
        self.env.remove(key.as_ref());
        self
    }

    /// Puts `dir` first on `PATH`, e.g. a directory holding a stub `rex`.
    pub fn path_prepend(mut self, dir: impl Into<PathBuf>) -> Self {
        self.path_prepend.push(dir.into());
        self
    }

    pub fn run(mut self) -> RunOutput {
        prepend_path(&mut self.env, &self.path_prepend);
        let out = Command::new(bin())
            .args(&self.args)
            .env_clear()
            .envs(&self.env)
            .stdin(Stdio::null())
            .output()
            .expect("running rex-resurrect");
        RunOutput {
            command: format!("rex-resurrect {:?}", self.args),
            status: out.status,
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }
}

/// What a `rex-resurrect` run printed and how it exited.
#[derive(Debug)]
pub struct RunOutput {
    pub command: String,
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

impl RunOutput {
    pub fn assert_success(self) -> Self {
        assert!(self.status.success(), "{} failed: {self}", self.command);
        self
    }

    pub fn assert_failure(self) -> Self {
        assert!(!self.status.success(), "{} unexpectedly succeeded: {self}", self.command);
        self
    }

    /// Stdout lines that start with `word` (e.g. `created`, `skipped`, `failed`).
    pub fn lines_starting(&self, word: &str) -> Vec<&str> {
        self.stdout
            .lines()
            .filter(|line| line.split_whitespace().next() == Some(word))
            .collect()
    }
}

impl std::fmt::Display for RunOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}\n--- stdout\n{}--- stderr\n{}",
            self.status, self.stdout, self.stderr
        )
    }
}
