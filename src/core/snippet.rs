use std::fmt;

use serde::{Deserialize, Serialize};

/// Definition of a template variable within a snippet command.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VarDef {
    /// Variable name (used in `{{name}}` placeholders).
    pub name: String,
    /// Human-readable description shown when prompting.
    pub desc: String,
    /// Optional default value for the variable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Optional list of allowed values for the variable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

impl VarDef {
    pub fn new(name: impl Into<String>, desc: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            desc: desc.into(),
            default: None,
            options: Vec::new(),
        }
    }

    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }

    pub fn with_options(mut self, options: Vec<String>) -> Self {
        self.options = options;
        self
    }
}

impl fmt::Display for VarDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.desc)?;
        if let Some(ref d) = self.default {
            write!(f, " (default: {})", d)?;
        }
        if !self.options.is_empty() {
            write!(f, " [{}]", self.options.join(", "))?;
        }
        Ok(())
    }
}

/// A single command snippet.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snippet {
    /// The command to execute (may contain `{{var}}` placeholders).
    pub cmd: String,
    /// Human-readable description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub desc: String,
    /// Template variable definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vars: Vec<VarDef>,
    /// Optional tags for categorisation and filtering.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Explicit shell override (e.g. "bash", "python").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    /// Working directory relative to the project root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
}

impl Snippet {
    pub fn new(cmd: impl Into<String>) -> Self {
        Self {
            cmd: cmd.into(),
            desc: String::new(),
            vars: Vec::new(),
            tags: Vec::new(),
            shell: None,
            dir: None,
        }
    }

    pub fn with_desc(mut self, desc: impl Into<String>) -> Self {
        self.desc = desc.into();
        self
    }

    pub fn with_vars(mut self, vars: Vec<VarDef>) -> Self {
        self.vars = vars;
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_shell(mut self, shell: impl Into<String>) -> Self {
        self.shell = Some(shell.into());
        self
    }

    pub fn with_dir(mut self, dir: impl Into<String>) -> Self {
        self.dir = Some(dir.into());
        self
    }

    /// Extract all `{{var_name}}` placeholder names from the command string.
    pub fn placeholder_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        let mut search_from = 0;
        while let Some(start) = self.cmd[search_from..].find("{{") {
            let abs_start = search_from + start + 2;
            if let Some(end) = self.cmd[abs_start..].find("}}") {
                let name = self.cmd[abs_start..abs_start + end].trim().to_string();
                if !names.contains(&name) {
                    names.push(name);
                }
                search_from = abs_start + end + 2;
            } else {
                break;
            }
        }
        names
    }

    /// Check whether the snippet has any `{{...}}` placeholders in its command.
    pub fn has_placeholders(&self) -> bool {
        self.cmd.contains("{{") && self.cmd.contains("}}")
    }

    /// Replace all `{{var}}` placeholders with the given values.
    /// Handles spaces inside the braces: `{{ var }}` matches `{{var}}`.
    pub fn substitute(&self, vars: &std::collections::HashMap<String, String>) -> String {
        let mut result = self.cmd.clone();
        for (name, value) in vars {
            // Replace both exact and space-padded forms.
            result = result.replace(&format!("{{{{{}}}}}", name), value);
            result = result.replace(&format!("{{{{ {} }}}}", name), value);
        }
        result
    }
}

impl fmt::Display for Snippet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.desc.is_empty() {
            write!(f, "{}", self.cmd)?;
        } else {
            write!(f, "{} — {}", self.desc, self.cmd)?;
        }
        if !self.vars.is_empty() {
            write!(f, "\n  vars: ")?;
            for (i, v) in self.vars.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", v)?;
            }
        }
        if !self.tags.is_empty() {
            write!(f, "\n  tags: {}", self.tags.join(", "))?;
        }
        if let Some(ref shell) = self.shell {
            write!(f, "\n  shell: {}", shell)?;
        }
        if let Some(ref dir) = self.dir {
            write!(f, "\n  dir: {}", dir)?;
        }
        Ok(())
    }
}

/// The top-level `.snips` file structure.
///
/// Stores snippets as an ordered flat list of `(fully-qualified-key, Snippet)`.
/// Keys use dot-notation for nesting: `"build"`, `"build.release"`, `"deploy.staging"`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SnipFile {
    /// Ordered entries: (fully-qualified key, snippet).
    entries: Vec<(String, Snippet)>,
}

impl SnipFile {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a snippet under the given fully-qualified key.
    /// If a key already exists it is replaced.
    pub fn insert(&mut self, key: impl Into<String>, snippet: Snippet) {
        let key = key.into();
        if let Some(pos) = self.entries.iter().position(|(k, _)| k == &key) {
            self.entries[pos] = (key, snippet);
        } else {
            self.entries.push((key, snippet));
        }
    }

