//! The program in a shell block's foreground: the last command of a block
//! whose shell has recorded none, because it started before the recorder was
//! installed.

use std::process::{Command, Stdio};

use crate::error::{Context, Result};
use crate::rex::Rex;

/// The full command line of the program in the block's foreground, or `None`
/// while the block's own shell is in the foreground.
pub fn foreground_command(rex: &Rex, session_id: &str, block_id: &str) -> Result<Option<String>> {
    let process = rex.terminal_process(session_id, block_id)?;
    match process.foreground_program() {
        Some(program) => command_line(program.pid),
        None => Ok(None),
    }
}

/// A process's full command line, from `ps`: the terminal `process` method
/// reports no arguments. `None` if the process has already exited.
fn command_line(pid: u32) -> Result<Option<String>> {
    let out = Command::new("ps")
        .args(["-ww", "-o", "args=", "-p", &pid.to_string()])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .with_context(|| "running ps")?;
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok((out.status.success() && !line.is_empty()).then_some(line))
}
