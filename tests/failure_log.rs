//! `save` and `restore` failures are appended to a timestamped log in the
//! state directory, so a failed autosave (which nobody sees) can still be
//! diagnosed. Successful runs write nothing to it.

mod common;

use common::*;
use serde_json::json;

/// Splits a log entry into its timestamp and message, checking the
/// timestamp looks like `2026-10-11T07:04:49.856123Z` (UTC).
fn timestamp_and_message(entry: &str) -> (&str, &str) {
    let (timestamp, message) = entry
        .split_once(' ')
        .unwrap_or_else(|| panic!("no message in log entry {entry:?}"));
    let shape_ok = timestamp.len() > 20
        && timestamp.ends_with('Z')
        && timestamp.chars().enumerate().all(|(i, c)| match i {
            4 | 7 => c == '-',
            10 => c == 'T',
            13 | 16 => c == ':',
            19 => c == '.',
            _ => c.is_ascii_digit() || c == 'Z',
        });
    assert!(
        shape_ok,
        "log entry doesn't start with a timestamp: {entry:?}"
    );
    (timestamp, message)
}

#[test]
fn each_failed_save_or_restore_appends_a_timestamped_entry() {
    let env = TestEnv::new();
    let endpoint = {
        let server = env.start_server();
        server.endpoint().to_string()
    }; // The server stops: nothing answers at its endpoint any more.

    env.resurrect_at(&endpoint)
        .arg("save")
        .run()
        .assert_failure();
    env.resurrect_at(&endpoint)
        .arg("restore")
        .run()
        .assert_failure();

    let entries = env.log_entries();
    assert_eq!(entries.len(), 2, "{entries:#?}");
    let (first, save) = timestamp_and_message(&entries[0]);
    let (second, restore) = timestamp_and_message(&entries[1]);
    assert!(first <= second, "entries are in time order: {entries:#?}");
    assert!(save.starts_with("save failed: "), "{save}");
    assert!(restore.starts_with("restore failed: "), "{restore}");
    for message in [save, restore] {
        assert!(message.contains("isn't running"), "{message}");
    }
}

#[test]
fn a_save_rejected_for_unexpected_rex_output_is_logged() {
    let env = TestEnv::new();
    let server = env.start_server();
    let stub = RexStub::new(&env).answer("session.list", &json!({ "sessionz": [] }));

    env.resurrect(&server)
        .arg("save")
        .path_prepend(stub.dir())
        .run()
        .assert_failure();

    let entries = env.log_entries();
    assert_eq!(entries.len(), 1, "{entries:#?}");
    let (_, message) = timestamp_and_message(&entries[0]);
    assert!(message.contains("session.list"), "{message}");
}

#[test]
fn each_session_that_fails_to_restore_is_logged() {
    let env = TestEnv::new();
    let a = env.mkdir("a");
    {
        let before = env.start_server();
        let narrow = before.create_session("zz-narrow", &[window(shell(&a))]);
        let wide = before.create_session("zz-wide", &[window(shell(&a)), window(shell(&a))]);
        before.wait_for_pwds(&narrow);
        before.wait_for_pwds(&wide);
        env.save(&before);
    } // The server stops, as in a restart.

    // The new server refuses a second window, so zz-wide can't be recreated.
    let after = env.server().arg("--max-windows").arg("1").start();
    env.resurrect(&after).arg("restore").run().assert_failure();

    let entries = env.log_entries();
    assert_eq!(entries.len(), 1, "{entries:#?}");
    let (_, message) = timestamp_and_message(&entries[0]);
    assert!(message.starts_with("restore failed: "), "{message}");
    assert!(message.contains("zz-wide"), "{message}");
}

#[test]
fn a_session_that_fails_to_restore_when_save_restores_first_is_logged() {
    let env = TestEnv::new();
    let a = env.mkdir("a");
    {
        let before = env.start_server();
        let wide = before.create_session("zz-wide", &[window(shell(&a)), window(shell(&a))]);
        before.wait_for_pwds(&wide);
        env.save(&before);
    } // The server stops, as in a restart.

    // A fresh server instance, so save restores first, and zz-wide fails.
    let after = env.server().arg("--max-windows").arg("1").start();
    env.resurrect(&after).arg("save").run().assert_failure();

    let entries = env.log_entries();
    assert_eq!(entries.len(), 1, "{entries:#?}");
    let (_, message) = timestamp_and_message(&entries[0]);
    assert!(message.starts_with("save"), "{message}");
    assert!(message.contains("zz-wide"), "{message}");
}

#[test]
fn successful_saves_and_restores_write_nothing_to_the_log() {
    let env = TestEnv::new();
    let server = env.start_server();
    let fixture = server.create_session("zz-fine", &[window(shell(env.mkdir("a")))]);
    server.wait_for_pwds(&fixture);

    env.save(&server);
    server.kill_session(&fixture.id);
    env.restore(&server);

    assert_eq!(env.log_entries(), Vec::<String>::new());
}
