//! Varredura completa (T018): a árvore sintética de `basic.json` tem de bater, sem diferença, com
//! o `expected.json` do gerador (SC-001), nos dois alvos.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{TempBase, fixture};
use disk_analyst_core::control::{CancelToken, ScanCounters};
use disk_analyst_core::report::build_report;
use disk_analyst_core::scan::{ScanError, ScanOptions, scan};
use disk_analyst_core::tree::{Kind, ScanTree};
use disk_analyst_synth::{allocated_size, generate, load_spec};

/// Link para uma pasta (symlink no Linux, junction no Windows), removido no `Drop` sem tocar no
/// alvo.
struct LinkGuard(PathBuf);

impl Drop for LinkGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        let _ = fs::remove_file(&self.0);
        #[cfg(windows)]
        let _ = fs::remove_dir(&self.0);
    }
}

#[cfg(unix)]
fn make_dir_link(link: &Path, target: &Path) -> LinkGuard {
    std::os::unix::fs::symlink(target, link).expect("criar symlink");
    LinkGuard(link.to_path_buf())
}

#[cfg(windows)]
fn make_dir_link(link: &Path, target: &Path) -> LinkGuard {
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .expect("executar cmd")
        .status;
    assert!(status.success(), "mklink /J falhou");
    LinkGuard(link.to_path_buf())
}

/// Valores do `expected.json` escrito pelo gerador.
struct Expected {
    logical: u64,
    allocated: u64,
    files: u64,
    folders: u64,
}

fn read_expected(dest: &Path) -> Expected {
    let text = fs::read_to_string(dest.join("expected.json")).expect("expected.json");
    let json: serde_json::Value = serde_json::from_str(&text).expect("JSON válido");
    let field = |name: &str| {
        json[name]
            .as_u64()
            .unwrap_or_else(|| panic!("campo {name}"))
    };
    Expected {
        logical: field("logical_bytes"),
        allocated: field("allocated_bytes"),
        files: field("files"),
        folders: field("folders"),
    }
}

/// Gera `basic.json` em `base/dest` e devolve o diretório de destino.
fn generate_basic(base: &TempBase) -> std::path::PathBuf {
    let spec = load_spec(&fixture("basic.json")).expect("basic.json");
    let dest = base.path().join("dest");
    generate(&spec, &dest).expect("gerar basic.json");
    dest
}

fn scan_path(path: &Path) -> (ScanTree, ScanCounters) {
    let counters = ScanCounters::default();
    let tree = scan(
        path,
        &ScanOptions::default(),
        &CancelToken::new(),
        &counters,
    )
    .expect("varredura");
    (tree, counters)
}

fn assert_zero_difference(tree: &ScanTree, expected: &Expected) {
    let totals = build_report(tree, 5).totals;
    println!(
        "lógico {} × {} · alocado {} × {} · arquivos {} × {} · pastas {} × {}",
        totals.logical_bytes,
        expected.logical,
        totals.allocated_bytes,
        expected.allocated,
        totals.files,
        expected.files,
        totals.folders,
        expected.folders
    );
    assert_eq!(totals.logical_bytes, expected.logical, "lógico");
    assert_eq!(totals.allocated_bytes, expected.allocated, "alocado");
    assert_eq!(totals.files, expected.files, "arquivos");
    assert_eq!(totals.folders, expected.folders, "pastas");
}

#[test]
fn basic_scan_has_zero_difference_from_expected() {
    let base = TempBase::new("basic");
    let dest = generate_basic(&base);

    let (tree, _) = scan_path(&dest.join("tree"));

    assert_zero_difference(&tree, &read_expected(&dest));
}

#[test]
fn scanning_through_a_link_gives_the_same_totals_and_keeps_the_requested_path() {
    let base = TempBase::new("link");
    let dest = generate_basic(&base);
    let link = base.path().join("atalho");
    let _guard = make_dir_link(&link, &dest.join("tree"));

    let (tree, _) = scan_path(&link);

    assert_zero_difference(&tree, &read_expected(&dest));
    assert_eq!(
        tree.root_path,
        link.to_string_lossy(),
        "root_path é o caminho pedido"
    );
}

