pub mod assemble;
pub mod control;
pub mod hardlinks;
pub mod names;
#[cfg(test)]
pub(crate) mod testgen;
pub mod tree;

/// Versão do crate, usada pelo teste de fumaça do workspace.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
