pub mod assemble;
pub mod control;
pub mod names;
pub mod tree;

/// Versão do crate, usada pelo teste de fumaça do workspace.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
