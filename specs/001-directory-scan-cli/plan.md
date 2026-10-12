# Implementation Plan: Varredura de diretórios e relatório na CLI

**Branch**: `001-directory-scan-cli` (trabalho em `main` até decisão de criar branch) | **Date**: 2026-10-12 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/001-directory-scan-cli/spec.md`

## Summary

Entregar a Fase 0 (workspace, portões de qualidade, gerador de árvore sintética, benchmark-base) e a
Fase 1 (Varredura de diretórios paralela com relatório de texto e JSON) do roadmap. O núcleo
enumera pastas em paralelo com `rayon`; no Windows usa `GetFileInformationByHandleEx` +
`FileIdBothDirectoryInfo` para obter Tamanho lógico, alocado, atributos e `FileId` sem abrir
arquivos (ADR-0007), e no Linux usa `std::fs`. Os resultados por pasta são montados numa arena
compacta (`Vec<Node>` ≤ 40 bytes por Item, nomes UTF-16 num buffer contínuo, ADR-0006), com
agregação de baixo para cima e deduplicação de hard links por ordenação. A CLI emite relatório em
português ou JSON versionado com a árvore completa. Detalhes em [research.md](research.md).

## Technical Context

**Language/Version**: Rust stable 1.99, edition 2024

**Primary Dependencies**: `windows` 0.62 (só Windows), `rayon` 1.12, `serde`/`serde_json` 1.0,
`lexopt` 0.3, `ctrlc` 3.5; dev: `proptest` 1.12, `criterion` 0.8, `jsonschema` 0.58 (R13)

**Storage**: N/A (nada persistido; snapshots são da Fase 5)

**Testing**: `cargo test` no Linux e com `--target x86_64-pc-windows-gnu` pelo interop (R1);
`proptest`; testes de contrato com `jsonschema`; benchmarks `criterion` e `hyperfine`

**Target Platform**: Windows 10/11 x64 (`x86_64-pc-windows-gnu`); Linux para desenvolvimento

**Project Type**: workspace Cargo com biblioteca (`core`), CLI (`cli`) e ferramenta de
desenvolvimento (`xtask`)

**Performance Goals**: `C:\` (≈ 235 GB) em < 15 s com cache quente, saída de texto (SC-002)

**Constraints**: ≤ 40 bytes por Node na arena (meta de design; SC-003 exige < 64); cancelamento
com relatório em ≤ 1 s; somente leitura; sem rede; sem administrador

**Scale/Scope**: milhões de Itens por volume; JSON de `C:\` na casa de centenas de MB (medido, sem
meta)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Princípio | Status | Como o plano cumpre |
|---|---|---|
| I. Desempenho medido | ✅ | Benchmark-base após o primeiro varredor; comando-padrão de benchmark rodado antes e depois de toda tarefa que altera o caminho crítico (marcadas ⏱ em tasks.md); `hyperfine` + `--stats` + `criterion`; resultados em `docs/benchmarks/` (R11) |
| II. Segurança dos dados | ✅ | Só leitura; nenhuma operação de escrita no caminho varrido (FR-014) |
| III. Testes primeiro | ✅ | TDD; `proptest` para arena, agregação e hard links; testes `#[ignore]` documentados (ponto de montagem). O leitor do buffer de diretório do Windows é parser binário: fica numa função pura e safe sobre `&[u8]` (`scan/windows/buffer.rs`) com `proptest`; o fuzzing dele entra junto com o da MFT na Fase 2, quando nightly e `cargo-fuzz` forem instalados (ADR-0002) |
| IV. Unsafe mínimo | ✅ | `unsafe` só em `crates/core/src/scan/windows/ffi.rs` (inclui a consulta de pico de memória) e em `crates/synth/src/ffi_windows.rs` (oráculo de teste), cada bloco com `// SAFETY:`; resto safe |
| V. Camadas | ✅ | `core` sem terminal: expõe contadores e token de cancelamento; `cli` faz argumentos, Ctrl+C, progresso e formatação |
| VI. Privacidade | ✅ | Nenhuma dependência de rede; nenhuma chamada de rede |
| VII. IA conselheira | N/A | Sem MCP nesta feature |
| VIII. Sala limpa | ✅ | Só documentação pública (Microsoft Learn, MS-FSCC, docs das crates) |
| IX. Plataforma | ✅ | Windows via `cfg(windows)`; Linux via `std::fs`; testes nos dois alvos |
| X. Formatos estáveis | ✅ | `schema_version: 1`, esquema publicado, testes de contrato (R9) |
| XI. Simplicidade | ✅ | Cada dependência justificada em R13; `lexopt` em vez de `clap`; `xtask` em vez de scripts |

