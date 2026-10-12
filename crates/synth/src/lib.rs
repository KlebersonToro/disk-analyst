//! Gerador de árvores sintéticas: o oráculo dos testes de varredura (R10, FR-016).
//!
//! **Não depende de `disk_analyst_core`**, para que o oráculo não seja tautológico: a contagem
//! esperada vem da descrição, não do código testado.
//!
//! Layout de saída: a árvore vai em `<destino>/tree/` e o `expected.json` em
//! `<destino>/expected.json`, fora da árvore, para que o arquivo de totais não entre na própria
//! varredura.
//!
//! # Regras
//!
//! - Caminhos são relativos, separados por `/`, e validados antes de qualquer escrita: sem raiz,
//!   sem prefixo de drive, sem `..`, sem componente vazio, sem `:` nem `\`.
//! - Conteúdo de arquivo: exatamente `size` bytes gerados por SplitMix64, semeado pelo FNV-1a do
//!   caminho relativo. Não é zero, então não depende de esparsidade nem de compressão.
//! - `bulk`: `depth` níveis de subpastas abaixo do caminho dado. Cada pasta, inclusive a de
//!   partida, tem `files_per_dir` arquivos; pastas de nível menor que `depth` têm `dirs_per_dir`
//!   subpastas. Tamanhos vêm de SplitMix64 semeado por `seed`. Nomes: `d000`, `f000`.
//! - `expected.json`: `schema_version` 1, `logical_bytes` (soma dos tamanhos), `allocated_bytes`
//!   (soma do alocado medido de cada arquivo depois de fechado), `files` e `folders`. O alocado
//!   só existe em [`generate`]; [`planned_of`] não o inventa. `folders` conta as pastas dentro de `tree/` **mais a própria `tree/`** como raiz,
//!   coerente com a regra do relatório "a raiz conta em folders".
//! - Tipos planejados (`symlink`, `hardlink`, `junction`, `denied_dir`, `offline_file`) são lidos,
//!   mas rejeitados com `tipo ainda não suportado: <tipo>` até as tarefas da US3.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

#[cfg(windows)]
mod ffi_windows;
mod oracle;

pub use oracle::allocated_size;

/// Limite de entradas planejadas, para uma descrição mal escrita não consumir a memória.
const MAX_ENTRIES: u64 = 10_000_000;

/// Descrição de uma árvore sintética.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct SyntheticTreeSpec {
    pub entries: Vec<Entry>,
}

/// Uma entrada da descrição. Os tipos planejados entram aqui só para que a rejeição seja clara.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Entry {
    File {
        path: String,
        size: u64,
    },
    Dir {
        path: String,
    },
    Bulk {
        path: String,
        depth: u32,
        dirs_per_dir: u32,
        files_per_dir: u32,
        max_size: u64,
        seed: u64,
    },
    Symlink {
        path: String,
        target: String,
    },
    Hardlink {
        path: String,
        target: String,
    },
    Junction {
        path: String,
        target: String,
    },
    DeniedDir {
        path: String,
    },
    OfflineFile {
        path: String,
    },
}

/// Totais planejados a partir da descrição, sem tocar no disco. Não há alocado aqui: ele só
/// existe depois de medido (ver [`Expected`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Planned {
    pub logical_bytes: u64,
    pub files: u64,
    pub folders: u64,
}

/// Totais medidos, gravados em `expected.json`. Campos na ordem de declaração.
///
/// `allocated_bytes` é a soma do Tamanho alocado medido de cada arquivo depois de fechado.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Expected {
    pub schema_version: u32,
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub files: u64,
    pub folders: u64,
}

/// Erros do gerador. Mensagens em português.
#[derive(Debug)]
pub enum SynthError {
    Io { path: PathBuf, source: io::Error },
    Spec(String),
    CaminhoInseguro { path: String, motivo: &'static str },
    DestinoNaoVazio(PathBuf),
    TipoNaoSuportado(&'static str),
}

impl fmt::Display for SynthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SynthError::Io { path, source } => {
                write!(f, "erro de E/S em {}: {source}", path.display())
            }
            SynthError::Spec(msg) => write!(f, "descrição inválida: {msg}"),
            SynthError::CaminhoInseguro { path, motivo } => {
                write!(f, "caminho inseguro {path:?}: {motivo}")
            }
            SynthError::DestinoNaoVazio(path) => {
                write!(
                    f,
                    "o destino já existe e não está vazio: {}",
                    path.display()
                )
            }
            SynthError::TipoNaoSuportado(kind) => {
                write!(f, "tipo ainda não suportado: {kind}")
            }
        }
    }
}

