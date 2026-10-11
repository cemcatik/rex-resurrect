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
use serde_json::json;

/// `rex-resurrect <command>` against `server`, with `home` as its home directory.
fn run_with_home(env: &TestEnv, server: &Server, home: &Path, command: &str) -> RunOutput {
    env.resurrect(server)
        .env("HOME", home)
        .arg(command)
        .run()
        .assert_success()
}

/// Moves a tiled block into a new floating layer of its window.
fn float(server: &Server, session_id: &str, block_id: &str) {
    server.call(
        "session.float_block",
        &json!({
            "session_id": session_id,
            "block_id": block_id,
            "bounds": { "x": 0.1, "y": 0.1, "w": 0.5, "h": 0.5 },
        }),
    );
}

/// How many floating layers each window of a live session has.
fn floating_layers(server: &Server, session_id: &str) -> Vec<usize> {
    let view = server.view(session_id);
    view["windows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|window| {
            window["layers"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|layer| layer["kind"] == "floating")
                .count()
        })
        .collect()
}

#[test]
fn a_window_with_a_floating_layer_saves_and_restores_only_its_tiled_layout() {
    let env = TestEnv::new();
    let server = env.start_server();
    let [a, b, floated] = ["a", "b", "floated"].map(|name| env.mkdir(name));
    let fixture = server.create_session(
        "zz-floating",
        &[window(hsplit(
            0.4,
            shell(&a),
            vsplit(0.5, shell(&floated), shell(&b)),
        ))],
    );
    server.wait_for_pwds(&fixture);
    float(&server, &fixture.id, &fixture.windows[0].block_ids[1]);

    env.save(&server);
    server.kill_session(&fixture.id);
    env.restore(&server);

    let restored = server.session_ids_labelled("zz-floating");
    assert_eq!(restored.len(), 1, "exactly one session restored");
    assert_eq!(
        server.window_shapes(&restored[0]),
        vec![hsplit(0.4, shell(&a), shell(&b)).expected_shape()],
        "the tiled layout as it was with the block floated away",
    );
    assert_eq!(floating_layers(&server, &restored[0]), [0]);
}

#[test]
fn a_window_whose_blocks_all_float_restores_as_one_shell_in_the_home_directory_in_its_place() {
    let env = TestEnv::new();
    let server = env.start_server();
    let home = env.mkdir("home");
    let [a, floated, c] = ["a", "floated", "c"].map(|name| env.mkdir(name));
    let fixture = server.create_session(
        "zz-all-floating",
        &[
            window(shell(&a)),
            window(shell(&floated)),
            window(shell(&c)),
        ],
    );
    server.wait_for_pwds(&fixture);
    float(&server, &fixture.id, &fixture.windows[1].block_ids[0]);

    run_with_home(&env, &server, &home, "save");
    server.kill_session(&fixture.id);
    run_with_home(&env, &server, &home, "restore");

    let restored = server.session_ids_labelled("zz-all-floating");
    assert_eq!(restored.len(), 1, "exactly one session restored");
    assert_eq!(
        server.window_shapes(&restored[0]),
        vec![
            shell(&a).expected_shape(),
            shell(&home).expected_shape(),
            shell(&c).expected_shape(),
        ],
        "the emptied window keeps its place in the window order",
    );
    assert_eq!(floating_layers(&server, &restored[0]), [0, 0, 0]);
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
