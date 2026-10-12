//! Oráculo do Tamanho alocado: o que o sistema de arquivos reservou para um arquivo, medido
//! depois que o arquivo foi criado e fechado (R10). Não usa código do núcleo.
//!
//! Este módulo é todo seguro. No Windows, a chamada ao sistema fica em `ffi_windows`.

use std::io;
use std::path::Path;

/// Tamanho alocado de um arquivo, em bytes.
///
/// Unix: `st_blocks` em unidades de 512 bytes, multiplicado por 512. Windows: `AllocationSize`
/// de `FileStandardInfo`, pelo handle aberto só com `FILE_READ_ATTRIBUTES`.
#[cfg(unix)]
pub fn allocated_size(path: &Path) -> io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    let blocks = std::fs::metadata(path)?.blocks();
    blocks.checked_mul(512).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("alocado estoura u64 em {}", path.display()),
        )
    })
}

#[cfg(windows)]
pub fn allocated_size(path: &Path) -> io::Result<u64> {
    crate::ffi_windows::allocated_size(path)
}

/// Plataforma sem medição: erro explícito, nunca um valor inventado.
#[cfg(not(any(unix, windows)))]
pub fn allocated_size(path: &Path) -> io::Result<u64> {
    let _ = path;
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "medição de tamanho alocado indisponível nesta plataforma",
    ))
}
