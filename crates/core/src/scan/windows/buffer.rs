//! Leitor do buffer de `FILE_ID_BOTH_DIR_INFO` devolvido por `GetFileInformationByHandleEx`
//! (R2, T016). É uma função PURA e SEGURA sobre `&[u8]`, compilada em todos os alvos para ser
//! testada também no Linux. Os bytes vêm do sistema de arquivos e são tratados como não
//! confiáveis: toda leitura é checada, toda aritmética usa `checked_add`/`try_from`, e um registro
//! inválido vira `Err`, nunca pânico nem leitura fora do buffer.
//!
//! # Layout de cada registro
//!
//! Fonte: MS-FSCC 2.4.17 (`FileIdBothDirectoryInformation`), conferido contra o
//! `FILE_ID_BOTH_DIR_INFO` da crate `windows` 0.62 (um teste só do Windows compara cada offset
//! abaixo com `offset_of!` do struct). Little-endian, offsets relativos ao início do registro:
//!
//! | offset | campo |
//! |---|---|
//! | 0 | `NextEntryOffset` u32 |
//! | 4 | `FileIndex` u32 |
//! | 8, 16, 24, 32 | `CreationTime`, `LastAccessTime`, `LastWriteTime`, `ChangeTime` i64 |
//! | 40 | `EndOfFile` i64 |
//! | 48 | `AllocationSize` i64 |
//! | 56 | `FileAttributes` u32 |
//! | 60 | `FileNameLength` u32, em BYTES |
//! | 64 | `EaSize` u32 |
//! | 68 | `ShortNameLength` i8 (+ `Reserved1` u8) |
//! | 70 | `ShortName` `[u16; 12]` |
//! | 94 | `Reserved2` u16 |
//! | 96 | `FileId` i64 |
//! | 104 | `FileName`, `FileNameLength` bytes, UTF-16LE, sem NUL |
//!
//! O último registro do lote tem `NextEntryOffset == 0`. Registros não têm alinhamento garantido
//! no `&[u8]`, então nada aqui usa cast de ponteiro nem `transmute`: os campos saem de
//! `from_le_bytes`.
//!
//! Este módulo não filtra `.` e `..` (isso é da T017): ele só lê.

use std::fmt;
use std::iter::FusedIterator;

const OFF_NEXT_ENTRY: usize = 0;
const OFF_END_OF_FILE: usize = 40;
const OFF_ALLOCATION_SIZE: usize = 48;
const OFF_FILE_ATTRIBUTES: usize = 56;
const OFF_FILE_NAME_LENGTH: usize = 60;
const OFF_EA_SIZE: usize = 64;
const OFF_FILE_ID: usize = 96;
/// Início do nome; também o tamanho mínimo de um registro.
const OFF_FILE_NAME: usize = 104;
/// `NextEntryOffset` não nulo é múltiplo de 8.
const RECORD_ALIGN: usize = 8;

/// Um registro lido do buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DirRecord<'a> {
    pub end_of_file: u64,
    pub allocation_size: u64,
    pub attributes: u32,
    pub ea_size: u32,
    /// `FileId` (i64) reinterpretado como u64 com `from_le_bytes`: é a identidade de 64 bits do
    /// arquivo no volume, não uma quantidade com sinal.
    pub file_id: u64,
    name: &'a [u8],
}

impl DirRecord<'_> {
    /// Unidades UTF-16 do nome. Lê par a par com `from_le_bytes`, sem supor alinhamento.
    pub(crate) fn name_units(&self) -> impl Iterator<Item = u16> + '_ {
        // O comprimento é par (validado em `parse_record`), então não sobra byte.
        let (pairs, _) = self.name.as_chunks::<2>();
        pairs.iter().map(|pair| u16::from_le_bytes(*pair))
    }
}