    /// Remove a snippet by fully-qualified key. Returns the removed snippet.
    pub fn remove(&mut self, key: &str) -> Option<Snippet> {
        if let Some(pos) = self.entries.iter().position(|(k, _)| k == key) {
            Some(self.entries.remove(pos).1)
        } else {
            None
        }
    }

    /// Get a snippet by fully-qualified key.
    pub fn get(&self, key: &str) -> Option<&Snippet> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, s)| s)
    }

    /// Iterate over all entries as `&(String, Snippet)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = &(String, Snippet)> {
        self.entries.iter()
    }

    /// Collect all keys.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.entries.iter().map(|(k, _)| k)
    }

    /// Number of snippets.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the file is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    // ── TOML conversion ────────────────────────────────────────────

    /// Convert this flat `SnipFile` into a nested `toml::Value` table suitable
    /// for writing. Keys like `"build.release"` become nested tables.
    pub fn to_toml_value(&self) -> toml::Value {
        let mut root: toml::Table = toml::Table::new();

        for (key, snippet) in &self.entries {
            let parts: Vec<&str> = key.split('.').collect();

            // Navigate/create nested tables, insert the snippet at the leaf.
            let mut current = &mut root;
            for (i, part) in parts.iter().enumerate() {
                let is_last = i == parts.len() - 1;
                if is_last {
                    let snippet_value = toml::Value::try_from(snippet)
                        .expect("Snippet serialization to TOML should never fail");
                    current.insert(part.to_string(), snippet_value);
                    break;
                }
                let entry = current
                    .entry(part.to_string())
                    .or_insert_with(|| toml::Value::Table(toml::Table::new()));
                current = entry
                    .as_table_mut()
                    .expect("intermediate entry should be a table");
            }
        }

        toml::Value::Table(root)
    }

    /// Parse a `toml::Value` (the entire document) into a `SnipFile`.
    ///
    /// Recursively walks nested tables, treating every table that contains a
    /// `cmd` key as a leaf snippet.
    pub fn from_toml_value(value: &toml::Value) -> anyhow::Result<Self> {
        let mut entries = Vec::new();
        let table = value
            .as_table()
            .ok_or_else(|| anyhow::anyhow!("top-level .snips value must be a table"))?;
        walk_table(table, String::new(), &mut entries)?;
        Ok(Self { entries })
    }
}

/// Recursively walk a TOML table, collecting snippet entries.
///
/// At each level, if the table contains a `cmd` key, it is treated as a
/// snippet. Then we recurse into any sub-tables (entries whose values are
/// themselves tables).
fn walk_table(
    table: &toml::Table,
    prefix: String,
    out: &mut Vec<(String, Snippet)>,
) -> anyhow::Result<()> {
    // If this table has a `cmd` key, it represents a snippet.
    if table.contains_key("cmd") {
        let snippet: Snippet = table
            .clone()
            .try_into()
            .map_err(|e| anyhow::anyhow!("invalid snippet [{}]: {}", prefix, e))?;
        out.push((prefix.clone(), snippet));
    }

    // Recurse into sub-tables.
    for (key, value) in table {
        if let Some(sub_table) = value.as_table() {
            let child_fqn = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", prefix, key)
            };
            walk_table(sub_table, child_fqn, out)?;
        }
    }

    Ok(())
}

impl fmt::Display for SnipFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (key, snippet) in &self.entries {
            writeln!(f, "[{}]", key)?;
            writeln!(f, "  {}", snippet)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_placeholder_extraction() {
        let s = Snippet::new("deploy --env {{env}} --region {{ region }}");
        let names = s.placeholder_names();
        assert_eq!(names, vec!["env", "region"]);
    }

    #[test]
    fn snippet_no_duplicates_in_placeholders() {
        let s = Snippet::new("echo {{x}} and {{ x }} and {{x}}");
        let names = s.placeholder_names();
        assert_eq!(names, vec!["x"]);
    }

    #[test]
    fn snippet_display_with_desc() {
        let s = Snippet::new("cargo build").with_desc("Build the project");
        let text = format!("{}", s);
        assert!(text.contains("Build the project"));
        assert!(text.contains("cargo build"));
    }

    #[test]
    fn snippet_substitute() {
        let s = Snippet::new("deploy --env {{env}} --region {{region}}");
        let mut vars = std::collections::HashMap::new();
        vars.insert("env".to_string(), "staging".to_string());
        vars.insert("region".to_string(), "us-east-1".to_string());
        let result = s.substitute(&vars);
        assert_eq!(result, "deploy --env staging --region us-east-1");
    }

