//! A varredura é somente leitura (FR-014, Princípio II): nada é criado, removido nem alterado, e
//! nenhuma dependência do binário é de rede (Princípio VI).
//!
//! # O que se compara
//!
//! Para cada item da árvore (arquivos E pastas): caminho relativo, tamanho (`len`, só de arquivos),
//! data de modificação e permissões (Linux: `mode`; Windows: `readonly`). O conjunto de caminhos
//! também tem de ser o mesmo (nada criado, nada removido). A data de modificação das PASTAS entra:
//! listar uma pasta não pode alterá-la.
//!
//! # O que NÃO se compara
//!
//! A data de **acesso** (`accessed()`). O sistema operacional pode atualizá-la ao ler, por política
//! própria (`relatime`/`noatime` no Linux, atualização de último acesso do NTFS), sem que a
//! varredura tenha escrito nada. Comparar esse campo daria falha por uma alteração que o programa
//! não controla.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use common::{TempBase, fixture};
use disk_analyst_core::control::{CancelToken, ScanCounters};
use disk_analyst_core::scan::{ScanOptions, scan};
use disk_analyst_synth::{generate, load_spec};

#[cfg(unix)]
type Permissions = u32;
#[cfg(windows)]
type Permissions = bool;

#[cfg(unix)]
fn permissions_of(metadata: &fs::Metadata) -> Permissions {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode()
}

#[cfg(windows)]
fn permissions_of(metadata: &fs::Metadata) -> Permissions {
    metadata.permissions().readonly()
}

#[derive(Debug, PartialEq)]
struct Item {
    is_dir: bool,
    /// Só arquivos: o tamanho de uma pasta é detalhe do sistema de arquivos.
    len: Option<u64>,
    modified: SystemTime,
    permissions: Permissions,
}

type Snapshot = BTreeMap<String, Item>;

/// Registra todos os itens sob `root`, com caminho relativo separado por `/`.
fn snapshot(root: &Path) -> Snapshot {
    fn walk(base: &Path, dir: &Path, out: &mut Snapshot) {
        for entry in fs::read_dir(dir).expect("ler diretório") {
            let path = entry.expect("entrada").path();
            let metadata = fs::symlink_metadata(&path).expect("metadados");
            let relative = path
                .strip_prefix(base)
                .expect("sob a raiz")
                .to_string_lossy()
                .replace('\\', "/");
            let is_dir = metadata.is_dir();
            out.insert(
                relative,
                Item {
                    is_dir,
                    len: (!is_dir).then_some(metadata.len()),
                    modified: metadata.modified().expect("data de modificação"),
                    permissions: permissions_of(&metadata),
                },
            );
            if is_dir {
                walk(base, &path, out);
            }
        }
    }
    let mut out = Snapshot::new();
    walk(root, root, &mut out);
    out
}

/// Devolve as permissões de escrita a um arquivo somente leitura antes de o `TempBase` apagá-lo.
struct WritableGuard(PathBuf);

impl Drop for WritableGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o644));
        }
        #[cfg(windows)]
        {
            if let Ok(metadata) = fs::metadata(&self.0) {
                let mut permissions = metadata.permissions();
                #[allow(
                    clippy::permissions_set_readonly_false,
                    reason = "no Windows set_readonly(false) só limpa o atributo; o risco do lint é de Unix"
                )]
                permissions.set_readonly(false);
                let _ = fs::set_permissions(&self.0, permissions);
            }
        }
    }
}

/// `basic.json` mais uma subpasta, um arquivo à mão e um arquivo somente leitura.
fn tree_with_extras(base: &TempBase) -> (PathBuf, WritableGuard) {
    let spec = load_spec(&fixture("basic.json")).expect("basic.json");
    let dest = base.path().join("dest");
    generate(&spec, &dest).expect("gerar basic.json");
    let tree = dest.join("tree");

    fs::create_dir_all(tree.join("extra").join("aninhada")).unwrap();
    fs::write(
        tree.join("extra").join("aninhada").join("a.txt"),
        b"conteudo",
    )
    .unwrap();
    let readonly = tree.join("extra").join("somente-leitura.txt");
    fs::write(&readonly, b"nao mexer").unwrap();
    let mut permissions = fs::metadata(&readonly).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&readonly, permissions).unwrap();
    (tree, WritableGuard(readonly))
}

fn run_scan(tree: &Path) {
    scan(
        tree,
        &ScanOptions::default(),
        &CancelToken::new(),
        &ScanCounters::default(),
    )
    .expect("varredura");
}

#[test]
fn scanning_changes_nothing_in_the_tree() {
    let base = TempBase::new("somente-leitura");
    let (tree, _guard) = tree_with_extras(&base);

    let before = snapshot(&tree);
    run_scan(&tree);
    let after = snapshot(&tree);

    assert!(before.len() > 13, "a árvore tem itens: {}", before.len());
    assert_eq!(before, after);
}

