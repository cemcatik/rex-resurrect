//! The program in a shell block's foreground: the last command of a block
//! nothing has recorded anything for (no recorder file), because its shell
//! started before the recorder was installed.

use crate::error::Result;
use crate::process;
use crate::rex::api::TerminalProcess;

/// The full command line of the program in the foreground of the block whose
/// terminal `process` method answered `process`, or `None` while the block's
/// own shell is in the foreground. The command line comes from `ps`: the
/// terminal `process` method reports no arguments.
pub fn foreground_command(process: &TerminalProcess) -> Result<Option<String>> {
    match process.foreground_program() {
        Some(program) => process::command_line(program.pid),
        None => Ok(None),
    }
}
