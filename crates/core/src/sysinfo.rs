//! Medição de memória do processo atual (FR-018): o pico usado pelo `scan_totals` e pelo
//! `--stats`. Função segura por plataforma; a chamada ao sistema fica em `scan/windows/ffi.rs`.

use std::io;

/// Pico de memória residente do processo atual, em bytes.
///
/// Linux: `VmHWM` de `/proc/self/status`. Windows: `PeakWorkingSetSize` de
/// `GetProcessMemoryInfo`. Outras plataformas: `Unsupported`.
pub fn peak_memory_bytes() -> io::Result<u64> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/self/status")?;
        parse_vmhwm(&text)
    }
    #[cfg(windows)]
    {
        crate::scan::windows::ffi::peak_working_set()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "pico de memória indisponível nesta plataforma",
        ))
    }
}

/// Lê `VmHWM:` (em kB) de um texto de `/proc/self/status` e devolve bytes.
#[cfg(any(test, target_os = "linux"))]
fn parse_vmhwm(status: &str) -> io::Result<u64> {
    let invalid = |motivo: &str| io::Error::new(io::ErrorKind::InvalidData, motivo.to_owned());
    let line = status
        .lines()
        .find(|l| l.starts_with("VmHWM:"))
        .ok_or_else(|| invalid("linha VmHWM ausente em /proc/self/status"))?;
    let kb: u64 = line["VmHWM:".len()..]
        .split_whitespace()
        .next()
        .ok_or_else(|| invalid("valor de VmHWM ausente"))?
        .parse()
        .map_err(|_| invalid("valor de VmHWM malformado"))?;
    kb.checked_mul(1024)
        .ok_or_else(|| invalid("VmHWM estoura u64 em bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL_STATUS: &str =
        "Name:\tdisk-analyst\nVmPeak:\t  200000 kB\nVmHWM:\t  123456 kB\nVmRSS:\t  100000 kB\n";

    #[test]
    fn vmhwm_is_read_in_bytes() {
        assert_eq!(parse_vmhwm(REAL_STATUS).unwrap(), 123_456 * 1024);
    }

    #[test]
    fn missing_vmhwm_line_is_an_error() {
        let err = parse_vmhwm("Name:\tx\nVmRSS:\t  1 kB\n").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn malformed_vmhwm_number_is_an_error() {
        let err = parse_vmhwm("VmHWM:\t  abc kB\n").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
