//! Varredor paralelo (T018, R6, ADR-0005): uma tarefa `rayon` por pasta. Cada tarefa enumera a
//! sua pasta com o enumerador da plataforma, empurra o `DirChunk` num `Mutex<Vec<_>>` (um lock por
//! pasta) e dispara uma tarefa para cada subpasta. A ordem em que os chunks chegam não importa:
//! `assemble` produz sempre a mesma árvore.
//!
//! Uma subpasta que não pode ser lida (por exemplo, permissão negada) não aborta a varredura: vira
//! um chunk vazio marcado `inaccessible`, e a montagem marca o Node dela. A distinção entre
//! "sumiu" e "negado" fica para a T033.

use std::io;
use std::path::Path;
use std::sync::atomic::AtomicU32;
use std::sync::{Mutex, PoisonError};

use crate::assemble::DirChunk;
use crate::control::ScanCounters;
use crate::scan::DirChunkWork;
use crate::tree::Kind;

/// Contexto da plataforma: no Linux, o dispositivo da raiz (para os hard links); no Windows, nada.
#[cfg(unix)]
#[derive(Clone, Copy)]
pub(crate) struct Platform {
    pub root_dev: u64,
}

#[cfg(windows)]
#[derive(Clone, Copy)]
pub(crate) struct Platform;

struct Shared<'a> {
    next_id: &'a AtomicU32,
    counters: &'a ScanCounters,
    platform: Platform,
    chunks: Mutex<Vec<DirChunk>>,
}

impl Shared<'_> {
    fn push(&self, chunk: DirChunk) {
        self.chunks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(chunk);
    }
}

/// Percorre a árvore a partir do chunk da raiz (já enumerado) e devolve todos os chunks.
pub(crate) fn walk(
    first: DirChunkWork,
    next_id: &AtomicU32,
    counters: &ScanCounters,
    platform: Platform,
) -> Vec<DirChunk> {
    let shared = Shared {
        next_id,
        counters,
        platform,
        chunks: Mutex::new(Vec::new()),
    };
    rayon::scope(|scope| handle(first, &shared, scope));
    shared
        .chunks
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Conta, guarda o chunk e dispara uma tarefa por subpasta.
fn handle<'s, 'a: 's>(work: DirChunkWork, shared: &'s Shared<'a>, scope: &rayon::Scope<'s>) {
    let DirChunkWork { chunk, subdirs } = work;
    count(&chunk, shared.counters);
    shared.push(chunk);

    for (id, path) in subdirs {
        scope.spawn(move |scope| match read_subdir(&path, id, shared) {
            Ok(work) => handle(work, shared, scope),
            Err(_) => shared.push(DirChunk {
                id,
                names: Vec::new(),
                entries: Vec::new(),
                inaccessible: true,
            }),
        });
    }
}

/// Contadores de progresso: por entrada, `items`; por arquivo (acessível), `files` e `bytes`, em
/// que `bytes` é o Tamanho ALOCADO.
fn count(chunk: &DirChunk, counters: &ScanCounters) {
    for entry in &chunk.entries {
        counters.add_items(1);
        if entry.kind == Kind::File && !entry.inaccessible {
            counters.add_files(1);
            counters.add_bytes(entry.allocated);
        }
    }
}

#[cfg(unix)]
fn read_subdir(path: &Path, id: u32, shared: &Shared<'_>) -> io::Result<DirChunkWork> {
    crate::scan::unix::read_dir_chunk(path, id, shared.next_id, shared.platform.root_dev)
}

/// Subpastas nunca seguem links (`follow_root_link = false`): só a raiz segue.
#[cfg(windows)]
fn read_subdir(path: &Path, id: u32, shared: &Shared<'_>) -> io::Result<DirChunkWork> {
    // No Windows o contexto da plataforma é vazio: não há dispositivo da raiz a comparar.
    let Platform = shared.platform;
    crate::scan::windows::read_dir_chunk(path, id, shared.next_id, false)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Remove a junction (sem tocar no alvo) e depois o diretório de teste, mesmo se o teste falhar.
    struct Cleanup {
        link: PathBuf,
        base: PathBuf,
    }

    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir(&self.link);
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    #[test]
    fn subfolders_are_read_without_following_links() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("da-par-{}-{nanos}", std::process::id()));
        let target = base.join("alvo");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("dentro.txt"), b"x").unwrap();
        let link = base.join("junc");
        let _cleanup = Cleanup {
            link: link.clone(),
            base: base.clone(),
        };
        let status = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()
            .expect("executar cmd")
            .status;
        assert!(status.success(), "mklink /J falhou");
        let verbatim = PathBuf::from(format!(r"\\?\{}", link.display()));

        let next_id = AtomicU32::new(1);
        let counters = ScanCounters::default();
        let shared = Shared {
            next_id: &next_id,
            counters: &counters,
            platform: Platform,
            chunks: Mutex::new(Vec::new()),
        };
        let work = read_subdir(&verbatim, 7, &shared).expect("abrir o próprio reparse point");

        assert!(
            work.chunk.entries.is_empty(),
            "uma subpasta não pode seguir o link para o alvo"
        );
        // Controle: seguindo o link, o conteúdo do alvo apareceria.
        let followed = crate::scan::windows::read_dir_chunk(&verbatim, 7, &AtomicU32::new(1), true)
            .expect("seguir o link");
        assert_eq!(followed.chunk.entries.len(), 1);
    }
}
