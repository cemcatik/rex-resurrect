# rex-resurrect

Save your Rex sessions and recreate them after the Rex server stops: after a
crash, a beta update that needs a restart, or a reboot. Like tmux-resurrect, for Rex.

rex-resurrect is a single self-contained binary. It talks to Rex only through the `rex` CLI and
needs nothing else at runtime.

> **Status:** early. `save` and `restore` bring back each session's label, its windows in order,
> each window's split layout and ratios, window and block labels you set, and each block's
> working directory. Last commands, autosave and auto-restore are coming.

## Install

With a [Rust toolchain](https://rustup.rs) installed:

```sh
cargo install --git https://github.com/cemcatik/rex-resurrect
```

That builds `rex-resurrect` and puts it in `~/.cargo/bin`, which rustup adds to your `PATH`.
The `rex` CLI that ships with Rex must be on your `PATH` too (it is inside Rex blocks).

## Usage

```sh
rex-resurrect save      # snapshot every session on the Rex server
rex-resurrect restore   # recreate the latest snapshot's sessions that aren't live
rex-resurrect list      # list the snapshots, newest first
```

- **`save`** records every session: its label, its windows in order, each window's tiled split
  layout (directions and ratios) and each block's working directory. Window and block labels
  are recorded only if you set them; windows and blocks with Rex's default labels get fresh
  default labels on restore, not stale copies like `Window 1a2b3c4d`. Each save is a new
  timestamped snapshot; `latest.json` points at the newest. The newest 10 snapshots are kept
  (set `REX_RESURRECT_KEEP` to keep a different number).
  - If there's nothing to save (no sessions, or only sessions with no windows left), `save`
    writes nothing, leaves `latest.json` alone and exits 0, so a server with nothing on it
    never replaces your latest snapshot.
  - If Rex's output doesn't have the shape rex-resurrect expects (Rex is in beta and its API
    can change) or the snapshot fails validation (every session needs a window, every split
    ratio must be between 0 and 1), `save` writes nothing, exits non-zero and says what didn't
    match.
- **`restore`** recreates each saved session from the latest snapshot, unless that session is
  already live, and prints what it `created`, `skipped` and why anything `failed`. One session
  failing doesn't stop the others; `restore` exits non-zero if any failed. Running it twice
  creates no duplicates. Options, which combine:
  - `--snapshot <id>` restores from an older snapshot, using an ID from `list`;
  - `--session <label>` restores only the saved sessions with that label (all of them, if
    several share it), and fails if the snapshot has none;
  - `--dry-run` (`-n`) prints what would be created and skipped, and creates nothing.
- **`list`** shows each snapshot's ID, when it was taken (UTC) and its session labels, newest
  first.

Whatever the snapshot, restore skips a saved session that's still live, even if it has changed
since, so restoring an older snapshot never brings back an old version of a session you still
have. Kill the live one first if that's what you want.

A block that can't come back as it was still comes back as a shell, so the layout around it
stays intact. A block whose directory no longer exists opens in your home directory. A
non-terminal block, a block on another host, and a window whose blocks have all floated come
back as a plain shell in your home directory. Floating blocks aren't saved.

Restore recognises sessions by where they came from, not by label: two saved sessions with the
same label both come back, a restored session you rename still counts as live, and a session
you created by hand under a saved label doesn't (see
[ADR-0001](docs/adr/0001-match-sessions-by-lineage-not-label.md)).

After a server restart, the first `save` restores before it saves, unless a `restore` already
ran on that server. A save that runs on a freshly started server (an autosave, say) therefore
can't replace your latest snapshot with a near-empty one. A save on a server already restored
into never restores again, even if you've killed restored sessions since. A `--dry-run` or
`--session` restore doesn't count, because it leaves sessions out; restoring a whole older
snapshot does. Only one restore runs at a time, and a `save` that starts during a restore waits
for it to finish.

rex-resurrect never starts a Rex server. If Rex isn't running, `save` and `restore` fail and
say so.

### Which server

`REX_SERVER` if set (Rex sets it in every block's shell), otherwise the `rex` CLI's default.

### Where state lives

Snapshots and rex-resurrect's own bookkeeping live in its state directory:

1. `$REX_RESURRECT_STATE_DIR`, if set;
2. otherwise `$XDG_STATE_HOME/rex-resurrect`;
3. otherwise `~/.local/state/rex-resurrect`.

It holds `snapshots/`, `latest.json` and `lineage.json`, which remembers which live session
restore created from which saved one. Deleting `lineage.json` makes the next `restore` recreate
sessions that are already live. `last-restored-instance` names the server instance restore last
ran in, and `restore.lock` makes restores take turns.

Snapshots can contain commands, and commands can contain secrets, so the state directory is
readable by you only (`save` and `restore` make it `0700` if it isn't) and every file in it is written with
mode `0600`. Files are written atomically (a temporary file, then a rename), so a crash never
leaves a partial snapshot behind.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```

The integration tests run the built binary against throwaway Rex servers, so they need the
`rex` CLI on `PATH` (or its path in `REX_BIN`). Each test starts its own server under
`/tmp/rxt.*` with its own state directory and a minimal zsh config, and stops it afterwards.
They never touch your real Rex server, sessions or snapshots. The shared harness is in
`tests/common/`.

Terms (session, window, block, label, lineage, ...) follow [GLOSSARY.md](GLOSSARY.md).
