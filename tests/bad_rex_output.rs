//! When Rex's output doesn't have the shape rex-resurrect expects, or fails
//! validation, `save` fails loudly, names what didn't match and writes
//! nothing: a changed beta API never produces an empty or broken snapshot.
//!
//! A real server can't produce bad output, so a stub `rex` answers one method
//! with the real server's output, changed, and hands every other call to it.

mod common;

use common::*;
use serde_json::{Value, json};

/// A server with one saved fixture session (a 70/30 split), so "latest"
/// exists before the bad save. Returns the layout it was created with.
fn server_with_a_saved_split(env: &TestEnv) -> (Server, FixtureSession, Layout) {
    let server = env.start_server();
    let [a, b] = ["a", "b"].map(|name| env.mkdir(name));
    let layout = hsplit(0.7, shell(&a), shell(&b));
    let fixture = server.create_session("zz-good", &[window(layout.clone())]);
    server.wait_for_pwds(&fixture);
    env.save(&server);
    (server, fixture, layout)
}

/// Runs `save` with `session.view` answered by `view` instead of the server.
fn save_with_view(env: &TestEnv, server: &Server, view: &Value) -> RunOutput {
    let stub = RexStub::new(env).answer("session.view", view);
    env.resurrect(server)
        .path_prepend(stub.dir())
        .arg("save")
        .run()
}

/// The latest snapshot is still the good one: after killing the fixture,
/// restore brings it back exactly as it was created.
fn assert_latest_is_unchanged(
    env: &TestEnv,
    server: &Server,
    fixture: &FixtureSession,
    layout: &Layout,
) {
    server.kill_session(&fixture.id);
    env.restore(server);
    let restored = server.session_ids_labelled("zz-good");
    assert_eq!(restored.len(), 1, "the good snapshot is still the latest");
    assert_eq!(server.window_shapes(&restored[0]), [layout.expected_shape()]);
}

fn tiled_layer(view: &mut Value, window: usize) -> &mut Value {
    view["windows"][window]["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|layer| layer["kind"] == "tiled")
        .unwrap()
}

#[test]
fn a_split_ratio_outside_0_to_1_fails_save_and_writes_nothing() {
    let env = TestEnv::new();
    let (server, fixture, layout) = server_with_a_saved_split(&env);
    let mut view = server.view(&fixture.id);
    tiled_layer(&mut view, 0)["layout"]["split"]["ratio"] = json!(1.5);

    let out = save_with_view(&env, &server, &view).assert_failure();
    assert!(out.stderr.contains("ratio"), "{out}");
    assert!(out.stderr.contains("1.5"), "{out}");

    assert_latest_is_unchanged(&env, &server, &fixture, &layout);
}
