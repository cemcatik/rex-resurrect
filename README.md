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
```

- **`save`** records every session: its label, its windows in order, each window's tiled split
  layout (directions and ratios) and each block's working directory. Window and block labels
  are recorded only if you set them; windows and blocks with Rex's default labels get fresh
  default labels on restore, not stale copies like `Window 1a2b3c4d`. Each save is a new
  timestamped snapshot; `latest.json` points at the newest.
- **`restore`** recreates each saved session from the latest snapshot, unless that session is
  already live, and prints what it `created`, `skipped` and why anything `failed`. One session
  failing doesn't stop the others; `restore` exits non-zero if any failed. Running it twice
  creates no duplicates.

A block that can't come back as it was still comes back as a shell, so the layout around it
stays intact. A block whose directory no longer exists opens in your home directory. A
non-terminal block, a block on another host, and a window whose blocks have all floated come
back as a plain shell in your home directory. Floating blocks aren't saved.

Restore recognises sessions by where they came from, not by label: two saved sessions with the
same label both come back, a restored session you rename still counts as live, and a session
you created by hand under a saved label doesn't (see
[ADR-0001](docs/adr/0001-match-sessions-by-lineage-not-label.md)).

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
sessions that are already live.

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
