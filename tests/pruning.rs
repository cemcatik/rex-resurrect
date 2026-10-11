//! After a save writes a snapshot, recorder files for blocks that aren't live
//! any more, and lineage entries for sessions that aren't, are removed, so
//! the state directory doesn't grow forever.
//!
//! Block and session IDs are never reused, so a removed file or entry can't
//! change what a later restore does: removal is observed in the state
//! directory, through the recorder-file contract (`recorder/<block ID>`).
//! What must survive is observed through Rex.

mod common;

use std::path::PathBuf;

use common::*;

fn recorder_file(env: &TestEnv, name: &str) -> PathBuf {
    env.state_dir().join("recorder").join(name)
}

/// The first block of a new single-shell session, once its shell is up.
fn session_with_one_block(env: &TestEnv, server: &Server, label: &str) -> (String, String) {
    let fixture = server.create_session(label, &[window(shell(env.mkdir(label)))]);
    server.wait_for_pwds(&fixture);
    let block = fixture.block_ids()[0].to_string();
    (fixture.id, block)
}

#[test]
fn a_save_removes_recorder_files_for_blocks_that_are_not_live_and_keeps_the_others() {
    let env = TestEnv::new();
    let server = env.start_server();
    let (live, live_block) = session_with_one_block(&env, &server, "zz-live");
    let (gone, gone_block) = session_with_one_block(&env, &server, "zz-gone");
    env.write_recorder_file(&live_block, "echo zz-live");
    env.write_recorder_file(&gone_block, "echo zz-gone");
    // A recorder's temporary file, renamed over the record a moment later.
    let in_flight = recorder_file(&env, &format!(".{gone_block}.4242"));
    std::fs::write(&in_flight, "echo zz-in-flight\n").unwrap();
    server.kill_session(&gone);

    env.save(&server);
    assert!(
        !recorder_file(&env, &gone_block).exists(),
        "the recorder file of a block that isn't live is removed"
    );
    assert!(in_flight.exists(), "temporary files are left alone");

    // The live block's file is still there for the next save to read.
    env.save(&server);
    server.kill_session(&live);
    env.restore(&server);
    let restored = server.the_session_labelled("zz-live");
    server.wait_for_typed(&restored, &server.block_ids(&restored)[0], "echo zz-live");
}

#[test]
fn a_save_that_fails_to_write_its_snapshot_removes_no_recorder_files() {
    let env = TestEnv::new();
    let server = env.start_server();
    let (_live, live_block) = session_with_one_block(&env, &server, "zz-live");
    let (gone, gone_block) = session_with_one_block(&env, &server, "zz-gone");
    env.write_recorder_file(&live_block, "echo zz-live");
    env.write_recorder_file(&gone_block, "echo zz-gone");
    server.kill_session(&gone);
    // A file where the snapshots directory goes: writing the snapshot fails.
    std::fs::write(env.state_dir().join("snapshots"), "").unwrap();

    env.resurrect(&server).arg("save").run().assert_failure();
    assert!(recorder_file(&env, &gone_block).exists());
}

/// Whether rex-resurrect's lineage map mentions `session_id` at all.
fn lineage_map_mentions(env: &TestEnv, session_id: &str) -> bool {
    std::fs::read_to_string(env.state_dir().join("lineage.json"))
        .unwrap_or_default()
        .contains(session_id)
}

#[test]
fn a_save_removes_lineage_entries_for_sessions_that_are_not_live_and_keeps_the_others() {
    let env = TestEnv::new();
    let server = env.start_server();
    let (kept, _) = session_with_one_block(&env, &server, "zz-kept");
    let (dropped, _) = session_with_one_block(&env, &server, "zz-dropped");
    env.save(&server);
    server.kill_session(&kept);
    server.kill_session(&dropped);
    env.restore(&server);
    let restored_dropped = server.the_session_labelled("zz-dropped");
    assert!(lineage_map_mentions(&env, &restored_dropped));
    server.kill_session(&restored_dropped);

    env.save(&server);
    assert!(
        !lineage_map_mentions(&env, &restored_dropped),
        "the lineage entry of a session that isn't live is removed"
    );
    // The live restored session is still recognised as its saved session.
    let again = env.restore(&server);
    assert!(again.lines_starting("created").is_empty(), "{again}");
    assert_eq!(again.lines_starting("skipped").len(), 1, "{again}");
}

#[test]
fn a_save_keeps_the_recorder_file_of_a_floating_block() {
    let env = TestEnv::new();
    let server = env.start_server();
    let [a, b] = ["a", "b"].map(|name| env.mkdir(name));
    let fixture = server.create_session("zz-float", &[window(hsplit(0.5, shell(&a), shell(&b)))]);
    server.wait_for_pwds(&fixture);
    let floating = fixture.block_ids()[1].to_string();
    env.write_recorder_file(&floating, "echo zz-floating");
    // Floating blocks aren't saved, but they're live.
    server.call(
        "session.float_block",
        &serde_json::json!({
            "session_id": fixture.id,
            "block_id": floating,
            "bounds": { "x": 0.1, "y": 0.1, "w": 0.5, "h": 0.5 },
        }),
    );

    env.save(&server);
    assert!(recorder_file(&env, &floating).exists());
}
