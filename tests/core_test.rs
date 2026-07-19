//! Integration tests for the snip core engine.
//!
//! Covers: TOML parsing, file I/O, add/remove, fuzzy matching, shell escaping,
//! variable substitution, and .snips file discovery from subdirectories.

use std::collections::HashMap;
use std::fs;

use snip::core::fuzzy;
use snip::core::snippet::{SnipFile, Snippet, VarDef};
use snip::core::snipfile;
use snip::core::validator;
use snip::utils::shell;

// ── TOML Parsing ───────────────────────────────────────────────────

#[test]
fn parse_valid_snips_file() {
    let toml_str = r#"
[test]
cmd = "cargo test"
desc = "Run tests"

[test.coverage]
cmd = "cargo test --coverage"
desc = "Run tests with coverage"

[build]
cmd = "cargo build --release"
"#;
    let value: toml::Value = toml_str.parse().unwrap();
    let file = SnipFile::from_toml_value(&value).unwrap();

    assert_eq!(file.len(), 3);
    assert_eq!(file.get("test").unwrap().cmd, "cargo test");
    assert_eq!(
        file.get("test.coverage").unwrap().cmd,
        "cargo test --coverage"
    );
    assert_eq!(file.get("build").unwrap().cmd, "cargo build --release");
}

#[test]
fn parse_snips_file_with_vars() {
    let toml_str = r#"
[deploy]
cmd = "kubectl apply -f {{file}} --namespace {{ns}}"
desc = "Deploy to kubernetes"
vars = [
    { name = "file", desc = "YAML file to apply" },
    { name = "ns", desc = "Target namespace", options = ["dev", "staging", "prod"] }
]
"#;
    let value: toml::Value = toml_str.parse().unwrap();
    let file = SnipFile::from_toml_value(&value).unwrap();

    assert_eq!(file.len(), 1);
    let deploy = file.get("deploy").unwrap();
    assert_eq!(deploy.vars.len(), 2);
    assert_eq!(deploy.vars[0].name, "file");
    assert_eq!(deploy.vars[1].name, "ns");
    assert_eq!(deploy.vars[1].options, vec!["dev", "staging", "prod"]);
}

#[test]
fn parse_empty_snips_file() {
    let value: toml::Value = toml::from_str("").unwrap();
    let file = SnipFile::from_toml_value(&value).unwrap();
    assert!(file.is_empty());
}

// ── Write & Read Back ─────────────────────────────────────────────

#[test]
fn write_and_read_back() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(".snips");

    let mut file = SnipFile::new();
    file.insert(
        "build",
        Snippet::new("cargo build --release").with_desc("Release build"),
    );
    file.insert("test", Snippet::new("cargo test --all"));

    snipfile::write_snippets(&path, &file).unwrap();
    let read_back = snipfile::read_snippets(&path).unwrap();

    assert_eq!(read_back.len(), 2);
    assert_eq!(
        read_back.get("build").unwrap().cmd,
        "cargo build --release"
    );
    assert_eq!(read_back.get("test").unwrap().cmd, "cargo test --all");
    assert_eq!(
        read_back.get("build").unwrap().desc,
        "Release build"
    );
}

#[test]
fn write_preserves_nesting() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(".snips");

    let mut file = SnipFile::new();
    file.insert("npm.build", Snippet::new("npm run build"));
    file.insert("npm.test", Snippet::new("npm test"));

    snipfile::write_snippets(&path, &file).unwrap();

    // Verify the TOML uses nested headers.
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("[npm.build]"));
    assert!(content.contains("[npm.test]"));

    // Verify it reads back correctly.
    let read_back = snipfile::read_snippets(&path).unwrap();
    assert_eq!(read_back.len(), 2);
    assert_eq!(read_back.get("npm.build").unwrap().cmd, "npm run build");
}

// ── Add / Remove ──────────────────────────────────────────────────

#[test]
fn add_snippet_to_existing_file() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(".snips");

    // Start with one snippet.
    let mut file = SnipFile::new();
    file.insert("build", Snippet::new("cargo build"));
    snipfile::write_snippets(&path, &file).unwrap();

    // Add another.
    snipfile::add_snippet(&path, "test", "", Snippet::new("cargo test")).unwrap();

    let read_back = snipfile::read_snippets(&path).unwrap();
    assert_eq!(read_back.len(), 2);
    assert!(read_back.get("test").is_some());
}

#[test]
fn add_snippet_creates_new_file() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(".snips");

    assert!(!path.exists());
    snipfile::add_snippet(&path, "hello", "", Snippet::new("echo hi")).unwrap();
    assert!(path.exists());

    let file = snipfile::read_snippets(&path).unwrap();
    assert_eq!(file.len(), 1);
    assert_eq!(file.get("hello").unwrap().cmd, "echo hi");
}

