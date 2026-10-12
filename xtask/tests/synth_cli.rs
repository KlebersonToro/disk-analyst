//! Testes do subcomando `cargo xtask synth`, rodando o binário do próprio xtask. Os diretórios
//! temporários são removidos pelo `Drop` de [`TempBase`], inclusive em falha.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const BIN: &str = env!("CARGO_BIN_EXE_xtask");

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempBase(PathBuf);

impl TempBase {
    fn new(tag: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("da-xtask-{tag}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).expect("criar diretório temporário");
        TempBase(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempBase {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/basic.json")
}

fn xtask(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("executar o xtask")
}

fn code(out: &Output) -> i32 {
    out.status.code().expect("código de saída")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn synth_writes_tree_and_expected_for_basic() {
    let base = TempBase::new("ok");
    let dest = base.path().join("dest");
    let fixture = fixture();

    let out = xtask(&["synth", fixture.to_str().unwrap(), dest.to_str().unwrap()]);

    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(dest.join("tree").is_dir());
    let expected = fs::read_to_string(dest.join("expected.json")).expect("expected.json");
    assert!(expected.contains("\"files\": 10"), "{expected}");
    assert!(expected.contains("\"folders\": 4"), "{expected}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("10 arquivos"), "resumo: {stdout}");
}

#[test]
fn non_empty_destination_exits_1_with_portuguese_message() {
    let base = TempBase::new("nonempty");
    let dest = base.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("keep.txt"), b"x").unwrap();
    let fixture = fixture();

    let out = xtask(&["synth", fixture.to_str().unwrap(), dest.to_str().unwrap()]);

    assert_eq!(code(&out), 1);
    assert!(
        stderr(&out).contains("não está vazio"),
        "stderr: {}",
        stderr(&out)
    );
    assert!(!dest.join("tree").exists());
}

#[test]
fn missing_arguments_print_usage_and_exit_2() {
    let out = xtask(&["synth"]);

    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("Uso"), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("synth <descrição.json> <destino>"));
}

#[test]
fn extra_arguments_print_usage_and_exit_2() {
    let out = xtask(&["synth", "a.json", "b", "sobrando"]);

    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("Uso"), "stderr: {}", stderr(&out));
}
