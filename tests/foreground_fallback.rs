//! A block with no recorder file (its shell started before rex-resurrect was
//! installed) falls back to the program in its foreground for its last
//! command. None of these blocks load the zsh recorder.

mod common;

use std::path::Path;
use std::time::Duration;

use common::server::wait_for_file;
use common::*;
use serde_json::json;

/// Waits for an empty prompt, then checks nothing gets typed after it either.
fn assert_empty_prompt(server: &Server, session: &str, block: &str) {
    server.wait_for_empty_prompt(session, block);
    std::thread::sleep(Duration::from_millis(500));
    let line = server.prompt_line(session, block);
    assert!(line.ends_with(" %"), "expected an empty prompt: {line:?}");
}

/// A shell block Rex starts without `login`: its shell is the block's first
/// process rather than `login`'s child.
fn non_login_shell(dir: &Path) -> Layout {
    block(json!({ "cwd": dir.to_str().unwrap(), "login": false }))
}

#[test]
fn a_block_without_a_recorder_file_running_a_program_restores_with_its_command_line_typed_not_run()
{
    let env = TestEnv::new();
    let server = env.start_server();
    let [a, b] = ["a", "b"].map(|name| env.mkdir(name));
    let fixture = server.create_session(
        "zz-fallback",
        &[window(hsplit(0.5, shell(&a), non_login_shell(&b)))],
    );
    server.wait_for_pwds(&fixture);
    let [login, non_login] = [0, 1].map(|i| fixture.block_ids()[i]);

    // A long-lived program with a visible side effect as it starts: `tee`
    // creates its file, then waits for input.
    let marker = a.join("zz-marker");
    let tee = format!("tee {}", marker.display());
    server.run(&fixture.id, login, &tee);
    wait_for_file(&marker);
    server.wait_for_foreground(&fixture.id, login, "tee");
    let tail = "tail -n 0 -f /dev/null";
    server.run(&fixture.id, non_login, tail);
    server.wait_for_foreground(&fixture.id, non_login, "tail");

    env.save(&server);
    server.kill_session(&fixture.id);
    std::fs::remove_file(&marker).unwrap();
    env.restore(&server);

    let restored = server.the_session_labelled("zz-fallback");
    let blocks = server.block_ids(&restored);
    server.wait_for_typed(&restored, &blocks[0], &tee);
    server.wait_for_typed(&restored, &blocks[1], tail);
    for block in &blocks {
        assert_eq!(
            server.foreground(&restored, block).as_deref(),
            Some("zsh"),
            "the restored shell is idle"
        );
    }
    assert!(!marker.exists(), "the typed command must not have run");
}

#[test]
fn a_shell_nested_in_a_block_without_a_recorder_file_counts_as_its_foreground_program() {
    let env = TestEnv::new();
    let server = env.start_server();
    let fixture = server.create_session("zz-nested", &[window(shell(env.mkdir("a")))]);
    server.wait_for_pwds(&fixture);
    let block = fixture.block_ids()[0];

    // A nested shell sitting at its own prompt, with a program run inside it
    // before: what's in the foreground at save time is the nested shell.
    server.run(&fixture.id, block, "zsh");
    server.wait_for_capture(&fixture.id, block, "the nested shell's prompt", |c| {
        c.contains("% zsh\n") && c.trim_end().ends_with(" %")
    });
    server.run(&fixture.id, block, "echo zz-inside");
    server.wait_for_capture(&fixture.id, block, "the echo", |c| {
        c.contains("\nzz-inside")
    });

    env.save(&server);
    server.kill_session(&fixture.id);
    env.restore(&server);

    let restored = server.the_session_labelled("zz-nested");
    let blocks = server.block_ids(&restored);
    server.wait_for_typed(&restored, &blocks[0], "zsh");
}

#[test]
fn a_block_with_a_recorder_file_uses_the_recorded_command_whatever_is_in_its_foreground() {
    // No zsh recorder: the recorder file is written by hand, playing one.
    let env = TestEnv::new();
    let server = env.start_server();
    let [a, b] = ["a", "b"].map(|name| env.mkdir(name));
    let fixture = server.create_session("zz-recorded", &[window(hsplit(0.5, shell(&a), shell(&b)))]);
    server.wait_for_pwds(&fixture);
    let [running, idle] = [0, 1].map(|i| fixture.block_ids()[i]);
    let tail = "tail -n 0 -f /dev/null";
    server.run(&fixture.id, running, tail);
    server.wait_for_foreground(&fixture.id, running, "tail");
    let recorded = ["echo zz-recorded-running", "echo zz-recorded-idle"];
    env.write_recorder_file(running, recorded[0]);
    env.write_recorder_file(idle, recorded[1]);

    env.save(&server);
    server.kill_session(&fixture.id);
    env.restore(&server);

    let restored = server.the_session_labelled("zz-recorded");
    let blocks = server.block_ids(&restored);
    server.wait_for_typed(&restored, &blocks[0], recorded[0]);
    server.wait_for_typed(&restored, &blocks[1], recorded[1]);
}

#[test]
fn a_block_without_a_recorder_file_sitting_at_a_prompt_restores_with_an_empty_prompt() {
    let env = TestEnv::new();
    let server = env.start_server();
    let [a, b] = ["a", "b"].map(|name| env.mkdir(name));
    let fixture = server.create_session(
        "zz-idle",
        &[window(hsplit(0.5, shell(&a), non_login_shell(&b)))],
    );
    server.wait_for_pwds(&fixture);

    env.save(&server);
    server.kill_session(&fixture.id);
    env.restore(&server);

    let restored = server.the_session_labelled("zz-idle");
    for block in server.block_ids(&restored) {
        assert_empty_prompt(&server, &restored, &block);
    }
}
