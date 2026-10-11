//! Throwaway Rex servers and the client calls tests make against them.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::env::{TestEnv, prepend_path};
use super::fixture::{FixtureSession, Shape, Window, tiled_layouts};
use super::tempdir::TempDir;

/// How long the harness waits for a server, a `pwd` or a capture.
pub const WAIT: Duration = Duration::from_secs(15);

/// The real `rex` binary: `$REX_BIN`, else the first `rex` on `PATH`.
///
/// Resolved once, before any test prepends a stub `rex` to a child's `PATH`,
/// so throwaway servers and harness calls always use the real one.
pub fn rex_bin() -> &'static Path {
    static REX: OnceLock<PathBuf> = OnceLock::new();
    REX.get_or_init(|| {
        if let Some(bin) = std::env::var_os("REX_BIN") {
            return PathBuf::from(bin);
        }
        let path = std::env::var_os("PATH").unwrap_or_default();
        std::env::split_paths(&path)
            .map(|dir| dir.join("rex"))
            .find(|candidate| candidate.is_file())
            .expect("the tests need the `rex` CLI on PATH (or REX_BIN)")
    })
}

/// Configures a throwaway server. Get one from [`TestEnv::server`].
pub struct ServerBuilder<'a> {
    env: &'a TestEnv,
    dir: PathBuf,
    init_lua: String,
    args: Vec<OsString>,
    extra_env: Vec<(OsString, OsString)>,
    path_prepend: Vec<PathBuf>,
}

impl<'a> ServerBuilder<'a> {
    pub(crate) fn new(env: &'a TestEnv, dir: PathBuf) -> Self {
        ServerBuilder {
            env,
            dir,
            init_lua: String::new(),
            args: Vec::new(),
            extra_env: Vec::new(),
            path_prepend: Vec::new(),
        }
    }

    /// The server's init.lua (empty by default, so the user's never runs).
    pub fn init_lua(mut self, lua: impl Into<String>) -> Self {
        self.init_lua = lua.into();
        self
    }

    /// An extra `rex server run` argument, e.g. `--max-windows`.
    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    /// An extra variable in the server's environment (inherited by every
    /// block shell and by anything init.lua starts).
    pub fn env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.extra_env
            .push((key.as_ref().to_os_string(), value.as_ref().to_os_string()));
        self
    }

    /// Puts `dir` first on the server's `PATH` (and so the block shells').
    pub fn path_prepend(mut self, dir: impl Into<PathBuf>) -> Self {
        self.path_prepend.push(dir.into());
        self
    }

    pub fn start(self) -> Server {
        std::fs::create_dir_all(self.dir.join("state")).unwrap();
        let config = self.dir.join("init.lua");
        std::fs::write(&config, &self.init_lua).unwrap();
        let socket = self.dir.join("s.sock");
        let endpoint = format!("unix://{}", socket.display());
        assert!(
            socket.as_os_str().len() < 100,
            "socket path too long for macOS: {}",
            socket.display()
        );

        let mut env = self.env.child_env();
        env.insert("REX_SERVER".into(), endpoint.clone().into());
        env.extend(self.extra_env);
        prepend_path(&mut env, &self.path_prepend);

        let log_path = self.dir.join("server.log");
        let log = File::create(&log_path).unwrap();
        let child = Command::new(rex_bin())
            .args(["server", "run", "--listen", &endpoint])
            .arg("--state-dir")
            .arg(self.dir.join("state"))
            .arg("--config")
            .arg(&config)
            .args(&self.args)
            .env_clear()
            .envs(&env)
            .current_dir(self.env.root())
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .expect("starting rex server run");

        let mut server = Server {
            dir: self.dir,
            endpoint,
            child,
            client_env: env,
            _root: self.env.root_handle(),
        };
        server.wait_until_ready(&log_path);
        server
    }
}

/// A running throwaway Rex server with its own socket, `--state-dir` and
/// init.lua. Stopped (SIGTERM, then SIGKILL) on drop, which also ends every
/// block process it started.
pub struct Server {
    dir: PathBuf,
    endpoint: String,
    child: Child,
    client_env: BTreeMap<OsString, OsString>,
    _root: Arc<TempDir>,
}

