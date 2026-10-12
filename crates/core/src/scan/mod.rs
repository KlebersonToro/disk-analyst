//! Varredura de diretórios (T018): as peças se juntam. O enumerador da plataforma (T015 no Linux,
//! T017 no Windows) produz um `DirChunk` por pasta; o varredor paralelo (`parallel`) reúne os
//! chunks; `assemble` (T007) monta a árvore e `aggregate` (T008) soma os tamanhos.
//!
//! Fora de escopo, com tarefa própria: dedup de hard links (T032), cancelamento real com
//! `not_scanned` (T036), classificação de erro NotFound × negado (T033), links e nuvem
//! (T030/T031).

use std::fmt;
use std::fs::{self, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU32;
use std::time::{Duration, Instant};

use crate::assemble::{DirChunk, aggregate, assemble};
use crate::control::{CancelToken, ScanCounters};
use crate::names::NameBuf;
use crate::tree::{Kind, NO_PARENT, Node, ScanTree};

mod parallel;
#[cfg(unix)]
pub(crate) mod unix;
pub(crate) mod windows;

use parallel::Platform;

/// Resultado de enumerar uma pasta: o chunk e as subpastas a percorrer, cada uma com o id
/// temporário já gravado em `ChunkEntry::subdir`. Compartilhado pelos enumeradores de Linux e de
/// Windows.
pub(crate) struct DirChunkWork {
    pub chunk: DirChunk,
    pub subdirs: Vec<(u32, PathBuf)>,
}

/// Opções da varredura. Vazia por ora: a CLI e as próximas tarefas acrescentam campos.
#[derive(Debug, Clone, Default)]
pub struct ScanOptions {}

/// Falha ao varrer a raiz.
#[derive(Debug)]
pub enum ScanError {
    /// O caminho pedido não existe.
    NotFound(PathBuf),
    /// O caminho existe mas não pôde ser lido.
    Unreadable { path: PathBuf, source: io::Error },
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScanError::NotFound(path) => write!(f, "caminho não encontrado: {}", path.display()),
            ScanError::Unreadable { path, source } => {
                write!(f, "não foi possível ler {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for ScanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ScanError::NotFound(_) => None,
            ScanError::Unreadable { source, .. } => Some(source),
        }
    }
}

fn unreadable(path: &Path) -> impl FnOnce(io::Error) -> ScanError + '_ {
    move |source| ScanError::Unreadable {
        path: path.to_path_buf(),
        source,
    }
}

/// Varre `path` e devolve a árvore já agregada. A raiz SEGUE links: um link para pasta é varrido
/// como a pasta, e um arquivo vira uma árvore de um Item. `root_path` da árvore é o texto do
/// caminho COMO FOI PEDIDO, não o canonicalizado.
///
/// `options` e `cancel` são recebidos e ignorados por ora: o cancelamento real (com pastas
/// `not_scanned`) é da T036. `counters` recebe `items`, `files` e `bytes` (este é o alocado).
pub fn scan(
    path: &Path,
    options: &ScanOptions,
    cancel: &CancelToken,
    counters: &ScanCounters,
) -> Result<ScanTree, ScanError> {
    let _ = (options, cancel);
    let started = Instant::now();

    let metadata = fs::metadata(path).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            ScanError::NotFound(path.to_path_buf())
        } else {
            ScanError::Unreadable {
                path: path.to_path_buf(),
                source,
            }
        }
    })?;
    let root_path = path.to_string_lossy().into_owned();

    let mut tree = if metadata.is_dir() {
        scan_folder(path, &metadata, root_path, counters)?
    } else {
        scan_file(path, &metadata, root_path, counters)?
    };
    tree.duration = started.elapsed();
    Ok(tree)
}

fn scan_folder(
    path: &Path,
    metadata: &Metadata,
    root_path: String,
    counters: &ScanCounters,
) -> Result<ScanTree, ScanError> {
    let next_id = AtomicU32::new(1);
    let (first, platform) = read_root(path, metadata, &next_id).map_err(unreadable(path))?;
    let chunks = parallel::walk(first, &next_id, counters, platform);
    let mut tree = assemble(chunks, root_path).tree;
    aggregate(&mut tree);
    Ok(tree)
}

/// Raiz que é arquivo: árvore de um Item.
fn scan_file(
    path: &Path,
    metadata: &Metadata,
    root_path: String,
    counters: &ScanCounters,
) -> Result<ScanTree, ScanError> {
    let (logical, allocated) = file_sizes(path, metadata).map_err(unreadable(path))?;
    let mut names = NameBuf::new();
    let (name_off, name_len) = names.push(&[]);
    let mut node = Node::new(Kind::File, name_off, name_len, NO_PARENT);
    node.logical = logical;
    node.allocated = allocated;

    counters.add_items(1);
    counters.add_files(1);
    counters.add_bytes(allocated);
    Ok(ScanTree {
        nodes: vec![node],
        names,
        root_path,
        partial: false,
        duration: Duration::ZERO,
    })
}

/// Enumera a raiz. Linux: guarda o dispositivo dela para os hard links.
#[cfg(unix)]
fn read_root(
    path: &Path,
    metadata: &Metadata,
    next_id: &AtomicU32,
) -> io::Result<(DirChunkWork, Platform)> {
    use std::os::unix::fs::MetadataExt;
    let root_dev = metadata.dev();
    let first = unix::read_dir_chunk(path, 0, next_id, root_dev)?;
    Ok((first, Platform { root_dev }))
}

/// Enumera a raiz. Windows: canonicaliza (forma verbatim, sem o limite de 260) e abre SEGUINDO
/// links; os filhos herdam o prefixo.
#[cfg(windows)]
fn read_root(
    path: &Path,
    _metadata: &Metadata,
    next_id: &AtomicU32,
) -> io::Result<(DirChunkWork, Platform)> {
    let root = fs::canonicalize(path)?;
    let first = windows::read_dir_chunk(&root, 0, next_id, true)?;
    Ok((first, Platform))
}

#[cfg(unix)]
fn file_sizes(_path: &Path, metadata: &Metadata) -> io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let allocated = metadata
        .blocks()
        .checked_mul(512)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "alocado estoura u64"))?;
    Ok((metadata.len(), allocated))
}

#[cfg(windows)]
fn file_sizes(path: &Path, _metadata: &Metadata) -> io::Result<(u64, u64)> {
    windows::ffi::file_sizes(&fs::canonicalize(path)?)
}
