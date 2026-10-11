//! rex-resurrect: save Rex sessions and recreate them after the Rex server
//! stops. Talks to Rex only through the `rex` CLI (see `rex`).

mod autosave;
mod capture;
mod cli;
mod create;
mod error;
mod foreground;
mod instance;
mod label;
mod lineage;
mod list;
mod log;
mod recorder;
mod restore;
mod retention;
mod rex;
mod save;
mod scheduler;
mod snapshot;
mod state;
mod store;
mod timestamp;
mod upkeep;
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
        Command::Save(options) => logging_failure("save", save::run(&options)),
        Command::Restore(options) => logging_failure("restore", restore::run(&options)),
        Command::List(options) => list::run(&options),
        Command::Autosave(command) => autosave::run(&command),
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

/// Appends `result`'s error, if any, to the log in the state directory.
fn logging_failure(command: &str, result: error::Result<ExitCode>) -> error::Result<ExitCode> {
    if let Err(e) = &result
        && let Ok(state) = state::StateDir::from_env()
    {
        log::log_failure(&state, command, e);
    }
    result
}
