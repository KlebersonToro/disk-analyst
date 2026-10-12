//! Relatório de texto em português (T021, contracts/cli.md, FR-019). Função PURA de formatação: recebe
//! o relatório e escreve texto; não chama `scan` nem decide código de saída (T022).
//!
//! # Forma
//!
//! ```text
//! Disk Analyst — <raiz> (PARCIAL, se cancelado)
//! Tamanho alocado: X   Tamanho lógico: Y
//! Arquivos: N   Pastas: N   Inacessíveis: N   Na nuvem: N   Links não seguidos: N
//! Duração: 9,8 s
//!
//! Maiores pastas (tamanho próprio)
//!   alocado próprio   alocado total   caminho
//!   ...
//!
//! Maiores arquivos
//!   alocado   lógico   caminho [na nuvem]
//!   ...
//! ```
//!
//! # Decisões
//!
//! - "Hard links duplicados: N" entra no fim da linha de contagens SÓ quando N > 0.
//! - Larguras fixas, com os tamanhos alinhados à direita: 15 e 13 colunas nos tamanhos das pastas
//!   (a largura dos próprios títulos) e 10 colunas nos dois tamanhos dos arquivos, porque
//!   "1023,9 KiB" tem 10 caracteres e é o maior valor formatado. Os títulos dos arquivos ficam
//!   alinhados à direita nessas 10 colunas.
//! - Ranking vazio: o título e uma linha `  (nenhum)`, sem cabeçalho de tabela.
//! - Tamanhos em unidades binárias com vírgula decimal, contagens com `.` de milhar, tudo em
//!   aritmética inteira (sem `f64`), exata e determinística.
//! - Os caminhos já vêm como texto (com U+FFFD) do relatório e não são reconvertidos.
//! - Nenhum caractere de controle ANSI.

use std::io::{self, Write};
use std::time::Duration;

use disk_analyst_core::report::ScanReport;
use disk_analyst_core::tree::ScanTree;

const UNITS: [&str; 6] = ["KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];

/// Largura das colunas de tamanho das pastas ("alocado próprio" e "alocado total").
const FOLDER_OWN_WIDTH: usize = 15;
const FOLDER_TOTAL_WIDTH: usize = 13;
/// Largura das duas colunas de tamanho dos arquivos.
const FILE_SIZE_WIDTH: usize = 10;

/// `décimos` de uma unidade como `"I,D"`.
fn tenths_text(tenths: u128) -> String {
    format!("{},{}", tenths / 10, tenths % 10)
}

/// Tamanho em unidades binárias, com uma casa decimal e vírgula: `1536` → `"1,5 KiB"`.
///
/// Abaixo de 1024 vai em bytes. Acima, usa a maior unidade em que o valor ARREDONDADO fica abaixo
/// de 1024 (1_048_575 B = 1023,999 KiB arredondaria para "1024,0 KiB", então vira "1,0 MiB").
/// Arredondamento ao mais próximo, meio para cima, em inteiros (u128, em décimos).
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let bytes = u128::from(bytes);
    let mut divisor: u128 = 1024;
    for (index, unit) in UNITS.iter().enumerate() {
        let tenths = (bytes * 10 + divisor / 2) / divisor;
        if tenths < 10_240 || index == UNITS.len() - 1 {
            return format!("{} {unit}", tenths_text(tenths));
        }
        divisor *= 1024;
    }
    unreachable!("o laço sempre retorna na última unidade")
}

/// Inteiro com `.` como separador de milhar: `1234567` → `"1.234.567"`.
pub fn format_count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push('.');
        }
        out.push(digit);
    }
    out
}

/// Duração em segundos com uma casa e vírgula, arredondada como em [`format_size`]: `"9,8 s"`.
pub fn format_duration(duration: Duration) -> String {
    let tenths = (duration.as_nanos() + 50_000_000) / 100_000_000;
    format!("{} s", tenths_text(tenths))
}

