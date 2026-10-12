//! Teste de contrato do modo `--json` (T026, SC-007): o binário real escreve em stdout só o
//! documento, e ele valida contra `docs/schemas/scan-report-v1.schema.json`.
//!
//! O esquema é lido do disco; o teste da T003 já garante que a cópia publicada é idêntica ao
//! contrato. O `jsonschema` roda sem features padrão (sem `resolve-http`): nada de rede.

mod common;

use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use common::{TempBase, basic_tree, code, run, stderr, stdout};
use serde_json::Value;

fn schema() -> Value {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/schemas/scan-report-v1.schema.json");
    serde_json::from_str(&fs::read_to_string(path).expect("esquema")).expect("esquema é JSON")
}

/// Valida o documento contra o esquema; na falha, mostra todos os erros do validador.
fn assert_valid(document: &Value) {
    let validator = jsonschema::validator_for(&schema()).expect("esquema compila");
    let errors: Vec<String> = validator
        .iter_errors(document)
        .map(|error| format!("{} (em {})", error, error.instance_path()))
        .collect();
    assert!(
        errors.is_empty(),
        "documento inválido:\n{}",
        errors.join("\n")
    );
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("stdout não é JSON ({e}):\n{text}"))
}

/// Valores do `expected.json` ao lado da árvore (em `dest/`, irmão de `dest/tree`).
fn expected(tree: &Path) -> Value {
    let path = tree.parent().expect("dest").join("expected.json");
    serde_json::from_str(&fs::read_to_string(path).expect("expected.json")).expect("JSON")
}

fn scan_json(tree: &Path, extra: &[&str]) -> std::process::Output {
    let mut args: Vec<&OsStr> = vec![OsStr::new("scan"), tree.as_os_str(), OsStr::new("--json")];
    args.extend(extra.iter().map(OsStr::new));
    run(&args)
}

#[test]
fn the_document_is_valid_and_matches_expected_json_with_zero_difference() {
    let base = TempBase::new("json-basico");
    let tree = basic_tree(&base);

    let output = scan_json(&tree, &[]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stderr(&output), "", "stderr deve ficar vazio");
    let document = parse(&stdout(&output));
    assert_valid(&document);

    // SC-007: a soma dos arquivos em `items` reproduz os totais, e os totais batem com o
    // `expected.json` do gerador (diferença zero).
    let items = document["items"].as_array().expect("items");
    let sum = |field: &str| -> u64 {
        items
            .iter()
            .filter(|item| item["kind"] == "file")
            .map(|item| item[field].as_u64().unwrap())
            .sum()
    };
    let totals = &document["totals"];
    let expected = expected(&tree);
    assert_eq!(
        sum("logical_bytes"),
        totals["logical_bytes"].as_u64().unwrap()
    );
    assert_eq!(
        sum("allocated_bytes"),
        totals["allocated_bytes"].as_u64().unwrap()
    );
    for (ours, theirs) in [
        ("logical_bytes", "logical_bytes"),
        ("allocated_bytes", "allocated_bytes"),
        ("files", "files"),
        ("folders", "folders"),
    ] {
        assert_eq!(
            totals[ours], expected[theirs],
            "totals.{ours} × expected.{theirs}"
        );
    }
    assert_eq!(
        items.len() as u64,
        expected["files"].as_u64().unwrap() + expected["folders"].as_u64().unwrap()
    );
}

#[test]
fn json_with_progress_keeps_stderr_empty_and_the_document_valid() {
    let base = TempBase::new("json-progress");
    let tree = basic_tree(&base);

    let output = scan_json(&tree, &["--progress"]);

    assert_eq!(code(&output), 0);
    assert_eq!(
        stderr(&output),
        "",
        "--progress é ignorado com --json, sem aviso"
    );
    assert_valid(&parse(&stdout(&output)));
}

#[test]
fn json_with_stats_still_warns_on_stderr_and_stdout_stays_json() {
    let base = TempBase::new("json-stats");
    let tree = basic_tree(&base);

    let output = scan_json(&tree, &["--stats"]);

    assert_eq!(code(&output), 0);
    assert!(stderr(&output).contains("aviso: --stats ainda não disponível; ignorado"));
    assert_valid(&parse(&stdout(&output)));
}

#[test]
fn json_on_a_missing_path_exits_2_with_empty_stdout() {
    let base = TempBase::new("json-ausente");
    let missing = base.path().join("nao-existe");

    let output = scan_json(&missing, &[]);

    assert_eq!(code(&output), 2);
    assert!(stderr(&output).contains("erro:"), "{}", stderr(&output));
    assert_eq!(stdout(&output), "", "nada de JSON parcial");
}

#[test]
fn json_top_limits_both_rankings() {
    let base = TempBase::new("json-top");
    let tree = basic_tree(&base);

    let output = scan_json(&tree, &["--top", "3"]);

    assert_eq!(code(&output), 0);
    let document = parse(&stdout(&output));
    assert_valid(&document);
    // basic: 10 arquivos e 3 pastas com arquivo direto.
    assert_eq!(document["top_files"].as_array().unwrap().len(), 3);
    assert_eq!(document["top_folders"].as_array().unwrap().len(), 3);
}

#[test]
fn a_file_root_gives_a_valid_document_with_one_item() {
    let base = TempBase::new("json-arquivo");
    let file = base.path().join("unico.bin");
    fs::write(&file, vec![1u8; 3000]).unwrap();

    let output = scan_json(&file, &[]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let document = parse(&stdout(&output));
    assert_valid(&document);
    let items = document["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0]["parent"].is_null());
    assert_eq!(items[0]["kind"], "file");
    assert_eq!(items[0]["logical_bytes"], 3000);
}
