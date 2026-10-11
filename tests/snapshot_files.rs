//! Snapshot files on disk: private to the user, and only the newest kept.
//!
//! The privacy test looks at the modes of the state directory's files, never
//! inside them. Retention is observed through `list`.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use common::*;

const KEEP_VAR: &str = "REX_RESURRECT_KEEP";

/// The snapshots `rex-resurrect list` shows, newest first, each as its
/// session labels (`zz-1, zz-2`).
fn listed(env: &TestEnv, server: &Server) -> Vec<String> {
    let out = env.resurrect(server).arg("list").run().assert_success();
    out.stdout
        .lines()
        .filter(|line| *line != "no snapshots yet")
        .map(|line| {
            // `<id>  <time>  <labels>`; the time has single spaces only.
            line.splitn(3, "  ")
                .nth(2)
                .unwrap_or_else(|| panic!("list line {line:?} has too few columns"))
                .to_string()
        })
        .collect()
}

fn sorted_labels(server: &Server) -> Vec<String> {
    let mut labels: Vec<String> = server.sessions().into_iter().map(|s| s.label).collect();
    labels.sort();
    labels
}

/// Every directory and file under `dir` (and `dir` itself) that its group or
/// other users can access, with its mode. Symlinks are skipped: their own
/// mode means nothing.
fn accessible_to_others(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(path) = pending.pop() {
        let meta = std::fs::symlink_metadata(&path).unwrap();
        if meta.file_type().is_symlink() {
            continue;
        }
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            found.push(format!("{} ({mode:o})", path.display()));
        }
        if meta.is_dir() {
            for entry in std::fs::read_dir(&path).unwrap() {
                pending.push(entry.unwrap().path());
            }
        }
    }
    found.sort();
    found
}

#[test]
fn the_state_directory_and_everything_in_it_are_private_to_the_user() {
    let env = TestEnv::new();
    // As an earlier version, or the user, might have left it.
    std::fs::set_permissions(env.state_dir(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let server = env.start_server();
    let fixture = server.create_session("zz-private", &[window(shell(env.mkdir("a")))]);
    server.wait_for_pwds(&fixture);

    env.save(&server);
    server.kill_session(&fixture.id);
    env.restore(&server); // Writes the lineage map too.
    env.save(&server);

    assert_eq!(accessible_to_others(&env.state_dir()), Vec::<String>::new());
}

#[test]
fn only_the_newest_snapshots_are_kept_and_latest_is_the_newest() {
    let env = TestEnv::new();
    let server = env.start_server();
    let a = env.mkdir("a");
    let mut fixtures = Vec::new();
    // Each save holds one more session than the one before.
    for label in ["zz-1", "zz-2", "zz-3"] {
        let fixture = server.create_session(label, &[window(shell(&a))]);
        server.wait_for_pwds(&fixture);
        env.resurrect(&server)
            .arg("save")
            .env(KEEP_VAR, "2")
            .run()
            .assert_success();
        fixtures.push(fixture);
    }

    assert_eq!(listed(&env, &server), ["zz-1, zz-2, zz-3", "zz-1, zz-2"]);

    for fixture in &fixtures {
        server.kill_session(&fixture.id);
    }
    env.restore(&server);
    assert_eq!(sorted_labels(&server), ["zz-1", "zz-2", "zz-3"]);
}

#[test]
fn ten_snapshots_are_kept_by_default() {
    let env = TestEnv::new();
    let server = env.start_server();
    let fixture = server.create_session("zz-often", &[window(shell(env.mkdir("a")))]);
    server.wait_for_pwds(&fixture);

    for _ in 0..12 {
        env.save(&server);
    }

    assert_eq!(listed(&env, &server).len(), 10);
}

#[test]
fn a_keep_count_that_is_not_a_positive_number_fails_save_and_writes_nothing() {
    let env = TestEnv::new();
    let server = env.start_server();
    let fixture = server.create_session("zz-kept", &[window(shell(env.mkdir("a")))]);
    server.wait_for_pwds(&fixture);

    for bad in ["0", "ten", "-1"] {
        let out = env
            .resurrect(&server)
            .arg("save")
            .env(KEEP_VAR, bad)
            .run()
            .assert_failure();
        assert!(out.stderr.contains(KEEP_VAR), "{out}");
    }

    assert_eq!(listed(&env, &server), Vec::<String>::new());
}
