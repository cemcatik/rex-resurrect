# rex-resurrect

Save your Rex sessions and recreate them after the Rex server stops: after a
crash, a beta update that needs a restart, or a reboot. Like tmux-resurrect, for Rex.

rex-resurrect is a single self-contained binary. It talks to Rex only through the `rex` CLI and
needs nothing else at runtime.

> **Status:** early. `save` and `restore` bring back each session's label, its windows in order,
> each window's split layout and ratios, each block's working directory and, with the zsh
> recorder loaded, each block's last command typed at its prompt. Window and block labels,
> autosave and auto-restore are coming.

## Install

With a [Rust toolchain](https://rustup.rs) installed:

```sh
cargo install --git https://github.com/cemcatik/rex-resurrect
```

That builds `rex-resurrect` and puts it in `~/.cargo/bin`, which rustup adds to your `PATH`.
The `rex` CLI that ships with Rex must be on your `PATH` too (it is inside Rex blocks).

To get last commands back, also load the zsh recorder (next section).

### The zsh recorder

[`rex-resurrect.plugin.zsh`](rex-resurrect.plugin.zsh) records each command a block's shell
starts as that block's last command. Load it from `~/.zshrc` in either of these ways.

**Plain `source`:**

```sh
mkdir -p ~/.zsh
curl -fsSL -o ~/.zsh/rex-resurrect.plugin.zsh \
  https://raw.githubusercontent.com/cemcatik/rex-resurrect/main/rex-resurrect.plugin.zsh
echo 'source ~/.zsh/rex-resurrect.plugin.zsh' >> ~/.zshrc
```

**As an oh-my-zsh custom plugin:**

```sh
dir="${ZSH_CUSTOM:-$HOME/.oh-my-zsh/custom}/plugins/rex-resurrect"
mkdir -p "$dir"
curl -fsSL -o "$dir/rex-resurrect.plugin.zsh" \
  https://raw.githubusercontent.com/cemcatik/rex-resurrect/main/rex-resurrect.plugin.zsh
```

Then add `rex-resurrect` to `plugins=(...)` in `~/.zshrc`. Cloning this repository into that
directory works too.

New shells pick it up. The recorder:

- does nothing outside Rex (no `REX_BLOCK`), so other terminals and tmux never write anything;
- records only in a block's outermost shell: commands you run in a nested shell (`nix develop`,
  `poetry shell`, a `zsh` started by hand) don't replace the command that started it, and after
  `exec zsh` the shell keeps recording;
- records a command as it starts, so a program still running at save time (`nvim`, a dev
  server, `claude`) counts too;
- skips multi-line commands, keeping the previous one;
- tracks the last command itself rather than reading your history, so `SHARE_HISTORY` and
  per-directory history don't confuse it.

## Usage

```sh
rex-resurrect save      # snapshot every session on the Rex server
rex-resurrect restore   # recreate the latest snapshot's sessions that aren't live
```

- **`save`** records every session: its label, its windows in order, each window's tiled split
  layout (directions and ratios), and each block's working directory and last command. Each
  save is a new timestamped snapshot; `latest.json` points at the newest.
- **`restore`** recreates each saved session from the latest snapshot, unless that session is
  already live, and prints what it `created`, `skipped` and why anything `failed`. One session
  failing doesn't stop the others; `restore` exits non-zero if any failed. Running it twice
  creates no duplicates.

Each restored block's last command is typed at its prompt but **never run**: press Enter to
resume, edit it, or clear it. Only single-line commands are typed; a block with no last
command gets an empty prompt. A restored block keeps its last command through later saves and
restores until you run something new in it.

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

It holds `snapshots/`, `latest.json`, `lineage.json`, which remembers which live session
restore created from which saved one, and `recorder/`, one file per block holding its last
command. Deleting `lineage.json` makes the next `restore` recreate sessions that are already
live. The recorder finds the state directory the same way, so set `REX_RESURRECT_STATE_DIR` (if
you use it) where both your shells and rex-resurrect see it.

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
