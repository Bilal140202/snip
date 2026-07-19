//! Interactive prompting for variable values.

use std::io::{self, Write};

use colored::Colorize;

/// Prompt the user for a value with an optional default.
pub fn prompt_var(name: &str, desc: &str, default: Option<&str>, options: &[String]) -> String {
    let mut stdout = io::stdout().lock();

    if !options.is_empty() {
        let _ = write!(
            stdout,
            "  {} {} [{}]: ",
            name.bold(),
            desc.dimmed(),
            options.join("/")
        );
    } else if let Some(d) = default {
        let _ = write!(stdout, "  {} {} [{}]: ", name.bold(), desc.dimmed(), d);
    } else {
        let _ = write!(stdout, "  {} {}: ", name.bold(), desc.dimmed());
    }
    let _ = stdout.flush();

    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    let input = input.trim();

    if input.is_empty() {
        default
            .map(|d| d.to_string())
            .or_else(|| options.first().cloned())
            .unwrap_or_default()
    } else {
        input.to_string()
    }
}