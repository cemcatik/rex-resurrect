//! A save on an unrestored server instance restores first, so a freshly
//! started server never replaces the latest snapshot with a near-empty one.
//! Restores take turns, and a save waits for a running restore.
//!
//! Each throwaway server is its own server instance, and servers started from
//! one `TestEnv` share its state directory, so stopping one server and
//! starting another is a restart.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use common::*;

fn sorted_labels(server: &Server) -> Vec<String> {
    let mut labels: Vec<String> = server.sessions().into_iter().map(|s| s.label).collect();
    labels.sort();
    labels
}

/// Saves one session per label, each a shell in its own directory, on a
/// server that then stops. The next server started from `env` is a restart.
fn saved_on_an_earlier_server(env: &TestEnv, labels: &[&str]) {
    let server = env.start_server();
    for label in labels {
        let fixture = server.create_session(label, &[window(shell(env.mkdir(label)))]);
        server.wait_for_pwds(&fixture);
    }
    env.save(&server);
}

/// A directory holding a `rex` that sleeps `seconds` before every call to
/// `method`, then runs the real `rex`. Put it first on a run's `PATH` to make
/// a restore slow enough to overlap with another run.
fn slow_rex(env: &TestEnv, method: &str, seconds: f64) -> PathBuf {
    let dir = env.mkdir("slow-rex");
    let script = format!(
        "#!/bin/sh\nfor arg in \"$@\"; do\n  [ \"$arg\" = {method} ] && sleep {seconds}\ndone\nexec '{}' \"$@\"\n",
        common::server::rex_bin().display()
    );
    let rex = dir.join("rex");
    std::fs::write(&rex, script).unwrap();
    std::fs::set_permissions(&rex, std::fs::Permissions::from_mode(0o755)).unwrap();
    dir
}

#[test]
fn save_on_a_fresh_server_instance_restores_the_latest_snapshot_first() {
    let env = TestEnv::new();
    let [proj, other] = ["proj", "other"].map(|name| env.mkdir(name));
    let layout = || hsplit(0.7, shell(&proj), shell(&other));
    {
        let before = env.start_server();
        let fixture = before.create_session("zz-proj", &[window(layout())]);
        before.wait_for_pwds(&fixture);
        env.save(&before);
    } // The server stops, as in a restart.

    // A session someone created on the new server before anything restored.
    let after = env.start_server();
    let early = after.create_session("zz-early", &[window(shell(&other))]);
    after.wait_for_pwds(&early);
    env.save(&after);

    assert_eq!(sorted_labels(&after), ["zz-early", "zz-proj"]);
    let restored = after.session_ids_labelled("zz-proj");
    assert_eq!(
        after.window_shapes(&restored[0]),
        [layout().expected_shape()]
    );

    // The snapshot that save wrote holds the restored session as well as
    // the one it found.
    drop(after);
    let third = env.start_server();
    env.restore(&third);
    assert_eq!(sorted_labels(&third), ["zz-early", "zz-proj"]);
}

#[test]
fn with_no_snapshot_yet_save_saves_and_counts_the_instance_as_restored() {
    let env = TestEnv::new();
    let server = env.start_server();
    let first = server.create_session("zz-first", &[window(shell(env.mkdir("a")))]);
    server.wait_for_pwds(&first);
    // First use: nothing to restore. (The round-trip test above shows that
    // this save writes a snapshot holding zz-first.)
    env.save(&server);

    server.kill_session(&first.id);
    env.save(&server);
    assert!(
        server.sessions().is_empty(),
        "the second save restored zz-first: {:?}",
        server.sessions()
    );
}

/// Restores the latest snapshot into a fresh server instance with `command`
/// (`restore`, or a `save` that restores first), then kills every session
/// restored and saves again.
fn kill_restored_sessions_and_save_again(command: &str) -> Server {
    let env = TestEnv::new();
    saved_on_an_earlier_server(&env, &["zz-proj"]);
    let after = env.start_server();
    env.resurrect(&after).arg(command).run().assert_success();
    let restored = after.session_ids_labelled("zz-proj");
    assert_eq!(restored.len(), 1, "`{command}` restored zz-proj");

    after.kill_session(&restored[0]);
    env.save(&after);
    after
}

#[test]
fn after_a_restore_save_does_not_restore_again_even_if_the_restored_sessions_were_killed() {
    let server = kill_restored_sessions_and_save_again("restore");
    assert!(server.sessions().is_empty(), "{:?}", server.sessions());
}

#[test]
fn after_a_save_that_restored_first_save_does_not_restore_again() {
    let server = kill_restored_sessions_and_save_again("save");
    assert!(server.sessions().is_empty(), "{:?}", server.sessions());
}

#[test]
fn two_restores_started_at_once_create_no_duplicates() {
    let env = TestEnv::new();
    let labels = ["zz-one", "zz-three", "zz-two"];
    saved_on_an_earlier_server(&env, &labels);
    let server = env.start_server();
    // Every session.create waits a moment, so both restores have listed the
    // live sessions long before either creates one.
    let slow = slow_rex(&env, "session.create", 0.3);
    let restore = || {
        env.resurrect(&server)
            .arg("restore")
            .path_prepend(&slow)
            .spawn()
    };
    let (first, second) = (restore(), restore());
    let outs = [first.wait(), second.wait()].map(RunOutput::assert_success);

    assert_eq!(sorted_labels(&server), labels);
    // One restore created every session; the other waited for it, then
    // found every saved session's lineage live.
    let counts = |out: &RunOutput| {
        (
            out.lines_starting("created").len(),
            out.lines_starting("skipped").len(),
        )
    };
    let mut counts: Vec<(usize, usize)> = outs.iter().map(counts).collect();
    counts.sort();
    assert_eq!(counts, [(0, 3), (3, 0)], "{}\n{}", outs[0], outs[1]);
}
