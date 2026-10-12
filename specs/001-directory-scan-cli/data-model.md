# Data Model: Varredura de diretórios e relatório na CLI

Termos conforme [GLOSSARY.md](../../GLOSSARY.md). Tipos descritos em nível de design; nomes finais
podem variar na implementação.

## Node (Item na arena)

Um registro por Item, num `Vec<Node>` indexado por `NodeId = u32`. Meta: `size_of::<Node>() <= 40`
(asserção em tempo de compilação; SC-003 exige < 64).

| Campo | Tipo | Regra |
|---|---|---|
| `name_off` | `u32` | Offset do nome no `NameBuf` |
| `name_len` | `u16` | Comprimento em unidades UTF-16 (nomes NTFS têm no máximo 255) |
| `flags` | `u16` | Bits: `kind` (2 bits: arquivo, pasta, link não seguido), `cloud`, `inaccessible`, `hardlink`, `name_lossy`, `not_scanned` |
| `parent` | `u32` | `NodeId` do pai; `u32::MAX` na raiz |
| `first_child` | `u32` | Primeiro filho (filhos são contíguos); só pastas |
| `child_count` | `u32` | Número de filhos diretos; só pastas |
| `logical` | `u64` | Arquivo: Tamanho lógico. Pasta: soma agregada dos descendentes |
| `allocated` | `u64` | Arquivo: Tamanho alocado. Pasta: soma agregada dos descendentes |

Regras de validação:
- Pasta: `logical`/`allocated` = soma dos filhos (FR-011). Tamanho próprio (ranking, FR-003) é
  calculado na hora como soma dos filhos do tipo arquivo, não armazenado.
- Link não seguido, Item inacessível e pasta `not_scanned`: tamanhos zero, `child_count = 0`.
- Arquivo na nuvem: `allocated = 0` (FR-009).
- Hard link secundário (FR-008): `logical = allocated = 0`, marca `hardlink`.
- Filhos de cada pasta ordenados por nome (ordinal UTF-16), para saída determinística.

## NameBuf

`Vec<u16>` contínuo com todos os nomes (ADR-0006). Nomes podem conter surrogates soltos; a
conversão para texto (com U+FFFD e marca `name_lossy`) acontece só na saída. No Linux, nomes que
não são UTF-8 válido também recebem `name_lossy`.

## ScanTree

| Campo | Descrição |
|---|---|
| `nodes` | `Vec<Node>`; índice 0 é a raiz |
| `names` | `NameBuf` |
| `root_path` | Caminho da raiz como pedido pelo usuário (texto) |
| `partial` | `true` se a varredura foi cancelada |
| `duration` | Duração da varredura |

Operações: caminho completo de um `NodeId` (sobe pelos pais); iteração em ordem; agregação.

## Estruturas temporárias da varredura

- **DirChunk**: resultado de enumerar uma pasta (id temporário, id temporário do pai, registros
  brutos com tamanhos, atributos e `FileId`, nomes UTF-16 locais). Descartado após a montagem.
- **HardLinkCandidate**: `(file_id: u64, node: NodeId)`; vetor ordenado após a montagem (R5) e
  descartado.
- **ScanCounters**: contadores atômicos (`items`, `files`, `bytes`) lidos pela CLI para o progresso.
- **CancelToken**: `Arc<AtomicBool>` ligado pela CLI no Ctrl+C.

## ScanReport (contrato, ver contracts/)

| Campo | Descrição |
|---|---|
| `schema_version` | Inteiro, começa em 1 |
| `root`, `engine`, `partial`, `duration_ms` | Metadados |
| `totals` | `logical_bytes`, `allocated_bytes`, `files`, `folders`, `inaccessible`, `cloud_files`, `links_not_followed`, `hardlink_duplicates` |
| `top_folders[]` | `path`, `own_allocated_bytes`, `own_logical_bytes`, `allocated_bytes`, `logical_bytes` |
| `top_files[]` | `path`, `allocated_bytes`, `logical_bytes`, `flags` |
| `items[]` | Só no JSON: `id`, `parent`, `name`, `kind`, `logical_bytes`, `allocated_bytes`, `flags` |

Regras: rankings ordenados por Tamanho alocado (pastas pelo próprio) decrescente, desempate pelo
caminho; no máximo N entradas cada. A raiz conta em `folders` quando é pasta. `files` exclui hard
links secundários; `links_not_followed` e `inaccessible` não entram em `files` nem em `folders`.

## SyntheticTreeSpec (crates/synth)

Descrição JSON de uma árvore de teste: lista de entradas com `path`, `kind` (`file`, `dir`,
`hardlink`, `symlink`, `junction`, `denied_dir`, `offline_file`), `size` (arquivos) e `target`
(links). O gerador cria a árvore e grava `expected.json` com totais e contagens esperados (R10).

## Ciclo de vida de uma varredura

```
Iniciada → Varrendo → (Cancelada | Concluída) → Montada (arena + agregação + hard links)
         → Relatório emitido
```

Cancelada também passa por Montada; o relatório sai com `partial = true`.