/// Escreve o relatório de texto. `tree` serve para ler a flag `cloud` dos arquivos pelo Node.
pub fn write_text(out: &mut impl Write, report: &ScanReport, tree: &ScanTree) -> io::Result<()> {
    let totals = &report.totals;

    write!(out, "Disk Analyst — {}", report.root)?;
    if report.partial {
        write!(out, " (PARCIAL)")?;
    }
    writeln!(out)?;
    writeln!(
        out,
        "Tamanho alocado: {}   Tamanho lógico: {}",
        format_size(totals.allocated_bytes),
        format_size(totals.logical_bytes)
    )?;
    write!(
        out,
        "Arquivos: {}   Pastas: {}   Inacessíveis: {}   Na nuvem: {}   Links não seguidos: {}",
        format_count(totals.files),
        format_count(totals.folders),
        format_count(totals.inaccessible),
        format_count(totals.cloud_files),
        format_count(totals.links_not_followed)
    )?;
    if totals.hardlink_duplicates > 0 {
        write!(
            out,
            "   Hard links duplicados: {}",
            format_count(totals.hardlink_duplicates)
        )?;
    }
    writeln!(out)?;
    writeln!(out, "Duração: {}", format_duration(report.duration))?;

    writeln!(out)?;
    writeln!(out, "Maiores pastas (tamanho próprio)")?;
    if report.top_folders.is_empty() {
        writeln!(out, "  (nenhum)")?;
    } else {
        writeln!(
            out,
            "  {:>FOLDER_OWN_WIDTH$}   {:>FOLDER_TOTAL_WIDTH$}   caminho",
            "alocado próprio", "alocado total"
        )?;
        for folder in &report.top_folders {
            writeln!(
                out,
                "  {:>FOLDER_OWN_WIDTH$}   {:>FOLDER_TOTAL_WIDTH$}   {}",
                format_size(folder.own_allocated_bytes),
                format_size(folder.allocated_bytes),
                folder.path
            )?;
        }
    }

    writeln!(out)?;
    writeln!(out, "Maiores arquivos")?;
    if report.top_files.is_empty() {
        writeln!(out, "  (nenhum)")?;
    } else {
        writeln!(
            out,
            "  {:>FILE_SIZE_WIDTH$}   {:>FILE_SIZE_WIDTH$}   caminho",
            "alocado", "lógico"
        )?;
        for file in &report.top_files {
            let in_cloud = usize::try_from(file.node)
                .ok()
                .and_then(|index| tree.nodes.get(index))
                .is_some_and(|node| node.cloud());
            writeln!(
                out,
                "  {:>FILE_SIZE_WIDTH$}   {:>FILE_SIZE_WIDTH$}   {}{}",
                format_size(file.allocated_bytes),
                format_size(file.logical_bytes),
                file.path,
                if in_cloud { " [na nuvem]" } else { "" }
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use disk_analyst_core::assemble::{ChunkEntry, DirChunk, aggregate, assemble};
    use disk_analyst_core::report::build_report;
    use disk_analyst_core::tree::Kind;
    use std::path::MAIN_SEPARATOR as SEP;
    use std::time::Duration;

    #[test]
    fn size_below_1024_is_in_plain_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1), "1 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn size_uses_binary_units_with_a_decimal_comma() {
        assert_eq!(format_size(1024), "1,0 KiB");
        assert_eq!(format_size(1536), "1,5 KiB");
        assert_eq!(format_size(1_048_576), "1,0 MiB");
        assert_eq!(format_size(195_850_508_698), "182,4 GiB");
    }

    #[test]
    fn size_rounds_to_nearest_with_half_up() {
        // 10_000 / 1024 = 9,765... -> 9,8
        assert_eq!(format_size(10_000), "9,8 KiB");
        // 1024 + 51 = 1075 -> 1,0498 -> 1,0; 1024 + 52 = 1076 -> 1,0508 -> 1,1
        assert_eq!(format_size(1075), "1,0 KiB");
        assert_eq!(format_size(1076), "1,1 KiB");
    }

    #[test]
    fn size_moves_up_a_unit_when_rounding_reaches_1024() {
        // 1_048_575 B = 1023,999 KiB, que arredondaria para "1024,0 KiB".
        assert_eq!(format_size(1_048_575), "1,0 MiB");
        assert_eq!(format_size(1_048_576 - 1), "1,0 MiB");
    }

    #[test]
    fn size_handles_the_largest_value() {
        assert_eq!(format_size(u64::MAX), "16,0 EiB");
    }

    #[test]
    fn count_uses_a_dot_as_thousands_separator() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1000), "1.000");
        assert_eq!(format_count(1_234_567), "1.234.567");
        assert_eq!(format_count(u64::MAX), "18.446.744.073.709.551.615");
    }

    #[test]
    fn duration_has_one_decimal_with_a_comma() {
        assert_eq!(format_duration(Duration::from_millis(9800)), "9,8 s");
        assert_eq!(format_duration(Duration::from_millis(300)), "0,3 s");
        assert_eq!(format_duration(Duration::ZERO), "0,0 s");
        assert_eq!(format_duration(Duration::from_millis(1250)), "1,3 s");
        assert_eq!(format_duration(Duration::from_millis(1249)), "1,2 s");
    }

    fn entry(
        name: &str,
        kind: Kind,
        subdir: Option<u32>,
        logical: u64,
        allocated: u64,
    ) -> (Vec<u16>, ChunkEntry) {
        let units: Vec<u16> = name.encode_utf16().collect();
        let entry = ChunkEntry {
            name_off: 0,
            name_len: u16::try_from(units.len()).unwrap(),
            kind,
            cloud: false,
            inaccessible: false,
            name_lossy: false,
            logical,
            allocated,
            file_id: None,
            subdir,
        };
        (units, entry)
    }

    /// Chunk com os nomes concatenados na ordem das entradas.
    fn chunk(id: u32, items: Vec<(Vec<u16>, ChunkEntry)>) -> DirChunk {
        let mut names = Vec::new();
        let mut entries = Vec::new();
        for (units, mut entry) in items {
            entry.name_off = u32::try_from(names.len()).unwrap();
            names.extend(units);
            entries.push(entry);
        }
        DirChunk {
            id,
            names,
            entries,
            inaccessible: false,
        }
    }

    /// Árvore: raiz com pasta_a (grande.bin), pasta_b (nuvem.bin na nuvem) e negado (inacessível).
    fn sample_tree() -> disk_analyst_core::tree::ScanTree {
        let root = chunk(
            0,
            vec![
                entry("pasta_a", Kind::Dir, Some(1), 0, 0),
                entry("pasta_b", Kind::Dir, Some(2), 0, 0),
                entry("negado", Kind::Dir, Some(3), 0, 0),
            ],
        );
        let a = chunk(
            1,
            vec![entry("grande.bin", Kind::File, None, 3_000_000, 3_002_368)],
        );
        let mut cloud = entry("nuvem.bin", Kind::File, None, 5000, 8192);
        cloud.1.cloud = true;
        let b = chunk(2, vec![cloud]);
        let denied = DirChunk {
            id: 3,
            names: Vec::new(),
            entries: Vec::new(),
            inaccessible: true,
        };
        let mut tree = assemble(vec![root, a, b, denied], "raiz".into()).tree;
        aggregate(&mut tree);
        tree.partial = true;
        tree.duration = Duration::from_millis(9800);
        tree
    }

    #[test]
    fn text_report_matches_the_exact_snapshot() {
        let tree = sample_tree();
        let report = build_report(&tree, 2);
        let mut out = Vec::new();

        write_text(&mut out, &report, &tree).unwrap();

        let expected = format!(
            "\
Disk Analyst — raiz (PARCIAL)
Tamanho alocado: 2,9 MiB   Tamanho lógico: 2,9 MiB
Arquivos: 2   Pastas: 3   Inacessíveis: 1   Na nuvem: 1   Links não seguidos: 0
Duração: 9,8 s

Maiores pastas (tamanho próprio)
  alocado próprio   alocado total   caminho
          2,9 MiB         2,9 MiB   raiz{SEP}pasta_a
              0 B             0 B   raiz{SEP}pasta_b

Maiores arquivos
     alocado       lógico   caminho
     2,9 MiB      2,9 MiB   raiz{SEP}pasta_a{SEP}grande.bin
         0 B      4,9 KiB   raiz{SEP}pasta_b{SEP}nuvem.bin [na nuvem]
"
        );
        assert_eq!(String::from_utf8(out).unwrap(), expected);
    }

    #[test]
    fn empty_rankings_say_none_instead_of_a_bare_header() {
        let tree = assemble(vec![chunk(0, vec![])], "vazia".into()).tree;
        let report = build_report(&tree, 0);
        let mut out = Vec::new();

        write_text(&mut out, &report, &tree).unwrap();

        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("Maiores pastas (tamanho próprio)\n  (nenhum)\n"),
            "{text}"
        );
        assert!(text.contains("Maiores arquivos\n  (nenhum)\n"), "{text}");
        assert!(
            !text.contains("alocado próprio"),
            "sem cabeçalho de tabela vazia"
        );
        assert!(!text.contains("(PARCIAL)"));
    }

    #[test]
    fn hard_link_duplicates_appear_only_when_there_are_some() {
        let tree = sample_tree();
        let mut report = build_report(&tree, 2);
        let mut without = Vec::new();
        write_text(&mut without, &report, &tree).unwrap();
        assert!(
            !String::from_utf8(without)
                .unwrap()
                .contains("Hard links duplicados")
        );

        report.totals.hardlink_duplicates = 3;
        let mut with = Vec::new();
        write_text(&mut with, &report, &tree).unwrap();
        assert!(
            String::from_utf8(with)
                .unwrap()
                .contains("Hard links duplicados: 3")
        );
    }

    #[test]
    fn output_has_no_ansi_escape_sequences() {
        let tree = sample_tree();
        let report = build_report(&tree, 2);
        let mut out = Vec::new();
        write_text(&mut out, &report, &tree).unwrap();
        assert!(!out.contains(&0x1B));
    }
}