/// Registro inválido, com o offset dele dentro do buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecordError {
    /// Menos de 104 bytes disponíveis a partir do início do registro.
    Truncated { offset: usize },
    /// `FileNameLength` ímpar (UTF-16 tem 2 bytes por unidade).
    OddNameLength { offset: usize },
    /// O nome passa do limite: `NextEntryOffset` se não nulo, senão o fim do buffer. Cobre
    /// também `NextEntryOffset` menor que `104 + FileNameLength`.
    NameOutOfBounds { offset: usize },
    /// `NextEntryOffset` não nulo e não múltiplo de 8.
    NextNotAligned { offset: usize },
    /// O próximo registro começaria no fim do buffer ou depois dele.
    NextBeyondBuffer { offset: usize },
    /// `EndOfFile` ou `AllocationSize` negativo.
    NegativeSize { offset: usize },
}

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecordError::Truncated { offset } => {
                write!(f, "registro truncado no offset {offset}")
            }
            RecordError::OddNameLength { offset } => {
                write!(f, "tamanho de nome ímpar no registro do offset {offset}")
            }
            RecordError::NameOutOfBounds { offset } => {
                write!(f, "nome fora dos limites no registro do offset {offset}")
            }
            RecordError::NextNotAligned { offset } => write!(
                f,
                "próximo registro não alinhado a 8 bytes no registro do offset {offset}"
            ),
            RecordError::NextBeyondBuffer { offset } => write!(
                f,
                "próximo registro além do fim do buffer no registro do offset {offset}"
            ),
            RecordError::NegativeSize { offset } => {
                write!(f, "tamanho negativo no registro do offset {offset}")
            }
        }
    }
}

impl std::error::Error for RecordError {}

/// Itera os registros de um buffer. Depois do primeiro `Err` o iterador termina: um registro
/// corrompido não é "pulado", porque o `NextEntryOffset` dele não é confiável.
pub(crate) struct Records<'a> {
    buf: &'a [u8],
    /// Offset do próximo registro a ler; `None` quando acabou.
    next: Option<usize>,
}

/// Registros de `buf`. Buffer vazio: nenhum registro e nenhum erro.
pub(crate) fn records(buf: &[u8]) -> Records<'_> {
    Records {
        buf,
        next: (!buf.is_empty()).then_some(0),
    }
}

impl<'a> Iterator for Records<'a> {
    type Item = Result<DirRecord<'a>, RecordError>;

    fn next(&mut self) -> Option<Self::Item> {
        let start = self.next.take()?;
        match parse_record(self.buf, start) {
            Ok((record, next)) => {
                self.next = next;
                Some(Ok(record))
            }
            // `next` fica `None`: o iterador termina.
            Err(error) => Some(Err(error)),
        }
    }
}

impl FusedIterator for Records<'_> {}

/// `N` bytes em `rest[off..]`, ou `None` se não couberem.
fn le<const N: usize>(rest: &[u8], off: usize) -> Option<[u8; N]> {
    rest.get(off..off.checked_add(N)?)?.try_into().ok()
}

