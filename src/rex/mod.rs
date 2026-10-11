//! The only way rex-resurrect talks to Rex: the `rex` CLI's `api call`.
//!
//! Every call is `rex [-S <REX_SERVER>] --autostart=false api call METHOD -`
//! with the arguments as JSON on stdin and the result as JSON on stdout.
//!
//! - `--autostart=false` always: otherwise a save that runs while Rex is quit
//!   would start an empty server.
//! - `-S` explicitly whenever `REX_SERVER` is set: if that server isn't
//!   running, a bare `rex` silently falls back to its default endpoint, which
//!   may be a different (real) server.

pub mod api;

use std::ffi::OsString;
use std::fmt;
use std::io::Write;
use std::process::{Command, Stdio};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use api::{
    CreateSession, CreatedSession, SessionList, SessionSummary, SessionView, TerminalProcess,
    TerminalPwd,
};

/// A handle on one Rex server, reached through the `rex` CLI on `PATH`.
#[derive(Debug, Clone)]
pub struct Rex {
    program: OsString,
    server: Option<OsString>,
}

/// Why a `rex` call failed.
#[derive(Debug)]
pub enum RexError {
    /// `rex` couldn't be run at all (not on `PATH`, ...).
    Spawn {
        method: String,
        error: std::io::Error,
    },
    /// No server at the endpoint (`rex` exit status 4). Autostart is off, so
    /// this is the normal answer while the Rex server is down.
    NotRunning { method: String, message: String },
    /// `rex` ran and reported an error (bad request, not found, timeout, ...).
    Failed {
        method: String,
        status: Option<i32>,
        message: String,
    },
    /// `rex` succeeded but its output isn't the shape rex-resurrect expects.
    UnexpectedOutput { method: String, detail: String },
}

impl fmt::Display for RexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RexError::Spawn { method, error } => {
                write!(f, "couldn't run `rex api call {method}`: {error}")
            }
            RexError::NotRunning { method, message } => {
                write!(
                    f,
                    "rex api call {method}: the Rex server isn't running: {message}"
                )
            }
            RexError::Failed {
                method,
                status,
                message,
            } => {
                let status = status.map_or("signal".to_string(), |s| s.to_string());
                write!(f, "rex api call {method} failed (exit {status}): {message}")
            }
            RexError::UnexpectedOutput { method, detail } => {
                write!(
                    f,
                    "rex api call {method} returned unexpected output: {detail}"
                )
            }
        }
    }
}

impl std::error::Error for RexError {}

impl Rex {
    /// The server in `REX_SERVER`, or `rex`'s own default when it's unset.
    pub fn from_env() -> Rex {
        Rex {
            program: "rex".into(),
            server: std::env::var_os("REX_SERVER").filter(|s| !s.is_empty()),
        }
    }

    /// Calls a raw API method and parses its result.
    pub fn call<T: DeserializeOwned>(
        &self,
        method: &str,
        args: &impl Serialize,
    ) -> Result<T, RexError> {
        let input = serde_json::to_vec(args).expect("API arguments serialize to JSON");
        let mut cmd = Command::new(&self.program);
        if let Some(server) = &self.server {
            cmd.arg("-S").arg(server);
        }
        let spawned = cmd
            .args(["--autostart=false", "api", "call", method, "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let spawn_error = |error| RexError::Spawn {
            method: method.to_string(),
            error,
        };
        let mut child = spawned.map_err(spawn_error)?;
        let mut stdin = child.stdin.take().expect("stdin is piped");
        let written = stdin.write_all(&input);
        drop(stdin);
        let out = child.wait_with_output().map_err(spawn_error)?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let message = stderr.trim().trim_start_matches("Error: ").to_string();
            return Err(match out.status.code() {
                Some(4) => RexError::NotRunning {
                    method: method.to_string(),
                    message,
                },
                status => RexError::Failed {
                    method: method.to_string(),
                    status,
                    message,
                },
            });
        }
        written.map_err(spawn_error)?;
        serde_json::from_slice(&out.stdout).map_err(|e| RexError::UnexpectedOutput {
            method: method.to_string(),
            detail: e.to_string(),
        })
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionSummary>, RexError> {
        let list: SessionList = self.call("session.list", &json!({}))?;
        Ok(list.sessions)
    }

    pub fn view_session(&self, session_id: &str) -> Result<SessionView, RexError> {
        self.call("session.view", &json!({ "session_id": session_id }))
    }

    pub fn create_session(&self, request: &CreateSession) -> Result<CreatedSession, RexError> {
        self.call("session.create", request)
    }

    /// The terminal `pwd` method: the directory the block's shell last reported.
    pub fn terminal_pwd(&self, session_id: &str, block_id: &str) -> Result<TerminalPwd, RexError> {
        self.block_method(session_id, block_id, "pwd", json!({}))
    }

    /// The terminal `process` method: the block's foreground process and its ancestors.
    pub fn terminal_process(
        &self,
        session_id: &str,
        block_id: &str,
    ) -> Result<TerminalProcess, RexError> {
        self.block_method(session_id, block_id, "process", json!({}))
    }

    /// Calls a block method by its short name (`pwd`, `process`, `write`)
    /// through the `block.method` wrapper, the only form that accepts `args`.
    fn block_method<T: DeserializeOwned>(
        &self,
        session_id: &str,
        block_id: &str,
        method: &str,
        args: Value,
    ) -> Result<T, RexError> {
        self.call(
            "block.method",
            &json!({ "session_id": session_id, "block_id": block_id, "method": method, "args": args }),
        )
    }
}
