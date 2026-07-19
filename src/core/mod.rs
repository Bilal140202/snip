pub mod snippet;
pub mod snipfile;
pub mod executor;
pub mod explainer;
pub mod fuzzy;
pub mod validator;
pub mod detector;
pub mod stale;
pub mod history;

// Re-exports for ergonomic CLI access: `crate::core::find_snipfile`, etc.
#[allow(unused_imports)]
pub use snippet::{Snippet, SnipFile, VarDef};
#[allow(unused_imports)]
pub use snipfile::{find_snipfile, read_snippets, write_snippets, read_all_snippets, find_snips_dir, list_snips_d_files};