impl std::error::Error for SynthError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SynthError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Lê uma descrição do disco.
pub fn load_spec(path: &Path) -> Result<SyntheticTreeSpec, SynthError> {
    let text = fs::read_to_string(path).map_err(|source| SynthError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&text)
        .map_err(|e| SynthError::Spec(format!("JSON inválido em {}: {e}", path.display())))
}

/// Calcula o [`Planned`] de uma descrição sem tocar no disco.
pub fn planned_of(spec: &SyntheticTreeSpec) -> Result<Planned, SynthError> {
    let plan = build_plan(spec)?;
    Ok(plan.planned())
}

/// Gera a árvore em `dest/tree/` e grava `dest/expected.json`. `dest` não pode existir não
/// vazio. Toda a descrição é validada antes de qualquer escrita.
pub fn generate(spec: &SyntheticTreeSpec, dest: &Path) -> Result<Expected, SynthError> {
    let plan = build_plan(spec)?;
    check_destination(dest)?;

    let io_err = |path: &Path| {
        let path = path.to_path_buf();
        move |source| SynthError::Io { path, source }
    };
    let tree = dest.join("tree");
    fs::create_dir_all(&tree).map_err(io_err(&tree))?;

    let mut allocated_bytes: u64 = 0;
    for (rel, node) in &plan.nodes {
        let target = tree.join(rel);
        match node {
            Node::Dir => fs::create_dir_all(&target).map_err(io_err(&target))?,
            Node::File(size) => {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(io_err(parent))?;
                }
                {
                    // O escopo fecha o arquivo antes da medição: o NTFS só fixa a alocação no
                    // fechamento, então medir com o handle aberto pode dar um valor provisório.
                    let file = fs::File::create(&target).map_err(io_err(&target))?;
                    let mut out = BufWriter::new(file);
                    write_content(&mut out, rel, *size).map_err(io_err(&target))?;
                    out.flush().map_err(io_err(&target))?;
                }
                let measured = oracle::allocated_size(&target).map_err(io_err(&target))?;
                allocated_bytes = allocated_bytes
                    .checked_add(measured)
                    .ok_or_else(|| SynthError::Spec("alocado total estoura u64".into()))?;
            }
        }
    }

    let planned = plan.planned();
    let expected = Expected {
        schema_version: 1,
        logical_bytes: planned.logical_bytes,
        allocated_bytes,
        files: planned.files,
        folders: planned.folders,
    };
    let expected_path = dest.join("expected.json");
    let mut text = serde_json::to_string_pretty(&expected)
        .map_err(|e| SynthError::Spec(format!("serializar expected: {e}")))?;
    text.push('\n');
    fs::write(&expected_path, text).map_err(io_err(&expected_path))?;
    Ok(expected)
}

fn check_destination(dest: &Path) -> Result<(), SynthError> {
    if !dest.exists() {
        return fs::create_dir_all(dest).map_err(|source| SynthError::Io {
            path: dest.to_path_buf(),
            source,
        });
    }
    let mut entries = fs::read_dir(dest).map_err(|source| SynthError::Io {
        path: dest.to_path_buf(),
        source,
    })?;
    if entries.next().is_some() {
        return Err(SynthError::DestinoNaoVazio(dest.to_path_buf()));
    }
    Ok(())
}

// --- Plano: a descrição expandida em nós, validada antes de qualquer escrita ----------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Node {
    Dir,
    File(u64),
}

/// Nós por caminho relativo normalizado (componentes unidos por `/`), em ordem.
struct Plan {
    nodes: BTreeMap<String, Node>,
}

impl Plan {
    fn planned(&self) -> Planned {
        let mut logical_bytes = 0u64;
        let mut files = 0u64;
        let mut folders = 1u64; // a própria tree/
        for node in self.nodes.values() {
            match node {
                Node::File(size) => {
                    logical_bytes += size;
                    files += 1;
                }
                Node::Dir => folders += 1,
            }
        }
        Planned {
            logical_bytes,
            files,
            folders,
        }
    }
}

