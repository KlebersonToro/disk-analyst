pub mod names;

/// Versão do crate, usada pelo teste de fumaça do workspace.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
