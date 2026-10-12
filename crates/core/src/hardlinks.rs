//! Deduplicação de hard links (R5, ADR-0010, FR-008).
//!
//! Candidatos com o mesmo `file_id` são o mesmo arquivo. Dentro de cada grupo, o Node de menor
//! caminho mantém o tamanho e os demais ficam com `logical = allocated = 0` e a marca `hardlink`.
//!
//! # Critério de "menor caminho"
//!
//! Compara-se a **sequência de componentes** do caminho, a partir da raiz: `Vec<&[u16]>` com o
//! `Ord` de slice, componente a componente, cada um em ordinal UTF-16. **Não** é a string com
//! separador. Motivos: (a) não depende do separador (`\` no Windows, `/` no Linux), então o
//! resultado é o mesmo nos dois alvos; (b) coincide com a ordem de exibição da árvore, em que os
//! filhos são ordenados por nome. Exemplo: `a\x` vence `a1\y`, porque o componente `a` é menor
//! que `a1`; como string, `a1\y` venceria, porque `'1'` (0x31) < `'\'` (0x5C).
//!
//! # Pré-condição
//!
//! Os Nodes de um mesmo `file_id` representam o mesmo arquivo, com os mesmos tamanhos e flags.
//! Não é validado em release; um `debug_assert!` checa os tamanhos.

use crate::assemble::aggregate;
use crate::tree::{NodeId, ScanTree};

/// Componentes do caminho do Node `id`, da raiz para a folha (sem a própria raiz).
fn path_components(tree: &ScanTree, mut id: NodeId) -> Vec<&[u16]> {
    let mut parts = Vec::new();
    while tree.nodes[id as usize].parent != crate::tree::NO_PARENT {
        parts.push(tree.name(id));
        id = tree.nodes[id as usize].parent;
    }
    parts.reverse();
    parts
}

