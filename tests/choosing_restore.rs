//! Choosing what to restore: `list` shows the snapshots, and `restore` can
//! take an older snapshot, a single session label and a dry run.

mod common;

use common::*;

/// One line of `rex-resurrect list`.
#[derive(Debug)]
struct Listed {
    id: String,
    time: String,
    labels: Vec<String>,
}

/// Runs `rex-resurrect list` and parses its lines: `<id>  <time>  <labels>`.
fn list(env: &TestEnv, server: &Server) -> Vec<Listed> {
    let out = env.resurrect(server).arg("list").run().assert_success();
    out.stdout
        .lines()
        .map(|line| {
            let mut columns = line.split("  ");
            let mut next = || {
                columns
                    .next()
                    .unwrap_or_else(|| panic!("list line {line:?} has too few columns"))
                    .to_string()
            };
            let (id, time, labels) = (next(), next(), next());
            Listed {
                id,
                time,
                labels: labels.split(", ").map(str::to_string).collect(),
            }
        })
        .collect()
}

/// `2026-10-11 07:04:49 UTC`
fn looks_like_utc_time(time: &str) -> bool {
    let shape = "dddd-dd-dd dd:dd:dd UTC";
    time.len() == shape.len()
        && time.chars().zip(shape.chars()).all(|(c, s)| match s {
            'd' => c.is_ascii_digit(),
            _ => c == s,
        })
}

/// Creates a session holding one shell block in `dir` and waits for it to
/// report its working directory, so a save sees it.
fn session_in(server: &Server, label: &str, dir: &std::path::Path) -> FixtureSession {
    let session = server.create_session(label, &[window(shell(dir))]);
    server.wait_for_pwds(&session);
    session
}

#[test]
fn list_shows_each_snapshot_newest_first_with_its_time_and_session_labels() {
    let env = TestEnv::new();
    let server = env.start_server();
    let a = env.mkdir("a");
    session_in(&server, "zz-first", &a);
    env.save(&server);
    session_in(&server, "zz-second", &a);
    env.save(&server);

    let snapshots = list(&env, &server);

    assert_eq!(snapshots.len(), 2, "{snapshots:?}");
    assert_eq!(snapshots[0].labels, ["zz-first", "zz-second"]);
    assert_eq!(snapshots[1].labels, ["zz-first"]);
    assert_ne!(snapshots[0].id, snapshots[1].id);
    for snapshot in &snapshots {
        assert!(looks_like_utc_time(&snapshot.time), "{snapshot:?}");
    }
    assert!(snapshots[0].time >= snapshots[1].time, "{snapshots:?}");
}
