//! A block that can't come back exactly as saved still comes back as a shell
//! block, so one bad block never breaks a session's layout.
//!
//! "The home directory" is rex-resurrect's own `HOME`. These tests give
//! rex-resurrect a home of its own, different from the throwaway server's, so
//! they can tell rex-resurrect choosing the home directory apart from Rex
//! falling back to its own home for a `cwd` that doesn't exist.

mod common;

use std::path::Path;

use common::*;

/// `rex-resurrect <command>` against `server`, with `home` as its home directory.
fn run_with_home(env: &TestEnv, server: &Server, home: &Path, command: &str) -> RunOutput {
    env.resurrect(server)
        .env("HOME", home)
        .arg(command)
        .run()
        .assert_success()
}

#[test]
fn a_block_whose_directory_was_deleted_restores_in_the_home_directory() {
    let env = TestEnv::new();
    let server = env.start_server();
    let home = env.mkdir("home");
    let [a, doomed, c, d] = ["a", "doomed", "c", "d"].map(|name| env.mkdir(name));

    let editor = |middle: &Path| hsplit(0.3, shell(&a), vsplit(0.6, shell(middle), shell(&c)));
    let fixture = server.create_session(
        "zz-missing-dir",
        &[window(editor(&doomed)), window(shell(&d))],
    );
    server.wait_for_pwds(&fixture);

    run_with_home(&env, &server, &home, "save");
    server.kill_session(&fixture.id);
    std::fs::remove_dir(&doomed).unwrap();
    run_with_home(&env, &server, &home, "restore");

    let restored = server.session_ids_labelled("zz-missing-dir");
    assert_eq!(restored.len(), 1, "exactly one session restored");
    assert_eq!(
        server.window_shapes(&restored[0]),
        vec![editor(&home).expected_shape(), shell(&d).expected_shape()],
        "only the block whose directory is gone moves to the home directory",
    );
}
