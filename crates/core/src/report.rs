//! Relatório em memória a partir de uma [`ScanTree`] agregada e deduplicada (FR-002, FR-003).
//!
//! Não serializa nada: JSON é a T025, e `items[]` também. Engine e ferramenta ficam para T018 e
//! T025.
//!
//! # Regras
//!
//! - **Totais**: `logical_bytes` e `allocated_bytes` são os da raiz. `files` conta Nodes File que
//!   não são inacessíveis nem hard links secundários. `folders` conta Nodes Dir não inacessíveis,
//!   com a raiz (quando é pasta) e pastas `not_scanned` (existem, só não foram percorridas).
//!   `inaccessible` conta Nodes com a flag, de qualquer kind. `cloud_files` conta File com a
//!   flag `cloud` e sem hard link. `links_not_followed` conta Nodes Link. `hardlink_duplicates`
//!   conta Nodes com a marca `hardlink`.
//! - **`top_files`**: só File que não são inacessíveis nem hard links secundários, por alocado
//!   decrescente. Desempate pelo caminho por componentes (ver [`ScanTree::path_components`]).
//! - **`top_folders`**: Dir não inacessíveis, por alocado **próprio** decrescente (soma só dos
//!   filhos diretos que são File; secundários já valem 0). Desempate pelo caminho. **Só entra pasta
//!   com pelo menos um filho DIRETO do tipo File acessível** (um hard link secundário conta como
//!   filho, com 0): pasta sem arquivo direto não tem "tamanho próprio". Por isso a raiz de uma
//!   pasta vazia, ou uma pasta que só tem subpastas, não aparece (spec: "pasta vazia: rankings
//!   vazios"). O ranking de ARQUIVOS não muda: entra todo File, inclusive de 0 B e na nuvem.
//! - **Caminho de texto**: `root_path`, separador `MAIN_SEPARATOR` e componentes convertidos por
//!   `to_text`. A raiz é só o `root_path`. Não se duplica o separador se `root_path` já termina
//!   com ele. O texto tem perdas e serve só para exibição: a ordenação usa os `u16`.
//! - **`top_n = 0`** devolve listas vazias.
//!
//! # Desempenho
//!
//! Os ranks de pré-ordem ([`ScanTree::preorder_ranks`]) são calculados uma vez por relatório.
//! Desempatar por rank equivale a desempatar pelo caminho por componentes, então o comparador é
//! só inteiros e nenhum caminho é montado para ordenar. Os `top_n` são escolhidos com `select_nth_unstable_by`,
//! ordenados e só então recebem o texto do caminho.

use std::path::MAIN_SEPARATOR;
use std::time::Duration;

use crate::names::to_text;
use crate::tree::{Kind, NodeId, ScanTree};

/// Totais do volume varrido.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub files: u64,
    pub folders: u64,
    pub inaccessible: u64,
    pub cloud_files: u64,
    pub links_not_followed: u64,
    pub hardlink_duplicates: u64,
}

/// Uma pasta no ranking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderEntry {
    pub node: NodeId,
    pub path: String,
    pub own_allocated_bytes: u64,
    pub own_logical_bytes: u64,
    pub allocated_bytes: u64,
    pub logical_bytes: u64,
}

/// Um arquivo no ranking. As flags ficam no Node, lido pelo id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub node: NodeId,
    pub path: String,
    pub allocated_bytes: u64,
    pub logical_bytes: u64,
}

/// Relatório em memória de uma varredura.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanReport {
    pub root: String,
    pub partial: bool,
    pub duration: Duration,
    pub totals: Totals,
    pub top_folders: Vec<FolderEntry>,
    pub top_files: Vec<FileEntry>,
}

