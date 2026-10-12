//! Testes do gerador sintético. Todo diretório temporário é criado sob `std::env::temp_dir()` e
//! removido pelo `Drop` de [`TempBase`], inclusive quando o teste falha.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use disk_analyst_synth::{Planned, SynthError, SyntheticTreeSpec, generate, load_spec, planned_of};

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
            std::env::temp_dir().join(format!("da-synth-{tag}-{}-{nanos}-{n}", std::process::id()));
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

fn spec_from_json(json: &str) -> SyntheticTreeSpec {
    serde_json::from_str(json).expect("descrição de teste válida")
}

/// Lista ordenada de (caminho relativo, bytes ou None para pasta) da árvore gerada.
fn listing(root: &Path) -> Vec<(String, Option<Vec<u8>>)> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, Option<Vec<u8>>)>) {
        for entry in fs::read_dir(dir).expect("ler diretório") {
            let path = entry.expect("entrada").path();
            let rel = path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if path.is_dir() {
                out.push((rel, None));
                walk(base, &path, out);
            } else {
                out.push((rel, Some(fs::read(&path).expect("ler arquivo"))));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

#[test]
fn same_spec_twice_gives_identical_trees_and_expected() {
    let base = TempBase::new("twice");
    let spec = load_spec(&fixture("basic.json")).expect("basic.json");
    let a = base.path().join("a");
    let b = base.path().join("b");

    generate(&spec, &a).expect("primeira geração");
    generate(&spec, &b).expect("segunda geração");

    assert_eq!(listing(&a.join("tree")), listing(&b.join("tree")));
    assert_eq!(
        fs::read(a.join("expected.json")).unwrap(),
        fs::read(b.join("expected.json")).unwrap()
    );
}

/// Tamanhos de `basic.json`, escritos à mão a partir da descrição (não lidos do gerador).
const BASIC_SIZES: [(&str, u64); 10] = [
    ("docs/leia.txt", 0),
    ("docs/nota.txt", 1),
    ("docs/relatorio.bin", 70_000),
    ("docs/sub/a.jpg", 4_096),
    ("docs/sub/b.jpg", 5_000),
    ("docs/sub/c.jpg", 123),
    ("raiz.dat", 65_536),
    ("raiz2.dat", 300_000),
    ("tiny.txt", 7),
    ("um.bin", 1),
];

#[test]
fn file_sizes_on_disk_match_the_description() {
    let base = TempBase::new("sizes");
    let spec = load_spec(&fixture("basic.json")).expect("basic.json");
    let dest = base.path().join("out");
    generate(&spec, &dest).expect("geração");

    for (rel, size) in BASIC_SIZES {
        let on_disk = fs::metadata(dest.join("tree").join(rel))
            .unwrap_or_else(|e| panic!("{rel}: {e}"))
            .len();
        assert_eq!(on_disk, size, "tamanho de {rel}");
    }
    let files = listing(&dest.join("tree"))
        .iter()
        .filter(|(_, b)| b.is_some())
        .count();
    assert_eq!(files, BASIC_SIZES.len());
}

#[test]
fn content_is_deterministic_and_not_zeros() {
    let base = TempBase::new("content");
    let spec = load_spec(&fixture("basic.json")).expect("basic.json");
    let dest = base.path().join("out");
    generate(&spec, &dest).expect("geração");

    let bytes = fs::read(dest.join("tree/docs/relatorio.bin")).unwrap();
    assert!(
        bytes.iter().any(|&b| b != 0),
        "conteúdo não pode ser só zeros"
    );
    assert_eq!(&bytes[..16], &EXPECTED_HEAD[..]);
}

/// Primeiros 16 bytes de `docs/relatorio.bin` (semente = FNV-1a do caminho relativo). Valor de
/// referência: se o formato do conteúdo mudar de propósito, atualizar aqui.
const EXPECTED_HEAD: [u8; 16] = [
    122, 138, 198, 26, 249, 87, 107, 170, 62, 172, 172, 48, 55, 5, 201, 26,
];

#[test]
fn bulk_same_seed_is_identical_and_other_seed_differs() {
    let base = TempBase::new("bulk");
    let spec = |seed: u64| {
        spec_from_json(&format!(
            r#"{{"entries":[{{"path":"g","kind":"bulk","depth":2,"dirs_per_dir":2,"files_per_dir":3,"max_size":1000,"seed":{seed}}}]}}"#
        ))
    };

    generate(&spec(1), &base.path().join("s1a")).expect("semente 1");
    generate(&spec(1), &base.path().join("s1b")).expect("semente 1 de novo");
    generate(&spec(2), &base.path().join("s2")).expect("semente 2");

    let s1a = listing(&base.path().join("s1a/tree"));
    let s1b = listing(&base.path().join("s1b/tree"));
    let s2 = listing(&base.path().join("s2/tree"));
    assert_eq!(s1a, s1b);
    assert_ne!(s1a, s2);
}

#[test]
fn planned_of_basic_counts_sizes_files_and_folders() {
    let spec = load_spec(&fixture("basic.json")).expect("basic.json");
    let planned: Planned = planned_of(&spec).expect("contagem");

    let sizes = [0u64, 1, 70_000, 4_096, 5_000, 123, 65_536, 300_000, 7, 1];
    assert_eq!(planned.logical_bytes, sizes.iter().sum::<u64>());
    assert_eq!(planned.files, 10);
    assert_eq!(
        planned.folders,
        3 + 1,
        "três pastas da descrição mais a raiz tree/"
    );
}

#[test]
fn expected_json_has_fixed_field_order() {
    let base = TempBase::new("expjson");
    let spec = load_spec(&fixture("basic.json")).expect("basic.json");
    let dest = base.path().join("out");
    generate(&spec, &dest).expect("geração");

    let text = fs::read_to_string(dest.join("expected.json")).unwrap();
    let fields = [
        "schema_version",
        "logical_bytes",
        "allocated_bytes",
        "files",
        "folders",
    ];
    let positions: Vec<usize> = fields
        .iter()
        .map(|f| {
            text.find(&format!("\"{f}\""))
                .unwrap_or_else(|| panic!("falta {f}"))
        })
        .collect();
    assert!(
        positions.windows(2).all(|w| w[0] < w[1]),
        "ordem: {fields:?}"
    );
}

#[test]
fn insecure_paths_are_rejected_with_their_reason_before_anything_is_written() {
    let cases = [
        ("../x", "componente \"..\""),
        ("/abs", "caminho absoluto"),
        ("a//b", "componente vazio"),
        ("C:/x", "prefixo de drive"),
        ("a/../b", "componente \"..\""),
    ];
    for (bad, motivo) in cases {
        let base = TempBase::new("inseguro");
        let spec = spec_from_json(&format!(
            r#"{{"entries":[{{"path":"ok.txt","kind":"file","size":1}},{{"path":"{bad}","kind":"file","size":1}}]}}"#
        ));
        let dest = base.path().join("out");

        let err = generate(&spec, &dest).expect_err(bad);

        assert!(
            matches!(err, SynthError::CaminhoInseguro { .. }),
            "{bad}: {err}"
        );
        assert!(
            err.to_string().contains(motivo),
            "{bad}: motivo errado: {err}"
        );
        assert!(!dest.join("tree").exists(), "{bad}: nada pode ser escrito");
        assert!(
            !base.path().join("x").exists(),
            "{bad}: nada fora do destino"
        );
    }
}

#[test]
fn non_empty_destination_is_rejected_and_left_alone() {
    let base = TempBase::new("nonempty");
    let dest = base.path().join("out");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("keep.txt"), b"nao mexer").unwrap();
    let spec = load_spec(&fixture("basic.json")).expect("basic.json");

    let err = generate(&spec, &dest).expect_err("destino não vazio");

    assert!(matches!(err, SynthError::DestinoNaoVazio(_)), "{err}");
    assert_eq!(fs::read(dest.join("keep.txt")).unwrap(), b"nao mexer");
    assert!(!dest.join("tree").exists());
}

#[test]
fn unsupported_kind_is_rejected_with_a_clear_message() {
    let base = TempBase::new("kind");
    let spec = spec_from_json(r#"{"entries":[{"path":"l","kind":"symlink","target":"x"}]}"#);
    let dest = base.path().join("out");

    let err = generate(&spec, &dest).expect_err("symlink ainda não suportado");

    assert!(
        matches!(err, SynthError::TipoNaoSuportado("symlink")),
        "{err}"
    );
    assert_eq!(err.to_string(), "tipo ainda não suportado: symlink");
    assert!(!dest.join("tree").exists());
}

#[test]
fn large_fixture_parses_and_its_count_is_in_range_without_generating() {
    let spec = load_spec(&fixture("large.json")).expect("large.json");
    let planned = planned_of(&spec).expect("contagem sem gerar");
    assert!(
        (190_000..=210_000).contains(&planned.files),
        "arquivos esperados: {}",
        planned.files
    );
}
