//! Medição do Tamanho alocado no Windows por `GetFileInformationByHandleEx(FileStandardInfo)`
//! (R10). É o único módulo com `unsafe` da crate; cada bloco tem `// SAFETY:`.
#![allow(unsafe_code)]

use std::io;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO, FileStandardInfo,
    GetFileInformationByHandleEx, OPEN_EXISTING,
};
use windows::core::PCWSTR;

/// Dono de um HANDLE: fecha no Drop, uma única vez. Não é `Copy` nem `Clone`.
struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` veio de um `CreateFileW` bem-sucedido e só é fechado aqui, pelo único
        // dono. Drop roda uma vez, então não há fechamento duplo. O erro do fechamento é ignorado,
        // porque não há o que fazer com ele num Drop.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Tamanho alocado de `path`, em bytes. O caminho é canonicalizado (forma `\\?\`, sem o limite
/// de 260 caracteres) e aberto só com `FILE_READ_ATTRIBUTES`.
pub(crate) fn allocated_size(path: &Path) -> io::Result<u64> {
    let canonical = std::fs::canonicalize(path)?;
    let mut wide: Vec<u16> = canonical.as_os_str().encode_wide().collect();
    wide.push(0);

    // SAFETY: `wide` termina em NUL e continua vivo até o fim desta chamada, então `wide.as_ptr()`
    // aponta para uma string UTF-16 terminada em NUL, válida durante a chamada. Os demais
    // argumentos são valores (flags, `None` para atributos de segurança e para template).
    let raw = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )
    }
    .map_err(|e| io::Error::other(format!("CreateFileW({}): {e}", path.display())))?;
    let handle = OwnedHandle(raw);

    let mut info = FILE_STANDARD_INFO::default();
    // 24 bytes: cabe em u32.
    let info_len = size_of::<FILE_STANDARD_INFO>() as u32;
    // SAFETY: `handle.0` é um HANDLE válido, aberto acima e possuído por `handle`, que o fecha no
    // Drop. O buffer é `info`, um `FILE_STANDARD_INFO` (`repr(C)`, alinhado pelo próprio tipo) com
    // exatamente `info_len` bytes, o tamanho declarado à chamada. A chamada só escreve ali.
    unsafe {
        GetFileInformationByHandleEx(
            handle.0,
            FileStandardInfo,
            (&mut info as *mut FILE_STANDARD_INFO).cast(),
            info_len,
        )
    }
    .map_err(|e| {
        io::Error::other(format!(
            "GetFileInformationByHandleEx({}): {e}",
            path.display()
        ))
    })?;

    u64::try_from(info.AllocationSize).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("AllocationSize negativo em {}", path.display()),
        )
    })
}
