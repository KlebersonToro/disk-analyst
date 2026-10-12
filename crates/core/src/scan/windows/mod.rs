//! Enumeração de UMA pasta no Windows (T017, R2): o equivalente do `read_dir_chunk` do Linux. Uma
//! pasta por chamada, sem recursão, sem `CancelToken` nem `ScanCounters` (T018). O leitor do
//! buffer (`buffer`) é puro e compila em todos os alvos, para ser testado também no Linux; só o
//! módulo de FFI (`ffi`) tem código de sistema.
//!
//! # Pré-condição
//!
//! `path` é absoluto e verbatim (`\\?\...`): a T018 obtém isso com `canonicalize` na raiz, e os
//! filhos herdam o prefixo via `join`. Há um `debug_assert!` disso.
//!
//! # Classificação PROVISÓRIA (conservadora, até a T031)
//!
//! - `FILE_ATTRIBUTE_REPARSE_POINT` → `Kind::Link`, sem subpasta, MESMO que também seja diretório:
//!   nunca se segue um link por engano. A T031 libera as pastas de nuvem e a classificação fina
//!   (ADR-0009, R3, R4).
//! - `FILE_ATTRIBUTE_DIRECTORY` sem reparse → `Kind::Dir`, com id de subpasta.
//! - Qualquer outro → `Kind::File`, com `file_id` (no Windows todo arquivo vai, sem `nlink`; R5).
//! - `cloud = false` por ora. A coleta de hard links pós-montagem é da T032.
//!
//! # Raiz e links
//!
//! `follow_root_link == true` abre a pasta SEGUINDO links (só a raiz, decidida pela T018). Com
//! `false` abre o próprio reparse point com `FILE_FLAG_OPEN_REPARSE_POINT`.

#[allow(dead_code, reason = "consumido pela enumeração de Windows da T017")]
pub(crate) mod buffer;
#[cfg(windows)]
#[allow(dead_code, reason = "consumido pelo varredor paralelo da T018")]
pub(crate) mod ffi;

#[cfg(windows)]
use std::ffi::OsString;
#[cfg(windows)]
use std::io;
#[cfg(windows)]
use std::os::windows::ffi::OsStringExt;
#[cfg(windows)]
use std::path::Path;
#[cfg(windows)]
use std::sync::atomic::{AtomicU32, Ordering};

#[cfg(windows)]
use ::windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
};

#[cfg(windows)]
use crate::assemble::{ChunkEntry, DirChunk};
#[cfg(windows)]
use crate::names::to_text;
#[cfg(windows)]
use crate::scan::DirChunkWork;
#[cfg(windows)]
use crate::tree::Kind;

/// Palavras de 8 bytes do buffer de lote: 8192 × 8 = 64 KiB, alinhado a 8 por ser `u64`.
#[cfg(windows)]
const BATCH_WORDS: usize = 8192;

#[cfg(windows)]
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Enumera `path`. Cada subpasta recebe um id de `next_id` (`fetch_add`, `Relaxed`).
#[cfg(windows)]
#[allow(dead_code, reason = "consumido pelo varredor paralelo da T018")]
pub(crate) fn read_dir_chunk(
    path: &Path,
    chunk_id: u32,
    next_id: &AtomicU32,
    follow_root_link: bool,
) -> io::Result<DirChunkWork> {
    read_dir_chunk_counted(path, chunk_id, next_id, follow_root_link, u32::MAX)
        .map(|(work, _)| work)
}

