//! Snapshot files on disk: private to the user, and only the newest kept.
//!
//! These tests look at the state directory's files (their modes, how many
//! snapshots there are), never inside them.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use common::*;

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
