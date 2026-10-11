//! Each block's last command comes back typed at its prompt, never run.

mod common;

use std::path::Path;

use common::server::wait_for;
use common::*;

fn wait_for_file(path: &Path) {
    wait_for(&format!("{} to exist", path.display()), || {
        path.exists().then_some(())
    });
}

#[test]
fn the_last_command_is_typed_at_the_prompt_after_restore_but_not_run() {
    let env = TestEnv::with_recorder();
    let server = env.start_server();
    let [a, b] = ["a", "b"].map(|name| env.mkdir(name));
    let fixture = server.create_session("zz-typed", &[window(hsplit(0.5, shell(&a), shell(&b)))]);
    server.wait_for_pwds(&fixture);
    let [finished, running] = [0, 1].map(|i| fixture.windows[0].block_ids[i].as_str());

    // A command with a visible side effect, so the test can tell whether it ran.
    let marker = a.join("zz-marker");
    let touch = format!("touch {}", marker.display());
    server.run(&fixture.id, finished, &touch);
    wait_for_file(&marker);
    // A program still running at save time is recorded as it starts.
    let tail = "tail -f /dev/null";
    server.run(&fixture.id, running, tail);
    server.wait_for_foreground(&fixture.id, running, "tail");

    env.save(&server);
    server.kill_session(&fixture.id);
    std::fs::remove_file(&marker).unwrap();
    env.restore(&server);

    let restored = server.the_session_labelled("zz-typed");
    let blocks = server.block_ids(&restored);
    server.wait_for_typed(&restored, &blocks[0], &touch);
    server.wait_for_typed(&restored, &blocks[1], tail);
    assert!(!marker.exists(), "the typed command must not have run");
}

/// Saves, kills `session` and restores; returns the restored session (the one
/// labelled `label`) and its first block.
fn save_kill_restore(
    env: &TestEnv,
    server: &Server,
    session: &str,
    label: &str,
) -> (String, String) {
    env.save(server);
    server.kill_session(session);
    env.restore(server);
    let restored = server.the_session_labelled(label);
    let block = server.block_ids(&restored).remove(0);
    (restored, block)
}

#[test]
fn the_last_command_carries_forward_through_later_saves_and_restores_until_a_new_one_runs() {
    let env = TestEnv::with_recorder();
    let server = env.start_server();
    let fixture = server.create_session("zz-carry", &[window(shell(env.mkdir("a")))]);
    server.wait_for_pwds(&fixture);
    let command = "echo zz-carried";
    server.run(&fixture.id, fixture.block_ids()[0], command);
    server.wait_for_capture(&fixture.id, fixture.block_ids()[0], "the echo", |c| {
        c.contains("\nzz-carried")
    });

    // Nothing runs in the restored blocks: the command is only typed.
    let (session, block) = save_kill_restore(&env, &server, &fixture.id, "zz-carry");
    server.wait_for_typed(&session, &block, command);
    let (session, block) = save_kill_restore(&env, &server, &session, "zz-carry");
    server.wait_for_typed(&session, &block, command);

    // Clear the typed command (Ctrl-U) and run a new one: it replaces the old.
    let new_command = "echo zz-new";
    server.write(&session, &block, &format!("\u{15}{new_command}\n"));
    server.wait_for_capture(&session, &block, "the new echo", |c| c.contains("\nzz-new"));
    let (session, block) = save_kill_restore(&env, &server, &session, "zz-carry");
    server.wait_for_typed(&session, &block, new_command);
}
