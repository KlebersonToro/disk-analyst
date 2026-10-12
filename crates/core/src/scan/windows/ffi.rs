//! Chamadas de sistema do Windows para a varredura (R2, Princípio IV). Único módulo com `unsafe`
//! do núcleo; cada bloco tem `// SAFETY:`. Por fora, funções pequenas e seguras.
#![allow(unsafe_code)]

use std::io;
use std::mem::{size_of, size_of_val};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileIdBothDirectoryInfo,
    FileIdBothDirectoryRestartInfo, GetFileInformationByHandleEx, OPEN_EXISTING,
};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::core::{HRESULT, PCWSTR};

/// Converte um erro do windows-rs em `io::Error`, preservando o tipo (`NotFound` etc.) quando o
/// HRESULT vem de um código Win32 (`0x8007xxxx`), e acrescentando o contexto à mensagem.
fn to_io_error(error: &windows::core::Error, context: String) -> io::Error {
    let hresult = u32::from_le_bytes(error.code().0.to_le_bytes());
    let kind = if hresult >> 16 == 0x8007 {
        let win32 = u16::try_from(hresult & 0xFFFF).unwrap_or(u16::MAX);
        io::Error::from_raw_os_error(i32::from(win32)).kind()
    } else {
        io::ErrorKind::Other
    };
    io::Error::new(kind, format!("{context}: {error}"))
}

/// Handle de uma pasta aberta para listagem. Fecha no Drop, uma única vez; não é `Copy` nem
/// `Clone`.
pub(crate) struct DirHandle(HANDLE);

impl Drop for DirHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` veio de um `CreateFileW` bem-sucedido em `open_dir` e só é fechado
        // aqui, pelo único dono. Drop roda uma vez, então não há fechamento duplo. O erro do
        // fechamento é ignorado: não há o que fazer com ele num Drop.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Abre `path` para listar o conteúdo. Com `follow_link == false` abre o próprio reparse point
/// (`FILE_FLAG_OPEN_REPARSE_POINT`) em vez do alvo. O caminho deve ser absoluto (a T018 usa a
/// forma verbatim `\\?\`).
pub(crate) fn open_dir(path: &Path, follow_link: bool) -> io::Result<DirHandle> {
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("caminho com NUL: {}", path.display()),
        ));
    }
    wide.push(0);

    let mut flags = FILE_FLAG_BACKUP_SEMANTICS;
    if !follow_link {
        flags |= FILE_FLAG_OPEN_REPARSE_POINT;
    }

    // SAFETY: `wide` termina em NUL (e não tem NUL no meio, conferido acima) e continua vivo até
    // o fim desta chamada, então `wide.as_ptr()` aponta para uma string UTF-16 terminada em NUL,
    // válida durante a chamada. Os demais argumentos são valores (acesso, compartilhamento,
    // disposição, flags) e `None` para atributos de segurança e para template.
    let raw = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_LIST_DIRECTORY.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            flags,
            None,
        )
    }
    .map_err(|e| to_io_error(&e, format!("CreateFileW({})", path.display())))?;
    Ok(DirHandle(raw))
}

/// Lê o próximo lote de registros `FILE_ID_BOTH_DIR_INFO` em `buf`. `restart == true` recomeça a
/// listagem. Devolve `false` quando não há mais arquivos (`ERROR_NO_MORE_FILES`); outros erros
/// são `Err`. O buffer é de `u64` para ficar alinhado a 8 bytes, como a API exige.
pub(crate) fn read_batch(dir: &DirHandle, buf: &mut [u64], restart: bool) -> io::Result<bool> {
    let class = if restart {
        FileIdBothDirectoryRestartInfo
    } else {
        FileIdBothDirectoryInfo
    };
    let len = u32::try_from(size_of_val(buf)).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "buffer maior que u32::MAX bytes",
        )
    })?;

    // SAFETY: `dir.0` é um HANDLE válido, aberto por `open_dir` e possuído por `dir`, que o fecha
    // no Drop. `buf.as_mut_ptr()` aponta para `len` bytes graváveis (`len` é exatamente
    // `size_of_val(buf)`), alinhados a 8 por serem `u64`, e o empréstimo exclusivo `&mut` garante
    // que ninguém mais lê ou escreve ali durante a chamada.
    let result =
        unsafe { GetFileInformationByHandleEx(dir.0, class, buf.as_mut_ptr().cast(), len) };
    match result {
        Ok(()) => Ok(true),
        Err(e) if e.code() == HRESULT::from_win32(ERROR_NO_MORE_FILES.0) => Ok(false),
        Err(e) => Err(to_io_error(&e, "GetFileInformationByHandleEx".to_owned())),
    }
}

/// Os mesmos bytes de `words`, como `&[u8]`, para o leitor de `buffer`.
pub(crate) fn as_bytes(words: &[u64]) -> &[u8] {
    // SAFETY: `u8` tem alinhamento 1, então qualquer ponteiro de `u64` serve. A região é a mesma
    // de `words` (`size_of_val(words)` bytes, ≤ isize::MAX porque `words` já existe), válida para
    // leitura e imutável enquanto o empréstimo `&[u8]` (de mesma duração) existir.
    unsafe { std::slice::from_raw_parts(words.as_ptr().cast::<u8>(), size_of_val(words)) }
}

/// Pico do conjunto de trabalho do processo atual, em bytes.
pub(crate) fn peak_working_set() -> io::Result<u64> {
    let mut counters = PROCESS_MEMORY_COUNTERS::default();
    // `cb` precisa ser o tamanho da struct antes da chamada; a API o confere.
    let cb = u32::try_from(size_of::<PROCESS_MEMORY_COUNTERS>())
        .map_err(|_| io::Error::other("PROCESS_MEMORY_COUNTERS maior que u32"))?;
    counters.cb = cb;

    // SAFETY: `GetCurrentProcess` devolve o pseudo-handle do processo atual (-1). Ele não é um
    // handle aberto: não se fecha com CloseHandle, não precisa de liberação e sempre vale para
    // este processo. Não há outro argumento.
    let process = unsafe { GetCurrentProcess() };

    // SAFETY: `process` é o pseudo-handle do processo atual, válido para consulta. `&mut counters`
    // aponta para um PROCESS_MEMORY_COUNTERS (repr(C)) inicializado com `cb` igual ao tamanho
    // exato dessa struct, então a chamada escreve somente dentro dela.
    unsafe { GetProcessMemoryInfo(process, &mut counters, cb) }
        .map_err(|e| io::Error::other(format!("GetProcessMemoryInfo: {e}")))?;

    u64::try_from(counters.PeakWorkingSetSize).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "pico de memória não cabe em u64",
        )
    })
}
