//! Montagem determinística da árvore a partir dos chunks produzidos pelos varredores (R6).
//!
//! Cada pasta enumerada vira um [`DirChunk`] com id temporário. A ordem de chegada dos chunks e
//! os próprios ids dependem da execução paralela, então a montagem não usa nenhum dos dois para
//! decidir nada: ordena as entradas por nome e percorre as pastas em largura a partir da raiz.
//! Assim a mesma floresta gera sempre a mesma arena e o mesmo `NameBuf`.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::names::NameBuf;
use crate::tree::{Kind, NO_PARENT, Node, NodeId, ScanTree};

/// Uma entrada de pasta, como o enumerador a produziu.
#[derive(Debug, Clone)]
pub struct ChunkEntry {
    /// Offset do nome nos nomes locais do chunk.
    pub name_off: u32,
    pub name_len: u16,
    pub kind: Kind,
    pub cloud: bool,
    pub inaccessible: bool,
    pub name_lossy: bool,
    pub logical: u64,
    pub allocated: u64,
    /// Identidade do arquivo no volume, para deduplicar hard links (T009).
    pub file_id: Option<u64>,
    /// Id temporário do chunk que enumerou esta subpasta; `None` se ela não é percorrida.
    pub subdir: Option<u32>,
}

/// Conteúdo de uma pasta. O id 0 é sempre a raiz. O pai é implícito: a entrada cujo `subdir`
/// é este `id`.
#[derive(Debug, Clone)]
pub struct DirChunk {
    pub id: u32,
    pub names: Vec<u16>,
    pub entries: Vec<ChunkEntry>,
}

impl DirChunk {
    fn entry_name(&self, entry: &ChunkEntry) -> &[u16] {
        let start = entry.name_off as usize;
        &self.names[start..start + usize::from(entry.name_len)]
    }
}

/// Resultado da montagem.
#[derive(Debug)]
pub struct Assembled {
    pub tree: ScanTree,
    /// Um por entrada com `file_id`, já com o `NodeId` final.
    pub hardlink_candidates: Vec<(u64, NodeId)>,
}

/// Monta a árvore a partir de todos os chunks, em qualquer ordem.
///
/// # Panics
///
/// Se o chunk 0 (raiz) estiver ausente, se dois chunks tiverem o mesmo id, ou se o mesmo id
/// de chunk for referenciado por duas entradas. São invariantes do varredor, não dados do disco.
/// Um chunk que nunca é referenciado dispara `debug_assert!`. Um `subdir` sem chunk correspondente
/// não é erro: a pasta fica com `child_count = 0`.
pub fn assemble(chunks: Vec<DirChunk>, root_path: String) -> Assembled {
    let index: HashMap<u32, usize> = chunks
        .iter()
        .enumerate()
        .map(|(i, chunk)| (chunk.id, i))
        .collect();
    assert_eq!(index.len(), chunks.len(), "id de chunk duplicado");
    assert!(index.contains_key(&0), "chunk da raiz (id 0) ausente");

    let mut names = NameBuf::new();
    let (root_off, root_len) = names.push(&[]);
    let mut nodes = vec![Node::new(Kind::Dir, root_off, root_len, NO_PARENT)];
    let mut hardlink_candidates = Vec::new();

    let mut seen: HashSet<u32> = HashSet::from([0]);
    let mut queue: VecDeque<(NodeId, u32)> = VecDeque::from([(0, 0)]);
    let mut consumed = 0;

    while let Some((parent, chunk_id)) = queue.pop_front() {
        let chunk = &chunks[index[&chunk_id]];
        consumed += 1;

        let mut order: Vec<&ChunkEntry> = chunk.entries.iter().collect();
        order.sort_by(|a, b| chunk.entry_name(a).cmp(chunk.entry_name(b)));

        let first_child = nodes.len() as NodeId;
        for entry in &order {
            let (name_off, name_len) = names.push(chunk.entry_name(entry));
            let mut node = Node::new(entry.kind, name_off, name_len, parent);
            node.set_cloud(entry.cloud);
            node.set_inaccessible(entry.inaccessible);
            node.set_name_lossy(entry.name_lossy);
            node.logical = entry.logical;
            node.allocated = entry.allocated;

            let node_id = nodes.len() as NodeId;
            nodes.push(node);
            if let Some(file_id) = entry.file_id {
                hardlink_candidates.push((file_id, node_id));
            }
            if let Some(sub) = entry.subdir {
                assert!(
                    seen.insert(sub),
                    "chunk {sub} referenciado por duas entradas"
                );
                if index.contains_key(&sub) {
                    queue.push_back((node_id, sub));
                }
            }
        }

        if !order.is_empty() {
            let parent_node = &mut nodes[parent as usize];
            parent_node.first_child = first_child;
            parent_node.child_count = order.len() as u32;
        }
    }

    debug_assert_eq!(consumed, chunks.len(), "chunk órfão: nunca referenciado");

    Assembled {
        tree: ScanTree {
            nodes,
            names,
            root_path,
            partial: false,
            duration: std::time::Duration::ZERO,
        },
        hardlink_candidates,
    }
}