fn parse_record(buf: &[u8], start: usize) -> Result<(DirRecord<'_>, Option<usize>), RecordError> {
    let truncated = RecordError::Truncated { offset: start };
    let rest = buf.get(start..).ok_or(truncated)?;
    if rest.len() < OFF_FILE_NAME {
        return Err(truncated);
    }

    let next_entry = le(rest, OFF_NEXT_ENTRY)
        .map(u32::from_le_bytes)
        .ok_or(truncated)?;
    let name_len = le(rest, OFF_FILE_NAME_LENGTH)
        .map(u32::from_le_bytes)
        .ok_or(truncated)?;
    let beyond = RecordError::NextBeyondBuffer { offset: start };
    let out_of_bounds = RecordError::NameOutOfBounds { offset: start };
    let next_entry = usize::try_from(next_entry).map_err(|_| beyond)?;
    let name_len = usize::try_from(name_len).map_err(|_| out_of_bounds)?;

    if name_len % 2 != 0 {
        return Err(RecordError::OddNameLength { offset: start });
    }

    // Limite do nome: o próximo registro, se houver; senão o resto do buffer.
    let (limit, next_start) = if next_entry == 0 {
        (rest.len(), None)
    } else {
        if next_entry % RECORD_ALIGN != 0 {
            return Err(RecordError::NextNotAligned { offset: start });
        }
        let next_start = start.checked_add(next_entry).ok_or(beyond)?;
        if next_start >= buf.len() {
            return Err(beyond);
        }
        (next_entry, Some(next_start))
    };

    let name_end = OFF_FILE_NAME.checked_add(name_len).ok_or(out_of_bounds)?;
    if name_end > limit {
        return Err(out_of_bounds);
    }
    let name = rest.get(OFF_FILE_NAME..name_end).ok_or(out_of_bounds)?;

    let negative = RecordError::NegativeSize { offset: start };
    let end_of_file = le(rest, OFF_END_OF_FILE)
        .map(i64::from_le_bytes)
        .ok_or(truncated)?;
    let allocation_size = le(rest, OFF_ALLOCATION_SIZE)
        .map(i64::from_le_bytes)
        .ok_or(truncated)?;
    let end_of_file = u64::try_from(end_of_file).map_err(|_| negative)?;
    let allocation_size = u64::try_from(allocation_size).map_err(|_| negative)?;

    let record = DirRecord {
        end_of_file,
        allocation_size,
        attributes: le(rest, OFF_FILE_ATTRIBUTES)
            .map(u32::from_le_bytes)
            .ok_or(truncated)?,
        ea_size: le(rest, OFF_EA_SIZE)
            .map(u32::from_le_bytes)
            .ok_or(truncated)?,
        file_id: le(rest, OFF_FILE_ID)
            .map(u64::from_le_bytes)
            .ok_or(truncated)?,
        name,
    };
    Ok((record, next_start))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::collection::vec;
    use proptest::prelude::*;

    /// Monta um registro pelos offsets do MS-FSCC 2.4.17, com literais independentes do código de
    /// produção. Com `next != 0`, preenche com zeros até `next` bytes.
    fn build_record(
        next: u32,
        eof: i64,
        alloc: i64,
        attrs: u32,
        ea: u32,
        file_id: i64,
        name: &str,
    ) -> Vec<u8> {
        let name_bytes: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let name_len = u32::try_from(name_bytes.len()).unwrap();
        let mut rec = vec![0u8; 104];
        rec[0..4].copy_from_slice(&next.to_le_bytes());
        rec[40..48].copy_from_slice(&eof.to_le_bytes());
        rec[48..56].copy_from_slice(&alloc.to_le_bytes());
        rec[56..60].copy_from_slice(&attrs.to_le_bytes());
        rec[60..64].copy_from_slice(&name_len.to_le_bytes());
        rec[64..68].copy_from_slice(&ea.to_le_bytes());
        rec[96..104].copy_from_slice(&file_id.to_le_bytes());
        rec.extend_from_slice(&name_bytes);
        if next != 0 {
            rec.resize(usize::try_from(next).unwrap(), 0);
        }
        rec
    }

    /// Tamanho de um registro com esse nome, arredondado para múltiplo de 8.
    fn aligned_len(name: &str) -> u32 {
        u32::try_from((104 + 2 * name.encode_utf16().count()).next_multiple_of(8)).unwrap()
    }

    fn utf16(name: &str) -> Vec<u16> {
        name.encode_utf16().collect()
    }

    fn set_u32(buf: &mut [u8], at: usize, value: u32) {
        buf[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn first_error(buf: &[u8]) -> RecordError {
        match records(buf).find_map(Result::err) {
            Some(e) => e,
            None => panic!("esperava um erro"),
        }
    }

    fn two_records() -> Vec<u8> {
        let mut buf = build_record(
            aligned_len("a.txt"),
            100,
            4096,
            0x20,
            7,
            0x1122_3344_5566_7788,
            "a.txt",
        );
        buf.extend(build_record(0, 5, 0, 0x10, 0, -2, "ção"));
        buf
    }

    #[test]
    fn reads_two_records_with_all_fields() {
        let buf = two_records();
        let recs: Vec<_> = records(&buf).collect::<Result<_, _>>().unwrap();

        assert_eq!(recs.len(), 2);
        let a = &recs[0];
        assert_eq!((a.end_of_file, a.allocation_size), (100, 4096));
        assert_eq!((a.attributes, a.ea_size), (0x20, 7));
        assert_eq!(a.file_id, 0x1122_3344_5566_7788);
        assert_eq!(a.name_units().collect::<Vec<_>>(), utf16("a.txt"));

        let b = &recs[1];
        assert_eq!((b.end_of_file, b.allocation_size), (5, 0));
        assert_eq!((b.attributes, b.ea_size), (0x10, 0));
        // FileId é identidade de 64 bits: -2 como i64 vira 0xFFFF_FFFF_FFFF_FFFE.
        assert_eq!(b.file_id, u64::from_le_bytes((-2i64).to_le_bytes()));
        assert_eq!(b.name_units().collect::<Vec<_>>(), utf16("ção"));
    }

    #[test]
    fn reads_correctly_from_an_odd_address() {
        let buf = two_records();
        let mut shifted = vec![0xEEu8];
        shifted.extend_from_slice(&buf);

        let from_odd: Vec<_> = records(&shifted[1..]).collect::<Result<_, _>>().unwrap();
        let from_start: Vec<_> = records(&buf).collect::<Result<_, _>>().unwrap();

        assert_eq!(from_odd.len(), 2);
        assert_eq!(from_odd[1].file_id, from_start[1].file_id);
        assert_eq!(from_odd[1].name_units().collect::<Vec<_>>(), utf16("ção"));
    }

    #[test]
    fn empty_buffer_has_no_records_and_no_error() {
        assert!(records(&[]).next().is_none());
    }

    #[test]
    fn truncated_record_is_an_error() {
        let buf = vec![0u8; 103];
        assert!(matches!(
            first_error(&buf),
            RecordError::Truncated { offset: 0 }
        ));
    }

    #[test]
    fn second_record_truncated_reports_its_offset() {
        let n1 = aligned_len("a.txt");
        let mut buf = build_record(n1, 1, 1, 0, 0, 1, "a.txt");
        buf.extend(vec![0u8; 50]);

        let err = first_error(&buf);

        assert!(
            matches!(err, RecordError::Truncated { offset } if offset == usize::try_from(n1).unwrap()),
            "{err}"
        );
    }

    #[test]
    fn odd_name_length_is_an_error() {
        let mut buf = build_record(0, 1, 1, 0, 0, 1, "ab");
        set_u32(&mut buf, 60, 3);
        assert!(matches!(
            first_error(&buf),
            RecordError::OddNameLength { offset: 0 }
        ));
    }

    #[test]
    fn name_past_the_end_of_the_buffer_is_an_error() {
        let mut buf = build_record(0, 1, 1, 0, 0, 1, "ab");
        set_u32(&mut buf, 60, 2000);
        assert!(matches!(
            first_error(&buf),
            RecordError::NameOutOfBounds { offset: 0 }
        ));
    }

    #[test]
    fn next_offset_smaller_than_the_name_is_a_name_error() {
        // 104 + 12 > 104: o nome passa do limite que o próprio NextEntryOffset impõe.
        let mut buf = build_record(0, 1, 1, 0, 0, 1, "abcdef");
        set_u32(&mut buf, 0, 104);
        assert!(matches!(
            first_error(&buf),
            RecordError::NameOutOfBounds { offset: 0 }
        ));
    }

    #[test]
    fn next_offset_not_a_multiple_of_8_is_an_error() {
        let mut buf = build_record(112, 1, 1, 0, 0, 1, "ab");
        set_u32(&mut buf, 0, 109);
        assert!(matches!(
            first_error(&buf),
            RecordError::NextNotAligned { offset: 0 }
        ));
    }

    #[test]
    fn next_offset_past_the_buffer_is_an_error() {
        let mut buf = build_record(0, 1, 1, 0, 0, 1, "ab");
        set_u32(&mut buf, 0, 4096);
        assert!(matches!(
            first_error(&buf),
            RecordError::NextBeyondBuffer { offset: 0 }
        ));
    }

    #[test]
    fn negative_sizes_are_errors() {
        let eof = build_record(0, -1, 0, 0, 0, 1, "a");
        assert!(matches!(
            first_error(&eof),
            RecordError::NegativeSize { offset: 0 }
        ));
        let alloc = build_record(0, 0, -5, 0, 0, 1, "a");
        assert!(matches!(
            first_error(&alloc),
            RecordError::NegativeSize { offset: 0 }
        ));
    }

    #[test]
    fn iterator_stops_after_the_first_error() {
        let mut buf = build_record(0, 1, 1, 0, 0, 1, "ab");
        set_u32(&mut buf, 60, 3);
        let mut it = records(&buf);

        assert!(matches!(it.next(), Some(Err(_))));
        assert!(it.next().is_none());
        assert!(it.next().is_none());
    }

    #[derive(Debug, Clone)]
    struct Gen {
        eof: i64,
        alloc: i64,
        attrs: u32,
        ea: u32,
        id: i64,
        name: String,
    }

    fn gen_record() -> impl Strategy<Value = Gen> {
        (
            0..i64::MAX,
            0..i64::MAX,
            any::<u32>(),
            any::<u32>(),
            any::<i64>(),
            ".{0,10}",
        )
            .prop_map(|(eof, alloc, attrs, ea, id, name)| Gen {
                eof,
                alloc,
                attrs,
                ea,
                id,
                name,
            })
    }

    fn build_buffer(items: &[Gen]) -> Vec<u8> {
        let mut buf = Vec::new();
        for (i, g) in items.iter().enumerate() {
            let next = if i + 1 == items.len() {
                0
            } else {
                aligned_len(&g.name)
            };
            buf.extend(build_record(
                next, g.eof, g.alloc, g.attrs, g.ea, g.id, &g.name,
            ));
        }
        buf
    }

    proptest! {
        #[test]
        fn arbitrary_bytes_never_panic(bytes in vec(any::<u8>(), 0..4096)) {
            let _ = records(&bytes).collect::<Vec<_>>();
        }

        #[test]
        fn valid_buffers_round_trip(items in vec(gen_record(), 1..20)) {
            let buf = build_buffer(&items);
            let recs: Vec<_> = records(&buf).collect::<Result<_, _>>().unwrap();

            prop_assert_eq!(recs.len(), items.len());
            for (rec, g) in recs.iter().zip(&items) {
                prop_assert_eq!(rec.end_of_file, u64::try_from(g.eof).unwrap());
                prop_assert_eq!(rec.allocation_size, u64::try_from(g.alloc).unwrap());
                prop_assert_eq!(rec.attributes, g.attrs);
                prop_assert_eq!(rec.ea_size, g.ea);
                prop_assert_eq!(rec.file_id, u64::from_le_bytes(g.id.to_le_bytes()));
                prop_assert_eq!(rec.name_units().collect::<Vec<_>>(), utf16(&g.name));
            }
        }

        #[test]
        fn one_corrupted_byte_never_panics_nor_reads_past_the_buffer(
            items in vec(gen_record(), 1..20),
            index in any::<usize>(),
            value in any::<u8>(),
        ) {
            let mut buf = build_buffer(&items);
            let at = index % buf.len();
            buf[at] = value;

            let results: Vec<_> = records(&buf).collect();

            // Cada registro lido ocupa pelo menos 104 bytes do buffer.
            let ok = results.iter().filter(|r| r.is_ok()).count();
            prop_assert!(ok <= buf.len() / 104);
        }
    }

    #[cfg(windows)]
    #[test]
    fn offsets_match_the_windows_struct() {
        use ::windows::Win32::Storage::FileSystem::FILE_ID_BOTH_DIR_INFO as S;
        use std::mem::offset_of;

        assert_eq!(OFF_NEXT_ENTRY, offset_of!(S, NextEntryOffset));
        assert_eq!(OFF_END_OF_FILE, offset_of!(S, EndOfFile));
        assert_eq!(OFF_ALLOCATION_SIZE, offset_of!(S, AllocationSize));
        assert_eq!(OFF_FILE_ATTRIBUTES, offset_of!(S, FileAttributes));
        assert_eq!(OFF_FILE_NAME_LENGTH, offset_of!(S, FileNameLength));
        assert_eq!(OFF_EA_SIZE, offset_of!(S, EaSize));
        assert_eq!(OFF_FILE_ID, offset_of!(S, FileId));
        assert_eq!(OFF_FILE_NAME, offset_of!(S, FileName));
    }
}
