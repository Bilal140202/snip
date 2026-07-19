use std::process::Command;

use anyhow::{Context, Result};

/// Execute a shell command string.
///
/// The command is run through the user's default shell (`sh -c` on Unix,
/// `cmd /C` on Windows).  Returns the exit status.
pub fn execute(cmd: &str) -> Result<()> {
    let status = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", cmd])
            .status()
            .context("spawning cmd")?
    } else {
        Command::new("sh")
            .args(["-c", cmd])
            .status()
            .context("spawning sh")?
    };

    if status.success() {
        Ok(())
    } else {
        let code = status.code().unwrap_or(-1);
        anyhow::bail!("command exited with status {}", code);
    }
}

/// Execute a shell command and capture stdout.
pub fn execute_capture(cmd: &str) -> Result<String> {
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", cmd])
            .output()
            .context("spawning cmd")?
    } else {
        Command::new("sh")
            .args(["-c", cmd])
            .output()
            .context("spawning sh")?
    };

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let code = output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("command exited with status {}: {}", code, stderr);
    }
}

/// Execute a shell command with the given working directory.
pub fn execute_in_dir(cmd: &str, dir: &std::path::Path) -> Result<()> {
    let status = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", cmd])
            .current_dir(dir)
            .status()
            .context("spawning cmd")?
    } else {
        Command::new("sh")
            .args(["-c", cmd])
            .current_dir(dir)
            .status()
            .context("spawning sh")?
    };

    if status.success() {
        Ok(())
    } else {
        let code = status.code().unwrap_or(-1);
        anyhow::bail!("command exited with status {}", code);
    }
}