    #[test]
    fn snippet_has_placeholders() {
        assert!(Snippet::new("echo {{x}}").has_placeholders());
        assert!(!Snippet::new("echo hello").has_placeholders());
        assert!(!Snippet::new("echo {{x").has_placeholders());
    }

    #[test]
    fn snipfile_insert_and_get() {
        let mut file = SnipFile::new();
        file.insert("build", Snippet::new("cargo build").with_desc("Build"));
        file.insert("test", Snippet::new("cargo test"));
        assert_eq!(file.len(), 2);
        assert!(file.get("build").is_some());
        assert!(file.get("missing").is_none());
    }

    #[test]
    fn snipfile_remove() {
        let mut file = SnipFile::new();
        file.insert("build", Snippet::new("cargo build"));
        let removed = file.remove("build");
        assert!(removed.is_some());
        assert!(file.is_empty());
        assert!(file.remove("build").is_none());
    }

    #[test]
    fn snipfile_insert_replaces() {
        let mut file = SnipFile::new();
        file.insert("build", Snippet::new("cargo build"));
        file.insert("build", Snippet::new("cargo build --release"));
        assert_eq!(file.len(), 1);
        assert_eq!(file.get("build").unwrap().cmd, "cargo build --release");
    }

    #[test]
    fn to_toml_value_single_key() {
        let mut file = SnipFile::new();
        file.insert(
            "build",
            Snippet::new("cargo build").with_desc("Build the project"),
        );
        let val = file.to_toml_value();
        let toml_str = toml::to_string_pretty(&val).unwrap();
        assert!(toml_str.contains("[build]"));
        assert!(toml_str.contains("cmd = \"cargo build\""));
    }

    #[test]
    fn to_toml_value_nested_key() {
        let mut file = SnipFile::new();
        file.insert(
            "build.release",
            Snippet::new("cargo build --release"),
        );
        let val = file.to_toml_value();
        let toml_str = toml::to_string_pretty(&val).unwrap();
        assert!(toml_str.contains("[build.release]"));
        assert!(toml_str.contains("cmd = \"cargo build --release\""));
    }

    #[test]
    fn from_toml_value_roundtrip() {
        let toml_str = r#"
[build]
cmd = "cargo build"
desc = "Build the project"

[build.release]
cmd = "cargo build --release"
desc = "Build in release mode"

[deploy.staging]
cmd = "deploy --env staging"
desc = "Deploy to staging"
"#;
        let value: toml::Value = toml_str.parse().unwrap();
        let file = SnipFile::from_toml_value(&value).unwrap();

        assert_eq!(file.len(), 3);
        assert_eq!(file.get("build").unwrap().cmd, "cargo build");
        assert_eq!(file.get("build.release").unwrap().cmd, "cargo build --release");
        assert_eq!(file.get("deploy.staging").unwrap().cmd, "deploy --env staging");
    }

    #[test]
    fn to_toml_then_from_toml_roundtrip() {
        let mut file = SnipFile::new();
        file.insert(
            "build",
            Snippet::new("cargo build").with_desc("Build the project"),
        );
        file.insert(
            "build.release",
            Snippet::new("cargo build --release").with_desc("Release build"),
        );
        file.insert("test", Snippet::new("cargo test"));

        let val = file.to_toml_value();
        let toml_str = toml::to_string_pretty(&val).unwrap();
        let val2: toml::Value = toml_str.parse().unwrap();
        let file2 = SnipFile::from_toml_value(&val2).unwrap();

        assert_eq!(file.len(), file2.len());
        for (k1, s1) in &file.entries {
            let s2 = file2.get(k1).unwrap();
            assert_eq!(s1.cmd, s2.cmd);
            assert_eq!(s1.desc, s2.desc);
        }
    }

    #[test]
    fn from_toml_value_with_vars() {
        let toml_str = r#"
[deploy]
cmd = "deploy --env {{env}} --region {{region}}"
desc = "Deploy to environment"
vars = [{ name = "env", desc = "Target environment", options = ["staging", "production"] }]
"#;
        let value: toml::Value = toml_str.parse().unwrap();
        let file = SnipFile::from_toml_value(&value).unwrap();

        assert_eq!(file.len(), 1);
        let deploy = file.get("deploy").unwrap();
        assert_eq!(deploy.vars.len(), 1);
        assert_eq!(deploy.vars[0].name, "env");
        assert_eq!(deploy.vars[0].options, vec!["staging", "production"]);
    }
}