//! A save from an empty server writes nothing and leaves the latest snapshot
//! alone, so an autosave between a restart and a restore can't destroy it.

mod common;

use common::*;

#[test]
fn save_on_a_fresh_server_with_no_sessions_keeps_the_latest_snapshot() {
    let env = TestEnv::new();
    let a = env.mkdir("a");
    let layout = || hsplit(0.6, shell(&a), shell(&a));
    {
        let before = env.start_server();
        let fixture = before.create_session("zz-kept", &[window(layout())]);
        before.wait_for_pwds(&fixture);
        env.save(&before);
    } // The server stops, as in a restart.

    let after = env.start_server();
    assert!(after.sessions().is_empty(), "a fresh server has no sessions");
    env.save(&after);

    env.restore(&after);
    let restored = after.session_ids_labelled("zz-kept");
    assert_eq!(restored.len(), 1, "the earlier snapshot is still the latest");
    assert_eq!(after.window_shapes(&restored[0]), [layout().expected_shape()]);
}

/// The same guard on a server instance that has already been restored into,
/// where a save never restores first.
#[test]
fn save_after_every_session_was_killed_keeps_the_latest_snapshot() {
    let env = TestEnv::new();
    let server = env.start_server();
    let a = env.mkdir("a");
    let fixture = server.create_session("zz-kept", &[window(shell(&a))]);
    server.wait_for_pwds(&fixture);
    env.save(&server);
    server.kill_session(&fixture.id);

    env.save(&server);

    env.restore(&server);
    let restored = server.session_ids_labelled("zz-kept");
    assert_eq!(restored.len(), 1, "the earlier snapshot is still the latest");
    assert_eq!(server.window_shapes(&restored[0]), [shell(&a).expected_shape()]);
}