/// One entry of `session.list`.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionSummary {
    pub id: String,
    pub label: String,
}

impl Server {
    /// `unix:///tmp/rxt.<pid>-<n>/s<k>/s.sock`
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// This server's own directory (socket, `state/`, `init.lua`, `server.log`).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// A `rex` command aimed at this server: `-S <endpoint> --autostart=false`
    /// and the scrubbed environment. Always use this (or the helpers below)
    /// rather than a bare `rex`.
    pub fn rex(&self) -> Command {
        let mut cmd = Command::new(rex_bin());
        cmd.args(["-S", &self.endpoint, "--autostart=false"])
            .env_clear()
            .envs(&self.client_env)
            .stdin(Stdio::null());
        cmd
    }

    /// `rex api call METHOD -` with `args` as JSON on stdin.
    pub fn try_call(&self, method: &str, args: &Value) -> Result<Value, String> {
        use std::io::Write;
        let mut child = self
            .rex()
            .args(["api", "call", method, "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("running rex");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(args.to_string().as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        if !out.status.success() {
            return Err(format!(
                "rex api call {method} failed ({}): {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        serde_json::from_slice(&out.stdout)
            .map_err(|e| format!("rex api call {method}: bad JSON ({e}): {:?}", out.stdout))
    }

    /// Like [`Server::try_call`], panicking on failure.
    pub fn call(&self, method: &str, args: &Value) -> Value {
        self.try_call(method, args)
            .unwrap_or_else(|e| panic!("{e}"))
    }

    pub fn status(&self) -> Value {
        self.call("server.status", &json!({}))
    }

    /// Every live session, in creation order.
    pub fn sessions(&self) -> Vec<SessionSummary> {
        let list = self.call("session.list", &json!({}));
        list["sessions"]
            .as_array()
            .expect("session.list has sessions[]")
            .iter()
            .map(|s| SessionSummary {
                id: s["session_id"].as_str().unwrap().to_string(),
                label: s["label"].as_str().unwrap().to_string(),
            })
            .collect()
    }

    /// IDs of the live sessions labelled `label`, in creation order.
    pub fn session_ids_labelled(&self, label: &str) -> Vec<String> {
        self.sessions()
            .into_iter()
            .filter(|s| s.label == label)
            .map(|s| s.id)
            .collect()
    }

    /// Creates a fixture session. Fixture labels must start with `zz-`.
    pub fn create_session(&self, label: &str, windows: &[Window]) -> FixtureSession {
        assert!(
            label.starts_with("zz-"),
            "fixture labels use the zz- prefix"
        );
        let request = json!({
            "label": label,
            "initial_windows": windows.iter().map(Window::to_json).collect::<Vec<_>>(),
        });
        let created = self.call("session.create", &request);
        FixtureSession::from_create_response(&created)
    }

    pub fn view(&self, session_id: &str) -> Value {
        self.call("session.view", &json!({ "session_id": session_id }))
    }

    pub fn kill_session(&self, session_id: &str) {
        self.call("session.destroy", &json!({ "session_id": session_id }));
    }

    pub fn rename_session(&self, session_id: &str, label: &str) {
        self.call(
            "session.set_label",
            &json!({ "session_id": session_id, "label": label }),
        );
    }

    /// Calls a terminal block method through the `block.method` wrapper.
    pub fn block_method(
        &self,
        session_id: &str,
        block_id: &str,
        method: &str,
        args: Value,
    ) -> Value {
        self.call(
            "block.method",
            &json!({ "session_id": session_id, "block_id": block_id, "method": method, "args": args }),
        )
    }

    /// Types `data` into a block as if at the keyboard (`\n` runs the line).
    pub fn write(&self, session_id: &str, block_id: &str, data: &str) {
        self.block_method(session_id, block_id, "write", json!({ "data": data }));
    }

    /// The block's working directory as its shell last reported it, decoded,
    /// or `None` while it hasn't reported one.
    pub fn pwd(&self, session_id: &str, block_id: &str) -> Option<String> {
        let pwd = self.block_method(session_id, block_id, "pwd", json!({}));
        pwd["pwd"].as_str().map(decode_file_url)
    }

    /// Waits until the block's shell reports a working directory.
    pub fn wait_for_pwd(&self, session_id: &str, block_id: &str) -> String {
        wait_for(&format!("pwd of {block_id}"), || {
            self.pwd(session_id, block_id)
        })
    }

    /// Waits until the block's shell reports exactly `expected`.
    pub fn wait_for_pwd_eq(&self, session_id: &str, block_id: &str, expected: &Path) {
        let expected = expected.to_str().unwrap();
        let mut last = None;
        let found = wait_for_or_none(WAIT, || {
            last = self.pwd(session_id, block_id);
            (last.as_deref() == Some(expected)).then_some(())
        });
        assert!(
            found.is_some(),
            "pwd of {block_id} never became {expected:?} (last: {last:?})"
        );
    }

    /// Waits until every block of `session` reports a working directory.
    pub fn wait_for_pwds(&self, session: &FixtureSession) {
        for block in session.block_ids() {
            self.wait_for_pwd(&session.id, block);
        }
    }

    /// `rex capture --trim --unwrap` of a block: its text, soft wraps joined.
    pub fn capture(&self, session_id: &str, block_id: &str) -> String {
        let out = self
            .rex()
            .args([
                "capture", "-s", session_id, "-b", block_id, "--trim", "--unwrap",
            ])
            .output()
            .unwrap();
        assert_success("rex capture", &out);
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Waits until a block's capture satisfies `pred`; returns that capture.
    pub fn wait_for_capture(
        &self,
        session_id: &str,
        block_id: &str,
        what: &str,
        pred: impl Fn(&str) -> bool,
    ) -> String {
        let mut last = String::new();
        let found = wait_for_or_none(WAIT, || {
            last = self.capture(session_id, block_id);
            pred(&last).then(|| last.clone())
        });
        found.unwrap_or_else(|| panic!("capture of {block_id} never showed {what}; last:\n{last}"))
    }

    /// Each window's tiled layout, in window order, with every block replaced
    /// by its working directory (waiting for each block to report one).
    pub fn window_shapes(&self, session_id: &str) -> Vec<Shape> {
        tiled_layouts(&self.view(session_id))
            .into_iter()
            .map(|layout| {
                layout.map_blocks(&mut |block_id| self.wait_for_pwd(session_id, &block_id))
            })
            .collect()
    }

    fn wait_until_ready(&mut self, log_path: &Path) {
        let deadline = Instant::now() + WAIT;
        loop {
            let ready = self
                .rex()
                .args(["--timeout", "1s", "api", "call", "server.status"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success();
            if ready {
                return;
            }
            if let Some(status) = self.child.try_wait().unwrap() {
                panic!(
                    "throwaway rex server exited early ({status}); log:\n{}",
                    std::fs::read_to_string(log_path).unwrap_or_default()
                );
            }
            if Instant::now() > deadline {
                panic!(
                    "throwaway rex server not ready after {WAIT:?}; log:\n{}",
                    std::fs::read_to_string(log_path).unwrap_or_default()
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = Command::new("kill")
            .args(["-TERM", &self.child.id().to_string()])
            .status();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Decodes the `file://<host>/<path>` URL a shell reports through OSC 7:
/// drops the scheme and any host, percent-decodes the path.
pub fn decode_file_url(url: &str) -> String {
    let rest = url.strip_prefix("file://").unwrap_or(url);
    let path = &rest[rest.find('/').unwrap_or(rest.len())..];
    let bytes = path.as_bytes();
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(hi << 4 | lo);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).expect("fixture paths are UTF-8")
}

/// Polls `f` until it returns `Some`, panicking after [`WAIT`].
pub fn wait_for<T>(what: &str, f: impl FnMut() -> Option<T>) -> T {
    wait_for_or_none(WAIT, f)
        .unwrap_or_else(|| panic!("timed out after {WAIT:?} waiting for {what}"))
}

/// Polls `f` until it returns `Some` or `timeout` passes.
pub fn wait_for_or_none<T>(timeout: Duration, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(v) = f() {
            return Some(v);
        }
        if Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn assert_success(what: &str, out: &Output) {
    assert!(
        out.status.success(),
        "{what} failed ({}): {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
}
