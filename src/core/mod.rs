pub mod detector;
pub mod executor;
pub mod explainer;
pub mod fuzzy;
pub mod history;
pub mod nl;
pub mod snipfile;
pub mod snippet;
pub mod stale;
pub mod validator;

// Re-exports for ergonomic CLI access: `crate::core::find_snipfile`, etc.
#[allow(unused_imports)]
pub use snipfile::{
    find_snipfile, find_snips_dir, list_snips_d_files, read_all_snippets, read_snippets,
    write_snippets,
};
#[allow(unused_imports)]
pub use snippet::{SnipFile, Snippet, VarDef};
