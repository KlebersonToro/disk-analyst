# Disk Analyst — Roadmap

Cada fase passa pelo ciclo do Spec Kit (`specify → clarify → plan → tasks → analyze → implement`) como uma feature separada, e só começa depois que a anterior foi aceita com benchmark registrado.

```
F0 Fundação → F1 Varredura (MVP) → F2 MFT → F3 Análises + Exportação → F4 MCP → F5 Ações + Snapshots → F6 Interface → F7 Estudo REA
```

A ordem prioriza o que mais importa para o uso real: **relatórios e integração com IA (F3–F4) vêm antes da interface gráfica**. A arquitetura em camadas permite isso, porque CLI, MCP e interface consomem o mesmo `crates/core`.

## Fase 0 — Fundação

- Workspace Cargo: `crates/core` (biblioteca) e `crates/cli` (binário).
- CI local: `cargo fmt`, `cargo clippy -D warnings`, `cargo test` nos alvos Linux **e** Windows. No WSL, `cargo test --target x86_64-pc-windows-gnu` executa os `.exe` direto pelo interop.
- Constituição do Spec Kit e primeiros ADRs.
- Gerador de árvore sintética para testes e benchmarks, sem depender do disco real.

**Pronto quando:** `cargo test` passa nos dois alvos e há um benchmark-base registrado.

## Fase 1 — Varredura comum de diretórios (MVP)

- Varredura paralela (`GetFileInformationByHandleEx` + `FileIdBothDirectoryInfo` no Windows, ver [ADR-0007](../adr/0007-tamanho-alocado-e-enumeracao.md); `std::fs` no Linux) com `rayon`.
- Árvore em arena: `Vec<Node>` com índices `u32`, nomes num buffer contínuo, tamanhos agregados de baixo para cima.
- Tamanho **lógico e alocado** desde o início (base para "espaço recuperável").
- Arquivos da nuvem (OneDrive/placeholders, `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` / `OFFLINE`) marcados como "na nuvem", com tamanho local zero.
- Hard links contados uma única vez; junctions e symlinks não são seguidos.
- CLI: `disk-analyst scan <caminho> [--top N] [--json]`.