fn build_plan(spec: &SyntheticTreeSpec) -> Result<Plan, SynthError> {
    let mut plan = Plan {
        nodes: BTreeMap::new(),
    };
    for entry in &spec.entries {
        match entry {
            Entry::File { path, size } => {
                let comps = validate_path(path)?;
                add_file(&mut plan, &comps, *size)?;
            }
            Entry::Dir { path } => {
                let comps = validate_path(path)?;
                add_dir(&mut plan, &comps)?;
            }
            Entry::Bulk {
                path,
                depth,
                dirs_per_dir,
                files_per_dir,
                max_size,
                seed,
            } => {
                let comps = validate_path(path)?;
                let params = BulkParams {
                    depth: *depth,
                    dirs_per_dir: *dirs_per_dir,
                    files_per_dir: *files_per_dir,
                    max_size: *max_size,
                    seed: *seed,
                };
                add_bulk(&mut plan, &comps, &params)?;
            }
            other => return Err(SynthError::TipoNaoSuportado(kind_name(other))),
        }
    }
    Ok(plan)
}

fn kind_name(entry: &Entry) -> &'static str {
    match entry {
        Entry::File { .. } => "file",
        Entry::Dir { .. } => "dir",
        Entry::Bulk { .. } => "bulk",
        Entry::Symlink { .. } => "symlink",
        Entry::Hardlink { .. } => "hardlink",
        Entry::Junction { .. } => "junction",
        Entry::DeniedDir { .. } => "denied_dir",
        Entry::OfflineFile { .. } => "offline_file",
    }
}

/// Valida um caminho relativo e devolve os componentes. Rejeita o que escaparia do destino.
fn validate_path(raw: &str) -> Result<Vec<String>, SynthError> {
    let inseguro = |motivo| SynthError::CaminhoInseguro {
        path: raw.to_owned(),
        motivo,
    };
    if raw.is_empty() {
        return Err(inseguro("caminho vazio"));
    }
    if raw.starts_with('/') {
        return Err(inseguro("caminho absoluto"));
    }
    if raw.contains('\\') {
        return Err(inseguro("barra invertida não é separador"));
    }
    let mut comps = Vec::new();
    for part in raw.split('/') {
        if part.is_empty() {
            return Err(inseguro("componente vazio"));
        }
        if part == ".." {
            return Err(inseguro("componente \"..\""));
        }
        if part.contains(':') {
            return Err(inseguro("prefixo de drive"));
        }
        // Dupla checagem com o próprio std: o componente tem que ser um nome normal.
        let mut it = Path::new(part).components();
        match (it.next(), it.next()) {
            (Some(Component::Normal(_)), None) => {}
            _ => return Err(inseguro("componente que não é nome normal")),
        }
        comps.push(part.to_owned());
    }
    Ok(comps)
}

fn key(comps: &[String]) -> String {
    comps.join("/")
}

fn add_dir(plan: &mut Plan, comps: &[String]) -> Result<(), SynthError> {
    for i in 1..=comps.len() {
        insert_dir(plan, &key(&comps[..i]))?;
    }
    Ok(())
}

fn insert_dir(plan: &mut Plan, rel: &str) -> Result<(), SynthError> {
    match plan.nodes.get(rel) {
        None => {
            plan.nodes.insert(rel.to_owned(), Node::Dir);
            Ok(())
        }
        Some(Node::Dir) => Ok(()),
        Some(Node::File(_)) => Err(SynthError::Spec(format!(
            "conflito: {rel:?} é arquivo e também pasta"
        ))),
    }
}

fn add_file(plan: &mut Plan, comps: &[String], size: u64) -> Result<(), SynthError> {
    if comps.len() > 1 {
        add_dir(plan, &comps[..comps.len() - 1])?;
    }
    let rel = key(comps);
    if plan.nodes.contains_key(&rel) {
        return Err(SynthError::Spec(format!("caminho duplicado: {rel:?}")));
    }
    plan.nodes.insert(rel, Node::File(size));
    Ok(())
}

struct BulkParams {
    depth: u32,
    dirs_per_dir: u32,
    files_per_dir: u32,
    max_size: u64,
    seed: u64,
}

