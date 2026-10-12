//! Varre um caminho e imprime as métricas da linha de base (T019, FR-018), uma por linha, no
//! formato `chave: valor`, fácil de extrair:
//!
//! ```text
//! itens: <número de Nodes da árvore>
//! tempo_ms: <duração da varredura, em ms, com 1 casa>
//! pico_bytes: <pico de memória do processo, em bytes>
//! bytes_por_item: <pico_bytes / itens, inteiro>
//! ```
//!
//! Uso: `scan_totals <caminho>`. Erro: mensagem em português no stderr e código de saída 1.

use std::path::Path;
use std::process::ExitCode;

use disk_analyst_core::control::{CancelToken, ScanCounters};
use disk_analyst_core::scan::{ScanOptions, scan};
use disk_analyst_core::sysinfo::peak_memory_bytes;

fn main() -> ExitCode {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("uso: scan_totals <caminho>");
        return ExitCode::from(1);
    };

    let tree = match scan(
        Path::new(&path),
        &ScanOptions::default(),
        &CancelToken::new(),
        &ScanCounters::default(),
    ) {
        Ok(tree) => tree,
        Err(error) => {
            eprintln!("erro: {error}");
            return ExitCode::from(1);
        }
    };

    let peak = match peak_memory_bytes() {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("erro ao medir a memória: {error}");
            return ExitCode::from(1);
        }
    };

    let items = u64::try_from(tree.nodes.len()).unwrap_or(u64::MAX);
    println!("itens: {items}");
    println!("tempo_ms: {:.1}", tree.duration.as_secs_f64() * 1000.0);
    println!("pico_bytes: {peak}");
    println!("bytes_por_item: {}", peak / items.max(1));
    ExitCode::SUCCESS
}