/// Agregação de baixo para cima (FR-011). Roda depois de [`assemble`] e de novo depois da
/// deduplicação de hard links (T009), por isso é idempotente.
///
/// # Invariante
///
/// `assemble` numera os Nodes em largura (BFS): todo filho tem índice maior que o do pai. Por
/// isso, percorrer os índices de `len - 1` até `1` processa cada filho antes do pai, e a soma de
/// cada pasta já está completa quando ela própria é processada. A correção depende disso; o
/// `debug_assert!` no laço checa.
///
/// # Regras (aplicadas antes de cada Node ser somado ao pai)
///
/// - Link, item inacessível e pasta `not_scanned`: tamanhos zero (a pasta também ignora os filhos).
/// - Nuvem: `allocated = 0`; `logical` é mantido.
///
/// # Panics
///
/// Se a soma de uma pasta estourar `u64`.
pub fn aggregate(tree: &mut ScanTree) {
    let nodes = &mut tree.nodes;
    for node in nodes.iter_mut().filter(|n| n.kind() == Kind::Dir) {
        node.logical = 0;
        node.allocated = 0;
    }
    for i in (1..nodes.len()).rev() {
        let parent = nodes[i].parent as usize;
        debug_assert!(
            parent < i,
            "pai {parent} deve ter índice menor que o filho {i}"
        );
        normalize(&mut nodes[i]);
        let (logical, allocated) = (nodes[i].logical, nodes[i].allocated);
        let target = &mut nodes[parent];
        target.logical = target
            .logical
            .checked_add(logical)
            .expect("soma de tamanhos estourou u64");
        target.allocated = target
            .allocated
            .checked_add(allocated)
            .expect("soma de tamanhos estourou u64");
    }
    if let Some(root) = nodes.first_mut() {
        normalize(root);
    }
}

