//! rex-resurrect: save Rex sessions and recreate them after the Rex server
//! stops. Talks to Rex only through the `rex` CLI (see `rex`).

mod account;
mod capture;
mod cli;
mod create;
mod error;
mod instance;
mod label;
mod lineage;
mod list;
mod recorder;
mod restore;
mod rex;
mod save;
mod scrollback;
mod snapshot;
mod state;
mod store;
mod timestamp;
mod workdir;

use std::process::ExitCode;

use cli::Command;

fn main() -> ExitCode {
    let command = match cli::parse(std::env::args_os().skip(1)) {
        Ok(command) => command,
        Err(e) => {
            eprintln!("rex-resurrect: {e}\nRun `rex-resurrect --help` for usage.");
            return ExitCode::from(2);
        }
    };
    let result = match command {
        Command::Save(options) => save::run(&options),
        Command::Restore(options) => restore::run(&options),
        Command::List(options) => list::run(&options),
        Command::Help => {
            print!("{}", cli::USAGE);
            Ok(ExitCode::SUCCESS)
        }
        Command::Version => {
            println!("rex-resurrect {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
    };
    result.unwrap_or_else(|e| {
        eprintln!("rex-resurrect: error: {e}");
        ExitCode::FAILURE
    })
}
