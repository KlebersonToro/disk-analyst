//! Serialização JSON do relatório (T025, R9, Princípio X): o documento de
//! `docs/schemas/scan-report-v1.schema.json`, com `schema_version` 1. Só a serialização; ligar
//! `--json` na CLI e validar contra o esquema é a T026.
//!
//! # Forma
//!
//! Documento COMPACTO (sem pretty-print, porque são milhões de itens), terminado em `\n`, com a ordem
//! de campos FIXA e determinística: `schema_version`, `tool`, `root`, `engine`, `partial`,
//! `duration_ms`, `totals`, `top_folders`, `top_files`, `items`.
//!
//! - As chaves do objeto de nível superior são escritas à mão, na ordem acima, e cada valor é
//!   serializado com `serde_json::to_writer` a partir de structs privadas que espelham os nomes do
//!   esquema. Assim a ordem é explícita no código e os `items` saem em STREAMING: um objeto de cada
//!   vez, sem montar um vetor com milhões deles. O chamador deve passar um `BufWriter`.
//! - `engine` é sempre `"dir"` (o esquema só aceita esse valor na v1).
//! - `tool.name` tem de ser `"disk-analyst"` (o esquema o exige); o núcleo não fixa nem esse nome
//!   nem a versão: quem chama passa ([`ToolInfo`]).
//! - `duration_ms` é inteiro (`as_millis`); um valor que não caiba em `u64` satura em `u64::MAX`,
//!   o que é inalcançável na prática.
//! - `items` é a lista plana: `id` = posição; `parent` é `null` na raiz e, nos demais, o `id` do
//!   pai (sempre menor, pela montagem em largura); `kind` é `file`, `dir` ou `link`. O `name` da
//!   RAIZ (`items[0]`) é `""`: o caminho pedido está em `root`.
//! - `flags`: array na ordem fixa `cloud`, `inaccessible`, `hardlink`, `name_lossy`, `not_scanned`,
//!   só as ligadas, sem repetição. `name_lossy` vem da flag do Node, que é a fonte autoritativa: no
//!   Linux o nome já foi convertido com perda na ingestão, então o `to_text` não "vê" a perda, mas
//!   a flag está lá. Nomes com surrogate solto saem com U+FFFD.

use std::io::{self, Write};

use serde::Serialize;

use crate::names::to_text;
use crate::report::ScanReport;
use crate::tree::{Kind, NO_PARENT, Node, NodeId, ScanTree};

const SCHEMA_VERSION: u32 = 1;
const ENGINE: &str = "dir";

/// Nome e versão da ferramenta que gerou o documento. O esquema exige `name == "disk-analyst"`.
pub struct ToolInfo<'a> {
    pub name: &'a str,
    pub version: &'a str,
}

#[derive(Serialize)]
struct ToolOut<'a> {
    name: &'a str,
    version: &'a str,
}

#[derive(Serialize)]
struct TotalsOut {
    logical_bytes: u64,
    allocated_bytes: u64,
    files: u64,
    folders: u64,
    inaccessible: u64,
    cloud_files: u64,
    links_not_followed: u64,
    hardlink_duplicates: u64,
}

#[derive(Serialize)]
struct FolderOut<'a> {
    path: &'a str,
    own_allocated_bytes: u64,
    own_logical_bytes: u64,
    allocated_bytes: u64,
    logical_bytes: u64,
}

#[derive(Serialize)]
struct FileOut<'a> {
    path: &'a str,
    allocated_bytes: u64,
    logical_bytes: u64,
    flags: &'a [&'static str],
}

#[derive(Serialize)]
struct ItemOut<'a> {
    id: NodeId,
    parent: Option<NodeId>,
    name: &'a str,
    kind: &'static str,
    logical_bytes: u64,
    allocated_bytes: u64,
    flags: &'a [&'static str],
}

/// As flags ligadas de um Node, na ordem fixa do esquema, num array (sem alocar por item).
struct Flags {
    names: [&'static str; 5],
    len: usize,
}

impl Flags {
    fn of(node: &Node) -> Self {
        let mut flags = Flags {
            names: [""; 5],
            len: 0,
        };
        for (name, on) in [
            ("cloud", node.cloud()),
            ("inaccessible", node.inaccessible()),
            ("hardlink", node.hardlink()),
            ("name_lossy", node.name_lossy()),
            ("not_scanned", node.not_scanned()),
        ] {
            if on {
                flags.names[flags.len] = name;
                flags.len += 1;
            }
        }
        flags
    }

