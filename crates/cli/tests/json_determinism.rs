//! Determinismo do JSON (T027, US2 cenário 3): varreduras da mesma árvore produzem documentos
//! IDÊNTICOS, ignorando só `duration_ms`.
//!
//! Compara duas coisas: o `Value` (sem `duration_ms`) e os BYTES do documento com o número de
//! `duration_ms` trocado por um marcador fixo. O segundo garante também a ORDEM das chaves e dos
//! `items`, o que o `Value` sozinho não garante.

mod common;

use std::path::{Path, PathBuf};

use common::{TempBase, basic_tree, code, command, stderr, stdout};
use disk_analyst_synth::{Entry, SyntheticTreeSpec, generate};
use serde_json::Value;

/// Roda `scan <tree> --json` (com `RAYON_NUM_THREADS` se pedido) e devolve o stdout.
fn scan_json(tree: &Path, threads: Option<&str>) -> String {
    let mut cmd = command();
    cmd.arg("scan").arg(tree).arg("--json");
    if let Some(threads) = threads {
        cmd.env("RAYON_NUM_THREADS", threads);
    }
    let output = cmd.output().expect("executar disk-analyst");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stderr(&output), "");
    stdout(&output)
}

/// Troca o número de `duration_ms` por `0`, por substituição precisa no texto: ancorada em
/// `"duration_ms":` e terminando na vírgula seguinte. Exatamente uma ocorrência.
fn neutralize(document: &str) -> String {
    const KEY: &str = "\"duration_ms\":";
    assert_eq!(
        document.matches(KEY).count(),
        1,
        "duration_ms aparece uma vez"
    );
    let start = document.find(KEY).unwrap() + KEY.len();
    let end = start
        + document[start..]
            .find(',')
            .expect("vírgula depois de duration_ms");
    format!("{}0{}", &document[..start], &document[end..])
}

/// O documento como `Value`, sem `duration_ms`.
fn without_duration(document: &str) -> Value {
    let mut value: Value = serde_json::from_str(document).expect("JSON válido");
    value
        .as_object_mut()
        .expect("objeto")
        .remove("duration_ms")
        .expect("duration_ms");
    value
}

/// Profundidade do bulk. No Linux: 3 níveis, 5.180 arquivos em 259 pastas. No Windows criar
/// arquivos no NTFS (com antivírus) é bem mais lento, então são 2 níveis: 860 arquivos em 43
/// pastas, o que ainda é largo, fundo e paralelo, e gera a árvore em poucos segundos.
#[cfg(unix)]
const BULK_DEPTH: u32 = 3;
#[cfg(windows)]
const BULK_DEPTH: u32 = 2;

/// Piso de itens que o teste exige da árvore (para provar que ela é de fato grande).
#[cfg(unix)]
const MIN_ITEMS: usize = 5000;
#[cfg(windows)]
const MIN_ITEMS: usize = 800;

/// Árvore larga e funda (bulk de `BULK_DEPTH` níveis) com EMPATES de tamanho que dominam o top 20
/// (30 arquivos de 100.000 B em pastas diferentes). No Linux, também nomes que diferem só no caso
/// (no NTFS eles colidem).
fn bulk_tree(base: &TempBase) -> PathBuf {
    let mut entries = vec![Entry::Bulk {
        path: "grande".into(),
        depth: BULK_DEPTH,
        dirs_per_dir: 6,
        files_per_dir: 20,
        max_size: 2000,
        seed: 7,
    }];
    for i in 0..30 {
        entries.push(Entry::File {
            path: format!("empates/p{i:02}/igual.bin"),
            size: 100_000,
        });
    }
    #[cfg(unix)]
    {
        entries.push(Entry::File {
            path: "casos/A.txt".into(),
            size: 100,
        });
        entries.push(Entry::File {
            path: "casos/a.txt".into(),
            size: 200,
        });
    }
    let dest = base.path().join("dest");
    generate(&SyntheticTreeSpec { entries }, &dest).expect("gerar árvore bulk");
    dest.join("tree")
}

#[test]
fn two_scans_of_the_basic_tree_give_identical_documents() {
    let base = TempBase::new("det-basico");
    let tree = basic_tree(&base);

    let first = scan_json(&tree, None);
    let second = scan_json(&tree, None);

    assert_eq!(without_duration(&first), without_duration(&second));
    assert_eq!(
        neutralize(&first),
        neutralize(&second),
        "bytes (ordem de chaves e itens)"
    );
}

#[test]
fn five_scans_of_a_wide_deep_tree_with_ties_are_byte_identical() {
    let base = TempBase::new("det-bulk");
    let tree = bulk_tree(&base);

    let first = scan_json(&tree, None);
    let reference = neutralize(&first);
    let value = without_duration(&first);

    // A árvore de teste é de fato larga e os empates dominam o ranking (senão o teste não
    // exercitaria o desempate).
    let items = value["items"].as_array().unwrap().len();
    assert!(items > MIN_ITEMS, "itens: {items}");
    let top_files = value["top_files"].as_array().unwrap();
    assert_eq!(top_files.len(), 20);
    assert!(
        top_files
            .iter()
            .all(|f| f["path"].as_str().unwrap().ends_with("igual.bin")),
        "os 20 maiores são todos empatados: {top_files:?}"
    );

    for run in 2..=5 {
        let next = scan_json(&tree, None);
        assert_eq!(without_duration(&next), value, "execução {run}: Value");
        assert_eq!(neutralize(&next), reference, "execução {run}: bytes");
    }
}

#[test]
fn one_and_eight_threads_give_identical_documents() {
    let base = TempBase::new("det-threads");
    let tree = bulk_tree(&base);

    let one = neutralize(&scan_json(&tree, Some("1")));
    let eight = neutralize(&scan_json(&tree, Some("8")));
    let default = neutralize(&scan_json(&tree, None));

    assert_eq!(one, eight, "1 thread × 8 threads");
    assert_eq!(one, default, "1 thread × padrão");
}
