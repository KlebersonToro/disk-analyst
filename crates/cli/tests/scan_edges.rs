//! Casos-limite da US1 (T023), com o binário real: raiz que é arquivo, pasta vazia, árvore com
//! menos itens que N e desempate de tamanho pelo caminho.
//!
//! Regra do ranking de pastas (decisão da T023): só entra pasta com pelo menos um filho DIRETO do
//! tipo arquivo. Pasta sem arquivo direto não tem "tamanho próprio". O ranking de arquivos não
//! muda: entra todo arquivo, inclusive de 0 B e na nuvem.

mod common;

use std::ffi::OsStr;
use std::fs;
use std::path::MAIN_SEPARATOR as SEP;

use common::{TempBase, basic_tree, code, run, run_str, stderr, stdout, table_lines};
use disk_analyst_synth::{Entry, SyntheticTreeSpec, generate};

const FOLDERS: &str = "Maiores pastas (tamanho próprio)";
const FILES: &str = "Maiores arquivos";

#[test]
fn a_file_root_is_the_only_item() {
    let base = TempBase::new("raiz-arquivo");
    let file = base.path().join("unico.bin");
    fs::write(&file, vec![1u8; 3000]).unwrap();

    let output = run(&[OsStr::new("scan"), file.as_os_str()]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("Arquivos: 1"), "{text}");
    assert!(text.contains("Pastas: 0"), "{text}");
    let files = table_lines(&text, FILES);
    assert_eq!(files.len(), 1, "{text}");
    assert!(
        files[0].ends_with(&file.to_string_lossy().into_owned()),
        "o caminho do arquivo é o pedido: {}",
        files[0]
    );
    assert_eq!(table_lines(&text, FOLDERS), ["  (nenhum)"], "{text}");
}

#[test]
fn an_empty_folder_has_zero_totals_and_empty_rankings() {
    let base = TempBase::new("pasta-vazia");
    let empty = base.path().join("vazia");
    fs::create_dir_all(&empty).unwrap();

    let output = run(&[OsStr::new("scan"), empty.as_os_str()]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains("Tamanho alocado: 0 B   Tamanho lógico: 0 B"),
        "{text}"
    );
    assert!(text.contains("Arquivos: 0"), "{text}");
    assert!(text.contains("Pastas: 1"), "{text}");
    assert_eq!(table_lines(&text, FOLDERS), ["  (nenhum)"], "{text}");
    assert_eq!(table_lines(&text, FILES), ["  (nenhum)"], "{text}");
}

#[test]
fn a_tree_with_fewer_items_than_n_lists_only_what_exists() {
    let base = TempBase::new("poucos");
    let tree = basic_tree(&base);

    let output = run(&[
        OsStr::new("scan"),
        tree.as_os_str(),
        OsStr::new("--top"),
        OsStr::new("50"),
    ]);

    assert_eq!(code(&output), 0);
    let text = stdout(&output);
    // basic: 10 arquivos; pastas com arquivo direto: a raiz, docs e docs/sub (a "vazia" sai).
    assert_eq!(table_lines(&text, FILES).len(), 10, "{text}");
    let folders = table_lines(&text, FOLDERS);
    assert_eq!(folders.len(), 3, "{text}");
    assert!(
        !folders.iter().any(|line| line.ends_with("vazia")),
        "a pasta vazia não entra no ranking: {folders:?}"
    );
}

fn two_equal_sizes_under_a_and_a1(base: &TempBase) -> std::path::PathBuf {
    let spec = SyntheticTreeSpec {
        entries: vec![
            Entry::File {
                path: "a1/y".into(),
                size: 5000,
            },
            Entry::File {
                path: "a/x".into(),
                size: 5000,
            },
        ],
    };
    let dest = base.path().join("dest");
    generate(&spec, &dest).expect("gerar árvore de empate");
    dest.join("tree")
}

#[test]
fn equal_sizes_are_ordered_by_path_components() {
    let base = TempBase::new("empate");
    let tree = two_equal_sizes_under_a_and_a1(&base);

    let output = run(&[OsStr::new("scan"), tree.as_os_str()]);

    assert_eq!(code(&output), 0);
    let text = stdout(&output);

    // Arquivos: a\x e a1\y empatam; por componentes, "a" < "a1" (e não por string com separador).
    let files = table_lines(&text, FILES);
    assert_eq!(files.len(), 2, "{text}");
    assert!(files[0].ends_with(&format!("a{SEP}x")), "{files:?}");
    assert!(files[1].ends_with(&format!("a1{SEP}y")), "{files:?}");

    // Pastas: a e a1 empatam no próprio; a raiz não tem arquivo direto e não entra.
    let folders = table_lines(&text, FOLDERS);
    assert_eq!(folders.len(), 2, "{text}");
    assert!(folders[0].ends_with(&format!("{SEP}a")), "{folders:?}");
    assert!(folders[1].ends_with(&format!("{SEP}a1")), "{folders:?}");
}

#[test]
fn the_tie_order_is_the_same_in_every_run() {
    let base = TempBase::new("estavel");
    let tree = two_equal_sizes_under_a_and_a1(&base);

    let first = stdout(&run_str(&["scan", &tree.to_string_lossy()]));
    let second = stdout(&run_str(&["scan", &tree.to_string_lossy()]));

    // A linha "Duração" muda de uma execução para outra; o resto tem de ser idêntico.
    let strip = |text: &str| {
        text.lines()
            .filter(|line| !line.starts_with("Duração:"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(strip(&first), strip(&second));
}
