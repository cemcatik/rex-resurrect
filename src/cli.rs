//! Command-line parsing.
//!
//! Each subcommand parses its own options into the options struct its module
//! defines. To add a subcommand: a `Command` variant, a line in `parse`, a
//! `parse_<name>` function and a line in `USAGE`.

use std::ffi::OsString;
use std::fmt;

use crate::restore::RestoreOptions;
use crate::save::SaveOptions;

pub enum Command {
    Save(SaveOptions),
    Restore(RestoreOptions),
    Help,
    Version,
}

pub const USAGE: &str = "\
Save Rex sessions and recreate them after the Rex server stops.

Usage: rex-resurrect <command>

Commands:
  save      Save every session on the Rex server to a new snapshot
  restore   Recreate the latest snapshot's sessions that aren't live
  help      Show this help

Options:
  -h, --help     Show this help
  -V, --version  Show the version

Environment:
  REX_SERVER               The Rex server to talk to (default: rex's own default)
  REX_RESURRECT_STATE_DIR  Where snapshots and state live (default:
                           $XDG_STATE_HOME/rex-resurrect, else
                           ~/.local/state/rex-resurrect)
";

#[derive(Debug)]
pub struct UsageError(String);

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, UsageError> {
    let mut args = Args::new(args)?;
    if args.wants_help() {
        return Ok(Command::Help);
    }
    let Some(command) = args.next() else {
        return Err(UsageError("missing command".into()));
    };
    match command.as_str() {
        "save" => parse_save(&mut args).map(Command::Save),
        "restore" => parse_restore(&mut args).map(Command::Restore),
        "help" => Ok(Command::Help),
        "-V" | "--version" | "version" => Ok(Command::Version),
        other => Err(UsageError(format!("unknown command {other:?}"))),
    }
}

fn parse_save(args: &mut Args) -> Result<SaveOptions, UsageError> {
    let options = SaveOptions::default();
    if let Some(arg) = args.next() {
        return Err(args.unexpected(&arg, "save"));
    }
    Ok(options)
}

fn parse_restore(args: &mut Args) -> Result<RestoreOptions, UsageError> {
    let options = RestoreOptions::default();
    if let Some(arg) = args.next() {
        return Err(args.unexpected(&arg, "restore"));
    }
    Ok(options)
}

/// The remaining arguments, as UTF-8 strings.
struct Args {
    rest: std::vec::IntoIter<String>,
}

impl Args {
    fn new(args: impl IntoIterator<Item = OsString>) -> Result<Args, UsageError> {
        let args = args
            .into_iter()
            .map(|a| {
                a.into_string()
                    .map_err(|a| UsageError(format!("argument {a:?} isn't valid UTF-8")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Args {
            rest: args.into_iter(),
        })
    }

    fn wants_help(&self) -> bool {
        self.rest.as_slice().iter().any(|a| a == "-h" || a == "--help")
    }

    fn next(&mut self) -> Option<String> {
        self.rest.next()
    }

    fn unexpected(&self, arg: &str, command: &str) -> UsageError {
        if arg.starts_with('-') {
            UsageError(format!("unknown option {arg:?} for {command}"))
        } else {
            UsageError(format!("unexpected argument {arg:?} for {command}"))
        }
    }
}