/// Como [`read_dir_chunk`], mas devolve também quantos lotes foram lidos e para com erro se
/// passar de `max_batches`. O limite protege contra laço infinito (por exemplo, se a listagem
/// recomeçasse a cada lote): nos testes vira falha em vez de travar o check.
#[cfg(windows)]
pub(crate) fn read_dir_chunk_counted(
    path: &Path,
    chunk_id: u32,
    next_id: &AtomicU32,
    follow_root_link: bool,
    max_batches: u32,
) -> io::Result<(DirChunkWork, u32)> {
    debug_assert!(
        path.as_os_str().to_string_lossy().starts_with(r"\\?\"),
        "o caminho deve começar com o prefixo verbatim: {}",
        path.display()
    );

    let dir = ffi::open_dir(path, follow_root_link)?;
    let mut buf = vec![0u64; BATCH_WORDS];
    let mut names: Vec<u16> = Vec::new();
    let mut entries = Vec::new();
    let mut subdirs = Vec::new();
    let mut batches: u32 = 0;
    let mut restart = true;

    loop {
        // Zera o lote: a API não informa quantos bytes escreveu; o fim da cadeia é o registro com
        // NextEntryOffset == 0, e o resto do buffer não pode vazar de um lote para o outro.
        buf.fill(0);
        if !ffi::read_batch(&dir, &mut buf, restart)? {
            break;
        }
        restart = false;
        batches = batches
            .checked_add(1)
            .filter(|&n| n <= max_batches)
            .ok_or_else(|| {
                io::Error::other(format!(
                    "limite de {max_batches} lotes excedido em {}",
                    path.display()
                ))
            })?;

        for record in buffer::records(ffi::as_bytes(&buf)) {
            let record = record.map_err(|e| invalid(format!("{}: {e}", path.display())))?;
            let units: Vec<u16> = record.name_units().collect();
            if units == [0x2E] || units == [0x2E, 0x2E] {
                continue;
            }
            let (_, name_lossy) = to_text(&units);
            let name_off = u32::try_from(names.len())
                .map_err(|_| invalid("offset de nomes do chunk não cabe em u32"))?;
            let name_len = u16::try_from(units.len())
                .map_err(|_| invalid("nome com mais de 65535 unidades UTF-16"))?;
            names.extend_from_slice(&units);

            let mut entry = ChunkEntry {
                name_off,
                name_len,
                kind: Kind::File,
                cloud: false,
                inaccessible: false,
                name_lossy,
                logical: record.end_of_file,
                allocated: record.allocation_size,
                file_id: None,
                subdir: None,
            };
            if record.attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
                entry.kind = Kind::Link;
            } else if record.attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0 {
                entry.kind = Kind::Dir;
                let id = next_id.fetch_add(1, Ordering::Relaxed);
                entry.subdir = Some(id);
                subdirs.push((id, path.join(OsString::from_wide(&units))));
            } else {
                entry.file_id = Some(record.file_id);
            }
            entries.push(entry);
        }
    }

    let chunk = DirChunk {
        id: chunk_id,
        names,
        entries,
    };
    Ok((DirChunkWork { chunk, subdirs }, batches))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::assemble::ChunkEntry;
    use crate::scan::DirChunkWork;
    use crate::tree::Kind;
    use disk_analyst_synth::{Entry, SyntheticTreeSpec, allocated_size, generate};
    use std::collections::HashSet;
    use std::ffi::OsString;
    use std::fs;
    use std::os::windows::ffi::OsStringExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Diretório temporário removido no `Drop`, inclusive quando o teste falha.
    struct TempBase(PathBuf);

    impl TempBase {
        fn new(tag: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir()
                .join(format!("da-win-{tag}-{}-{nanos}-{n}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            TempBase(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempBase {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Remove a junction (sem tocar no alvo) mesmo se o teste falhar.
    struct JunctionGuard(PathBuf);

    impl Drop for JunctionGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir(&self.0);
        }
    }

    /// Caminho verbatim (`\\?\...`), como a T018 entrega ao enumerador.
    fn canon(path: &Path) -> PathBuf {
        fs::canonicalize(path).expect("canonicalize")
    }

    fn name_of(work: &DirChunkWork, entry: &ChunkEntry) -> String {
        let start = usize::try_from(entry.name_off).unwrap();
        let units = &work.chunk.names[start..start + usize::from(entry.name_len)];
        String::from_utf16_lossy(units)
    }

    fn entry<'a>(work: &'a DirChunkWork, name: &str) -> &'a ChunkEntry {
        work.chunk
            .entries
            .iter()
            .find(|e| name_of(work, e) == name)
            .unwrap_or_else(|| panic!("entrada {name:?} ausente"))
    }

    fn names(work: &DirChunkWork) -> HashSet<String> {
        work.chunk
            .entries
            .iter()
            .map(|e| name_of(work, e))
            .collect()
    }

    fn synth_tree(base: &TempBase) -> PathBuf {
        let spec = SyntheticTreeSpec {
            entries: vec![
                Entry::File {
                    path: "vazio.bin".into(),
                    size: 0,
                },
                Entry::File {
                    path: "grande.bin".into(),
                    size: 10_000,
                },
                Entry::File {
                    path: "sub/dentro.txt".into(),
                    size: 5,
                },
            ],
        };
        let dest = base.path().join("dest");
        generate(&spec, &dest).expect("gerar árvore sintética");
        canon(&dest.join("tree"))
    }

    #[test]
    fn lists_one_folder_without_recursing_nor_dot_entries() {
        let base = TempBase::new("lista");
        let tree = synth_tree(&base);

        let (work, _) =
            read_dir_chunk_counted(&tree, 0, &AtomicU32::new(1), false, 100).expect("ler pasta");

        assert_eq!(work.chunk.id, 0);
        assert_eq!(
            names(&work),
            HashSet::from(["vazio.bin".into(), "grande.bin".into(), "sub".into()])
        );
        assert_eq!(work.chunk.entries.len(), 3);

        let vazio = entry(&work, "vazio.bin");
        assert_eq!((vazio.kind, vazio.logical), (Kind::File, 0));
        assert_eq!(
            vazio.allocated,
            allocated_size(&tree.join("vazio.bin")).unwrap()
        );
        assert!(vazio.file_id.is_some());

        let grande = entry(&work, "grande.bin");
        assert_eq!((grande.kind, grande.logical), (Kind::File, 10_000));
        assert_eq!(
            grande.allocated,
            allocated_size(&tree.join("grande.bin")).unwrap()
        );
        assert!(grande.file_id.is_some());

        let sub = entry(&work, "sub");
        assert_eq!(sub.kind, Kind::Dir);
        assert_eq!(sub.subdir, Some(1));
        assert_eq!(sub.file_id, None);
        assert_eq!(work.subdirs, vec![(1, tree.join("sub"))]);
    }

    #[test]
    fn many_files_need_several_batches_and_none_is_lost_or_duplicated() {
        let base = TempBase::new("lotes");
        let dir = base.path().join("muitos");
        fs::create_dir_all(&dir).unwrap();
        let mut expected = HashSet::new();
        for i in 0..1500 {
            let name = format!("{i:060}");
            fs::write(dir.join(&name), b"x").unwrap();
            expected.insert(name);
        }

        let (work, batches) =
            read_dir_chunk_counted(&canon(&dir), 0, &AtomicU32::new(1), false, 200).unwrap();

        println!("lotes lidos: {batches}");
        assert!(batches >= 3, "esperava ≥ 3 lotes, leu {batches}");
        assert_eq!(work.chunk.entries.len(), 1500, "sem perda nem duplicata");
        assert_eq!(names(&work), expected);
    }

    /// Cria `link` como junction para `target` (caminhos comuns, não verbatim, que o mklink aceita).
    fn make_junction(link: &Path, target: &Path) -> JunctionGuard {
        let status = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .expect("executar cmd")
            .status;
        assert!(status.success(), "mklink /J falhou");
        JunctionGuard(link.to_path_buf())
    }

    #[test]
    fn junction_is_a_link_without_subdir() {
        let base = TempBase::new("junction");
        let dir = base.path().join("raiz");
        fs::create_dir_all(dir.join("alvo")).unwrap();
        fs::write(dir.join("alvo").join("dentro.txt"), b"x").unwrap();
        let _guard = make_junction(&dir.join("junc"), &dir.join("alvo"));

        let (work, _) =
            read_dir_chunk_counted(&canon(&dir), 0, &AtomicU32::new(1), false, 100).unwrap();

        let junc = entry(&work, "junc");
        assert_eq!(junc.kind, Kind::Link);
        assert_eq!(junc.subdir, None);
        assert_eq!(entry(&work, "alvo").kind, Kind::Dir);
        assert_eq!(work.subdirs.len(), 1, "só a pasta real é percorrida");
    }

    #[test]
    fn follow_root_link_lists_the_target_of_a_junction_root() {
        let base = TempBase::new("seguir");
        let dir = base.path().join("raiz");
        fs::create_dir_all(dir.join("alvo")).unwrap();
        fs::write(dir.join("alvo").join("dentro.txt"), b"x").unwrap();
        let link = dir.join("junc");
        let _guard = make_junction(&link, &dir.join("alvo"));
        // canonicalize resolveria a junction; monta o verbatim da própria junction.
        let verbatim = PathBuf::from(format!(r"\\?\{}", link.display()));

        let (work, _) =
            read_dir_chunk_counted(&verbatim, 0, &AtomicU32::new(1), true, 100).unwrap();

        assert_eq!(names(&work), HashSet::from(["dentro.txt".to_string()]));
    }

    #[test]
    fn missing_folder_is_an_error() {
        let base = TempBase::new("ausente");
        let missing = canon(base.path()).join("nao-existe");

        assert!(read_dir_chunk_counted(&missing, 0, &AtomicU32::new(1), false, 100).is_err());
    }

    #[test]
    fn lone_surrogate_name_is_lossy() {
        let base = TempBase::new("surrogate");
        let dir = base.path().join("raiz");
        fs::create_dir_all(&dir).unwrap();
        let name = OsString::from_wide(&[0x61, 0xD800, 0x62]);
        fs::write(dir.join(&name), b"x").expect("o NTFS aceita surrogate solto");

        let (work, _) =
            read_dir_chunk_counted(&canon(&dir), 0, &AtomicU32::new(1), false, 100).unwrap();

        assert_eq!(work.chunk.entries.len(), 1);
        let e = &work.chunk.entries[0];
        assert!(e.name_lossy);
        assert_eq!(name_of(&work, e), "a\u{FFFD}b");
    }
}
