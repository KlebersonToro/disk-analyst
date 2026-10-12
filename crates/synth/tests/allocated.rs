//! Testes do oráculo de Tamanho alocado. Rodam nos dois alvos; no Windows, medem no NTFS de
//! `%TEMP%`. Todo diretório temporário é removido pelo `Drop` de [`TempBase`].

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use disk_analyst_synth::{allocated_size, generate, load_spec};

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
            std::env::temp_dir().join(format!("da-alloc-{tag}-{}-{nanos}-{n}", std::process::id()));
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

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

/// Soma do alocado de todos os arquivos sob `dir`, recalculada pelo teste (recursão própria).
fn sum_allocated_recursive(dir: &Path) -> u64 {
    let mut total = 0;
    for entry in fs::read_dir(dir).expect("ler diretório") {
        let path = entry.expect("entrada").path();
        if path.is_dir() {
            total += sum_allocated_recursive(&path);
        } else {
            total += allocated_size(&path).expect("medir arquivo");
        }
    }
    total
}

#[test]
fn ten_thousand_bytes_allocate_at_least_their_size_in_whole_units() {
    let base = TempBase::new("ten");
    let file = base.path().join("dez-mil.bin");
    fs::write(&file, vec![0xA5u8; 10_000]).unwrap();

    let allocated = allocated_size(&file).expect("medir");
    println!("alocado de 10_000 bytes: {allocated}");

    assert!(allocated >= 10_000, "alocado {allocated}");
    assert_eq!(
        allocated % 512,
        0,
        "alocado {allocated} não é múltiplo de 512"
    );
    // Teto: o próximo múltiplo de 64 KiB cobre clusters de até 64 KiB (NTFS usa 4 KiB por padrão).
    // Pega unidades erradas, como blocos de 4096 contados em vez de 512 (98 304 > 65 536).
    let ceiling = 10_000u64.next_multiple_of(64 * 1024);
    assert!(
        allocated <= ceiling,
        "alocado {allocated} acima do teto {ceiling}"
    );
}

#[test]
fn empty_file_allocates_nothing() {
    let base = TempBase::new("empty");
    let file = base.path().join("vazio.bin");
    fs::write(&file, b"").unwrap();

    assert_eq!(allocated_size(&file).expect("medir"), 0);
}

#[test]
fn missing_path_is_an_error_not_a_panic() {
    let base = TempBase::new("missing");
    let missing = base.path().join("nao-existe.bin");

    assert!(allocated_size(&missing).is_err());
}

#[test]
fn generate_basic_records_the_sum_of_measured_allocations() {
    let base = TempBase::new("basic");
    let spec = load_spec(&fixture("basic.json")).expect("basic.json");
    let dest = base.path().join("out");

    let expected = generate(&spec, &dest).expect("geração");

    // Não se exige allocated >= logical: arquivos pequenos no NTFS podem ficar residentes na
    // MFT, com alocado zero. Por isso o teste confere a soma medida, e não uma desigualdade.
    let recomputed = sum_allocated_recursive(&dest.join("tree"));
    assert_eq!(expected.allocated_bytes, recomputed);
    assert!(expected.allocated_bytes > 0);
}

#[test]
fn generated_expected_json_has_schema_version_1() {
    let base = TempBase::new("schema");
    let spec = load_spec(&fixture("basic.json")).expect("basic.json");
    let dest = base.path().join("out");

    let expected = generate(&spec, &dest).expect("geração");

    assert_eq!(expected.schema_version, 1);
    assert_eq!(expected.files, 10);
    assert_eq!(expected.folders, 3 + 1);
}
