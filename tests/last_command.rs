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
