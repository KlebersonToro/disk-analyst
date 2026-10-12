//! Arena de Itens: `Vec<Node>` compacto (data-model.md, seção Node).

use crate::names::NameBuf;
use std::time::Duration;

/// Índice de um [`Node`] na arena.
pub type NodeId = u32;

/// Valor de `parent` da raiz.
pub const NO_PARENT: NodeId = u32::MAX;

const KIND_MASK: u16 = 0b11;
const KIND_FILE: u16 = 0;
const KIND_DIR: u16 = 1;
const KIND_LINK: u16 = 2;

const CLOUD: u16 = 1 << 2;
const INACCESSIBLE: u16 = 1 << 3;
const HARDLINK: u16 = 1 << 4;
const NAME_LOSSY: u16 = 1 << 5;
const NOT_SCANNED: u16 = 1 << 6;

/// Tipo do Item, codificado em 2 bits de `flags`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
    /// Link não seguido.
    Link,
}

/// Um Item da árvore. No máximo 40 bytes (meta de design; SC-003 exige < 64).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    pub name_off: u32,
    pub name_len: u16,
    flags: u16,
    pub parent: NodeId,
    pub first_child: NodeId,
    pub child_count: u32,
    pub logical: u64,
    pub allocated: u64,
}

const _: () = assert!(size_of::<Node>() <= 40);

/// Árvore completa de uma varredura: arena de Itens e nomes. Índice 0 é a raiz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanTree {
    pub nodes: Vec<Node>,
    pub names: NameBuf,
    pub root_path: String,
    pub partial: bool,
    pub duration: Duration,
}

impl ScanTree {
    /// Nome do Item `id`, em UTF-16 bruto.
    pub fn name(&self, id: NodeId) -> &[u16] {
        let node = &self.nodes[id as usize];
        self.names.get(node.name_off, node.name_len)
    }

    /// Posição de cada Item na pré-ordem: `rank[id]` é o índice de `id` nessa ordem.
    ///
    /// Pré-ordem = pai antes dos filhos, filhos na ordem de `first_child`. **Fato**: `assemble`
    /// ordena os filhos de cada pasta por nome (nomes únicos entre irmãos), então esta pré-ordem é
    /// exatamente a ordem lexicográfica dos caminhos por componentes. Se dois caminhos divergem no
    /// componente k, o irmão de nome menor e toda a sua subárvore vêm antes; se um é prefixo do
    /// outro, o ancestral vem antes, como em `Vec::cmp`. Logo, comparar ranks equivale a comparar
    /// caminhos sem montá-los.
    ///
    /// Iterativo, com pilha explícita (filhos empilhados em ordem reversa), O(n).
    pub fn preorder_ranks(&self) -> Vec<u32> {
        let mut rank = vec![0; self.nodes.len()];
        let mut stack: Vec<NodeId> = vec![0];
        let mut next = 0;
        while let Some(id) = stack.pop() {
            rank[id as usize] = next;
            next += 1;
            let node = self.nodes[id as usize];
            stack.extend((node.first_child..node.first_child + node.child_count).rev());
        }
        rank
    }

    /// Componentes do caminho do Item `id`, da raiz para a folha (sem a própria raiz). Ordenar
    /// essas sequências compara componente a componente, em ordinal UTF-16.
    pub fn path_components(&self, mut id: NodeId) -> Vec<&[u16]> {
        let mut parts = Vec::new();
        while self.nodes[id as usize].parent != NO_PARENT {
            parts.push(self.name(id));
            id = self.nodes[id as usize].parent;
        }
        parts.reverse();
        parts
    }
}

macro_rules! flag_accessors {
    ($($get:ident, $set:ident, $bit:ident;)*) => {$(
        pub fn $get(&self) -> bool {
            self.flags & $bit != 0
        }

        pub fn $set(&mut self, on: bool) {
            if on {
                self.flags |= $bit;
            } else {
                self.flags &= !$bit;
            }
        }
    )*};
}

impl Node {
    /// Item sem tamanhos, sem filhos e com todas as flags limpas além de `kind`.
    pub fn new(kind: Kind, name_off: u32, name_len: u16, parent: NodeId) -> Self {
        let mut node = Self {
            name_off,
            name_len,
            flags: 0,
            parent,
            first_child: 0,
            child_count: 0,
            logical: 0,
            allocated: 0,
        };
        node.set_kind(kind);
        node
    }

    pub fn kind(&self) -> Kind {
        match self.flags & KIND_MASK {
            KIND_FILE => Kind::File,
            KIND_DIR => Kind::Dir,
            KIND_LINK => Kind::Link,
            // `flags` é privado e só `set_kind` escreve os bits de kind, sempre com um dos três
            // valores acima; 0b11 é inalcançável.
            _ => unreachable!("padrão inválido nos bits de kind"),
        }
    }

    pub fn set_kind(&mut self, kind: Kind) {
        let bits = match kind {
            Kind::File => KIND_FILE,
            Kind::Dir => KIND_DIR,
            Kind::Link => KIND_LINK,
        };
        self.flags = (self.flags & !KIND_MASK) | bits;
    }

