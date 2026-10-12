//! Varredura de diretórios. Enumeração de uma pasta no Linux (T015) e no Windows (T017), o leitor
//! do buffer de diretório do Windows (T016, puro, compilado em todos os alvos) e a FFI de Windows.
//! O varredor paralelo chega com a T018.

use std::path::PathBuf;

use crate::assemble::DirChunk;

#[cfg(unix)]
#[allow(dead_code, reason = "consumido pelo varredor paralelo da T018")]
pub(crate) mod unix;
pub(crate) mod windows;

/// Resultado de enumerar uma pasta: o chunk e as subpastas a percorrer, cada uma com o id
/// temporário já gravado em `ChunkEntry::subdir`. Compartilhado pelos enumeradores de Linux e de
/// Windows.
#[allow(dead_code, reason = "consumido pelo varredor paralelo da T018")]
pub(crate) struct DirChunkWork {
    pub chunk: DirChunk,
    pub subdirs: Vec<(u32, PathBuf)>,
}
