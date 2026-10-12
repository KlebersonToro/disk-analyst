//! Varredura de diretórios. Por ora só o esqueleto da parte de Windows (FFI); a enumeração
//! chega com T015 e T017.

#[cfg(windows)]
pub(crate) mod windows;