/// O instrumento tem de enxergar escrita, criação e remoção; senão a comparação acima seria vazia.
#[test]
fn the_snapshot_detects_a_write_a_creation_and_a_removal() {
    let base = TempBase::new("instrumento");
    let (tree, _guard) = tree_with_extras(&base);
    let original = snapshot(&tree);

    // Escrita: o tamanho do arquivo muda.
    let file = tree.join("extra").join("aninhada").join("a.txt");
    fs::write(&file, b"conteudo alterado").unwrap();
    assert_ne!(snapshot(&tree), original, "escrita não detectada");
    fs::write(&file, b"conteudo").unwrap();

    // Criação: o conjunto de caminhos muda.
    let created = tree.join("extra").join("novo.txt");
    fs::write(&created, b"x").unwrap();
    assert_ne!(snapshot(&tree), original, "criação não detectada");
    fs::remove_file(&created).unwrap();

    // Remoção.
    fs::remove_file(&file).unwrap();
    assert_ne!(snapshot(&tree), original, "remoção não detectada");
}

/// Dependências de rede conhecidas. Lista pequena e explícita: o que importa é que o binário
/// nunca abra conexão (Princípio VI), então nenhuma destas pode estar no grafo dele.
const NETWORK_CRATES: [&str; 16] = [
    "reqwest",
    "hyper",
    "ureq",
    "tokio",
    "socket2",
    "h2",
    "rustls",
    "native-tls",
    "openssl",
    "curl",
    "isahc",
    "attohttpc",
    "surf",
    "async-std",
    "smol",
    "tungstenite",
];

/// Grafo de dependências lido do `Cargo.lock`: pacote → nomes das dependências.
fn lock_graph(lock: &str) -> BTreeMap<String, Vec<String>> {
    let mut graph = BTreeMap::new();
    let (mut name, mut dependencies, mut in_list) = (None::<String>, Vec::new(), false);
    let mut flush = |name: &mut Option<String>, dependencies: &mut Vec<String>| {
        if let Some(name) = name.take() {
            graph.insert(name, std::mem::take(dependencies));
        }
    };
    for line in lock.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            flush(&mut name, &mut dependencies);
            in_list = false;
        } else if in_list {
            if line == "]" {
                in_list = false;
            } else if let Some(entry) = line
                .trim_end_matches(',')
                .trim_matches('"')
                .split(' ')
                .next()
            {
                dependencies.push(entry.to_owned());
            }
        } else if let Some(value) = line.strip_prefix("name = ") {
            name = Some(value.trim_matches('"').to_owned());
        } else if line == "dependencies = [" {
            in_list = true;
        }
    }
    flush(&mut name, &mut dependencies);
    graph
}

/// Tudo o que o pacote `root` alcança no grafo (inclui o próprio `root`).
fn closure(graph: &BTreeMap<String, Vec<String>>, root: &str) -> Vec<String> {
    let mut seen = vec![root.to_owned()];
    let mut stack = vec![root.to_owned()];
    while let Some(package) = stack.pop() {
        for dependency in graph.get(&package).into_iter().flatten() {
            if !seen.contains(dependency) {
                seen.push(dependency.clone());
                stack.push(dependency.clone());
            }
        }
    }
    seen.sort();
    seen
}

#[test]
fn the_binary_depends_on_no_network_crate() {
    let lock = include_str!("../../../Cargo.lock");
    let graph = lock_graph(lock);

    let reachable = closure(&graph, "disk-analyst");

    // O leitor funciona: o fecho inclui o núcleo e o que ele usa.
    assert!(
        reachable.contains(&"disk-analyst-core".to_owned()),
        "{reachable:?}"
    );
    assert!(reachable.contains(&"rayon".to_owned()), "{reachable:?}");
    let found: Vec<&&str> = NETWORK_CRATES
        .iter()
        .filter(|name| reachable.iter().any(|r| r == **name))
        .collect();
    assert!(
        found.is_empty(),
        "crates de rede no grafo do binário: {found:?}"
    );
}

#[test]
fn the_lock_reader_finds_a_network_crate_when_one_is_present() {
    // Instrumento: um lock sintético com `ureq` entre as dependências do binário.
    let lock = "\
[[package]]
name = \"disk-analyst\"
version = \"0.1.0\"
dependencies = [
 \"disk-analyst-core\",
 \"ureq 2.9.0\",
]

[[package]]
name = \"disk-analyst-core\"
version = \"0.1.0\"

[[package]]
name = \"ureq\"
version = \"2.9.0\"
";
    let reachable = closure(&lock_graph(lock), "disk-analyst");
    assert!(reachable.contains(&"ureq".to_owned()), "{reachable:?}");
}
