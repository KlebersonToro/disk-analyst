//! Helpers compartilhados pelos testes de montagem e deduplicação. Só existe em `cargo test`.

use crate::assemble::{ChunkEntry, DirChunk};
use crate::names::to_text;
use crate::tree::{Kind, NodeId, ScanTree};

/// Especificação de uma entrada de chunk: (nome, kind, subdir, file_id, logical, allocated).
pub(crate) type Spec<'a> = (&'a str, Kind, Option<u32>, Option<u64>, u64, u64);

/// Chunk com as entradas dadas, nomes concatenados na ordem em que aparecem.
pub(crate) fn chunk(id: u32, specs: &[Spec]) -> DirChunk {
    let mut names = Vec::new();
    let mut entries = Vec::new();
    for &(name, kind, subdir, file_id, logical, allocated) in specs {
        let units: Vec<u16> = name.encode_utf16().collect();
        entries.push(ChunkEntry {
            name_off: names.len() as u32,
            name_len: units.len() as u16,
            kind,
            cloud: false,
            inaccessible: false,
            name_lossy: false,
            logical,
            allocated,
            file_id,
            subdir,
        });
        names.extend(units);
    }
    DirChunk {
        id,
        names,
        entries,
        inaccessible: false,
    }
}

/// Node cujo nome é `name`. Os nomes dos testes que usam isto são únicos na árvore.
pub(crate) fn finder(tree: &ScanTree, name: &str) -> NodeId {
    (0..tree.nodes.len() as NodeId)
        .find(|&id| to_text(tree.name(id)).0 == name)
        .unwrap_or_else(|| panic!("nome {name} não encontrado"))
}

/// Permutação dos índices ordenada pelas chaves (empate desfeito pelo índice).
pub(crate) fn perm(keys: &[u32]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..keys.len()).collect();
    idx.sort_by_key(|&i| (keys[i], i));
    idx
}

/// Ids temporários como permutação das posições; a raiz fica sempre com id 0.
pub(crate) fn ids_from(mut keys: Vec<u32>) -> Vec<u32> {
    keys[0] = 0;
    let order = perm(&keys);
    let mut ids = vec![0; keys.len()];
    for (pos, &folder) in order.iter().enumerate() {
        ids[folder] = pos as u32;
    }
    ids
}