/// Monta o relatório com no máximo `top_n` entradas em cada ranking.
pub fn build_report(tree: &ScanTree, top_n: usize) -> ScanReport {
    let root = tree.nodes[0];
    let mut totals = Totals {
        logical_bytes: root.logical,
        allocated_bytes: root.allocated,
        ..Totals::default()
    };
    let mut files = Vec::new();
    let mut folders = Vec::new();
    let rank = tree.preorder_ranks();

    for (i, node) in tree.nodes.iter().enumerate() {
        let id = i as NodeId;
        if node.inaccessible() {
            totals.inaccessible += 1;
        }
        if node.hardlink() {
            totals.hardlink_duplicates += 1;
        }
        match node.kind() {
            Kind::File => {
                if node.cloud() && !node.hardlink() {
                    totals.cloud_files += 1;
                }
                if !node.inaccessible() && !node.hardlink() {
                    totals.files += 1;
                    files.push((id, node.allocated));
                }
            }
            Kind::Dir if !node.inaccessible() => {
                totals.folders += 1;
                if let Some((own_allocated, _)) = own_sizes(tree, id) {
                    folders.push((id, own_allocated));
                }
            }
            Kind::Dir => {}
            Kind::Link => totals.links_not_followed += 1,
        }
    }

    let top_files = select_top(&rank, files, top_n)
        .into_iter()
        .map(|(id, _)| {
            let node = tree.nodes[id as usize];
            FileEntry {
                node: id,
                path: path_text(tree, id),
                allocated_bytes: node.allocated,
                logical_bytes: node.logical,
            }
        })
        .collect();

    let top_folders = select_top(&rank, folders, top_n)
        .into_iter()
        .map(|(id, _)| {
            let node = tree.nodes[id as usize];
            // Só pastas com arquivo direto foram candidatas, então `own_sizes` tem valor.
            let (own_allocated_bytes, own_logical_bytes) = own_sizes(tree, id).unwrap_or_default();
            FolderEntry {
                node: id,
                path: path_text(tree, id),
                own_allocated_bytes,
                own_logical_bytes,
                allocated_bytes: node.allocated,
                logical_bytes: node.logical,
            }
        })
        .collect();

    ScanReport {
        root: tree.root_path.clone(),
        partial: tree.partial,
        duration: tree.duration,
        totals,
        top_folders,
        top_files,
    }
}

/// Tamanhos próprios de uma pasta: soma dos filhos diretos que são File acessíveis. `None` se não
/// há nenhum (a pasta não entra no ranking).
fn own_sizes(tree: &ScanTree, id: NodeId) -> Option<(u64, u64)> {
    let folder = tree.nodes[id as usize];
    let mut files = (folder.first_child..folder.first_child + folder.child_count)
        .map(|c| tree.nodes[c as usize])
        .filter(|n| n.kind() == Kind::File && !n.inaccessible())
        .peekable();
    files.peek()?;
    Some(files.fold((0, 0), |(a, l), n| (a + n.allocated, l + n.logical)))
}

/// Os `top_n` maiores por tamanho, desempate pela pré-ordem (`rank`). Devolve já ordenados.
fn select_top(rank: &[u32], mut items: Vec<(NodeId, u64)>, top_n: usize) -> Vec<(NodeId, u64)> {
    if top_n == 0 {
        return Vec::new();
    }
    let by_rank = |a: &(NodeId, u64), b: &(NodeId, u64)| {
        b.1.cmp(&a.1)
            .then_with(|| rank[a.0 as usize].cmp(&rank[b.0 as usize]))
    };
    if items.len() > top_n {
        items.select_nth_unstable_by(top_n - 1, by_rank);
        items.truncate(top_n);
    }
    items.sort_unstable_by(by_rank);
    items
}

