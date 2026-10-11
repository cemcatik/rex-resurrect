//! A stub `rex` for output no real server produces (a changed beta API).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::Value;

use super::env::TestEnv;
use super::server::rex_bin;

/// A directory holding a stub `rex` script. Put it first on one run's
/// `PATH` with `env.resurrect(&server).path_prepend(stub.dir())`.
///
/// The stub answers the `api call` methods given to [`RexStub::answer`] with
/// canned JSON and hands every other invocation, arguments and stdin
/// unchanged, to the real `rex`. So the binary under test still reaches the
/// throwaway server (through the `-S` it passes) for everything else, and a
/// canned answer can be Rex's real output with one thing changed.
pub struct RexStub {
    dir: PathBuf,
}

impl RexStub {
    pub fn new(env: &TestEnv) -> RexStub {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = env.root().join(format!("stub{n}"));
        std::fs::create_dir_all(dir.join("answers")).unwrap();
        let script = dir.join("rex");
        std::fs::write(&script, stub_script(&dir.join("answers"), rex_bin())).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        RexStub { dir }
    }

    /// Answers `rex ... api call <method> ...` with `result` on stdout, exit 0.
    pub fn answer(self, method: &str, result: &Value) -> RexStub {
        let path = self.dir.join("answers").join(format!("{method}.json"));
        std::fs::write(path, format!("{result}\n")).unwrap();
        self
    }

    /// The directory to put first on `PATH`.
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

fn stub_script(answers: &Path, real_rex: &Path) -> String {
    format!(
        r#"#!/bin/sh
# Stub rex (rex-resurrect test harness): canned answers for chosen
# `api call` methods, everything else handed to the real rex.
answers={answers}
method=
prev=
for arg in "$@"; do
  if [ "$prev" = call ]; then method=$arg; break; fi
  prev=$arg
done
if [ -n "$method" ] && [ -f "$answers/$method.json" ]; then
  cat >/dev/null
  cat "$answers/$method.json"
  exit 0
fi
exec {real_rex} "$@"
"#,
        answers = sh_quote(answers),
        real_rex = sh_quote(real_rex),
    )
}

/// `path` as one single-quoted shell word.
fn sh_quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', r"'\''"))
}
