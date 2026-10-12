//! O exemplo `scan_totals` roda de ponta a ponta sobre uma árvore sintética e imprime as métricas
//! no formato combinado (T019).
//!
//! O binário do exemplo é achado a partir do executável de teste: `cargo test` compila os
//! exemplos junto, em `target/<perfil>/examples/`, ao lado de `target/<perfil>/deps/`. Se este
//! teste rodar sozinho (`cargo test --test example_runs`), o exemplo pode não estar compilado: nesse
//! caso rode antes `cargo build --example scan_totals` (com o mesmo `--target`, se houver).

mod common;

use std::path::PathBuf;
use std::process::Command;

use common::{TempBase, fixture};
use disk_analyst_synth::{generate, load_spec};

/// Caminho do exemplo, relativo ao executável de teste (`.../deps/example_runs-<hash>`).
fn example_binary(name: &str) -> PathBuf {
    let exe = std::env::current_exe().expect("caminho do executável de teste");
    let profile_dir = exe
        .parent()
        .and_then(|deps| deps.parent())
        .expect("target/<perfil>");
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let path = profile_dir.join("examples").join(file);
    assert!(
        path.exists(),
        "exemplo não compilado: {} (rode `cargo build --example {name}`)",
        path.display()
    );
    path
}

/// Valor da métrica `chave: valor` na saída do exemplo.
fn metric(stdout: &str, key: &str) -> String {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}: ")))
        .unwrap_or_else(|| panic!("métrica {key} ausente em:\n{stdout}"))
        .trim()
        .to_owned()
}

#[test]
fn scan_totals_prints_the_agreed_metrics_for_the_basic_tree() {
    let base = TempBase::new("exemplo");
    let spec = load_spec(&fixture("basic.json")).expect("basic.json");
    let dest = base.path().join("dest");
    let expected = generate(&spec, &dest).expect("gerar basic.json");

    let output = Command::new(example_binary("scan_totals"))
        .arg(dest.join("tree"))
        .output()
        .expect("executar scan_totals");

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "falhou: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{stdout}");

    let items: u64 = metric(&stdout, "itens").parse().expect("itens inteiro");
    assert_eq!(items, expected.files + expected.folders);
    let peak: u64 = metric(&stdout, "pico_bytes").parse().expect("pico inteiro");
    assert!(peak > 0);
    let per_item: u64 = metric(&stdout, "bytes_por_item")
        .parse()
        .expect("bytes por item inteiro");
    assert_eq!(per_item, peak / items);
    let time_ms: f64 = metric(&stdout, "tempo_ms").parse().expect("tempo decimal");
    assert!(time_ms >= 0.0);
}

#[test]
fn scan_totals_reports_a_missing_path_in_portuguese_with_exit_code_1() {
    let base = TempBase::new("exemplo-erro");
    let missing = base.path().join("nao-existe");

    let output = Command::new(example_binary("scan_totals"))
        .arg(&missing)
        .output()
        .expect("executar scan_totals");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(stderr.contains("não encontrado"), "stderr: {stderr}");
    assert!(output.stdout.is_empty());
}
