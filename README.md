# rex-resurrect

Save your Rex sessions and recreate them after the Rex server stops: after a
crash, a beta update that needs a restart, or a reboot. Like tmux-resurrect, for Rex.

rex-resurrect is a single self-contained binary. It talks to Rex only through the `rex` CLI and
needs nothing else at runtime.

> **Status:** early. `save` and `restore` bring back each session's label, its windows in order,
> each window's split layout and ratios, window and block labels you set, each block's working
> directory, each block's earlier output above its prompt, and each block's last command typed
> at that prompt: the one the zsh recorder recorded or, without one, the program that was
> running in the block. Autosave runs `save` every 15 minutes, and auto-restore runs `restore`
> when the Rex server starts.

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
- skips trivial commands on its ignore list (below), keeping the previous one, so the restored
  prompt shows your last meaningful command;
- tracks the last command itself rather than reading your history, so `SHARE_HISTORY` and
  per-directory history don't confuse it.

#### The ignore list

By default the recorder ignores `ls`, `ll`, `la`, `l`, `cd`, `pushd`, `popd`, `pwd`, `clear`,
`reset` and `exit`. A command line is ignored when it runs one of these on its own, with any
arguments: `ls -la` and `cd src` are ignored, but `cd src && make` is recorded. Commands are
matched by name as you typed them, so an alias counts under its own name.

The list is the zsh array `REX_RESURRECT_IGNORE`. You don't have to set it. To change it, set it
in `~/.zshrc`:

```sh
# After loading the recorder: add to the defaults.
REX_RESURRECT_IGNORE+=(htop tig)

# Anywhere in ~/.zshrc: replace the defaults (before or after loading the recorder).
REX_RESURRECT_IGNORE=(ls cd clear)

# Ignore nothing but multi-line commands.
REX_RESURRECT_IGNORE=()
```

The recorder fills in the defaults when it loads, unless `REX_RESURRECT_IGNORE` is already set.
So adding to the list only works after loading the recorder; with oh-my-zsh, that means after
`source $ZSH/oh-my-zsh.sh`. The list is read at every command, so a change applies straight
away.

## Usage

```sh
rex-resurrect save      # snapshot every session on the Rex server
rex-resurrect restore   # recreate the latest snapshot's sessions that aren't live
rex-resurrect list      # list the snapshots, newest first
```

- **`save`** records every session: its label, its windows in order, each window's tiled split
  layout (directions and ratios), and each block's working directory and last command. Window
  and block labels are recorded only if you set them; windows and blocks with Rex's default
  labels get fresh default labels on restore, not stale copies like `Window 1a2b3c4d`. Each
  save is a new timestamped snapshot; `latest.json` points at the newest. The newest 10
  snapshots are kept (set `REX_RESURRECT_KEEP` to keep a different number).
  - Untouched sessions are left out: one window holding one shell, with Rex's default window
    and block labels, still in your home directory, and no last command. Sessions that Rex or
    you create and then abandon therefore don't pile up across restarts. A session's own label
    doesn't count, because Rex's generated ones (`amber-ridge`) look like any other, so a
    session you only renamed is still untouched. A restored session keeps its last command, so
    it doesn't turn untouched just because you haven't run anything in it since.
  - If there's nothing to save (no sessions, or only untouched sessions or sessions with no
    windows left), `save` writes nothing, leaves `latest.json` alone and exits 0, so a server
    with nothing on it never replaces your latest snapshot.
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

Each restored block's last command is typed at its prompt but **never run**: press Enter to
resume, edit it, or clear it. Only single-line commands are typed; a block with no last
command gets an empty prompt. A restored block keeps its last command through later saves and
restores until you run something new in it.