#[test]
fn a_link_inside_the_tree_is_not_followed() {
    let base = TempBase::new("aninhado");
    let dest = generate_basic(&base);
    let outside = base.path().join("externo");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("grande.bin"), vec![7u8; 1_000_000]).unwrap();
    let _guard = make_dir_link(&dest.join("tree").join("atalho"), &outside);

    let (tree, _) = scan_path(&dest.join("tree"));

    let totals = build_report(&tree, 5).totals;
    let expected = read_expected(&dest);
    assert_eq!(totals.links_not_followed, 1);
    assert_eq!(
        totals.logical_bytes, expected.logical,
        "o conteúdo do link não é somado"
    );
    assert_eq!(totals.files, expected.files);
}

#[test]
fn scanning_twice_gives_identical_nodes_and_names() {
    let base = TempBase::new("determinismo");
    let dest = generate_basic(&base);

    let (a, _) = scan_path(&dest.join("tree"));
    let (b, _) = scan_path(&dest.join("tree"));

    assert_eq!(a.nodes, b.nodes);
    assert_eq!(a.names, b.names);
}

#[test]
fn missing_path_is_not_found() {
    let base = TempBase::new("ausente");
    let missing = base.path().join("nao-existe");

    let result = scan(
        &missing,
        &ScanOptions::default(),
        &CancelToken::new(),
        &ScanCounters::default(),
    );

    assert!(matches!(result, Err(ScanError::NotFound(_))), "{result:?}");
}

#[test]
fn a_file_root_is_a_tree_of_one_node() {
    let base = TempBase::new("arquivo");
    let dest = generate_basic(&base);
    let file = dest.join("tree").join("docs").join("relatorio.bin");

    let (tree, counters) = scan_path(&file);

    assert_eq!(tree.nodes.len(), 1);
    assert_eq!(tree.nodes[0].kind(), Kind::File);
    assert_eq!(tree.nodes[0].logical, 70_000);
    assert_eq!(tree.nodes[0].allocated, allocated_size(&file).unwrap());
    let snapshot = counters.snapshot();
    assert_eq!((snapshot.items, snapshot.files), (1, 1));
}

#[test]
fn counters_report_items_files_and_allocated_bytes() {
    let base = TempBase::new("contadores");
    let dest = generate_basic(&base);

    let (_, counters) = scan_path(&dest.join("tree"));

    let snapshot = counters.snapshot();
    assert_eq!(snapshot.files, 10);
    assert_eq!(snapshot.items, 13, "10 arquivos + 3 pastas");
    assert_eq!(
        snapshot.bytes,
        read_expected(&dest).allocated,
        "bytes é o alocado"
    );
}

#[cfg(unix)]
#[test]
fn an_unreadable_subfolder_is_marked_inaccessible_without_aborting() {
    use std::os::unix::fs::PermissionsExt;

    /// Restaura as permissões antes de o `TempBase` remover a árvore.
    struct PermGuard(std::path::PathBuf);
    impl Drop for PermGuard {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
        }
    }

    let base = TempBase::new("negada");
    let root = base.path().join("raiz");
    let denied = root.join("negada");
    fs::create_dir_all(&denied).unwrap();
    fs::write(denied.join("oculto.txt"), b"x").unwrap();
    fs::write(root.join("visivel.txt"), b"yy").unwrap();
    fs::set_permissions(&denied, fs::Permissions::from_mode(0o000)).unwrap();
    let _perm = PermGuard(denied.clone());
    if fs::read_dir(&denied).is_ok() {
        eprintln!("teste pulado: o processo ignora permissão 000 (provavelmente root)");
        return;
    }

    let (tree, _) = scan_path(&root);

    let totals = build_report(&tree, 5).totals;
    assert_eq!(totals.inaccessible, 1);
    assert_eq!(totals.files, 1, "só visivel.txt");
    assert_eq!(totals.logical_bytes, 2);
}
