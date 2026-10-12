//! Enumeração de UMA pasta no Linux (T015, R5, R6): a unidade de trabalho que o varredor paralelo
//! chama uma vez por pasta. Não é recursiva e não toca em `CancelToken` nem em `ScanCounters`.
//!
//! # Decisões
//!
//! - Nunca segue links: cada entrada passa por `symlink_metadata`.
//! - Entrada que falha no `symlink_metadata` (sumiu entre o `read_dir` e o `stat`) ou cujo
//!   alocado estoura `u64` entra como `inaccessible`, `Kind::File`, tamanhos 0 e sem subpasta, sem
//!   abortar a pasta. Falha ao abrir ou ler a PRÓPRIA pasta é `Err`: quem marca a pasta como
//!   inacessível é a T018/US3. Erro no meio do iterador de `read_dir` não traz o nome da entrada,
//!   então também é propagado como `Err`.
//! - `file_id` (hard link): como o campo é `u64`, usa-se só `st_ino`, e apenas para arquivos com
//!   `st_nlink > 1` no MESMO dispositivo da raiz (`st_dev == root_dev`). Arquivos de outro sistema
//!   de arquivos montado dentro da varredura não são deduplicados: lacuna aceita, porque o Linux é
//!   só o ambiente de desenvolvimento.
//! - Nomes em UTF-16: UTF-8 válido vai direto; senão, `from_utf8_lossy` e `name_lossy`.
//! - A ordem das entradas é a do `read_dir`; quem ordena é `assemble` (T007).

use std::ffi::OsStr;
use std::fs::{self, Metadata};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use super::DirChunkWork;
use crate::assemble::{ChunkEntry, DirChunk};
use crate::tree::Kind;

/// Enumera `path`. Cada subpasta recebe um id de `next_id` (`fetch_add`, `Relaxed`).
pub(crate) fn read_dir_chunk(
    path: &Path,
    chunk_id: u32,
    next_id: &AtomicU32,
    root_dev: u64,
) -> io::Result<DirChunkWork> {
    let mut names: Vec<u16> = Vec::new();
    let mut entries = Vec::new();
    let mut subdirs = Vec::new();

    for dirent in fs::read_dir(path)? {
        let dirent = dirent?;
        let (units, name_lossy) = name_units(&dirent.file_name());
        let name_off = u32::try_from(names.len()).expect("offset de nomes do chunk cabe em u32");
        // NAME_MAX no Linux é 255 bytes, então um nome tem no máximo 255 unidades UTF-16.
        let name_len = u16::try_from(units.len()).expect("nome de Linux tem no máximo 255 bytes");
        names.extend_from_slice(&units);

        let mut entry = ChunkEntry {
            name_off,
            name_len,
            kind: Kind::File,
            cloud: false,
            inaccessible: false,
            name_lossy,
            logical: 0,
            allocated: 0,
            file_id: None,
            subdir: None,
        };

        let full = dirent.path();
        match fs::symlink_metadata(&full) {
            Err(_) => entry.inaccessible = true,
            Ok(meta) => classify(&mut entry, &meta, &full, next_id, root_dev, &mut subdirs),
        }
        entries.push(entry);
    }

    Ok(DirChunkWork {
        chunk: DirChunk {
            id: chunk_id,
            names,
            entries,
            inaccessible: false,
        },
        subdirs,
    })
}

/// Preenche `entry` a partir dos metadados (sem seguir links).
fn classify(
    entry: &mut ChunkEntry,
    meta: &Metadata,
    full: &Path,
    next_id: &AtomicU32,
    root_dev: u64,
    subdirs: &mut Vec<(u32, PathBuf)>,
) {
    let file_type = meta.file_type();
    if file_type.is_dir() {
        entry.kind = Kind::Dir;
        let id = next_id.fetch_add(1, Ordering::Relaxed);
        entry.subdir = Some(id);
        subdirs.push((id, full.to_path_buf()));
        return;
    }

    let Some(allocated) = meta.blocks().checked_mul(512) else {
        entry.inaccessible = true;
        return;
    };
    entry.logical = meta.len();
    entry.allocated = allocated;

    if file_type.is_symlink() {
        entry.kind = Kind::Link;
    } else if meta.nlink() > 1 && meta.dev() == root_dev {
        entry.file_id = Some(meta.ino());
    }
}