    fn as_slice(&self) -> &[&'static str] {
        &self.names[..self.len]
    }
}

fn put<W: Write, T: Serialize + ?Sized>(out: &mut W, value: &T) -> io::Result<()> {
    serde_json::to_writer(out, value).map_err(io::Error::from)
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}

/// Escreve o documento JSON v1 de `report` e `tree` em `out`. Erros de escrita são propagados.
pub fn write_json<W: Write>(
    out: &mut W,
    report: &ScanReport,
    tree: &ScanTree,
    tool: &ToolInfo,
) -> io::Result<()> {
    write!(out, "{{\"schema_version\":{SCHEMA_VERSION},\"tool\":")?;
    put(
        out,
        &ToolOut {
            name: tool.name,
            version: tool.version,
        },
    )?;
    out.write_all(b",\"root\":")?;
    put(out, &report.root)?;
    let duration_ms = u64::try_from(report.duration.as_millis()).unwrap_or(u64::MAX);
    write!(
        out,
        ",\"engine\":\"{ENGINE}\",\"partial\":{},\"duration_ms\":{duration_ms}",
        report.partial
    )?;

    let totals = &report.totals;
    out.write_all(b",\"totals\":")?;
    put(
        out,
        &TotalsOut {
            logical_bytes: totals.logical_bytes,
            allocated_bytes: totals.allocated_bytes,
            files: totals.files,
            folders: totals.folders,
            inaccessible: totals.inaccessible,
            cloud_files: totals.cloud_files,
            links_not_followed: totals.links_not_followed,
            hardlink_duplicates: totals.hardlink_duplicates,
        },
    )?;

    let folders: Vec<FolderOut> = report
        .top_folders
        .iter()
        .map(|folder| FolderOut {
            path: &folder.path,
            own_allocated_bytes: folder.own_allocated_bytes,
            own_logical_bytes: folder.own_logical_bytes,
            allocated_bytes: folder.allocated_bytes,
            logical_bytes: folder.logical_bytes,
        })
        .collect();
    out.write_all(b",\"top_folders\":")?;
    put(out, &folders)?;

    let file_flags: Vec<Flags> = report
        .top_files
        .iter()
        .map(|file| {
            usize::try_from(file.node)
                .ok()
                .and_then(|index| tree.nodes.get(index))
                .map_or_else(
                    || Flags {
                        names: [""; 5],
                        len: 0,
                    },
                    Flags::of,
                )
        })
        .collect();
    let files: Vec<FileOut> = report
        .top_files
        .iter()
        .zip(&file_flags)
        .map(|(file, flags)| FileOut {
            path: &file.path,
            allocated_bytes: file.allocated_bytes,
            logical_bytes: file.logical_bytes,
            flags: flags.as_slice(),
        })
        .collect();
    out.write_all(b",\"top_files\":")?;
    put(out, &files)?;

    // `items` em streaming: um objeto de cada vez, sem montar um vetor com milhões deles.
    out.write_all(b",\"items\":[")?;
    for (index, node) in tree.nodes.iter().enumerate() {
        if index > 0 {
            out.write_all(b",")?;
        }
        let id = NodeId::try_from(index).map_err(|_| invalid("mais itens do que cabem em u32"))?;
        let name = to_text(tree.name(id)).0;
        let flags = Flags::of(node);
        put(
            out,
            &ItemOut {
                id,
                parent: (node.parent != NO_PARENT).then_some(node.parent),
                name: &name,
                kind: match node.kind() {
                    Kind::File => "file",
                    Kind::Dir => "dir",
                    Kind::Link => "link",
                },
                logical_bytes: node.logical,
                allocated_bytes: node.allocated,
                flags: flags.as_slice(),
            },
        )?;
    }
    out.write_all(b"]}\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble::{ChunkEntry, DirChunk, aggregate, assemble};
    use crate::report::build_report;
    use crate::tree::{Kind, ScanTree};
    use std::path::MAIN_SEPARATOR;
    use std::time::Duration;

    fn entry(
        units: &[u16],
        kind: Kind,
        subdir: Option<u32>,
        logical: u64,
        allocated: u64,
    ) -> (Vec<u16>, ChunkEntry) {
        (
            units.to_vec(),
            ChunkEntry {
                name_off: 0,
                name_len: u16::try_from(units.len()).unwrap(),
                kind,
                cloud: false,
                inaccessible: false,
                name_lossy: false,
                logical,
                allocated,
                file_id: None,
                subdir,
            },
        )
    }

    fn chunk(id: u32, items: Vec<(Vec<u16>, ChunkEntry)>) -> DirChunk {
        let mut names = Vec::new();
        let mut entries = Vec::new();
        for (units, mut entry) in items {
            entry.name_off = u32::try_from(names.len()).unwrap();
            names.extend(units);
            entries.push(entry);
        }
        DirChunk {
            id,
            names,
            entries,
            inaccessible: false,
        }
    }

    fn utf16(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    /// Raiz + `a\u{D800}` (arquivo na nuvem, nome com surrogate solto, `name_lossy`) + `d` +
    /// `d/a.txt`: 4 Itens. O arquivo de nome inválido tem DUAS flags (`cloud`, `name_lossy`),
    /// para o teste enxergar a ordem delas.
    fn sample_tree() -> ScanTree {
        let mut lossy = entry(&[0x61, 0xD800], Kind::File, None, 100, 4096);
        lossy.1.cloud = true;
        lossy.1.name_lossy = true;
        let root = chunk(0, vec![lossy, entry(&utf16("d"), Kind::Dir, Some(1), 0, 0)]);
        let d = chunk(1, vec![entry(&utf16("a.txt"), Kind::File, None, 10, 12)]);
        let mut tree = assemble(vec![root, d], "raiz".into()).tree;
        aggregate(&mut tree);
        tree.partial = true;
        tree.duration = Duration::from_millis(2500);
        tree
    }

    fn document(tree: &ScanTree) -> String {
        let report = build_report(tree, 5);
        let mut out = Vec::new();
        let tool = ToolInfo {
            name: "disk-analyst",
            version: "0.1.0",
        };
        write_json(&mut out, &report, tree, &tool).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn document_matches_the_exact_golden() {
        let sep = if MAIN_SEPARATOR == '\\' { "\\\\" } else { "/" };
        let golden = concat!(
            r#"{"schema_version":1,"tool":{"name":"disk-analyst","version":"0.1.0"},"#,
            r#""root":"raiz","engine":"dir","partial":true,"duration_ms":2500,"#,
            r#""totals":{"logical_bytes":110,"allocated_bytes":12,"files":2,"folders":2,"#,
            r#""inaccessible":0,"cloud_files":1,"links_not_followed":0,"hardlink_duplicates":0},"#,
            r#""top_folders":[{"path":"raiz{SEP}d","own_allocated_bytes":12,"own_logical_bytes":10,"#,
            r#""allocated_bytes":12,"logical_bytes":10},"#,
            r#"{"path":"raiz","own_allocated_bytes":0,"own_logical_bytes":100,"#,
            r#""allocated_bytes":12,"logical_bytes":110}],"#,
            r#""top_files":[{"path":"raiz{SEP}d{SEP}a.txt","allocated_bytes":12,"logical_bytes":10,"flags":[]},"#,
            r#"{"path":"raiz{SEP}a�","allocated_bytes":0,"logical_bytes":100,"flags":["cloud","name_lossy"]}],"#,
            r#""items":[{"id":0,"parent":null,"name":"","kind":"dir","logical_bytes":110,"allocated_bytes":12,"flags":[]},"#,
            r#"{"id":1,"parent":0,"name":"a�","kind":"file","logical_bytes":100,"allocated_bytes":0,"flags":["cloud","name_lossy"]},"#,
            r#"{"id":2,"parent":0,"name":"d","kind":"dir","logical_bytes":10,"allocated_bytes":12,"flags":[]},"#,
            r#"{"id":3,"parent":2,"name":"a.txt","kind":"file","logical_bytes":10,"allocated_bytes":12,"flags":[]}]}"#,
            "\n"
        )
        .replace("{SEP}", sep);

        assert_eq!(document(&sample_tree()), golden);
    }

    fn parsed() -> serde_json::Value {
        serde_json::from_str(&document(&sample_tree())).expect("JSON válido")
    }

    #[test]
    fn a_lone_surrogate_comes_out_as_the_replacement_character_with_the_flag() {
        let value = parsed();
        let item = &value["items"][1];
        assert_eq!(item["name"], "a\u{FFFD}");
        let flags: Vec<&str> = item["flags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f.as_str().unwrap())
            .collect();
        assert!(flags.contains(&"name_lossy"), "{flags:?}");
    }

    #[test]
    fn file_sizes_in_items_add_up_to_the_totals() {
        let value = parsed();
        let sum = |field: &str| -> u64 {
            value["items"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|item| item["kind"] == "file")
                .map(|item| item[field].as_u64().unwrap())
                .sum()
        };
        assert_eq!(
            sum("logical_bytes"),
            value["totals"]["logical_bytes"].as_u64().unwrap()
        );
        assert_eq!(
            sum("allocated_bytes"),
            value["totals"]["allocated_bytes"].as_u64().unwrap()
        );
    }

    #[test]
    fn the_root_has_a_null_parent_and_every_other_parent_is_a_smaller_id() {
        let value = parsed();
        let items = value["items"].as_array().unwrap();
        assert!(items[0]["parent"].is_null());
        for (index, item) in items.iter().enumerate().skip(1) {
            assert_eq!(item["id"].as_u64().unwrap(), u64::try_from(index).unwrap());
            let parent = item["parent"].as_u64().expect("pai inteiro");
            assert!(
                parent < u64::try_from(index).unwrap(),
                "item {index} aponta para {parent}"
            );
        }
    }

    #[test]
    fn partial_and_duration_are_a_bool_and_an_integer() {
        let value = parsed();
        assert_eq!(value["partial"], true);
        assert!(value["duration_ms"].is_u64());
        assert_eq!(value["duration_ms"], 2500);
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["engine"], "dir");
    }

    #[test]
    fn the_document_is_compact_and_ends_with_a_single_newline() {
        let text = document(&sample_tree());
        assert!(text.ends_with("}\n"));
        assert_eq!(
            text.matches('\n').count(),
            1,
            "uma linha só, sem pretty-print"
        );
    }
}
