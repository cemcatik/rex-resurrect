//! Each shell block's scrollback comes back after a restore, printed above
//! the new prompt, with the block's last command still typed (not run) at
//! that prompt.

mod common;

use common::server::wait_for_file;
use common::*;
use serde_json::json;

/// The lines of a capture before its last (the prompt line).
fn history(capture: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = capture.trim_end().lines().collect();
    lines.pop();
    lines
}

#[test]
fn a_shell_blocks_earlier_output_is_printed_above_the_new_prompt_with_its_last_command_typed() {
    let env = TestEnv::with_recorder();
    let server = env.start_server();
    let a = env.mkdir("a");
    let fixture = server.create_session("zz-scrollback", &[window(shell(&a))]);
    server.wait_for_pwds(&fixture);
    let block = fixture.block_ids()[0];

    for i in 1..=3 {
        server.run(&fixture.id, block, &format!("echo zz-earlier-{i}"));
    }
    // A command with a visible side effect, so the test can tell whether it
    // ran again after the restore.
    let marker = a.join("zz-marker");
    let touch = format!("touch {}", marker.display());
    server.run(&fixture.id, block, &touch);
    wait_for_file(&marker);
    server.wait_for_empty_prompt(&fixture.id, block);

    env.save(&server);
    server.kill_session(&fixture.id);
    std::fs::remove_file(&marker).unwrap();
    env.restore(&server);

    let restored = server.the_session_labelled("zz-scrollback");
    let block = server.block_ids(&restored).remove(0);
    server.wait_for_typed(&restored, &block, &touch);
    let capture = server.capture(&restored, &block);
    let history = history(&capture);
    for i in 1..=3 {
        assert!(
            history.contains(&format!("zz-earlier-{i}").as_str()),
            "earlier output zz-earlier-{i} is above the prompt:\n{capture}"
        );
    }
    // Once where it ran before the restart, once typed at the new prompt.
    // Rex echoes typed input as the block starts; that echo is erased before
    // the scrollback is printed, so it isn't glued to the first old line.
    assert_eq!(
        capture.matches(&touch).count(),
        2,
        "the last command shows where it ran and where it's typed:\n{capture}"
    );
    assert!(!marker.exists(), "the typed command must not have run");

    // The block's shell started the way Rex starts every block's shell: as a
    // login shell under login(1).
    let process = server.block_method(&restored, &block, "process", json!({}));
    assert_eq!(process["foreground"]["argv0"], "-zsh", "{process}");
    assert_eq!(process["child"]["name"], "login", "{process}");
}
