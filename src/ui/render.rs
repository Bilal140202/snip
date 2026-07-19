//! Rendering utilities for the terminal UI.

use colored::Colorize;

/// Render a header line.
pub fn header(text: &str) {
    println!("{}", text.bold().underline());
}

/// Render a snippet entry for listing.
pub fn snippet_entry(key: &str, cmd: &str, desc: &str, tags: &[String]) {
    println!("{}", key.green().bold());
    println!("  {}", cmd);
    if !desc.is_empty() {
        println!("  {}", desc.dimmed());
    }
    if !tags.is_empty() {
        println!("  tags: {}", tags.join(", ").cyan());
    }
}

/// Render a key-value pair.
pub fn kv(key: &str, value: &str) {
    println!("  {}: {}", key.dimmed(), value);
}