fn add_bulk(plan: &mut Plan, base: &[String], p: &BulkParams) -> Result<(), SynthError> {
    if p.max_size == u64::MAX {
        return Err(SynthError::Spec("max_size não pode ser u64::MAX".into()));
    }
    let estimate = bulk_entry_count(p).ok_or_else(|| {
        SynthError::Spec(format!(
            "bulk grande demais: mais de {MAX_ENTRIES} entradas"
        ))
    })?;
    if estimate > MAX_ENTRIES {
        return Err(SynthError::Spec(format!(
            "bulk grande demais: {estimate} entradas (limite {MAX_ENTRIES})"
        )));
    }

    add_dir(plan, base)?;
    let mut rng = SplitMix64::new(p.seed);
    // Pilha explícita: a profundidade não consome a pilha de chamadas.
    let mut stack = vec![(key(base), 0u32)];
    while let Some((dir, level)) = stack.pop() {
        for i in 0..p.files_per_dir {
            let size = rng.next_u64() % (p.max_size + 1);
            let rel = format!("{dir}/f{i:03}");
            if plan.nodes.contains_key(&rel) {
                return Err(SynthError::Spec(format!("caminho duplicado: {rel:?}")));
            }
            plan.nodes.insert(rel, Node::File(size));
        }
        if level < p.depth {
            for j in 0..p.dirs_per_dir {
                let child = format!("{dir}/d{j:03}");
                insert_dir(plan, &child)?;
                stack.push((child, level + 1));
            }
        }
    }
    Ok(())
}

/// Número de entradas (pastas e arquivos) que um bulk gera, ou `None` se estourar `u64`.
/// Pastas: 1 (partida) + Σ dirs_per_dir^l para l = 1..=depth. Arquivos: files_per_dir × pastas.
fn bulk_entry_count(p: &BulkParams) -> Option<u64> {
    let mut folders: u64 = 1;
    let mut level_count: u64 = 1;
    for _ in 0..p.depth {
        level_count = level_count.checked_mul(u64::from(p.dirs_per_dir))?;
        folders = folders.checked_add(level_count)?;
    }
    let files = folders.checked_mul(u64::from(p.files_per_dir))?;
    files.checked_add(folders)
}

// --- Conteúdo e PRNG ------------------------------------------------------------------------

/// SplitMix64: gerador pequeno e determinístico, sem dependência externa.
struct SplitMix64(u64);

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        SplitMix64(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// FNV-1a 64 bits sobre os bytes do caminho relativo.
fn fnv1a(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Tamanho do bloco de escrita. A memória usada não cresce com o tamanho do arquivo.
const BLOCK_SIZE: usize = 64 * 1024;

/// Escreve exatamente `size` bytes de conteúdo determinístico, semeado pelo caminho relativo.
///
/// Streaming com buffer fixo de [`BLOCK_SIZE`]: o laço só aloca esse bloco, qualquer que seja
/// `size`. Os bytes são os mesmos de gerar a sequência de palavras de 8 bytes e truncar no fim.
fn write_content<W: Write>(out: &mut W, rel: &str, size: u64) -> io::Result<()> {
    let mut rng = SplitMix64::new(fnv1a(rel));
    let mut block = vec![0u8; BLOCK_SIZE];
    let mut remaining = size;
    while remaining > 0 {
        // `min` garante n <= BLOCK_SIZE, que cabe em usize.
        let n = remaining.min(BLOCK_SIZE as u64) as usize;
        for word in block[..n].chunks_mut(8) {
            let bytes = rng.next_u64().to_le_bytes();
            word.copy_from_slice(&bytes[..word.len()]);
        }
        out.write_all(&block[..n])?;
        remaining -= n as u64;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Implementação de referência em memória: a versão anterior à escrita em streaming. Mantida
    /// só para comparar byte a byte com `write_content`.
    fn reference(rel: &str, size: u64) -> Vec<u8> {
        let size = usize::try_from(size).expect("tamanho cabe em usize");
        let mut rng = SplitMix64::new(fnv1a(rel));
        let mut out = Vec::with_capacity(size + 8);
        while out.len() < size {
            out.extend_from_slice(&rng.next_u64().to_le_bytes());
        }
        out.truncate(size);
        out
    }

    #[test]
    fn streaming_matches_reference_across_block_edges() {
        for size in [0, 1, 7, 8, 9, 65_535, 65_536, 65_537, 3 * 65_536 + 5] {
            let mut out = Vec::new();
            write_content(&mut out, "docs/x.bin", size).expect("escrita em memória");
            assert_eq!(out, reference("docs/x.bin", size), "tamanho {size}");
        }
    }
}
