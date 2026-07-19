pub mod snippet;
pub mod snipfile;
pub mod executor;
pub mod fuzzy;
pub mod validator;
pub mod detector;

// Re-exports for ergonomic CLI access: `crate::core::find_snipfile`, etc.
#[allow(unused_imports)]
pub use snippet::{Snippet, SnipFile, VarDef};
#[allow(unused_imports)]
pub use snipfile::{find_snipfile, read_snippets, write_snippets};