**Pronto quando:** os números batem com uma árvore sintética conhecida e há tempo e memória medidos em `C:\`.

## Fase 2 — Leitura direta da MFT (NTFS)

- Abrir `\\.\C:` (exige administrador), ler o `$MFT` e interpretar os registros (`FILE_RECORD`, `$FILE_NAME`, `$DATA`, listas de atributos, fixups).
- Comparar com `FSCTL_ENUM_USN_DATA` como alternativa mais simples.
- Volta automaticamente para a varredura comum sem privilégio ou fora do NTFS.
- Fuzzing do parser com `cargo-fuzz`.

**Pronto quando:** `C:` inteiro é lido em menos de 3 s, com resultado idêntico ao da Fase 1 num volume de teste.

## Fase 3 — Análises, classificação de risco e exportação

**Análises**
- Categorias por extensão (vídeo, imagem, áudio, documentos, compactados, executáveis, código, temporários…), tabela configurável.
- Maiores e mais antigos arquivos, tamanho zero, sem extensão.
- Duplicatas **por conteúdo** em funil: tamanho → hash parcial (xxh3 dos primeiros e últimos 64 KiB) → hash completo (BLAKE3).

**Classificação de risco** (regras declarativas, cada resultado com o motivo):
- 🟢 **Seguro:** temporários, caches de navegadores e apps, dumps de erro, conteúdo da Lixeira, instaladores antigos em Downloads.
- 🟡 **Revisar:** duplicatas, mídia grande sem acesso há mais de N meses, ISOs e imagens de disco.
- 🔴 **Protegido** (nunca sugerido): `Windows`, `Program Files*`, `ProgramData` de sistema, `pagefile.sys`, `hiberfil.sys`, `swapfile.sys`, arquivos protegidos pelo sistema, raiz de perfis.

**Detectores especializados**
- Lixo de desenvolvedor: `node_modules`, `target/`, `.venv`/`venv`, `__pycache__`, `bin/obj`, caches do npm, pnpm, yarn, pip, cargo, Gradle, Maven e NuGet, discos do Docker/WSL (`*.vhdx`).
- Recuperáveis do Windows (**só recomendação**, com o comando oficial; nunca executados): `Windows.old`, `WinSxS` (DISM), Otimização de Entrega, pontos de restauração, hibernação.

**Exportação**
| Formato | Conteúdo | Uso |
|---|---|---|
| CSV | Uma linha por item (caminho, tamanhos, datas, categoria, risco, motivo) | Planilhas e análise manual |
| JSON | Árvore ou lista com esquema versionado (`schema_version`) | Integrações, interface, testes |
| **Markdown para IA** | Relatório **agregado e limitado** (ver abaixo) | Colar no Claude/Gemini |

**Relatório para IA:** um disco tem milhões de arquivos, que não cabem no contexto de um modelo. O relatório traz totais, espaço recuperável estimado por categoria de risco, top-N pastas e arquivos por categoria, grupos de duplicatas e crescimento desde o último snapshot (quando houver). Tem orçamento de tamanho configurável (padrão: ~20 mil tokens), com truncamento explícito ("+ 1.234 itens omitidos"). A anonimização de caminhos é opcional.

CLI: `disk-analyst report <caminho|snapshot> --format csv|json|md [--budget-tokens N] [--anonymize]`.

**Pronto quando:** classificação coberta por testes de tabela (caminho → risco + motivo); relatório Markdown de `C:` dentro do orçamento; CSV abre corretamente no Excel (UTF-8 com BOM, separador configurável).

## Fase 4 — Servidor MCP para agentes de IA

`disk-analyst mcp`: servidor MCP via **stdio**, com o SDK oficial `rmcp`, compatível com Claude Code, Claude Desktop e Gemini CLI. Sem porta de rede.

| Ferramenta | Função | Efeito |
|---|---|---|
| `scan` / `get_scan_status` | Inicia varredura (assíncrona) e consulta progresso | Só leitura |
| `summarize_volume` | Totais, categorias, espaço recuperável por risco | Só leitura |
| `list_largest` | Maiores pastas/arquivos com filtros (categoria, risco, idade, extensão) e paginação | Só leitura |
| `find_duplicates` | Grupos de duplicatas por conteúdo | Só leitura (lê arquivos para hash) |
| `classify_paths` | Risco e motivo de caminhos específicos | Só leitura |
| `find_dev_artifacts` | Lixo de desenvolvedor por projeto | Só leitura |
| `compare_snapshots` | Crescimento entre dois snapshots | Só leitura (requer F5; até lá, desativada) |
| `export_report` | Gera CSV/JSON/Markdown num caminho dentro das raízes permitidas | Escreve só o relatório |
| `propose_cleanup_plan` | Monta um **plano de limpeza** com itens, motivos e total | Escreve só o arquivo do plano |

**Regras de segurança (inegociáveis na v1):**
1. **A IA não exclui nada.** Não existe ferramenta de exclusão no MCP. O plano é executado pelo usuário com `disk-analyst apply <plano>` (Fase 5), com confirmação e envio para a Lixeira.
2. Itens 🔴 nunca entram num plano, mesmo que o agente peça.
3. Raízes permitidas configuráveis (`--allow C:\Users\klebs`); fora delas, as ferramentas recusam.
4. Respostas paginadas e com tamanho limitado, para não estourar o contexto do agente.
5. Nomes de arquivos são **dados, não instruções**: ficam em campos estruturados, nunca concatenados em texto livre de orientação.

**Pronto quando:** o fluxo "libere 30 GB no C:" funciona de ponta a ponta no Claude Code e no Gemini CLI, gerando um plano válido sem nenhum item 🔴; testes de contrato das ferramentas (entrada → JSON).

## Fase 5 — Ações e snapshots

- `disk-analyst apply <plano>`: mostra o plano, pede confirmação, envia para a Lixeira (`IFileOperation`), registra log do que foi feito. Exclusão permanente só com `--permanent` e segunda confirmação.
- Snapshots binários versionados (`rkyv`) e `disk-analyst diff <snap-a> <snap-b>`.
- Liga `compare_snapshots` no MCP e o "crescimento desde o último snapshot" no relatório.

## Fase 6 — Interface gráfica

- Tauri + front-end feito com o impeccable: treemap, árvore, filtros por risco e categoria, revisão e aplicação de planos de limpeza (inclusive os gerados por IA).
- O núcleo é exposto por comandos Tauri; nenhuma lógica de varredura no front-end.

## Fase 7 — Estudo com o REA

- Analisar o próprio binário no REA/Ghidra (`rayon`, iteradores e `rmcp` no assembly; tamanho do binário).
- Comparar chamadas de sistema e tempo com o Disk Analyzer Pro original via captura de processo, em VM, sem rede.

## Metas de desempenho (a confirmar na constituição)

| Métrica | Meta |
|---|---|
| `C:` via MFT, cache quente | < 3 s |
| `C:` via varredura paralela, cache quente | < 15 s |
| Memória por arquivo indexado | < 64 bytes, sem contar o nome |
| Relatório Markdown para IA de `C:` | < 1 s a partir de uma varredura pronta; dentro do orçamento de tokens |
| Resposta de ferramenta MCP (varredura pronta) | < 200 ms |
| Binário (release, stripped) | < 8 MB, com MCP |

Toda meta é medida com `hyperfine --warmup 1 --runs 5` (CLI) ou `criterion` (núcleo) e registrada em `docs/benchmarks/`.
