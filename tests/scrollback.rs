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

#[test]
fn a_block_showing_a_full_screen_program_gets_no_scrollback_replay() {
    let env = TestEnv::with_recorder();
    let server = env.start_server();
    let [a, b] = ["a", "b"].map(|name| env.mkdir(name));
    let paged = a.join("paged.txt");
    std::fs::write(&paged, "zz-paged-content\n".repeat(5)).unwrap();
    let fixture = server.create_session(
        "zz-full-screen",
        &[window(hsplit(0.5, shell(&a), shell(&b)))],
    );
    server.wait_for_pwds(&fixture);
    let [pager, switched] = [0, 1].map(|i| fixture.windows[0].block_ids[i].as_str());

    // A full-screen program (on the alternate screen).
    server.run(&fixture.id, pager, "echo zz-before-less");
    let less = format!("less {}", paged.display());
    server.run(&fixture.id, pager, &less);
    server.wait_for_foreground(&fixture.id, pager, "less");
    server.wait_for_capture(&fixture.id, pager, "the paged file", |c| {
        c.contains("zz-paged-content")
    });
    // The shell itself in the foreground, but left on the alternate screen
    // (as by a full-screen program that was killed): still no history to show.
    server.run(&fixture.id, switched, "echo zz-before-switch");
    let switch = r"printf '\033[?1049h'";
    server.run(&fixture.id, switched, switch);
    server.wait_for_capture(
        &fixture.id,
        switched,
        "a prompt on the alternate screen",
        |c| !c.contains("zz-before-switch") && c.trim_end().ends_with(" %"),
    );

    env.save(&server);
    server.kill_session(&fixture.id);
    env.restore(&server);

    let restored = server.the_session_labelled("zz-full-screen");
    let blocks = server.block_ids(&restored);
    server.wait_for_typed(&restored, &blocks[0], &less);
    let capture = server.capture(&restored, &blocks[0]);
    for absent in ["zz-paged-content", "zz-before-less"] {
        assert!(
            !capture.contains(absent),
            "no replay of {absent}:\n{capture}"
        );
    }
    server.wait_for_typed(&restored, &blocks[1], switch);
    let capture = server.capture(&restored, &blocks[1]);
    assert!(
        !capture.contains("zz-before-switch"),
        "no replay:\n{capture}"
    );
}

#[test]
fn replayed_output_wraps_at_the_width_of_the_restored_block() {
    let env = TestEnv::new();
    let server = env.start_server();
    let fixture = server.create_session("zz-wrap", &[window(shell(env.mkdir("a")))]);
    server.wait_for_pwds(&fixture);
    let block = fixture.block_ids()[0];
    // Saved from a block 60 columns wide.
    server.resize(&fixture.id, block, 60, 24);
    let long = format!("zz-long-{}-end", "abcdefghij".repeat(15));
    server.run(&fixture.id, block, &format!("echo {long}"));
    server.wait_for_capture(&fixture.id, block, "the long line", |c| {
        c.lines().any(|line| line == long)
    });

    env.save(&server);
    server.kill_session(&fixture.id);
    env.restore(&server);

    let restored = server.the_session_labelled("zz-wrap");
    let block = server.block_ids(&restored).remove(0);
    server.wait_for_empty_prompt(&restored, &block);
    let rows_at = |columns: usize| {
        let rows = server.capture_rows(&restored, &block);
        assert!(
            rows.lines().any(|row| row == &long[..columns]),
            "the long line wraps at {columns} columns:\n{rows}"
        );
        rows
    };
    // Restored 80 columns wide: wrapped at 80, not where it wrapped when saved.
    let rows = rows_at(80);
    assert!(
        !rows.lines().any(|row| row == &long[..60]),
        "no break left at the saved width:\n{rows}"
    );
    let capture = server.capture(&restored, &block);
    assert!(
        capture.lines().any(|line| line == long),
        "still one line:\n{capture}"
    );
    // And it reflows when the block is resized later.
    server.resize(&restored, &block, 50, 24);
    rows_at(50);
}

#[test]
fn a_block_running_a_program_on_the_main_screen_keeps_its_scrollback() {
    let env = TestEnv::with_recorder();
    let server = env.start_server();
    let fixture = server.create_session("zz-main-screen", &[window(shell(env.mkdir("a")))]);
    server.wait_for_pwds(&fixture);
    let block = fixture.block_ids()[0];
    server.run(&fixture.id, block, "echo zz-before-tail");
    let tail = "tail -f /dev/null";
    server.run(&fixture.id, block, tail);
    server.wait_for_foreground(&fixture.id, block, "tail");

    env.save(&server);
    server.kill_session(&fixture.id);
    env.restore(&server);

    let restored = server.the_session_labelled("zz-main-screen");
    let block = server.block_ids(&restored).remove(0);
    server.wait_for_typed(&restored, &block, tail);
    let capture = server.capture(&restored, &block);
    assert!(
        history(&capture).contains(&"zz-before-tail"),
        "the output before the program is above the prompt:\n{capture}"
    );
}
