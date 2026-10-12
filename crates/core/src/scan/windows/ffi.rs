//! Chamadas de sistema do Windows para a varredura (R2, Princípio IV). Único módulo com `unsafe`
//! do núcleo; cada bloco tem `// SAFETY:`.
#![allow(unsafe_code)]

use std::io;
use std::mem::size_of;

use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Threading::GetCurrentProcess;

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
