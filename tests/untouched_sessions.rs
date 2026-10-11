//! Untouched sessions aren't saved, so sessions that Rex or the user create
//! and abandon don't pile up across restarts.
//!
//! An untouched session: one window holding one shell block, default window
//! and block labels, its working directory still home, and no last command.
//! Its label doesn't count.
//!
//! Each test gives its servers and rex-resurrect a home directory of their
//! own ([`TestEnvBuilder::own_home`]), so "home" is a directory the test
//! knows and the user's real home is never involved.

mod common;

use common::*;
use serde_json::json;

fn env_with_own_home() -> TestEnv {
    TestEnv::builder().own_home().build()
}

/// A shell block created without a `cwd`, the way the Rex app creates one:
/// it starts in the server's home directory.
fn shell_in_home() -> Layout {
    block(json!({}))
}

fn sorted_labels(server: &Server) -> Vec<String> {
    let mut labels: Vec<String> = server.sessions().into_iter().map(|s| s.label).collect();
    labels.sort();
    labels
}

/// Saves, kills every live session, restores, and returns the labels of the
/// sessions that came back: the saved ones.
fn labels_saved(env: &TestEnv, server: &Server) -> Vec<String> {
    env.save(server);
    for session in server.sessions() {
        server.kill_session(&session.id);
    }
    env.restore(server);
    sorted_labels(server)
}

#[test]
fn a_session_with_one_shell_in_home_default_labels_and_no_last_command_isnt_saved() {
    let env = env_with_own_home();
    let server = env.start_server();
    let a = env.mkdir("a");
    let untouched = server.create_session("zz-untouched", &[window(shell_in_home())]);
    let touched = server.create_session("zz-touched", &[window(shell(&a))]);
    server.wait_for_pwds(&untouched);
    server.wait_for_pwds(&touched);

    assert_eq!(labels_saved(&env, &server), ["zz-touched"]);
}

/// Each session differs from an untouched one in one way only.
#[test]
fn a_second_block_or_window_a_user_set_label_another_directory_or_a_last_command_makes_a_session_touched()
 {
    let env = env_with_own_home();
    let server = env.start_server();
    let projects = env.home().join("projects");
    std::fs::create_dir(&projects).unwrap();
    let in_home = || window(shell_in_home());
    let sessions = [
        server.create_session("zz-untouched", &[in_home()]),
        server.create_session(
            "zz-two-blocks",
            &[window(hsplit(0.5, shell_in_home(), shell_in_home()))],
        ),
        server.create_session("zz-two-windows", &[in_home(), in_home()]),
        server.create_session("zz-window-label", &[in_home().labeled("notes")]),
        server.create_session(
            "zz-block-label",
            &[window(shell_in_home().labeled("server"))],
        ),
        server.create_session("zz-below-home", &[window(shell(&projects))]),
        server.create_session("zz-last-command", &[in_home()]),
    ];
    for session in &sessions {
        server.wait_for_pwds(session);
    }
    env.write_recorder_file(sessions[6].block_ids()[0], "make test");

    assert_eq!(
        labels_saved(&env, &server),
        [
            "zz-below-home",
            "zz-block-label",
            "zz-last-command",
            "zz-two-blocks",
            "zz-two-windows",
            "zz-window-label",
        ]
    );
}

/// Without an OSC 7 emitter (zsh without oh-my-zsh, say) a shell never
/// reports its working directory, and save takes its foreground process's
/// instead: a physical path (`/private/tmp/...`), while `HOME` here is
/// logical (`/tmp/...`). It's home all the same.
#[test]
fn a_shell_in_home_that_never_reports_its_working_directory_is_untouched_too() {
    let env = TestEnv::builder()
        .own_home()
        .zshrc_line("precmd_functions=(${precmd_functions:#_rxt_osc7})")
        .build();
    let server = env.start_server();
    let a = env.mkdir("a");
    let untouched = server.create_session("zz-untouched", &[window(shell_in_home())]);
    let touched = server.create_session("zz-touched", &[window(shell(&a))]);
    for session in [&untouched, &touched] {
        server.wait_for_empty_prompt(&session.id, session.block_ids()[0]);
        assert_eq!(server.pwd(&session.id, session.block_ids()[0]), None);
    }

    assert_eq!(labels_saved(&env, &server), ["zz-touched"]);
}