    flag_accessors! {
        cloud, set_cloud, CLOUD;
        inaccessible, set_inaccessible, INACCESSIBLE;
        hardlink, set_hardlink, HARDLINK;
        name_lossy, set_name_lossy, NAME_LOSSY;
        not_scanned, set_not_scanned, NOT_SCANNED;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    type Getter = fn(&Node) -> bool;
    type Setter = fn(&mut Node, bool);

    const FLAGS: [(&str, Getter, Setter); 5] = [
        ("cloud", Node::cloud, Node::set_cloud),
        ("inaccessible", Node::inaccessible, Node::set_inaccessible),
        ("hardlink", Node::hardlink, Node::set_hardlink),
        ("name_lossy", Node::name_lossy, Node::set_name_lossy),
        ("not_scanned", Node::not_scanned, Node::set_not_scanned),
    ];

    const KINDS: [Kind; 3] = [Kind::File, Kind::Dir, Kind::Link];

    fn flag_states(node: &Node) -> [bool; 5] {
        FLAGS.map(|(_, get, _)| get(node))
    }

    fn new_node(kind: Kind) -> Node {
        Node::new(kind, 0, 0, NO_PARENT)
    }

    #[test]
    fn node_size_is_reported() {
        assert_eq!(size_of::<Node>(), 40);
    }

    #[test]
    fn each_flag_toggles_without_touching_the_others() {
        for kind in KINDS {
            for (i, (name, get, set)) in FLAGS.iter().enumerate() {
                let mut node = new_node(kind);
                set(&mut node, true);
                let mut expected = [false; 5];
                expected[i] = true;
                assert_eq!(flag_states(&node), expected, "ligar {name}");
                assert_eq!(node.kind(), kind, "ligar {name} alterou kind");
                assert!(get(&node));

                set(&mut node, false);
                assert_eq!(flag_states(&node), [false; 5], "desligar {name}");
                assert_eq!(node.kind(), kind, "desligar {name} alterou kind");
            }
        }
    }

    #[test]
    fn set_kind_preserves_flags_and_flags_preserve_kind() {
        for from in KINDS {
            for to in KINDS {
                let mut node = new_node(from);
                for (_, _, set) in FLAGS {
                    set(&mut node, true);
                }
                node.set_kind(to);
                assert_eq!(node.kind(), to);
                assert_eq!(flag_states(&node), [true; 5]);
            }
        }
    }

    #[test]
    fn new_keeps_identity_and_zeroes_the_rest() {
        let node = Node::new(Kind::Dir, 7, 3, NO_PARENT);
        assert_eq!(node.kind(), Kind::Dir);
        assert_eq!((node.name_off, node.name_len), (7, 3));
        assert_eq!(node.parent, NO_PARENT);
        assert_eq!((node.first_child, node.child_count), (0, 0));
        assert_eq!((node.logical, node.allocated), (0, 0));
        assert_eq!(flag_states(&node), [false; 5]);
    }

    #[derive(Debug, Clone)]
    enum Op {
        Kind(Kind),
        Flag(usize, bool),
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            proptest::sample::select(KINDS.to_vec()).prop_map(Op::Kind),
            (0..5usize, any::<bool>()).prop_map(|(i, v)| Op::Flag(i, v)),
        ]
    }

    proptest! {
        #[test]
        fn random_operations_match_a_simple_model(ops in proptest::collection::vec(op(), 0..64)) {
            let mut node = new_node(Kind::File);
            let mut kind = Kind::File;
            let mut flags = [false; 5];
            for op in ops {
                match op {
                    Op::Kind(k) => {
                        node.set_kind(k);
                        kind = k;
                    }
                    Op::Flag(i, v) => {
                        FLAGS[i].2(&mut node, v);
                        flags[i] = v;
                    }
                }
                prop_assert_eq!(node.kind(), kind);
                prop_assert_eq!(flag_states(&node), flags);
            }
        }
    }
}

#[cfg(test)]
mod preorder_tests {
    use super::*;
    use crate::assemble::{Assembled, DirChunk, assemble};
    use crate::testgen::{Spec, chunk, ids_from};
    use proptest::collection::vec;
    use proptest::prelude::*;
    use std::collections::HashSet;

    const NAMES: [&str; 4] = ["a", "a1", "b", "x"];
    const SUBS: [&str; 5] = ["s1", "s2", "s3", "s4", "s5"];

    /// Árvore pequena: até 6 pastas, nomes com prefixos (a, a1) para exercitar prefixo e irmão.
    fn small_tree() -> impl Strategy<Value = ScanTree> {
        (1usize..=6)
            .prop_flat_map(|k| {
                (
                    vec(any::<u32>(), k),
                    vec(any::<u32>(), k),
                    vec(vec((0..NAMES.len(), any::<bool>()), 0..=4), k),
                )
            })
            .prop_map(|(parents, keys, files)| {
                let ids = ids_from(keys);
                let mut specs: Vec<Vec<Spec>> = vec![Vec::new(); parents.len()];
                for child in 1..parents.len() {
                    let parent = parents[child] as usize % child;
                    specs[parent].push((SUBS[child - 1], Kind::Dir, Some(ids[child]), None, 0, 0));
                }
                for (f, list) in files.iter().enumerate() {
                    let mut seen = HashSet::new();
                    for &(name, link) in list {
                        if seen.insert(name) {
                            let kind = if link { Kind::Link } else { Kind::File };
                            specs[f].push((NAMES[name], kind, None, None, 0, 0));
                        }
                    }
                }
                let chunks: Vec<DirChunk> =
                    (0..specs.len()).map(|f| chunk(ids[f], &specs[f])).collect();
                let Assembled { tree, .. } = assemble(chunks, "raiz".into());
                tree
            })
    }

    proptest! {
        #[test]
        fn preorder_rank_order_equals_component_order(tree in small_tree()) {
            let rank = tree.preorder_ranks();
            for a in 0..tree.nodes.len() as NodeId {
                for b in 0..tree.nodes.len() as NodeId {
                    prop_assert_eq!(
                        rank[a as usize].cmp(&rank[b as usize]),
                        tree.path_components(a).cmp(&tree.path_components(b)),
                        "pares ({}, {})", a, b
                    );
                }
            }
        }
    }
}