/// Texto do caminho de exibição: `root_path`, separador e componentes com perdas.
fn path_text(tree: &ScanTree, id: NodeId) -> String {
    let components = tree.path_components(id);
    let mut text = tree.root_path.clone();
    if components.is_empty() {
        return text;
    }
    if !text.ends_with(MAIN_SEPARATOR) {
        text.push(MAIN_SEPARATOR);
    }
    for (i, component) in components.iter().enumerate() {
        if i > 0 {
            text.push(MAIN_SEPARATOR);
        }
        text.push_str(&to_text(component).0);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble::{DirChunk, assemble};
    use crate::hardlinks::dedup_hardlinks;
    use crate::testgen::{chunk, finder};
    use crate::tree::{Kind, ScanTree};
    use std::path::MAIN_SEPARATOR as SEP;

    /// Árvore montada, deduplicada e agregada, como a varredura entrega.
    fn scanned(chunks: Vec<DirChunk>, root: &str) -> ScanTree {
        let a = assemble(chunks, root.into());
        let mut tree = a.tree;
        dedup_hardlinks(&mut tree, a.hardlink_candidates);
        tree
    }

    #[test]
    fn file_ties_are_broken_by_path_components() {
        // "a/x" e "a1/y" empatam em alocado; por componentes, "a" < "a1".
        let root = chunk(
            0,
            &[
                ("a", Kind::Dir, Some(1), None, 0, 0),
                ("a1", Kind::Dir, Some(2), None, 0, 0),
            ],
        );
        let a = chunk(1, &[("x", Kind::File, None, None, 10, 12)]);
        let a1 = chunk(2, &[("y", Kind::File, None, None, 10, 12)]);
        let report = build_report(&scanned(vec![root, a, a1], "raiz"), 2);

        let paths: Vec<&str> = report.top_files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            [format!("raiz{SEP}a{SEP}x"), format!("raiz{SEP}a1{SEP}y")]
        );
    }

    #[test]
    fn folder_ties_are_broken_by_path_components() {
        let root = chunk(
            0,
            &[
                ("a", Kind::Dir, Some(1), None, 0, 0),
                ("a1", Kind::Dir, Some(2), None, 0, 0),
            ],
        );
        let a = chunk(1, &[("x", Kind::File, None, None, 10, 12)]);
        let a1 = chunk(2, &[("y", Kind::File, None, None, 10, 12)]);
        let report = build_report(&scanned(vec![root, a, a1], "raiz"), 3);

        let paths: Vec<&str> = report.top_folders.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, [format!("raiz{SEP}a"), format!("raiz{SEP}a1")]);
    }

    #[test]
    fn top_n_larger_than_the_items_returns_all_and_zero_returns_none() {
        let root = chunk(0, &[("x", Kind::File, None, None, 10, 12)]);
        let tree = scanned(vec![root], "raiz");

        let all = build_report(&tree, 100);
        assert_eq!(all.top_files.len(), 1);
        assert_eq!(all.top_folders.len(), 1);

        let none = build_report(&tree, 0);
        assert!(none.top_files.is_empty());
        assert!(none.top_folders.is_empty());
    }

    #[test]
    fn empty_tree_has_only_the_root_folder() {
        let report = build_report(&scanned(vec![chunk(0, &[])], "raiz"), 5);

        assert_eq!(report.totals.folders, 1);
        assert_eq!(report.totals.files, 0);
        assert_eq!(report.totals.logical_bytes, 0);
        assert_eq!(report.totals.allocated_bytes, 0);
        assert!(
            report.top_folders.is_empty(),
            "pasta sem arquivo direto não entra no ranking"
        );
        assert!(report.top_files.is_empty());
    }

    #[test]
    fn large_file_in_a_nested_folder_ranks_only_its_own_folder() {
        // raiz/a/b/c: só c tem arquivo direto. Com top_n = 1, a cadeia de pais não entra.
        let root = chunk(0, &[("a", Kind::Dir, Some(1), None, 0, 0)]);
        let a = chunk(1, &[("b", Kind::Dir, Some(2), None, 0, 0)]);
        let b = chunk(2, &[("c", Kind::Dir, Some(3), None, 0, 0)]);
        let c = chunk(3, &[("big", Kind::File, None, None, 500, 600)]);
        let report = build_report(&scanned(vec![root, a, b, c], "raiz"), 1);

        assert_eq!(report.top_folders.len(), 1);
        assert_eq!(
            report.top_folders[0].path,
            format!("raiz{SEP}a{SEP}b{SEP}c")
        );
        assert_eq!(report.top_folders[0].own_allocated_bytes, 600);
        assert_eq!(report.top_folders[0].allocated_bytes, 600);
    }

    #[test]
    fn counts_with_hardlinks_inaccessible_cloud_and_links() {
        // h1 (raiz) e h2 (d) são o mesmo arquivo; d/h2 vence por componentes e h1 é secundário.
        let mut root = chunk(
            0,
            &[
                ("bad", Kind::File, None, None, 7, 7),
                ("cl", Kind::File, None, None, 9, 8),
                ("d", Kind::Dir, Some(1), None, 0, 0),
                ("h1", Kind::File, None, Some(5), 10, 12),
                ("ina", Kind::Dir, None, None, 0, 0),
                ("lk", Kind::Link, None, None, 99, 99),
            ],
        );
        root.entries[0].inaccessible = true; // bad
        root.entries[1].cloud = true; // cl
        root.entries[4].inaccessible = true; // ina
        let d = chunk(1, &[("h2", Kind::File, None, Some(5), 10, 12)]);

        let report = build_report(&scanned(vec![root, d], "raiz"), 10);
        let t = &report.totals;

        assert_eq!(t.logical_bytes, 19);
        assert_eq!(t.allocated_bytes, 12);
        assert_eq!(t.files, 2, "cl e d/h2; h1 é secundário e bad é inacessível");
        assert_eq!(t.folders, 2, "raiz e d; ina é inacessível");
        assert_eq!(t.inaccessible, 2, "bad e ina");
        assert_eq!(t.cloud_files, 1, "cl");
        assert_eq!(t.links_not_followed, 1, "lk");
        assert_eq!(t.hardlink_duplicates, 1, "h1");

        let files: Vec<(&str, u64, u64)> = report
            .top_files
            .iter()
            .map(|f| (f.path.as_str(), f.allocated_bytes, f.logical_bytes))
            .collect();
        assert_eq!(
            files,
            [
                (format!("raiz{SEP}d{SEP}h2").as_str(), 12, 10),
                (format!("raiz{SEP}cl").as_str(), 0, 9),
            ]
        );
        assert_eq!(report.top_folders[0].path, format!("raiz{SEP}d"));
        assert_eq!(report.top_folders[0].own_allocated_bytes, 12);
        assert_eq!(report.top_folders[1].path, "raiz");
        assert_eq!(report.top_folders[1].own_logical_bytes, 9);
    }

    #[test]
    fn root_path_with_trailing_separator_is_not_doubled() {
        let root = chunk(0, &[("x", Kind::File, None, None, 1, 4096)]);
        let report = build_report(&scanned(vec![root], &format!("mnt{SEP}")), 1);
        assert_eq!(report.top_files[0].path, format!("mnt{SEP}x"));
        assert_eq!(report.root, format!("mnt{SEP}"));
    }

    #[test]
    fn root_path_without_trailing_separator_gets_one() {
        let root = chunk(0, &[("x", Kind::File, None, None, 1, 4096)]);
        let report = build_report(&scanned(vec![root], "raiz"), 1);
        assert_eq!(report.top_files[0].path, format!("raiz{SEP}x"));
        assert_eq!(report.root, "raiz");
    }

    #[test]
    fn top_entry_keeps_its_node_id() {
        let root = chunk(
            0,
            &[
                ("small", Kind::File, None, None, 1, 1),
                ("big", Kind::File, None, None, 1, 900),
            ],
        );
        let tree = scanned(vec![root], "raiz");
        let report = build_report(&tree, 1);
        assert_eq!(report.top_files.len(), 1);
        assert_eq!(report.top_files[0].node, finder(&tree, "big"));
    }

    #[test]
    fn ties_follow_the_component_order_not_the_node_id_order() {
        // Em largura (NodeId): a=1, z=2, a/m=3, z/f=4, a/m/f=5. Por componentes: a/m/f < z/f e
        // a/m < z. As duas ordens divergem, então um desempate por NodeId trocaria o vencedor.
        let root = chunk(
            0,
            &[
                ("z", Kind::Dir, Some(1), None, 0, 0),
                ("a", Kind::Dir, Some(2), None, 0, 0),
            ],
        );
        let z = chunk(1, &[("f", Kind::File, None, None, 10, 12)]);
        let a = chunk(2, &[("m", Kind::Dir, Some(3), None, 0, 0)]);
        let m = chunk(3, &[("f", Kind::File, None, None, 10, 12)]);
        let tree = scanned(vec![root, z, a, m], "raiz");

        let report = build_report(&tree, 1);

        assert_eq!(report.top_files[0].path, format!("raiz{SEP}a{SEP}m{SEP}f"));
        assert_eq!(report.top_folders[0].path, format!("raiz{SEP}a{SEP}m"));
    }
}