/// Aplica as regras de tamanho de um Node.
fn normalize(node: &mut Node) {
    if node.kind() == Kind::Link || node.inaccessible() || node.not_scanned() {
        node.logical = 0;
        node.allocated = 0;
    }
    if node.cloud() {
        node.allocated = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::to_text;
    use crate::tree::NO_PARENT;
    use proptest::collection::vec;
    use proptest::prelude::*;

    /// Modelo de uma entrada de pasta, independente de ids e da ordem de entrega.
    #[derive(Debug, Clone)]
    struct MEntry {
        name: Vec<u16>,
        kind: Kind,
        cloud: bool,
        inaccessible: bool,
        name_lossy: bool,
        logical: u64,
        allocated: u64,
        file_id: Option<u64>,
        /// Índice da pasta-modelo que esta entrada representa, se for percorrida.
        subdir: Option<usize>,
    }

    #[derive(Debug, Clone)]
    struct RawEntry {
        units: Vec<u16>,
        kind: Kind,
        cloud: bool,
        inaccessible: bool,
        name_lossy: bool,
        logical: u64,
        allocated: u64,
        file_id: Option<u64>,
    }

    impl From<RawEntry> for MEntry {
        fn from(r: RawEntry) -> Self {
            MEntry {
                name: r.units,
                kind: r.kind,
                cloud: r.cloud,
                inaccessible: r.inaccessible,
                name_lossy: r.name_lossy,
                logical: r.logical,
                allocated: r.allocated,
                file_id: r.file_id,
                subdir: None,
            }
        }
    }

    #[derive(Debug)]
    struct Scenario {
        forest: Vec<Vec<MEntry>>,
        ids_a: Vec<u32>,
        order_a: Vec<usize>,
        ids_b: Vec<u32>,
        order_b: Vec<usize>,
    }

    fn raw_entry() -> impl Strategy<Value = RawEntry> {
        (
            vec(any::<u16>(), 0..6),
            prop_oneof![Just(Kind::File), Just(Kind::Dir), Just(Kind::Link)],
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            0..1_000_000u64,
            0..1_000_000u64,
            proptest::option::of(0..50u64),
        )
            .prop_map(
                |(units, kind, cloud, inaccessible, name_lossy, logical, allocated, file_id)| {
                    RawEntry {
                        units,
                        kind,
                        cloud,
                        inaccessible,
                        name_lossy,
                        logical,
                        allocated,
                        file_id,
                    }
                },
            )
    }

    /// Floresta com pastas 0..k: cada pasta i > 0 é filha de uma pasta de índice menor, então
    /// todas são alcançáveis a partir da raiz.
    fn build_forest(parents: &[u32], raws: Vec<Vec<RawEntry>>) -> Vec<Vec<MEntry>> {
        let mut folders: Vec<Vec<MEntry>> = raws
            .into_iter()
            .map(|raw| raw.into_iter().map(MEntry::from).collect())
            .collect();
        for (child, &key) in parents.iter().enumerate().skip(1).take(folders.len() - 1) {
            let parent = key as usize % child;
            folders[parent].push(MEntry {
                name: Vec::new(),
                kind: Kind::Dir,
                cloud: false,
                inaccessible: false,
                name_lossy: false,
                logical: 0,
                allocated: 0,
                file_id: None,
                subdir: Some(child),
            });
        }
        // Sufixo "@posição": nomes únicos por pasta, mesmo com unidades arbitrárias.
        for folder in &mut folders {
            for (pos, entry) in folder.iter_mut().enumerate() {
                entry.name.extend(format!("@{pos}").encode_utf16());
            }
        }
        folders
    }

    /// Permutação dos índices ordenada pelas chaves (empate desfeito pelo índice).
    fn perm(keys: &[u32]) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..keys.len()).collect();
        idx.sort_by_key(|&i| (keys[i], i));
        idx
    }

    /// Ids temporários como permutação das posições; a raiz fica sempre com id 0.
    fn ids_from(mut keys: Vec<u32>) -> Vec<u32> {
        keys[0] = 0;
        let order = perm(&keys);
        let mut ids = vec![0; keys.len()];
        for (pos, &folder) in order.iter().enumerate() {
            ids[folder] = pos as u32;
        }
        ids
    }

    fn scenario() -> impl Strategy<Value = Scenario> {
        (1usize..=10)
            .prop_flat_map(|k| {
                (
                    vec(any::<u32>(), k),
                    vec(vec(raw_entry(), 0..=6), k),
                    vec(any::<u32>(), k),
                    vec(any::<u32>(), k),
                    vec(any::<u32>(), k),
                    vec(any::<u32>(), k),
                )
            })
            .prop_map(|(parents, raws, ids_a, order_a, ids_b, order_b)| Scenario {
                forest: build_forest(&parents, raws),
                ids_a: ids_from(ids_a),
                order_a: perm(&order_a),
                ids_b: ids_from(ids_b),
                order_b: perm(&order_b),
            })
    }

    /// Converte o modelo em chunks: ids dados por `ids`, emitidos na ordem `order`. Com
    /// `reverse`, as entradas de cada chunk também saem invertidas.
    fn build_chunks(
        forest: &[Vec<MEntry>],
        ids: &[u32],
        order: &[usize],
        reverse: bool,
    ) -> Vec<DirChunk> {
        order
            .iter()
            .map(|&folder| {
                let mut names = Vec::new();
                let mut entries = Vec::new();
                for e in &forest[folder] {
                    entries.push(ChunkEntry {
                        name_off: names.len() as u32,
                        name_len: e.name.len() as u16,
                        kind: e.kind,
                        cloud: e.cloud,
                        inaccessible: e.inaccessible,
                        name_lossy: e.name_lossy,
                        logical: e.logical,
                        allocated: e.allocated,
                        file_id: e.file_id,
                        subdir: e.subdir.map(|g| ids[g]),
                    });
                    names.extend_from_slice(&e.name);
                }
                if reverse {
                    entries.reverse();
                }
                DirChunk {
                    id: ids[folder],
                    names,
                    entries,
                }
            })
            .collect()
    }

    /// Confere a árvore montada contra o modelo. Devolve os candidatos a hard link encontrados.
    fn check_against_model(tree: &ScanTree, forest: &[Vec<MEntry>]) -> Vec<(u64, NodeId)> {
        assert_eq!(
            tree.nodes.len(),
            1 + forest.iter().map(Vec::len).sum::<usize>(),
            "total de Nodes = raiz + entradas"
        );
        let mut visited = Vec::new();
        let mut candidates = Vec::new();
        walk(tree, forest, 0, 0, &mut visited, &mut candidates);
        visited.sort_unstable();
        assert_eq!(
            visited,
            (1..tree.nodes.len()).collect::<Vec<_>>(),
            "cada Node não raiz aparece como filho de exatamente um pai"
        );
        candidates.sort_unstable();
        candidates
    }

    fn walk(
        tree: &ScanTree,
        forest: &[Vec<MEntry>],
        node: NodeId,
        folder: usize,
        visited: &mut Vec<usize>,
        candidates: &mut Vec<(u64, NodeId)>,
    ) {
        let parent = tree.nodes[node as usize];
        assert_eq!(
            parent.child_count as usize,
            forest[folder].len(),
            "número de filhos do Node {node}"
        );
        let mut previous: Option<&[u16]> = None;
        for child_id in parent.first_child..parent.first_child + parent.child_count {
            visited.push(child_id as usize);
            let child = tree.nodes[child_id as usize];
            assert_eq!(child.parent, node, "parent do Node {child_id}");

            let name = tree.name(child_id);
            if let Some(prev) = previous {
                assert!(prev < name, "filhos de {node} fora de ordem por nome");
            }
            previous = Some(name);

            let e = forest[folder]
                .iter()
                .find(|e| e.name.as_slice() == name)
                .expect("filho sem entrada correspondente no modelo");
            assert_eq!(child.kind(), e.kind, "kind do Node {child_id}");
            assert_eq!(child.cloud(), e.cloud, "cloud do Node {child_id}");
            assert_eq!(
                child.inaccessible(),
                e.inaccessible,
                "inaccessible do Node {child_id}"
            );
            assert_eq!(
                child.name_lossy(),
                e.name_lossy,
                "name_lossy do Node {child_id}"
            );
            assert_eq!(child.logical, e.logical, "logical do Node {child_id}");
            assert_eq!(child.allocated, e.allocated, "allocated do Node {child_id}");
            if let Some(fid) = e.file_id {
                candidates.push((fid, child_id));
            }
            match e.subdir {
                Some(sub) => walk(tree, forest, child_id, sub, visited, candidates),
                None => assert_eq!(child.child_count, 0, "pasta não percorrida sem filhos"),
            }
        }
    }

    proptest! {
        #[test]
        fn output_does_not_depend_on_ids_or_chunk_order(s in scenario()) {
            let a = assemble(build_chunks(&s.forest, &s.ids_a, &s.order_a, false), "raiz".into());
            let b = assemble(build_chunks(&s.forest, &s.ids_b, &s.order_b, true), "raiz".into());
            prop_assert_eq!(&a.tree.nodes, &b.tree.nodes);
            prop_assert_eq!(&a.tree.names, &b.tree.names);
            let mut ca = a.hardlink_candidates.clone();
            ca.sort_unstable();
            let mut cb = b.hardlink_candidates.clone();
            cb.sort_unstable();
            prop_assert_eq!(ca, cb);
        }

        #[test]
        fn result_matches_the_model_and_its_invariants(s in scenario()) {
            let a = assemble(build_chunks(&s.forest, &s.ids_a, &s.order_a, false), "raiz".into());
            let expected_candidates = check_against_model(&a.tree, &s.forest);
            let mut got = a.hardlink_candidates.clone();
            got.sort_unstable();
            prop_assert_eq!(got, expected_candidates);
        }
    }

    fn chunk(id: u32, entries: &[(&str, Kind, Option<u32>)]) -> DirChunk {
        let mut names = Vec::new();
        let mut out = Vec::new();
        for &(name, kind, subdir) in entries {
            let units: Vec<u16> = name.encode_utf16().collect();
            out.push(ChunkEntry {
                name_off: names.len() as u32,
                name_len: units.len() as u16,
                kind,
                cloud: false,
                inaccessible: false,
                name_lossy: false,
                logical: 0,
                allocated: 0,
                file_id: None,
                subdir,
            });
            names.extend(units);
        }
        DirChunk {
            id,
            names,
            entries: out,
        }
    }

    #[test]
    fn small_tree_is_sorted_and_linked_by_hand() {
        let root = chunk(0, &[("b", Kind::File, None), ("a", Kind::Dir, Some(1))]);
        let a = chunk(1, &[("x", Kind::File, None)]);

        let tree = assemble(vec![root, a], "C:\\teste".into()).tree;

        let names: Vec<String> = (0..tree.nodes.len() as NodeId)
            .map(|id| to_text(tree.name(id)).0)
            .collect();
        assert_eq!(names, ["", "a", "b", "x"]);
        assert_eq!(
            tree.nodes.iter().map(|n| n.parent).collect::<Vec<_>>(),
            [NO_PARENT, 0, 0, 1]
        );
        assert_eq!(
            tree.nodes.iter().map(|n| n.first_child).collect::<Vec<_>>(),
            [1, 3, 0, 0]
        );
        assert_eq!(
            tree.nodes.iter().map(|n| n.child_count).collect::<Vec<_>>(),
            [2, 1, 0, 0]
        );
        assert_eq!(
            tree.nodes.iter().map(|n| n.kind()).collect::<Vec<_>>(),
            [Kind::Dir, Kind::Dir, Kind::File, Kind::File]
        );
        assert_eq!(tree.root_path, "C:\\teste");
    }

    #[test]
    fn subdir_without_chunk_has_no_children_and_does_not_panic() {
        let root = chunk(0, &[("d", Kind::Dir, Some(9))]);

        let tree = assemble(vec![root], "raiz".into()).tree;

        assert_eq!(tree.nodes.len(), 2);
        assert_eq!(tree.nodes[1].child_count, 0);
        assert_eq!(tree.nodes[1].first_child, 0);
    }

    /// Árvore montada e agregada a partir do cenário; devolve (antes, depois) para que os
    /// testes calculem o esperado a partir dos valores de entrada, sem depender de `aggregate`.
    fn before_and_after(s: &Scenario) -> (ScanTree, ScanTree) {
        let before = assemble(
            build_chunks(&s.forest, &s.ids_a, &s.order_a, false),
            "raiz".into(),
        )
        .tree;
        let mut after = before.clone();
        aggregate(&mut after);
        (before, after)
    }

    fn children(tree: &ScanTree, id: usize) -> std::ops::Range<usize> {
        let node = tree.nodes[id];
        node.first_child as usize..(node.first_child + node.child_count) as usize
    }

    fn finder(tree: &ScanTree, name: &str) -> NodeId {
        (0..tree.nodes.len() as NodeId)
            .find(|&id| to_text(tree.name(id)).0 == name)
            .unwrap_or_else(|| panic!("nome {name} não encontrado"))
    }

    proptest! {
        #[test]
        fn folders_sum_their_direct_children(s in scenario()) {
            let (_, tree) = before_and_after(&s);
            for id in 0..tree.nodes.len() {
                let node = tree.nodes[id];
                if node.kind() != Kind::Dir {
                    continue;
                }
                let (logical, allocated) = if node.inaccessible() || node.not_scanned() {
                    (0, 0)
                } else {
                    children(&tree, id).fold((0u64, 0u64), |(l, a), c| {
                        (l + tree.nodes[c].logical, a + tree.nodes[c].allocated)
                    })
                };
                prop_assert_eq!((node.logical, node.allocated), (logical, allocated), "pasta {}", id);
            }
        }

        #[test]
        fn root_equals_the_flat_sum_of_files_with_the_rules(s in scenario()) {
            let (before, after) = before_and_after(&s);
            // Soma plana, a partir dos valores de entrada, com as regras aplicadas aqui.
            let (mut logical, mut allocated) = (0u64, 0u64);
            for node in &before.nodes {
                if node.kind() == Kind::File && !node.inaccessible() {
                    logical += node.logical;
                    if !node.cloud() {
                        allocated += node.allocated;
                    }
                }
            }
            prop_assert_eq!((after.nodes[0].logical, after.nodes[0].allocated), (logical, allocated));
        }

        #[test]
        fn normalization_rules_hold_for_every_node(s in scenario()) {
            let (_, tree) = before_and_after(&s);
            for (id, node) in tree.nodes.iter().enumerate() {
                if node.kind() == Kind::Link || node.inaccessible() || node.not_scanned() {
                    prop_assert_eq!((node.logical, node.allocated), (0, 0), "Node {}", id);
                }
                if node.cloud() {
                    prop_assert_eq!(node.allocated, 0, "Node {}", id);
                }
            }
        }

        #[test]
        fn aggregate_is_idempotent(s in scenario()) {
            let (_, once) = before_and_after(&s);
            let mut twice = once.clone();
            aggregate(&mut twice);
            prop_assert_eq!(&once.nodes, &twice.nodes);
        }
    }

    #[test]
    fn hand_built_tree_aggregates_by_hand() {
        let mut root = chunk(
            0,
            &[
                ("a", Kind::Dir, Some(1)),
                ("b", Kind::Link, None),
                ("c", Kind::File, None),
            ],
        );
        root.entries[1].logical = 99;
        root.entries[1].allocated = 99;
        root.entries[2].logical = 7;
        root.entries[2].allocated = 7;
        root.entries[2].inaccessible = true;
        let mut a = chunk(1, &[("x", Kind::File, None), ("y", Kind::File, None)]);
        a.entries[0].logical = 10;
        a.entries[0].allocated = 12;
        a.entries[1].logical = 5;
        a.entries[1].allocated = 8;
        a.entries[1].cloud = true;

        let mut tree = assemble(vec![root, a], "raiz".into()).tree;
        aggregate(&mut tree);

        let size = |name: &str| {
            let n = tree.nodes[finder(&tree, name) as usize];
            (n.logical, n.allocated)
        };
        assert_eq!(size("a"), (15, 12));
        assert_eq!(size("b"), (0, 0));
        assert_eq!(size("c"), (0, 0));
        assert_eq!(size("y"), (5, 0));
        assert_eq!(size("x"), (10, 12));
        assert_eq!((tree.nodes[0].logical, tree.nodes[0].allocated), (15, 12));
    }

    #[test]
    fn tree_with_only_the_root_aggregates_to_zero() {
        let mut tree = assemble(vec![chunk(0, &[])], "raiz".into()).tree;
        aggregate(&mut tree);
        assert_eq!((tree.nodes[0].logical, tree.nodes[0].allocated), (0, 0));
    }
}
