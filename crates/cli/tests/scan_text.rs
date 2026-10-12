//! A CLI de ponta a ponta no modo texto (T022): roda o binário de verdade sobre a árvore sintética
//! `basic.json`.

mod common;

#[cfg(unix)]
use common::command;
use common::{TempBase, basic_tree, code, run, run_str, stderr, stdout, table_lines};

#[test]
fn scan_prints_the_text_report_for_the_basic_tree() {
    let base = TempBase::new("basico");
    let tree = basic_tree(&base);

    let output = run(&[std::ffi::OsStr::new("scan"), tree.as_os_str()]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.starts_with("Disk Analyst — "), "{text}");
    assert!(text.contains("Arquivos: 10"), "{text}");
    assert!(text.contains("Pastas: 4"), "{text}");
    // 444.764 bytes de tamanho lógico = 434,34 KiB.
    assert!(text.contains("Tamanho lógico: 434,3 KiB"), "{text}");
    assert!(text.contains("Maiores pastas (tamanho próprio)"), "{text}");
    assert!(text.contains("Maiores arquivos"), "{text}");
    assert_eq!(stderr(&output), "", "stderr deve ficar vazio");
}

#[test]
fn top_limits_each_ranking() {
    let base = TempBase::new("top");
    let tree = basic_tree(&base);

    let output = run(&[
        std::ffi::OsStr::new("scan"),
        tree.as_os_str(),
        std::ffi::OsStr::new("--top"),
        std::ffi::OsStr::new("3"),
    ]);

    assert_eq!(code(&output), 0);
    let text = stdout(&output);
    // O basic tem 10 arquivos e só 4 pastas: com --top 3 há 3 linhas em cada ranking.
    assert_eq!(
        table_lines(&text, "Maiores pastas (tamanho próprio)").len(),
        3,
        "{text}"
    );
    assert_eq!(table_lines(&text, "Maiores arquivos").len(), 3, "{text}");
}

#[test]
fn missing_path_exits_2_with_an_error_and_empty_stdout() {
    let base = TempBase::new("ausente");
    let missing = base.path().join("nao-existe");

    let output = run(&[std::ffi::OsStr::new("scan"), missing.as_os_str()]);

    assert_eq!(code(&output), 2);
    assert!(stderr(&output).contains("erro:"), "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
}

#[test]
fn engine_mft_exits_2_with_the_agreed_message() {
    let output = run_str(&["scan", ".", "--engine", "mft"]);

    assert_eq!(code(&output), 2);
    assert!(
        stderr(&output).contains("Leitura da MFT ainda não disponível"),
        "{}",
        stderr(&output)
    );
    assert!(stderr(&output).contains("use --help para ver o uso"));
    assert_eq!(stdout(&output), "");
}

#[test]
fn json_is_temporarily_a_usage_error_and_prints_nothing_to_stdout() {
    let output = run_str(&["scan", ".", "--json"]);

    assert_eq!(code(&output), 2);
    assert!(
        stderr(&output).contains("--json ainda não disponível"),
        "{}",
        stderr(&output)
    );
    assert_eq!(
        stdout(&output),
        "",
        "quem pede JSON não pode receber outra coisa"
    );
}

#[test]
fn help_and_version_exit_0_on_stdout() {
    let help = run_str(&["--help"]);
    assert_eq!(code(&help), 0);
    assert!(
        stdout(&help).starts_with("Disk Analyst"),
        "{}",
        stdout(&help)
    );

    let version = run_str(&["--version"]);
    assert_eq!(code(&version), 0);
    assert_eq!(
        stdout(&version),
        format!("disk-analyst {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn usage_error_exits_2_and_points_to_help() {
    let output = run_str(&["--frobnicate"]);

    assert_eq!(code(&output), 2);
    let err = stderr(&output);
    assert!(
        err.contains("erro: opção desconhecida: --frobnicate"),
        "{err}"
    );
    assert!(err.contains("use --help para ver o uso"), "{err}");
    assert_eq!(stdout(&output), "");
}

#[test]
fn progress_and_stats_are_ignored_with_a_warning() {
    let base = TempBase::new("avisos");
    let tree = basic_tree(&base);

    let output = run(&[
        std::ffi::OsStr::new("scan"),
        tree.as_os_str(),
        std::ffi::OsStr::new("--progress"),
        std::ffi::OsStr::new("--stats"),
    ]);

    assert_eq!(code(&output), 0);
    assert!(stdout(&output).contains("Arquivos: 10"));
    let err = stderr(&output);
    assert!(
        err.contains("aviso: --progress ainda não disponível; ignorado"),
        "{err}"
    );
    assert!(
        err.contains("aviso: --stats ainda não disponível; ignorado"),
        "{err}"
    );
}

/// Pipe fechado: a ponta de leitura é fechada ANTES de o processo nascer, então toda escrita falha
/// com EPIPE de forma determinística (sem corrida). Deve sair com 1 e sem pânico.
#[cfg(unix)]
#[test]
fn closed_stdout_pipe_exits_1_without_panicking() {
    let base = TempBase::new("pipe");
    let tree = basic_tree(&base);
    let (reader, writer) = std::io::pipe().expect("criar pipe");
    drop(reader);

    let output = command()
        .arg("scan")
        .arg(&tree)
        .stdout(writer)
        .output()
        .expect("executar disk-analyst");

    assert_eq!(code(&output), 1, "stderr: {}", stderr(&output));
    let err = stderr(&output);
    assert!(!err.contains("panicked"), "{err}");
    assert!(err.contains("erro ao escrever a saída"), "{err}");
}
