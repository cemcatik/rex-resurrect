//! The program in a shell block's foreground: the last command of a block
//! nothing has recorded anything for (no recorder file), because its shell
//! started before the recorder was installed.

use crate::error::Result;
use crate::process;
use crate::rex::Rex;

/// The full command line of the program in the block's foreground, or `None`
/// while the block's own shell is in the foreground. The command line comes
/// from `ps`: the terminal `process` method reports no arguments.
pub fn foreground_command(rex: &Rex, session_id: &str, block_id: &str) -> Result<Option<String>> {
    let process = rex.terminal_process(session_id, block_id)?;
    match process.foreground_program() {
        Some(program) => process::command_line(program.pid),
        None => Ok(None),
    }
}