/// Deduplica os hard links e re-agrega a árvore. Devolve quantos Nodes ficaram como secundários
/// (marcados com `hardlink`).
///
/// Ordena `candidates` por `file_id`; grupos de um elemento não são tocados. Caminhos só são
/// construídos para grupos com mais de um elemento. Ao final chama [`aggregate`], então a árvore
/// sai re-agregada.
pub fn dedup_hardlinks(tree: &mut ScanTree, mut candidates: Vec<(u64, NodeId)>) -> u64 {
    candidates.sort_unstable();

    let mut secondary = Vec::new();
    for group in candidates.chunk_by(|a, b| a.0 == b.0) {
        if group.len() < 2 {
            continue;
        }
        let first = tree.nodes[group[0].1 as usize];
        debug_assert!(
            group.iter().all(|&(_, id)| {
                let n = &tree.nodes[id as usize];
                n.kind() == first.kind()
                    && n.logical == first.logical
                    && n.allocated == first.allocated
            }),
            "hard links com file_id igual devem ter o mesmo arquivo (tamanhos e tipo)"
        );
        let winner = group
            .iter()
            .map(|&(_, id)| id)
            .min_by(|&a, &b| path_components(tree, a).cmp(&path_components(tree, b)))
            .expect("grupo com mais de um elemento");
        secondary.extend(group.iter().map(|&(_, id)| id).filter(|&id| id != winner));
    }

    for &id in &secondary {
        let node = &mut tree.nodes[id as usize];
        node.logical = 0;
        node.allocated = 0;
        node.set_hardlink(true);
    }
    aggregate(tree);
    secondary.len() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble::{Assembled, ChunkEntry, DirChunk, assemble};
    use crate::testgen::{chunk, finder, ids_from, perm};
    use crate::tree::{Kind, NO_PARENT};
    use proptest::collection::vec;
    use proptest::prelude::*;
    use std::collections::BTreeMap;

    #[test]
    fn three_links_keep_the_size_only_once() {
        let root = chunk(
            0,
            &[
                ("b", Kind::Dir, Some(1), None, 0, 0),
                ("a", Kind::Dir, Some(2), None, 0, 0),
                ("c", Kind::Dir, Some(3), None, 0, 0),
            ],
        );
        let b = chunk(1, &[("x", Kind::File, None, Some(7), 100, 104)]);
        let a = chunk(2, &[("y", Kind::File, None, Some(7), 100, 104)]);
        let c = chunk(3, &[("z", Kind::File, None, Some(7), 100, 104)]);
        let Assembled {
            mut tree,
            hardlink_candidates,
        } = assemble(vec![root, b, a, c], "raiz".into());

        let removed = dedup_hardlinks(&mut tree, hardlink_candidates);

        assert_eq!(removed, 2);
        let size = |tree: &ScanTree, name: &str| {
            let n = tree.nodes[finder(tree, name) as usize];
            (n.logical, n.allocated, n.hardlink())
        };
        assert_eq!(size(&tree, "y"), (100, 104, false));
        assert_eq!(size(&tree, "x"), (0, 0, true));
        assert_eq!(size(&tree, "z"), (0, 0, true));
        assert_eq!((tree.nodes[0].logical, tree.nodes[0].allocated), (100, 104));
    }

    #[test]
    fn winner_is_the_smallest_path_by_components_not_by_string() {
        // Como string com '\', "a1\y" venceria ('1' < '\'). Por componentes, "a" < "a1".
        let root = chunk(
            0,
            &[
                ("a", Kind::Dir, Some(1), None, 0, 0),
                ("a1", Kind::Dir, Some(2), None, 0, 0),
            ],
        );
        let a = chunk(1, &[("x", Kind::File, None, Some(9), 10, 12)]);
        let a1 = chunk(2, &[("y", Kind::File, None, Some(9), 10, 12)]);
        let Assembled {
            mut tree,
            hardlink_candidates,
        } = assemble(vec![root, a, a1], "raiz".into());

        let removed = dedup_hardlinks(&mut tree, hardlink_candidates);

        assert_eq!(removed, 1);
        assert!(!tree.nodes[finder(&tree, "x") as usize].hardlink());
        assert!(tree.nodes[finder(&tree, "y") as usize].hardlink());
        assert_eq!((tree.nodes[0].logical, tree.nodes[0].allocated), (10, 12));
    }

    #[test]
    fn winner_has_a_larger_node_id_than_the_loser() {
        // BFS: z=1, a=2 (filhos da raiz em ordem de nome); z/f=4 e a/m=3, a/m/f=5.
        // O menor NodeId é z/f (perdedor); quem vence por componentes é a/m/f.
        let root = chunk(
            0,
            &[
                ("z", Kind::Dir, Some(1), None, 0, 0),
                ("a", Kind::Dir, Some(2), None, 0, 0),
            ],
        );
        let z = chunk(1, &[("f", Kind::File, None, Some(1), 10, 12)]);
        let a = chunk(2, &[("m", Kind::Dir, Some(3), None, 0, 0)]);
        let m = chunk(3, &[("f", Kind::File, None, Some(1), 10, 12)]);
        let Assembled {
            mut tree,
            hardlink_candidates,
        } = assemble(vec![root, z, a, m], "raiz".into());

        let z_f = tree.nodes[finder(&tree, "z") as usize].first_child;
        let a_m = tree.nodes[finder(&tree, "a") as usize].first_child;
        let a_m_f = tree.nodes[a_m as usize].first_child;
        assert!(
            a_m_f > z_f,
            "o vencedor precisa ter NodeId maior que o perdedor"
        );

        let removed = dedup_hardlinks(&mut tree, hardlink_candidates);

        assert_eq!(removed, 1);
        assert!(!tree.nodes[a_m_f as usize].hardlink());
        assert!(tree.nodes[z_f as usize].hardlink());
        assert_eq!((tree.nodes[0].logical, tree.nodes[0].allocated), (10, 12));
    }

    #[test]
    fn distinct_ids_and_single_entries_are_untouched() {
        let root = chunk(
            0,
            &[
                ("x", Kind::File, None, Some(1), 10, 12),
                ("y", Kind::File, None, Some(2), 5, 8),
            ],
        );
        let Assembled {
            mut tree,
            hardlink_candidates,
        } = assemble(vec![root], "raiz".into());

        let removed = dedup_hardlinks(&mut tree, hardlink_candidates);

        assert_eq!(removed, 0);
        assert!(tree.nodes.iter().all(|n| !n.hardlink()));
        assert_eq!((tree.nodes[0].logical, tree.nodes[0].allocated), (15, 20));
    }

    // Gerador de árvores com file_id repetidos. Os tamanhos e as flags de um arquivo com file_id
    // vêm só do file_id, para respeitar a pré-condição de que o mesmo file_id é o mesmo arquivo.

    const NAMES: [&str; 6] = ["a", "a1", "b", "x", "y", "z"];

    #[derive(Debug, Clone)]
    struct MEntry {
        name: Vec<u16>,
        kind: Kind,
        fid: Option<u64>,
        logical: u64,
        allocated: u64,
        cloud: bool,
        inaccessible: bool,
        /// Índice da pasta-modelo, se for subpasta.
        sub: Option<usize>,
    }

    #[derive(Debug, Clone)]
    struct RawFile {
        name: usize,
        fid: Option<u64>,
        logical: u64,
        allocated: u64,
        cloud: bool,
        inaccessible: bool,
    }

    fn units(name: &str) -> Vec<u16> {
        name.encode_utf16().collect()
    }

    fn file_entry(raw: &RawFile) -> MEntry {
        let (logical, allocated, cloud, inaccessible) = match raw.fid {
            Some(fid) => (100 + 7 * fid, 100 + 7 * fid + 4, fid % 3 == 0, fid % 5 == 4),
            None => (raw.logical, raw.allocated, raw.cloud, raw.inaccessible),
        };
        MEntry {
            name: units(NAMES[raw.name]),
            kind: Kind::File,
            fid: raw.fid,
            logical,
            allocated,
            cloud,
            inaccessible,
            sub: None,
        }
    }

    /// Pasta 0 é a raiz. Cada pasta i > 0 é subpasta de uma pasta de índice menor. Nomes de
    /// subpasta têm sufixo com o índice, para serem únicos entre irmãos; arquivos repetidos na
    /// mesma pasta são descartados.
    fn build(parents: &[u32], folder_names: &[usize], raws: &[Vec<RawFile>]) -> Vec<Vec<MEntry>> {
        let mut folders: Vec<Vec<MEntry>> = vec![Vec::new(); raws.len()];
        for child in 1..raws.len() {
            let parent = parents[child] as usize % child;
            folders[parent].push(MEntry {
                name: units(&format!("{}#{child}", NAMES[folder_names[child]])),
                kind: Kind::Dir,
                fid: None,
                logical: 0,
                allocated: 0,
                cloud: false,
                inaccessible: false,
                sub: Some(child),
            });
        }
        for (f, files) in raws.iter().enumerate() {
            folders[f].extend(files.iter().map(file_entry));
        }
        for folder in &mut folders {
            let mut seen = std::collections::HashSet::new();
            folder.retain(|e| seen.insert(e.name.clone()));
        }
        folders
    }

    fn chunks_of(
        folders: &[Vec<MEntry>],
        ids: &[u32],
        order: &[usize],
        reverse: bool,
    ) -> Vec<DirChunk> {
        order
            .iter()
            .map(|&f| {
                let mut names = Vec::new();
                let mut entries = Vec::new();
                for e in &folders[f] {
                    entries.push(ChunkEntry {
                        name_off: names.len() as u32,
                        name_len: e.name.len() as u16,
                        kind: e.kind,
                        cloud: e.cloud,
                        inaccessible: e.inaccessible,
                        name_lossy: false,
                        logical: e.logical,
                        allocated: e.allocated,
                        file_id: e.fid,
                        subdir: e.sub.map(|g| ids[g]),
                    });
                    names.extend_from_slice(&e.name);
                }
                if reverse {
                    entries.reverse();
                }
                DirChunk {
                    id: ids[f],
                    names,
                    entries,
                }
            })
            .collect()
    }

    #[derive(Debug)]
    struct Scenario {
        folders: Vec<Vec<MEntry>>,
        ids_a: Vec<u32>,
        order_a: Vec<usize>,
        ids_b: Vec<u32>,
        order_b: Vec<usize>,
    }

    fn raw_file() -> impl Strategy<Value = RawFile> {
        (
            0..NAMES.len(),
            proptest::option::of(0..6u64),
            0..1000u64,
            0..1000u64,
            any::<bool>(),
            any::<bool>(),
        )
            .prop_map(
                |(name, fid, logical, allocated, cloud, inaccessible)| RawFile {
                    name,
                    fid,
                    logical,
                    allocated,
                    cloud,
                    inaccessible,
                },
            )
    }

    fn hardlink_scenario() -> impl Strategy<Value = Scenario> {
        (1usize..=6)
            .prop_flat_map(|k| {
                (
                    vec(any::<u32>(), k),
                    vec(0..NAMES.len(), k),
                    vec(vec(raw_file(), 0..=4), k),
                    vec(any::<u32>(), k),
                    vec(any::<u32>(), k),
                    vec(any::<u32>(), k),
                    vec(any::<u32>(), k),
                )
            })
            .prop_map(
                |(parents, names, raws, ids_a, order_a, ids_b, order_b)| Scenario {
                    folders: build(&parents, &names, &raws),
                    ids_a: ids_from(ids_a),
                    order_a: perm(&order_a),
                    ids_b: ids_from(ids_b),
                    order_b: perm(&order_b),
                },
            )
    }

    /// (file_id, caminho do modelo, NodeId na arena).
    type Path = Vec<Vec<u16>>;
    type ModelFile = (u64, Path, NodeId);

    /// Caminho de cada arquivo com file_id, lido da arena mas com nomes e estrutura do modelo.
    fn model_files(tree: &ScanTree, folders: &[Vec<MEntry>]) -> Vec<ModelFile> {
        fn collect_files(
            tree: &ScanTree,
            folders: &[Vec<MEntry>],
            node: NodeId,
            folder: usize,
            prefix: &mut Vec<Vec<u16>>,
            out: &mut Vec<ModelFile>,
        ) {
            let parent = tree.nodes[node as usize];
            for child in parent.first_child..parent.first_child + parent.child_count {
                let name = tree.name(child).to_vec();
                let entry = folders[folder]
                    .iter()
                    .find(|e| e.name == name)
                    .expect("filho sem entrada no modelo");
                prefix.push(name);
                match entry.sub {
                    Some(sub) => collect_files(tree, folders, child, sub, prefix, out),
                    None => {
                        if let Some(fid) = entry.fid {
                            out.push((fid, prefix.clone(), child));
                        }
                    }
                }
                prefix.pop();
            }
        }
        let mut out = Vec::new();
        collect_files(tree, folders, 0, 0, &mut Vec::new(), &mut out);
        out
    }

    /// Tamanho normalizado de um arquivo do modelo (regras de aggregate, aplicadas aqui).
    fn normalized(e: &MEntry) -> (u64, u64) {
        if e.inaccessible {
            (0, 0)
        } else {
            (e.logical, if e.cloud { 0 } else { e.allocated })
        }
    }

    /// Total esperado da raiz: cada file_id conta uma vez, mais os arquivos sem file_id.
    fn expected_total(folders: &[Vec<MEntry>]) -> (u64, u64) {
        let mut seen = std::collections::HashSet::new();
        let (mut logical, mut allocated) = (0, 0);
        for e in folders.iter().flatten().filter(|e| e.sub.is_none()) {
            if e.fid.is_none() || seen.insert(e.fid) {
                let (l, a) = normalized(e);
                logical += l;
                allocated += a;
            }
        }
        (logical, allocated)
    }

    proptest! {
        #[test]
        fn dedup_keeps_the_model_winner_and_the_totals(s in hardlink_scenario()) {
            let a = assemble(chunks_of(&s.folders, &s.ids_a, &s.order_a, false), "raiz".into());
            let files = model_files(&a.tree, &s.folders);
            let mut tree = a.tree;

            let removed = dedup_hardlinks(&mut tree, a.hardlink_candidates);

            // (c) retorno = Σ(tamanho do grupo − 1)
            let mut groups: BTreeMap<u64, Vec<(Path, NodeId)>> = BTreeMap::new();
            for (fid, path, id) in files {
                groups.entry(fid).or_default().push((path, id));
            }
            let expected_removed: u64 = groups.values().map(|g| g.len().saturating_sub(1) as u64).sum();
            prop_assert_eq!(removed, expected_removed);

            // (b) em cada grupo, exatamente um sem hardlink: o de menor caminho do modelo
            for members in groups.values() {
                let winner = members.iter().min_by(|x, y| x.0.cmp(&y.0)).expect("grupo não vazio").1;
                for (_, id) in members {
                    let n = tree.nodes[*id as usize];
                    if *id == winner {
                        prop_assert!(!n.hardlink(), "vencedor {} marcado", id);
                    } else {
                        prop_assert!(n.hardlink(), "secundário {} sem marca", id);
                        prop_assert_eq!((n.logical, n.allocated), (0, 0));
                    }
                }
            }
            prop_assert_eq!(tree.nodes.iter().filter(|n| n.hardlink()).count() as u64, removed);

            // (a) raiz = soma dos file_id distintos (com regras) + arquivos sem file_id
            let (logical, allocated) = expected_total(&s.folders);
            prop_assert_eq!((tree.nodes[0].logical, tree.nodes[0].allocated), (logical, allocated));
        }

        #[test]
        fn dedup_is_deterministic_across_ids_and_order(s in hardlink_scenario()) {
            let a = assemble(chunks_of(&s.folders, &s.ids_a, &s.order_a, false), "raiz".into());
            let b = assemble(chunks_of(&s.folders, &s.ids_b, &s.order_b, true), "raiz".into());
            let (mut ta, mut tb) = (a.tree, b.tree);
            dedup_hardlinks(&mut ta, a.hardlink_candidates);
            dedup_hardlinks(&mut tb, b.hardlink_candidates);
            prop_assert_eq!(&ta.nodes, &tb.nodes);
            prop_assert_eq!(&ta.names, &tb.names);
        }
    }

    #[test]
    fn root_only_tree_has_nothing_to_dedup() {
        let mut tree = assemble(vec![chunk(0, &[])], "raiz".into()).tree;
        assert_eq!(dedup_hardlinks(&mut tree, Vec::new()), 0);
        assert_eq!(tree.nodes[0].parent, NO_PARENT);
    }
}