#[test]
fn remove_snippet() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(".snips");

    let mut file = SnipFile::new();
    file.insert("keep", Snippet::new("echo keep"));
    file.insert("remove-me", Snippet::new("echo gone"));
    snipfile::write_snippets(&path, &file).unwrap();

    let removed = snipfile::remove_snippet(&path, "remove-me", "").unwrap();
    assert_eq!(removed.cmd, "echo gone");

    let read_back = snipfile::read_snippets(&path).unwrap();
    assert_eq!(read_back.len(), 1);
    assert!(read_back.get("keep").is_some());
    assert!(read_back.get("remove-me").is_none());
}

#[test]
fn remove_snippet_not_found_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(".snips");

    let file = SnipFile::new();
    snipfile::write_snippets(&path, &file).unwrap();

    let result = snipfile::remove_snippet(&path, "nonexistent", "");
    assert!(result.is_err());
}

// ── Fuzzy Matching ────────────────────────────────────────────────

#[test]
fn fuzzy_exact_match() {
    let keys: Vec<String> = vec![
        "build".into(),
        "build.release".into(),
        "test".into(),
        "deploy".into(),
    ];
    let results = fuzzy::fuzzy_match("build", &keys);
    assert_eq!(results[0].key, "build");
    assert!(results[0].score > 0);
}

#[test]
fn fuzzy_prefix_match_ranks_higher() {
    let keys: Vec<String> = vec![
        "build-release".into(),
        "test-release".into(),
        "build-debug".into(),
        "release".into(),
    ];
    let results = fuzzy::fuzzy_match("build", &keys);
    // "build-release" and "build-debug" should rank above "release"
    assert_eq!(results[0].key, "build-release");
    assert_eq!(results[1].key, "build-debug");
}

#[test]
fn fuzzy_approximate_match() {
    let keys: Vec<String> = vec![
        "build-release".into(),
        "test-release".into(),
        "deploy-staging".into(),
    ];
    let results = fuzzy::fuzzy_match("bldrls", &keys);
    assert!(!results.is_empty());
    assert_eq!(results[0].key, "build-release");
}

#[test]
fn fuzzy_no_match_returns_empty() {
    let keys: Vec<String> = vec!["abc".into(), "def".into()];
    let results = fuzzy::fuzzy_match("xyz", &keys);
    assert!(results.is_empty());
}

#[test]
fn fuzzy_empty_query_returns_all_or_none() {
    // The fuzzy matcher may or may not return results for an empty query
    // depending on the implementation. Verify it doesn't panic.
    let keys: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
    let _results = fuzzy::fuzzy_match("", &keys);
    // The important thing is it doesn't crash.
}

#[test]
fn fuzzy_case_insensitive() {
    let keys: Vec<String> = vec!["Build-Release".into(), "test".into()];
    let results = fuzzy::fuzzy_match("build", &keys);
    assert_eq!(results[0].key, "Build-Release");
}

#[test]
fn fuzzy_best_single_match() {
    let keys: Vec<String> = vec!["hello".into(), "world".into()];
    let result = fuzzy::fuzzy_best("hel", &keys);
    assert_eq!(result, Some("hello".to_string()));
}

#[test]
fn fuzzy_best_no_match() {
    let keys: Vec<String> = vec!["abc".into(), "def".into()];
    let result = fuzzy::fuzzy_best("xyz", &keys);
    assert!(result.is_none());
}

// ── Shell Escaping ────────────────────────────────────────────────

#[test]
fn shell_parse_simple() {
    let tokens = shell::parse_command("echo hello world");
    assert_eq!(tokens, vec!["echo", "hello", "world"]);
}

#[test]
fn shell_parse_single_quotes() {
    let tokens = shell::parse_command("echo 'hello world'");
    assert_eq!(tokens, vec!["echo", "hello world"]);
}

