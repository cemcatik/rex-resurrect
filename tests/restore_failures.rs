//! One session failing to restore doesn't stop the others.

mod common;

use common::*;

#[test]
fn a_session_that_fails_to_restore_does_not_stop_the_others_and_restore_exits_non_zero() {
    let env = TestEnv::new();
    let a = env.mkdir("a");
    {
        let before = env.start_server();
        let first = before.create_session("zz-narrow-1", &[window(shell(&a))]);
        let wide = before.create_session("zz-wide", &[window(shell(&a)), window(shell(&a))]);
        let last = before.create_session("zz-narrow-2", &[window(shell(&a))]);
        for session in [&first, &wide, &last] {
            before.wait_for_pwds(session);
        }
        env.save(&before);
    } // The server stops, as in a restart.

    // The new server refuses a second window in any session, so zz-wide
    // can't be recreated.
    let after = env.server().arg("--max-windows").arg("1").start();
    let out = env.resurrect(&after).arg("restore").run().assert_failure();

    let created = out.lines_starting("created");
    let failed = out.lines_starting("failed");
    assert_eq!(created.len(), 2, "{out}");
    assert_eq!(failed.len(), 1, "{out}");
    assert!(failed[0].contains("zz-wide"), "{out}");
    let mut labels: Vec<String> = after.sessions().into_iter().map(|s| s.label).collect();
    labels.sort();
    assert_eq!(labels, ["zz-narrow-1", "zz-narrow-2"]);
}
