//! Benchmarks-base do núcleo (T019, Princípio I, FR-018): montagem + agregação e deduplicação de
//! hard links sobre cerca de 1 milhão de Itens sintéticos, em memória. Os dados são montados fora
//! do laço medido; `iter_batched` com `LargeInput` deixa o clone fora da medição.
//!
//! Rodar: `cargo bench -p disk-analyst-core --bench baseline`.

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};

use disk_analyst_core::assemble::{ChunkEntry, DirChunk, aggregate, assemble};
use disk_analyst_core::hardlinks::dedup_hardlinks;
use disk_analyst_core::tree::Kind;

/// 1000 pastas × 1000 arquivos = 1.000.000 de arquivos (mais 1000 pastas e a raiz).
const DIRS: usize = 1000;
const FILES_PER_DIR: usize = 1000;

fn entry(
    name: &str,
    kind: Kind,
    subdir: Option<u32>,
    size: u64,
    file_id: Option<u64>,
    names: &mut Vec<u16>,
) -> ChunkEntry {
    let units: Vec<u16> = name.encode_utf16().collect();
    let entry = ChunkEntry {
        name_off: u32::try_from(names.len()).expect("offset cabe em u32"),
        name_len: u16::try_from(units.len()).expect("nome curto"),
        kind,
        cloud: false,
        inaccessible: false,
        name_lossy: false,
        logical: size,
        allocated: size,
        file_id,
        subdir,
    };
    names.extend(units);
    entry
}

/// Um chunk raiz com `DIRS` pastas e um chunk por pasta com `FILES_PER_DIR` arquivos. O tamanho e
/// o `file_id` de cada arquivo saem do índice global do arquivo.
fn build_chunks(
    file_id: impl Fn(usize) -> Option<u64>,
    size: impl Fn(usize) -> u64,
) -> Vec<DirChunk> {
    let mut chunks = Vec::with_capacity(DIRS + 1);

    let mut names = Vec::new();
    let entries = (0..DIRS)
        .map(|d| {
            let id = u32::try_from(d + 1).expect("id cabe em u32");
            entry(
                &format!("d{d:04}"),
                Kind::Dir,
                Some(id),
                0,
                None,
                &mut names,
            )
        })
        .collect();
    chunks.push(DirChunk {
        id: 0,
        names,
        entries,
        inaccessible: false,
    });

    for d in 0..DIRS {
        let mut names = Vec::new();
        let entries = (0..FILES_PER_DIR)
            .map(|i| {
                let global = d * FILES_PER_DIR + i;
                entry(
                    &format!("f{i:04}"),
                    Kind::File,
                    None,
                    size(global),
                    file_id(global),
                    &mut names,
                )
            })
            .collect();
        chunks.push(DirChunk {
            id: u32::try_from(d + 1).expect("id cabe em u32"),
            names,
            entries,
            inaccessible: false,
        });
    }
    chunks
}

/// Bench A: montagem determinística + agregação de ~1 milhão de Nodes.
fn assemble_and_aggregate(c: &mut Criterion) {
    let chunks = build_chunks(
        |_| None,
        |global| 1 + u64::try_from(global % 4096).unwrap_or(0) * 16,
    );
    let mut group = c.benchmark_group("montagem");
    group.sample_size(10);
    group.bench_function("assemble_aggregate_1M_nodes", |b| {
        b.iter_batched(
            || chunks.clone(),
            |chunks| {
                let mut tree = assemble(chunks, "bench".into()).tree;
                aggregate(&mut tree);
                tree
            },
            BatchSize::LargeInput,
        );
    });
    group.finish();
}

/// Bench B: deduplicação de 1 milhão de pares sobre uma árvore já montada. São 500 mil `file_id`
/// distintos, cada um em dois caminhos (as pastas `d` e `d + 500`).
fn dedup(c: &mut Criterion) {
    const DISTINCT: usize = 500_000;
    let size = |global: usize| 1 + u64::try_from(global % DISTINCT % 4096).unwrap_or(0) * 16;
    let file_id = |global: usize| Some(u64::try_from(global % DISTINCT).unwrap_or(0));
    let assembled = assemble(build_chunks(file_id, size), "bench".into());

    let mut group = c.benchmark_group("hardlinks");
    group.sample_size(10);
    group.bench_function("dedup_1M_pairs", |b| {
        b.iter_batched(
            || {
                (
                    assembled.tree.clone(),
                    assembled.hardlink_candidates.clone(),
                )
            },
            |(mut tree, candidates)| {
                let removed = dedup_hardlinks(&mut tree, candidates);
                (tree, removed)
            },
            BatchSize::LargeInput,
        );
    });
    group.finish();
}

criterion_group!(benches, assemble_and_aggregate, dedup);
criterion_main!(benches);
