//! A block with no recorder file (its shell started before rex-resurrect was
//! installed) falls back to the program in its foreground for its last
//! command. None of these blocks load the zsh recorder.

mod common;

use common::server::wait_for_file;
use common::*;

#[test]
fn a_block_without_a_recorder_file_running_a_program_restores_with_its_command_line_typed_not_run()
{
    let env = TestEnv::new();
    let server = env.start_server();
    let a = env.mkdir("a");
    let fixture = server.create_session("zz-fallback", &[window(shell(&a))]);
    server.wait_for_pwds(&fixture);
    let block = fixture.block_ids()[0];

    // A long-lived program with a visible side effect as it starts: `tee`
    // creates its file, then waits for input.
    let marker = a.join("zz-marker");
    let tee = format!("tee {}", marker.display());
    server.run(&fixture.id, block, &tee);
    wait_for_file(&marker);
    server.wait_for_foreground(&fixture.id, block, "tee");

    env.save(&server);
    server.kill_session(&fixture.id);
    std::fs::remove_file(&marker).unwrap();
    env.restore(&server);

    let restored = server.the_session_labelled("zz-fallback");
    let blocks = server.block_ids(&restored);
    server.wait_for_typed(&restored, &blocks[0], &tee);
    assert_eq!(
        server.foreground(&restored, &blocks[0]).as_deref(),
        Some("zsh"),
        "the restored shell is idle"
    );
    assert!(!marker.exists(), "the typed command must not have run");
}