#[test]
fn shell_parse_double_quotes() {
    let tokens = shell::parse_command(r#"echo "hello world""#);
    assert_eq!(tokens, vec!["echo", "hello world"]);
}

#[test]
fn shell_parse_pipes() {
    let tokens = shell::parse_command("echo hello | grep hello | wc -l");
    assert_eq!(tokens, vec!["echo", "hello", "|", "grep", "hello", "|", "wc", "-l"]);
}

#[test]
fn shell_parse_dollar_sign() {
    let tokens = shell::parse_command("echo $HOME");
    assert_eq!(tokens, vec!["echo", "$HOME"]);
}

#[test]
fn shell_parse_empty() {
    let tokens = shell::parse_command("");
    assert!(tokens.is_empty());
}

#[test]
fn shell_parse_backticks() {
    // shell-words may or may not expand backticks, but should not panic.
    let _tokens = shell::parse_command("echo `date`");
}

#[test]
fn shell_detect_shell_returns_something() {
    let shell = shell::default_shell();
    assert!(!shell.is_empty());
}

// ── Variable Substitution ─────────────────────────────────────────

#[test]
fn substitute_single_var() {
    let s = Snippet::new("deploy --env {{env}}");
    let mut vars = HashMap::new();
    vars.insert("env".to_string(), "staging".to_string());
    assert_eq!(s.substitute(&vars), "deploy --env staging");
}

#[test]
fn substitute_multiple_vars() {
    let s = Snippet::new("deploy --env {{env}} --region {{region}}");
    let mut vars = HashMap::new();
    vars.insert("env".to_string(), "prod".to_string());
    vars.insert("region".to_string(), "us-east-1".to_string());
    assert_eq!(
        s.substitute(&vars),
        "deploy --env prod --region us-east-1"
    );
}

#[test]
fn substitute_no_vars() {
    let s = Snippet::new("echo hello");
    let vars = HashMap::new();
    assert_eq!(s.substitute(&vars), "echo hello");
}

#[test]
fn substitute_partial() {
    let s = Snippet::new("echo {{greeting}} from {{user}}");
    let mut vars = HashMap::new();
    vars.insert("greeting".to_string(), "hi".to_string());
    // user not provided — should remain as placeholder
    assert_eq!(s.substitute(&vars), "echo hi from {{user}}");
}

#[test]
fn substitute_with_spaces_in_placeholder() {
    let s = Snippet::new("deploy --env {{ env }}");
    let mut vars = HashMap::new();
    vars.insert("env".to_string(), "staging".to_string());
    assert_eq!(s.substitute(&vars), "deploy --env staging");
}

// ── .snips Discovery from Subdirectory ────────────────────────────

#[test]
fn find_snipfile_from_nested_subdir() {
    let tmp = tempfile::tempdir().unwrap();
    let snipfile = tmp.path().join(".snips");
    fs::write(&snipfile, "[test]\ncmd = 'echo test'\n").unwrap();

    let nested = tmp.path().join("a/b/c/d");
    fs::create_dir_all(&nested).unwrap();

    let found = snipfile::find_snipfile(Some(&nested)).unwrap().unwrap();
    assert_eq!(found, snipfile);
}

#[test]
fn find_snipfile_not_found() {
    let tmp = tempfile::tempdir().unwrap();
    let found = snipfile::find_snipfile(Some(tmp.path())).unwrap();
    assert!(found.is_none());
}

#[test]
fn find_snipfile_in_cwd() {
    let tmp = tempfile::tempdir().unwrap();
    let snipfile = tmp.path().join(".snips");
    fs::write(&snipfile, "").unwrap();

    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let found = snipfile::find_snipfile(None).unwrap().unwrap();
    assert_eq!(found, snipfile);

    std::env::set_current_dir(&original).unwrap();
}

// ── Validation ────────────────────────────────────────────────────

#[test]
fn validation_empty_command() {
    let s = Snippet::new("");
    let issues = validator::validate(&{
        let mut f = SnipFile::new();
        f.insert("bad", s);
        f
    });
    assert!(issues.iter().any(|i| i.severity == validator::Severity::Error));
}

#[test]
fn validation_undefined_variable() {
    let s = Snippet::new("deploy --env {{env}}");
    let mut f = SnipFile::new();
    f.insert("tpl", s);
    let issues = validator::validate(&f);
    assert!(issues.iter().any(|i| i.message.contains("undefined variable")));
}

#[test]
fn validation_clean_snippet() {
    let s = Snippet::new("echo hello");
    let mut f = SnipFile::new();
    f.insert("good", s);
    let issues = validator::validate(&f);
    assert!(issues.is_empty());
}

// ── Roundtrip ─────────────────────────────────────────────────────

#[test]
fn full_roundtrip_write_read_compare() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(".snips");

    let mut original = SnipFile::new();
    original.insert(
        "build",
        Snippet::new("cargo build").with_desc("Build the project"),
    );
    original.insert(
        "build.release",
        Snippet::new("cargo build --release").with_desc("Release build"),
    );
    original.insert(
        "deploy",
        Snippet::new("deploy --env {{env}}").with_vars(vec![VarDef::new(
            "env",
            "Environment",
        )
        .with_options(vec!["staging".to_string(), "production".to_string()])]),
    );
    original.insert(
        "lint",
        Snippet::new("clippy --all-targets").with_tags(vec!["ci".to_string()]),
    );

    snipfile::write_snippets(&path, &original).unwrap();
    let read_back = snipfile::read_snippets(&path).unwrap();

    assert_eq!(original.len(), read_back.len());
    for (key, snippet) in original.iter() {
        let other = read_back.get(key).unwrap();
        assert_eq!(snippet.cmd, other.cmd);
        assert_eq!(snippet.desc, other.desc);
        assert_eq!(snippet.vars.len(), other.vars.len());
    }
}