**Re-check pós-design**: sem violações. Nenhuma entrada em Complexity Tracking.

## Project Structure

### Documentation (this feature)

```text
specs/001-directory-scan-cli/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── cli.md
│   └── scan-report-v1.schema.json
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
Cargo.toml                      # workspace: crates/core, crates/cli, crates/synth, xtask
.cargo/config.toml              # alias: xtask = "run -p xtask --"
crates/core/                    # biblioteca disk_analyst_core
├── src/
│   ├── lib.rs
│   ├── tree.rs                 # Node, NodeId, ScanTree, flags
│   ├── names.rs                # NameBuf (UTF-16), conversão com perdas
│   ├── assemble.rs             # DirChunk → arena determinística + agregação
│   ├── hardlinks.rs            # deduplicação por ordenação (R5)
│   ├── report.rs               # ScanReport, rankings, serde
│   ├── json.rs                 # escrita em streaming de items[]
│   ├── control.rs              # CancelToken, ScanCounters
│   ├── sysinfo.rs              # pico de memória do processo (wrapper safe)
│   └── scan/
│       ├── mod.rs              # API pública: scan(path, opts) -> ScanTree
│       ├── parallel.rs         # rayon::scope, uma tarefa por pasta
│       ├── unix.rs             # cfg(unix): std::fs
│       └── windows/
│           ├── buffer.rs       # leitor puro e safe de FILE_ID_BOTH_DIR_INFO (todos os alvos)
│           ├── mod.rs          # cfg(windows): enumeração, reparse, nuvem
│           └── ffi.rs          # único módulo com unsafe
├── tests/                      # integração sobre árvores sintéticas
└── benches/                    # criterion
crates/cli/                     # binário disk-analyst
├── src/
│   ├── main.rs
│   ├── args.rs                 # lexopt, mensagens em português
│   ├── text.rs                 # relatório de texto, unidades binárias
│   ├── progress.rs
│   └── stats.rs                # --stats: pico de memória
└── tests/                      # contrato: códigos de saída, stdout/stderr, esquema
crates/synth/                   # biblioteca (publish = false), dev-dependency de core e cli
└── src/
    ├── lib.rs                  # FR-016: descrição JSON → árvore + expected.json
    ├── oracle.rs               # Tamanho alocado medido arquivo a arquivo (R10)
    └── ffi_windows.rs          # cfg(windows): FileStandardInfo por arquivo; único unsafe da crate
xtask/
└── src/
    ├── main.rs                 # subcomandos: check, synth (embrulha crates/synth)
    └── check.rs                # FR-017
tests/fixtures/                 # descrições de árvores sintéticas (JSON)
docs/schemas/scan-report-v1.schema.json
docs/benchmarks/
```

**Structure Decision**: workspace com `crates/core` e `crates/cli` (Fase 0 do roadmap), mais
`crates/synth` (gerador e oráculo de árvores sintéticas, usado como biblioteca pelos testes nos dois
alvos, independente do núcleo para não ser tautológico) e `xtask` (verificação de qualidade e
atalho de linha de comando para o gerador). Nada de `synth` ou `xtask` entra no binário entregue.

## Complexity Tracking

Sem violações da constituição a justificar.