Each restored block also shows its scrollback, the output it held at save time, above the
`Last login:` line and the new prompt, styles included. It wraps to the block's current width
and reflows when you resize the block. A block that was showing a full-screen program (`nvim`,
`less`, `top`, anything on the terminal's alternate screen) gets no scrollback, so you never see
a frozen picture of the program; its command still comes back typed at the prompt.

A block the recorder never recorded anything in, such as one whose shell started before you
loaded the recorder, falls back to the program running in it at save time: `nvim`, a dev server
or a nested shell comes back with its full command line typed at the prompt. Arguments come
from `ps`, which drops their quoting. A block that was sitting at a prompt gets an empty prompt.

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

Every failure of `save` or `restore`, including each session that fails to restore, is also
appended to `rex-resurrect.log` in the state directory, one line per failure starting with a
UTC timestamp. Successful runs write nothing there, except auto-restore, which logs every
outcome (see [Auto-restore](#auto-restore)).

### Which server

`REX_SERVER` if set (Rex sets it in every block's shell), otherwise the `rex` CLI's default.
Autosave saves the server `autosave install` names (see [Autosave](#autosave)).

### Where state lives

Snapshots and rex-resurrect's own bookkeeping live in its state directory:

1. `$REX_RESURRECT_STATE_DIR`, if set;
2. otherwise `$XDG_STATE_HOME/rex-resurrect`;
3. otherwise `~/.local/state/rex-resurrect`.

It holds `snapshots/`, `latest.json` and `lineage.json`, which remembers which live session
restore created from which saved one. Deleting `lineage.json` makes the next `restore` recreate
sessions that are already live. `last-restored-instance` names the server instance restore last
ran in, and `restore.lock` makes restores take turns. `recorder/` holds one file per block with
its last command. Each save that writes a snapshot then removes the `recorder/` files of
blocks that no longer exist and the `lineage.json` entries of sessions that no longer exist, so
the directory doesn't grow forever. A save that fails or writes nothing removes nothing.
`scrollback/` holds each snapshot's block scrollback, which goes when its snapshot goes; a block
with 50,000 lines of output takes about 340 KB per snapshot. `rex-resurrect.log` is the log of
failures and auto-restore outcomes. The recorder finds the state directory the same way, so set
`REX_RESURRECT_STATE_DIR` (if you use it) where both your shells and rex-resurrect see it.

Snapshots, scrollback and recorder files hold commands, and commands can hold secrets, so the
state directory is readable by you only: `save` and `restore` make it `0700` if it isn't.
rex-resurrect writes its own files with mode `0600`, and the recorder keeps its files in a
`0700` directory. Every file except the log is written atomically (a temporary file, then a
rename), so a crash never leaves a partial snapshot or record behind; the log is appended to.

## Autosave

```sh
rex-resurrect autosave install                 # save every 15 minutes
rex-resurrect autosave install --interval 5    # or every 5
rex-resurrect autosave uninstall
```

`autosave install` puts a launchd user agent in `~/Library/LaunchAgents`
(`io.github.cemcatik.rex-resurrect.autosave.plist`) and loads it, so `rex-resurrect save` runs
every 15 minutes, or every `--interval` minutes, from now on and after every login. Running it
again replaces the agent, so there's only ever one. macOS may show a one-time "Background Items
Added" notification.

Autosave is silent: its output is discarded, and a failure (Rex not running, an API error, a
snapshot that fails validation) only adds a line to `rex-resurrect.log` in the state directory.
While Rex isn't running, each autosave fails and logs it; it never starts a server. On a freshly
started server, autosave restores before it saves, like any `save`.

launchd starts the agent with almost no environment: no `REX_SERVER`, a bare `PATH`, nothing
from your shell config. So `autosave install` writes into the agent what `save` needs, as it
finds it when you run it, and prints the server and state directory it chose:

- **the server**: `REX_SERVER` if set, so run it from a Rex block to save the server that block
  is on. Otherwise the Rex app's server,
  `unix://$HOME/Library/Application Support/rex/server.sock`, even if `XDG_RUNTIME_DIR` would
  point the `rex` CLI elsewhere. Autosave always names that server explicitly, so it can't end
  up on another one;
- **the state directory**, resolved as above, so autosave shares snapshots with your manual
  runs and the recorder;
- `REX_RESURRECT_KEEP`, if set;
- the directory of the `rex` on your `PATH`. Install fails if there's no `rex` on it.

Run `autosave install` again after changing any of these, or after moving `rex-resurrect`.

`autosave uninstall` unloads the agent and removes its plist, then tells you how to remove the
zsh recorder and the auto-restore line in `init.lua`. Your snapshots stay in the state
directory.

To check that autosave works, run `launchctl print gui/$(id -u)/io.github.cemcatik.rex-resurrect.autosave`
(its `last exit code` should be 0 after the first run) and `rex-resurrect list` (a new snapshot
every interval), and look in `rex-resurrect.log` for failures.

## Auto-restore

Auto-restore runs `restore` by itself once each time the Rex server starts, so your sessions come
back after a crash, an update or a reboot without you doing anything. It's one line in Rex's
`init.lua`, which is `~/.config/rex/init.lua` unless you've moved it (`rex config check` shows
where yours is).

**To turn it on**, add the line. This appends it, with the full path of `rex-resurrect`:

```sh
mkdir -p ~/.config/rex &&
  echo "os.execute([['$(command -v rex-resurrect)' auto-restore]])" >> ~/.config/rex/init.lua
```

The line reads `os.execute([['/Users/you/.cargo/bin/rex-resurrect' auto-restore]])`. Keep the full
path: the Rex app starts its server with a bare `PATH`, so a plain `rex-resurrect` isn't found.

**To turn it off**, delete that line. `autosave uninstall` reminds you to.

The server runs `init.lua` when it starts, and again on every `rex config reload`. The line starts
`rex-resurrect auto-restore`, which returns at once, because the server can't answer anything
until `init.lua` has finished, and carries on in the background:

- It waits for the server to be ready, then restores the latest snapshot into it the way
  `restore` does. Saved sessions whose lineage is live are skipped, so nothing is duplicated.
- It restores once per server instance. On a config reload, or on a server that `restore` or a
  `save` already restored into (see [Usage](#usage)), it restores nothing, so sessions you killed
  stay gone. Adding the line and running `rex config reload` restores into the running server
  only if nothing has restored into it yet.
- It restores into the server that ran `init.lua`, even one started from inside another server's
  block, whose `REX_SERVER` that server inherits. It reads the server's endpoint from the server
  process's command line, uses it only once the server there reports the right process ID, and
  then names it explicitly in every call. It runs the `rex` that the server itself runs.
- It's silent. Its output is discarded, and each run adds one line to `rex-resurrect.log` saying
  which server it restored into and what it created and skipped, that there was nothing to do,
  or why it gave up, plus a line for each session that failed. If the server isn't ready within
  60 seconds, it gives up and logs that. Set `REX_RESURRECT_READY_TIMEOUT` (seconds) in the
  server's environment, or in the line, to wait longer.
- The Rex app may create a session of its own when it finds the server empty. Auto-restore
  leaves it alone, and while it's untouched `save` never saves it.

It runs in the Rex server's environment, not your shell's. If you set `REX_RESURRECT_STATE_DIR`
for your shells, the server most likely doesn't have it, so put it in the line:
`os.execute([[REX_RESURRECT_STATE_DIR=/path/to/state '/Users/you/.cargo/bin/rex-resurrect' auto-restore]])`.

`rex config check` runs `init.lua` too. There auto-restore says it wasn't started by a Rex server
and does nothing. Any other Rex server that reads the same `init.lua` (one you start by hand with
`rex server run` and no `--config`, say) gets your sessions restored into it too.

If auto-restore can't tell which endpoint is the server's (one containing a space that isn't
percent-encoded; Rex's own endpoints are encoded), it gives up and logs that rather than restore
into any other server.

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

Autosave's tests never touch the real launchd either: `launchctl` is found on `PATH`, so they
put a stand-in first, and `REX_RESURRECT_LAUNCH_AGENTS_DIR` replaces `~/Library/LaunchAgents`.
To try autosave on the real launchd without touching your own agent, also set
`REX_RESURRECT_AUTOSAVE_LABEL` to a scratch label (for both `install` and `uninstall`).

Terms (session, window, block, label, lineage, ...) follow [GLOSSARY.md](GLOSSARY.md).
