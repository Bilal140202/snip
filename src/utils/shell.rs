/// Shell-related utilities.

use std::process::Command;

/// Return the user's `$SHELL` environment variable, falling back to `"sh"`.
pub fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
}

/// Return the user's `$EDITOR` environment variable, falling back to `"vi"`.
pub fn default_editor() -> String {
    std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string())
}

/// Open a file in the user's `$EDITOR`.
pub fn open_in_editor(path: &std::path::Path) -> anyhow::Result<()> {
    let editor = default_editor();
    let status = Command::new(&editor)
        .arg(path)
        .status()
        .map_err(|e| anyhow::anyhow!("failed to open editor '{}': {}", editor, e))?;
    if status.success() {
        Ok(())
    } else {
        let code = status.code().unwrap_or(-1);
        anyhow::bail!("editor exited with status {}", code);
    }
}

/// Parse a command string into tokens using `shell_words`.
/// Returns an empty vec on parse failure.
pub fn parse_command(cmd: &str) -> Vec<String> {
    shell_words::split(cmd).unwrap_or_default()
}