//! A save on an unrestored server instance restores first, so a freshly
//! started server never replaces the latest snapshot with a near-empty one.
//!
//! Each throwaway server is its own server instance, and servers started from
//! one `TestEnv` share its state directory, so stopping one server and
//! starting another is a restart.

mod common;

use common::*;

fn sorted_labels(server: &Server) -> Vec<String> {
    let mut labels: Vec<String> = server.sessions().into_iter().map(|s| s.label).collect();
    labels.sort();
    labels
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
