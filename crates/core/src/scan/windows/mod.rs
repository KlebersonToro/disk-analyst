//! Implementação de Windows da varredura. O leitor do buffer (`buffer`) é puro e compila em todos
//! os alvos, para ser testado também no Linux; só o módulo de FFI tem código de sistema.

#[allow(dead_code, reason = "consumido pela enumeração de Windows da T017")]
pub(crate) mod buffer;
#[cfg(windows)]
pub(crate) mod ffi;