/// Nome em UTF-16 e se houve perda (bytes que não são UTF-8 válido).
fn name_units(name: &OsStr) -> (Vec<u16>, bool) {
    match name.to_str() {
        Some(text) => (text.encode_utf16().collect(), false),
        None => {
            use std::os::unix::ffi::OsStrExt;
            let text = String::from_utf8_lossy(name.as_bytes());
            (text.encode_utf16().collect(), true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble::{aggregate, assemble};
    use crate::tree::Kind;
    use disk_analyst_synth::{Entry, SyntheticTreeSpec, allocated_size, generate};
    use std::ffi::OsStr;
    use std::fs;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{MetadataExt, symlink};
    use std::path::{Path, PathBuf};
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
                .join(format!("da-unix-{tag}-{}-{nanos}-{n}", std::process::id()));
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

    fn dev_of(path: &Path) -> u64 {
        fs::metadata(path).unwrap().dev()
    }

    fn name_of(work: &DirChunkWork, entry: &crate::assemble::ChunkEntry) -> String {
        let start = entry.name_off as usize;
        let units = &work.chunk.names[start..start + usize::from(entry.name_len)];
        String::from_utf16(units).expect("nome em UTF-16 válido nos testes")
    }

    fn entry<'a>(work: &'a DirChunkWork, name: &str) -> &'a crate::assemble::ChunkEntry {
        work.chunk
            .entries
            .iter()
            .find(|e| name_of(work, e) == name)
            .unwrap_or_else(|| panic!("entrada {name:?} ausente"))
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
        dest.join("tree")
    }

    #[test]
    fn lists_one_folder_without_recursing() {
        let base = TempBase::new("lista");
        let tree = synth_tree(&base);

        let work = read_dir_chunk(&tree, 0, &AtomicU32::new(1), dev_of(&tree)).expect("ler pasta");

        assert_eq!(work.chunk.id, 0);
        assert_eq!(work.chunk.entries.len(), 3);
        assert!(
            !work
                .chunk
                .entries
                .iter()
                .any(|e| name_of(&work, e) == "dentro.txt")
        );

        let vazio = entry(&work, "vazio.bin");
        assert_eq!((vazio.kind, vazio.logical), (Kind::File, 0));
        assert_eq!(
            vazio.allocated,
            allocated_size(&tree.join("vazio.bin")).unwrap()
        );

        let grande = entry(&work, "grande.bin");
        assert_eq!((grande.kind, grande.logical), (Kind::File, 10_000));
        assert_eq!(
            grande.allocated,
            allocated_size(&tree.join("grande.bin")).unwrap()
        );

        let sub = entry(&work, "sub");
        assert_eq!(sub.kind, Kind::Dir);
        assert_eq!(sub.subdir, Some(1));
        assert_eq!(work.subdirs, vec![(1, tree.join("sub"))]);
    }

    #[test]
    fn symlink_is_a_link_without_subdir_even_to_a_folder() {
        let base = TempBase::new("symlink");
        let dir = base.path().join("raiz");
        fs::create_dir_all(dir.join("alvo")).unwrap();
        symlink(dir.join("alvo"), dir.join("ln")).unwrap();

        let work = read_dir_chunk(&dir, 0, &AtomicU32::new(1), dev_of(&dir)).unwrap();

        let ln = entry(&work, "ln");
        assert_eq!(ln.kind, Kind::Link);
        assert_eq!(ln.subdir, None);
        assert_eq!(entry(&work, "alvo").kind, Kind::Dir);
        assert_eq!(work.subdirs.len(), 1, "só a pasta real é percorrida");
    }

    #[test]
    fn hard_links_share_the_inode_as_file_id_and_single_files_have_none() {
        let base = TempBase::new("hardlink");
        let dir = base.path().join("raiz");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.bin"), vec![1u8; 100]).unwrap();
        fs::hard_link(dir.join("a.bin"), dir.join("b.bin")).unwrap();
        fs::write(dir.join("solo.bin"), vec![2u8; 100]).unwrap();
        let ino = fs::metadata(dir.join("a.bin")).unwrap().ino();

        let work = read_dir_chunk(&dir, 0, &AtomicU32::new(1), dev_of(&dir)).unwrap();

        assert_eq!(entry(&work, "a.bin").file_id, Some(ino));
        assert_eq!(entry(&work, "b.bin").file_id, Some(ino));
        assert_eq!(entry(&work, "solo.bin").file_id, None);
    }

    #[test]
    fn hard_link_on_another_device_is_not_deduplicated() {
        let base = TempBase::new("device");
        let dir = base.path().join("raiz");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.bin"), b"x").unwrap();
        fs::hard_link(dir.join("a.bin"), dir.join("b.bin")).unwrap();

        // root_dev diferente do dispositivo real: simula um sistema de arquivos montado dentro.
        let work = read_dir_chunk(&dir, 0, &AtomicU32::new(1), dev_of(&dir) ^ 1).unwrap();

        assert_eq!(entry(&work, "a.bin").file_id, None);
    }

    #[test]
    fn non_utf8_name_is_lossy_with_replacement_character() {
        let base = TempBase::new("lossy");
        let dir = base.path().join("raiz");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(OsStr::from_bytes(b"x\xFFy")), b"z").unwrap();

        let work = read_dir_chunk(&dir, 0, &AtomicU32::new(1), dev_of(&dir)).unwrap();

        assert_eq!(work.chunk.entries.len(), 1);
        let e = &work.chunk.entries[0];
        assert!(e.name_lossy);
        assert_eq!(name_of(&work, e), "x\u{FFFD}y");
    }

    #[test]
    fn utf8_name_is_not_lossy() {
        let base = TempBase::new("utf8");
        let dir = base.path().join("raiz");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("áéí.txt"), b"z").unwrap();

        let work = read_dir_chunk(&dir, 0, &AtomicU32::new(1), dev_of(&dir)).unwrap();

        assert!(!work.chunk.entries[0].name_lossy);
        assert_eq!(name_of(&work, &work.chunk.entries[0]), "áéí.txt");
    }

    #[test]
    fn missing_folder_is_an_error() {
        let base = TempBase::new("ausente");
        let missing = base.path().join("nao-existe");

        assert!(read_dir_chunk(&missing, 0, &AtomicU32::new(1), 0).is_err());
    }

    #[test]
    fn chunks_assemble_and_aggregate_to_the_description_total() {
        let base = TempBase::new("integra");
        let tree = synth_tree(&base);
        let next_id = AtomicU32::new(1);
        let dev = dev_of(&tree);

        let root = read_dir_chunk(&tree, 0, &next_id, dev).unwrap();
        let mut chunks = vec![root.chunk];
        for (id, path) in root.subdirs {
            chunks.push(read_dir_chunk(&path, id, &next_id, dev).unwrap().chunk);
        }
        let mut built = assemble(chunks, tree.display().to_string()).tree;
        aggregate(&mut built);

        assert_eq!(built.nodes[0].logical, 10_000 + 5);
    }
}
