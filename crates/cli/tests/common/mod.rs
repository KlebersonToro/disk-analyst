//! Utilidades compartilhadas pelos testes de integração da CLI. Todo diretório temporário é
//! removido pelo `Drop` de [`TempBase`], inclusive quando o teste falha.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use disk_analyst_synth::{generate, load_spec};

const BIN: &str = env!("CARGO_BIN_EXE_disk-analyst");

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Diretório temporário com nome único, removido no `Drop`.
pub struct TempBase(PathBuf);

impl TempBase {
    pub fn new(tag: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("da-cli-{tag}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).expect("criar diretório temporário");
        TempBase(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempBase {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Gera `basic.json` em `base/dest` e devolve `dest/tree`.
pub fn basic_tree(base: &TempBase) -> PathBuf {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/basic.json");
    let spec = load_spec(&fixture).expect("basic.json");
    let dest = base.path().join("dest");
    generate(&spec, &dest).expect("gerar basic.json");
    dest.join("tree")
}

/// O binário da CLI, ainda sem argumentos.
pub fn command() -> Command {
    Command::new(BIN)
}

pub fn run(args: &[&OsStr]) -> Output {
    command()
        .args(args)
        .output()
        .expect("executar disk-analyst")
}

pub fn run_str(args: &[&str]) -> Output {
    let os: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    run(&os)
}

pub fn code(output: &Output) -> i32 {
    output.status.code().expect("código de saída")
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Linhas de dados de uma tabela do relatório: depois do título e do cabeçalho, até a linha em
/// branco (ou o fim). Ranking vazio (`  (nenhum)`, sem cabeçalho) devolve a própria linha.
pub fn table_lines<'a>(text: &'a str, title: &str) -> Vec<&'a str> {
    let mut lines = text.lines().skip_while(|line| *line != title);
    lines.next().expect("título presente");
    let first = lines.next().expect("linha depois do título");
    if first == "  (nenhum)" {
        return vec![first];
    }
    lines.take_while(|line| !line.is_empty()).collect()
}
