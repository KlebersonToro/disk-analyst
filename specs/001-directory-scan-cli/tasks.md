---

description: "Tarefas da feature 001: varredura de diretórios e relatório na CLI"
---

# Tasks: Varredura de diretórios e relatório na CLI

**Input**: Design documents from `specs/001-directory-scan-cli/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md),
[data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: obrigatórios (Princípio III, TDD). **Toda tarefa começa escrevendo o teste descrito,
vendo-o falhar (`cargo test` vermelho), e só então implementa.** Uma tarefa está pronta quando o
teste passa e os portões rodam limpos (`cargo xtask check`, a partir de T002), nos dois alvos.

**Benchmark antes/depois (Princípio I)**: toda tarefa marcada **⏱** altera o caminho crítico da
varredura. Antes de começar e depois de terminar, rodar o **comando-padrão de benchmark** definido em
T019 e registrar os dois resultados em `docs/benchmarks/` no formato de T003. Regressão sem
justificativa registrada bloqueia a tarefa.

**Organization**: por história de usuário (spec.md). A US6 (fundação) é pré-requisito de todas as
outras e por isso está nas Fases 1–2, sem rótulo de história, como manda o formato.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: pode rodar em paralelo (arquivos diferentes, sem dependência pendente)
- **[Story]**: história de usuário (US1…US5)
- Caminhos relativos à raiz do repositório

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: workspace compilando nos dois alvos e portão de qualidade único (US6, FR-017)

- [x] T001 Criar o workspace: `Cargo.toml` raiz (membros `crates/core`, `crates/cli`, `crates/synth`, `xtask`; `edition = "2024"`, `rust-version = "1.99"`, `[workspace.lints]` com `unsafe_code = "deny"`, liberado com `#[allow(unsafe_code)]` só nos módulos FFI), `.cargo/config.toml` com `alias xtask = "run -p xtask --"`, e esqueletos `crates/core/src/lib.rs` (lib `disk_analyst_core`), `crates/cli/src/main.rs` (bin `disk-analyst`), `crates/synth/src/lib.rs` (`publish = false`), `xtask/src/main.rs`. Teste primeiro: `crates/core/tests/smoke.rs` com um teste que chama `disk_analyst_core::version()` e espera `env!("CARGO_PKG_VERSION")`. Verificar com `cargo test` e `cargo test --target x86_64-pc-windows-gnu`.
- [x] T002 Implementar `cargo xtask check` em `xtask/src/check.rs` (R12): função pura `steps() -> Vec<Step>` com, em ordem, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, o mesmo clippy com `--target x86_64-pc-windows-gnu`, `cargo test --workspace`, `cargo test --workspace --target x86_64-pc-windows-gnu`; executor que para no primeiro código ≠ 0 e o propaga. Teste primeiro (unitário em `xtask/src/check.rs`): `steps()` devolve exatamente essa sequência; o executor, com um passo falso que falha, não executa os seguintes.
- [x] T003 [P] Criar `docs/benchmarks/README.md` com o formato obrigatório de registro (comando exato, commit, hardware de `docs/planning/00-visao.md`, data, mediana/min/max, observações) e copiar `specs/001-directory-scan-cli/contracts/scan-report-v1.schema.json` para `docs/schemas/scan-report-v1.schema.json`. Teste primeiro: `crates/cli/tests/schema_file.rs` verifica que os dois arquivos de esquema são idênticos byte a byte (evita divergência).

**Checkpoint**: `cargo xtask check` verde nos dois alvos.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: tipos da arena, montagem, agregação, hard links, relatório, gerador sintético e
medição de memória. **Nenhuma história começa antes desta fase.**

- [x] T004 [P] Implementar `NameBuf` em `crates/core/src/names.rs` (ADR-0006): `push(&[u16]) -> (u32 offset, u16 len)`, `get(off, len) -> &[u16]`, `to_text(&[u16]) -> (String, lossy: bool)` com U+FFFD para surrogates soltos. Teste primeiro: proptest — toda `String` válida faz ida e volta sem `lossy`; unitário — `[0xD800, 0x0041]` vira `"\u{FFFD}A"` com `lossy = true`; nome de 255 unidades aceito.
- [x] T005 [P] Implementar `Node`, `NodeId = u32`, `Kind {File, Dir, Link}` e flags em `crates/core/src/tree.rs` conforme data-model.md: campos `name_off: u32`, `name_len: u16`, `flags: u16` ("Bits: `kind` (2 bits: arquivo, pasta, link não seguido), `cloud`, `inaccessible`, `hardlink`, `name_lossy`, `not_scanned`"), `parent: u32` ("`u32::MAX` na raiz"), `first_child: u32`, `child_count: u32`, `logical: u64`, `allocated: u64`. Teste primeiro: `const _: () = assert!(size_of::<Node>() <= 40);` e unitários de set/get de cada flag e de `kind` sem interferência entre bits.
- [x] T006 [P] Implementar `CancelToken` (`Arc<AtomicBool>`) e `ScanCounters` (`items`, `files`, `bytes` atômicos, `Relaxed`) em `crates/core/src/control.rs`. Teste primeiro: clone do token vê o cancelamento; contadores somados por 8 threads dão o total exato.
- [x] T007 Implementar `DirChunk` e a montagem determinística em `crates/core/src/assemble.rs` (R6): chunks com id temporário e pai temporário → `ScanTree` com raiz no índice 0, filhos de cada pasta contíguos e ordenados por nome (ordinal UTF-16), `parent`/`first_child`/`child_count` coerentes. Teste primeiro: proptest — a mesma floresta de chunks entregue em ordens aleatórias produz arenas idênticas; invariantes: todo filho aponta para o pai correto e os filhos estão ordenados.
- [x] T008 Implementar a agregação de baixo para cima em `crates/core/src/assemble.rs` (FR-011): pasta = soma dos filhos; "Link não seguido, Item inacessível e pasta `not_scanned`: tamanhos zero, `child_count = 0`"; "Arquivo na nuvem: `allocated = 0`". Teste primeiro: proptest — em árvores aleatórias, toda pasta tem `logical`/`allocated` iguais à soma dos filhos e a raiz iguala a soma de todos os arquivos.
- [x] T009 Implementar a deduplicação de hard links em `crates/core/src/hardlinks.rs` (R5, ADR-0010): entrada `Vec<(u64 file_id, NodeId)>`; ordena por `file_id`; em cada grupo com mais de um elemento, o caminho de menor ordem lexicográfica (UTF-16 código a código) mantém o tamanho e os demais ficam com "`logical = allocated = 0`, marca `hardlink`"; re-agrega depois. Teste primeiro: unitário com 3 links em `b/x`, `a/y`, `c/z` → só `a/y` mantém o tamanho, total contado uma vez; proptest — o total da raiz após a deduplicação é igual à soma dos tamanhos de `file_id` distintos.
- [ ] T010 Implementar `ScanReport` em `crates/core/src/report.rs` (FR-002, FR-003, data-model.md): totais (`logical_bytes`, `allocated_bytes`, `files`, `folders`, `inaccessible`, `cloud_files`, `links_not_followed`, `hardlink_duplicates`), `top_files` por alocado e `top_folders` por alocado **próprio** (soma só dos arquivos filhos diretos), cada um com no máximo N, desempate pelo caminho; "A raiz conta em `folders` quando é pasta. `files` exclui hard links secundários; `links_not_followed` e `inaccessible` não entram em `files` nem em `folders`." Teste primeiro: testes de tabela sobre árvores montadas à mão (empate, N maior que o número de Itens, árvore vazia, pasta aninhada grande que aparece sem a cadeia de pais).
- [ ] T011 Implementar o gerador sintético em `crates/synth/src/lib.rs` (FR-016, R10), sem depender de `disk_analyst_core`: `SyntheticTreeSpec` lido de JSON (`path`, `kind`, `size`, `target`; nesta tarefa os tipos `file`, `dir` e `bulk` — este último com `depth`, `dirs_per_dir`, `files_per_dir`, `max_size`, `seed`, gerando de forma determinística árvores grandes sem listar cada arquivo), `generate(spec, dest) -> Expected` que cria a árvore e grava `expected.json` com Tamanho lógico total e contagens. Teste primeiro: gerar a mesma descrição duas vezes em pastas temporárias diferentes produz conteúdo idêntico e `expected.json` idêntico; tamanhos em disco batem com a descrição; `bulk` com a mesma semente gera a mesma árvore. Criar `tests/fixtures/basic.json` (3 pastas, 10 arquivos, tamanhos variados incluindo 0 e 1 byte) e `tests/fixtures/large.json` (`bulk` com ≈ 200 mil arquivos).
- [ ] T012 Implementar o oráculo de Tamanho alocado em `crates/synth/src/oracle.rs` e `crates/synth/src/ffi_windows.rs` (R10): Linux `metadata.blocks() * 512`; Windows `CreateFileW` + `GetFileInformationByHandleEx(FileStandardInfo).AllocationSize` por arquivo, cada bloco `unsafe` com `// SAFETY:`. `generate` passa a gravar `allocated_bytes` esperado. Teste primeiro: nos dois alvos, um arquivo de 10 000 bytes tem alocado ≥ 10 000 e múltiplo de 512; arquivo vazio tem alocado 0.
- [ ] T013 Expor o gerador em `xtask/src/main.rs` como `cargo xtask synth <descrição.json> <destino>`. Teste primeiro: teste de integração em `xtask/tests/synth_cli.rs` roda o subcomando com `tests/fixtures/basic.json` e encontra `expected.json` no destino.
- [ ] T014 [P] Implementar a consulta de pico de memória em `crates/core/src/sysinfo.rs` (wrapper safe) e `crates/core/src/scan/windows/ffi.rs` (`GetProcessMemoryInfo`, `PeakWorkingSetSize`); Linux lê `VmHWM` de `/proc/self/status`. Teste primeiro: após alocar 64 MiB e tocá-los, o pico informado é ≥ 64 MiB, nos dois alvos.

**Checkpoint**: fundação pronta; `cargo xtask check` verde.

---

## Phase 3: User Story 1 - Descobrir o que ocupa espaço num caminho (Priority: P1) 🎯 MVP

**Goal**: `disk-analyst scan <caminho>` mostra totais, contagens e rankings em português.

**Independent Test**: varrer `tests/fixtures/basic.json` gerado e comparar com `expected.json`
(diferença zero), nos dois alvos.

- [ ] T015 [P] [US1] Enumerar uma pasta no Linux em `crates/core/src/scan/unix.rs` (`std::fs::read_dir` + `symlink_metadata`, alocado `blocks * 512`, nomes para UTF-16 com `name_lossy` se não forem UTF-8) produzindo um `DirChunk` com os subdiretórios a percorrer. Teste primeiro (`#[cfg(unix)]`): pasta gerada pelo synth com 2 arquivos e 1 subpasta → chunk com 3 entradas, tamanhos lógicos e alocados iguais aos do oráculo.
- [ ] T016 [P] [US1] Implementar o leitor do buffer de diretório em `crates/core/src/scan/windows/buffer.rs` como função **pura e safe** sobre `&[u8]`, compilada em todos os alvos (sem `cfg(windows)`): itera registros `FILE_ID_BOTH_DIR_INFO` por `NextEntryOffset` (offsets de [MS-FSCC]), devolvendo `EndOfFile`, `AllocationSize`, `FileAttributes`, `EaSize`, `FileId` e o nome UTF-16; valida alinhamento, limites e comprimentos e devolve erro, nunca pânico, em dados malformados. Teste primeiro: unitário com buffer montado à mão (2 registros, último com `NextEntryOffset = 0`); proptest — bytes arbitrários e buffers válidos com campos corrompidos nunca causam pânico nem leitura fora do buffer. (Fuzzing com `cargo-fuzz` entra na Fase 2, ADR-0002.)
- [ ] T017 [US1] Enumerar uma pasta no Windows em `crates/core/src/scan/windows/ffi.rs` e `crates/core/src/scan/windows/mod.rs` (R2): `CreateFileW(FILE_LIST_DIRECTORY, FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)` com prefixo `\\?\`, `GetFileInformationByHandleEx` (`FileIdBothDirectoryRestartInfo` e depois `FileIdBothDirectoryInfo`, buffer de 64 KiB) e leitura dos registros com o leitor de T016; ignorar `.` e `..`; `unsafe` só em `ffi.rs`, só para as chamadas de sistema, com `// SAFETY:`. Teste primeiro (`#[cfg(windows)]`): o mesmo cenário de T015, rodando pelo interop.
- [ ] T018 [US1] Implementar o varredor paralelo e a API pública em `crates/core/src/scan/parallel.rs` e `crates/core/src/scan/mod.rs` (R6, ADR-0005): `scan(path, &ScanOptions, &CancelToken, &ScanCounters) -> Result<ScanTree, ScanError>` com `rayon::scope`, uma tarefa por pasta, chunks num `Mutex<Vec<Option<DirChunk>>>`, montagem (T007) e agregação (T008) no fim. A **raiz** é aberta seguindo links (no Windows, sem `FILE_FLAG_OPEN_REPARSE_POINT`; no Linux, `metadata` em vez de `symlink_metadata`); as demais pastas não. Raiz que é arquivo vira árvore de um Item; caminho inexistente ou raiz ilegível → `ScanError`. Teste primeiro: `crates/core/tests/scan_basic.rs` gera `basic.json` com o synth, varre e compara totais, contagens e alocado com `expected.json` (diferença zero), nos dois alvos; e varre um link para essa árvore (symlink no Linux, junction no Windows via `cmd /c mklink /J`) obtendo os mesmos totais.
- [ ] T019 [US1] **Benchmark-base** (Princípio I, FR-018), logo após o primeiro varredor funcional: `crates/core/benches/baseline.rs` com `criterion` (montagem + agregação de 1 milhão de Nodes sintéticos em memória; deduplicação de 1 milhão de pares de hard links) e `crates/core/examples/scan_totals.rs` (varre um caminho e imprime Itens, tempo, pico de memória via T014 e bytes por Item). Definir e documentar em `docs/benchmarks/README.md` o **comando-padrão de benchmark**: `hyperfine --warmup 1 --runs 5` do `scan_totals.exe` release sobre a árvore de `tests/fixtures/large.json` gerada em `C:\Users\Public\da-large`. Medir também `C:\`, `RAYON_NUM_THREADS=1` versus padrão, o custo fixo do interop (um `.exe` vazio) e o `dua` só se já instalado. Registrar em `docs/benchmarks/2026-10-baseline.md`. Teste primeiro: `crates/core/tests/example_runs.rs` roda o exemplo numa árvore synth e confere o número de Itens com `expected.json`.
- [ ] T020 [P] [US1] Implementar os argumentos em `crates/cli/src/args.rs` com `lexopt` conforme [contracts/cli.md](contracts/cli.md): `scan <CAMINHO>`, `--top N` (padrão 20, inteiro ≥ 1), `--json`, `--progress`, `--engine auto|dir|mft`, `--stats`, `--help`, `--version`; mensagens em português (FR-019). Teste primeiro: tabela de entradas → `Args` ou erro em português (`--top 0`, `--top x`, opção desconhecida, caminho ausente, `--engine mft` → erro "Leitura da MFT ainda não disponível").
- [ ] T021 [P] [US1] Implementar o relatório de texto em `crates/cli/src/text.rs`: cabeçalho com caminho, totais em unidades binárias com vírgula decimal (`182,4 GiB`), contagens com separador de milhar `.`, rankings com colunas "alocado próprio / alocado total / caminho" e "alocado / lógico / caminho [na nuvem]", marca `(PARCIAL)` quando `partial`. Teste primeiro: snapshot de texto exato para um `ScanReport` montado à mão; `format_size(1536) == "1,5 KiB"`, `format_size(0) == "0 B"`.
- [ ] T022 [US1] Ligar tudo em `crates/cli/src/main.rs`: parse → `scan` → `ScanReport` → texto em stdout; códigos de saída 0, 2 e 1 conforme [contracts/cli.md](contracts/cli.md). Teste primeiro: `crates/cli/tests/scan_text.rs` executa o binário (`env!("CARGO_BIN_EXE_disk-analyst")`) sobre a árvore synth: código 0 e totais no texto; `--top 5` mostra 5 de cada; caminho inexistente → código 2 e mensagem em stderr; stdout sem nada além do relatório.
- [ ] T023 [US1] Cobrir os casos-limite de US1 em `crates/cli/tests/scan_edges.rs`: raiz que é arquivo, pasta vazia (totais zero, rankings vazios), árvore com menos Itens que N, empate de tamanho com desempate pelo caminho. Teste primeiro: cada caso escrito e falhando antes de ajustes em `report.rs`/`text.rs`.
- [ ] T024 [US1] Somente leitura (FR-014): `crates/core/tests/scan_readonly.rs` registra, para cada Item da árvore synth, nome, tamanho e data de modificação antes da varredura e confere que nada mudou depois, nos dois alvos. Conferir também em `Cargo.lock` que nenhuma dependência do binário é de rede. Teste primeiro: o próprio teste.

**Checkpoint**: MVP utilizável no terminal, nos dois alvos.

---

## Phase 4: User Story 2 - Resultado em formato estável para máquinas (Priority: P1)

**Goal**: `--json` emite relatório + árvore completa, validado pelo esquema.

**Independent Test**: varrer a árvore synth com `--json`, validar contra
`docs/schemas/scan-report-v1.schema.json` e comparar valores com `expected.json`.

- [ ] T025 [US2] Implementar a serialização em `crates/core/src/json.rs` (R9): `serde` para o relatório e escrita em streaming de `items[]` (lista plana, `id` = posição, `parent` nulo na raiz, `kind` em `file|dir|link`, `flags` em `cloud|inaccessible|hardlink|name_lossy|not_scanned`, nomes convertidos por `NameBuf::to_text` com `name_lossy`) num `impl Write`. Teste primeiro: JSON exato (golden) de uma árvore de 4 Itens montada à mão, incluindo um nome com surrogate solto.
- [ ] T026 [US2] Ligar `--json` em `crates/cli/src/main.rs`: stdout recebe só o documento; progresso nunca escrito em modo JSON. Teste primeiro: `crates/cli/tests/contract_json.rs` com `jsonschema` (dev-dependency, `default-features = false`) valida a saída contra `docs/schemas/scan-report-v1.schema.json`; verifica que a soma dos arquivos em `items` reproduz `totals` (SC-007); `--json --progress` deixa stderr vazio.
- [ ] T027 [US2] Determinismo (US2, cenário 3): `crates/cli/tests/json_determinism.rs` roda duas varreduras da mesma árvore synth e compara os documentos ignorando só `duration_ms`. Teste primeiro: o teste em si; corrigir qualquer fonte de não determinismo encontrada.

**Checkpoint**: US1 e US2 completas; contrato JSON v1 congelado.

---

## Phase 5: User Story 3 - Varredura correta em casos reais do Windows (Priority: P2)

**Goal**: links não seguidos, hard links uma vez, inacessíveis contados, nuvem com alocado zero.

**Independent Test**: árvore synth `tests/fixtures/links.json` comparada com `expected.json`, nos
dois alvos; cenários manuais do quickstart.

- [ ] T028 [US3] Estender o synth em `crates/synth/src/lib.rs` com os tipos de link: `hardlink` (`std::fs::hard_link`), `symlink` (`std::os::{unix,windows}::fs`) e `junction` (`cmd /c mklink /J`, só Windows), com `expected.json` coerente (FR-008: tamanho no caminho de menor ordem lexicográfica; conteúdo de links não somado). Criar `tests/fixtures/links.json`. Teste primeiro: cada tipo é criado e removido na limpeza, nos dois alvos (tipos só do Windows ignorados no Linux com motivo).
- [ ] T029 [US3] Estender o synth em `crates/synth/src/lib.rs` com `denied_dir` (`chmod 000` no Linux; `icacls <dir> /deny <usuário>:(OI)(CI)(RX)` no Windows), `offline_file` (`attrib +O`, só Windows) e `compressed_file` (`compact /c`, só Windows), com limpeza que restaura permissões (`Drop`) e alocado esperado medido pelo oráculo. Acrescentar esses tipos a `tests/fixtures/links.json`. Teste primeiro: cada tipo é criado, o oráculo mede o comprimido com alocado < lógico, e a limpeza remove tudo.
- [ ] T030 [P] [US3] ⏱ No Linux, em `crates/core/src/scan/unix.rs`: symlinks viram `Kind::Link` sem seguir; pasta com `st_dev` diferente da raiz vira `Kind::Link` (ponto de montagem); `EACCES` ao abrir pasta → `inaccessible`. Teste primeiro (`#[cfg(unix)]`): varredura de `links.json` bate com `expected.json`.
- [ ] T031 [P] [US3] ⏱ No Windows, em `crates/core/src/scan/windows/mod.rs`, a política do ADR-0009 e de R4: função pura `classify(attributes, reparse_tag) -> {Descend, LinkNotFollowed, File{cloud}}`, tag lida de `EaSize`, percorrer só `IO_REPARSE_TAG_CLOUD*`, nuvem por `RECALL_ON_DATA_ACCESS | RECALL_ON_OPEN | OFFLINE` com alocado forçado a 0; `ERROR_ACCESS_DENIED` ao abrir pasta → `inaccessible`. Teste primeiro: tabela de `classify` (junction, symlink, mount point, cloud, tag desconhecida, arquivo comum, offline) e varredura de `links.json` (`#[cfg(windows)]`), incluindo o arquivo comprimido com alocado igual ao do oráculo.
- [ ] T032 [US3] ⏱ Coletar candidatos a hard link nos varredores e chamar `hardlinks.rs` após a montagem: Windows envia `FileId` de todo arquivo; Linux só `(st_dev, st_ino)` com `st_nlink > 1`. Teste primeiro: `crates/core/tests/scan_hardlinks.rs` com 3 links em pastas diferentes → contado uma vez, atribuído ao menor caminho, os demais com marca `hardlink`, nos dois alvos.
- [ ] T033 [US3] Pasta que some durante a varredura (edge case da spec): nos dois varredores, falha de abertura com "não encontrado" (`ENOENT` / `ERROR_FILE_NOT_FOUND` / `ERROR_PATH_NOT_FOUND`) vira Item inacessível, sem abortar. Teste primeiro: unitário com a função de mapeamento de erro → resultado, em `crates/core/src/scan/mod.rs`; integração que remove uma subpasta entre a geração e a abertura simulada via gancho de teste.
- [ ] T034 [US3] Caminhos longos e nomes inválidos: `crates/core/tests/scan_names.rs` com caminho > 260 caracteres (nos dois alvos) e, no Windows, arquivo com surrogate solto criado via `OsString::from_wide` → varrido, contado e com `name_lossy`. Teste primeiro: o próprio teste; corrigir o que falhar.
- [ ] T035 [US3] Testes manuais documentados: ponto de montagem (`#[ignore = "requer admin: montar volume em pasta"]` em `crates/core/tests/scan_mountpoint.rs`, com o passo a passo no comentário) e OneDrive real (quickstart §3). Executar o cenário OneDrive e registrar o resultado (confirma ou derruba o risco de R3) em `specs/001-directory-scan-cli/quickstart.md`. Teste primeiro: o teste `#[ignore]` compila e falha se executado sem o volume montado, com mensagem clara.

**Checkpoint**: números corretos em disco real (SC-004, SC-005).

---

## Phase 6: User Story 4 - Interromper uma varredura longa (Priority: P2)

**Goal**: Ctrl+C → relatório parcial consistente em ≤ 1 s, código 130.

**Independent Test**: cancelar a varredura de uma árvore synth grande e verificar `partial` e FR-011.

- [ ] T036 [US4] ⏱ Cancelamento no núcleo em `crates/core/src/scan/parallel.rs`, `crates/core/src/scan/unix.rs` e `crates/core/src/scan/windows/mod.rs` (R7): conferir o `CancelToken` antes de abrir cada pasta e entre lotes de 64 KiB; pastas não iniciadas entram com `not_scanned`; uma pasta é incluída inteira ou não é incluída; `ScanTree.partial = true`. Teste primeiro: `crates/core/tests/scan_cancel.rs` cancela quando `ScanCounters.items` passa de um limiar (thread observadora) e verifica `partial`, invariantes de FR-011 e presença de `not_scanned`.
- [ ] T037 [US4] Ctrl+C na CLI em `crates/cli/src/main.rs` com `ctrlc`: primeiro sinal liga o token e o relatório parcial sai no formato pedido com código 130; segundo sinal encerra com 130 sem relatório. Teste primeiro (`#[cfg(unix)]`, `crates/cli/tests/cancel.rs`): processo sobre `large.json` recebe `kill -INT <pid>`; relatório com `partial: true` chega em ≤ 1 s e o código é 130. No Windows: cenário manual registrado no quickstart.

**Checkpoint**: interrupção segura nos dois alvos.

---

## Phase 7: User Story 5 - Acompanhar o progresso (Priority: P3)

**Goal**: `--progress` escreve Itens e arquivos/s em stderr.

**Independent Test**: stdout redirecionado contém só o relatório; stderr recebeu atualizações.

- [ ] T038 [US5] Implementar `crates/cli/src/progress.rs`: thread que lê `ScanCounters` a cada 250 ms e reescreve uma linha em stderr ("Itens: 1.234.567 · 98.765 arquivos/s"), com linha final ao terminar; desligado sem `--progress` e sempre com `--json`. A decisão de quando escrever fica numa função pura `due(last: Instant, now: Instant) -> bool`. Teste primeiro: unitário — `due` é verdadeiro para intervalos ≥ 250 ms e falso abaixo, garantindo ≥ 1 atualização/s (US5); `crates/cli/tests/progress.rs` — com `--progress` sobre `large.json`, stderr tem ao menos uma atualização e stdout é idêntico ao da execução sem progresso; sem `--progress`, stderr vazio.

**Checkpoint**: todas as histórias funcionais.

---

## Phase 8: Polish & Cross-Cutting Concerns

- [ ] T039 Implementar `--stats` em `crates/cli/src/stats.rs` usando `sysinfo.rs` (T014): pico de memória, Itens, bytes por Item da arena (`size_of::<Node>()` + nomes ÷ Itens) em stderr. Teste primeiro: `crates/cli/tests/stats.rs` verifica as três linhas em stderr e stdout inalterado.
- [ ] T040 Benchmark final em `docs/benchmarks/2026-10-001-final.md` (SC-002, SC-003, SC-005): `hyperfine --warmup 1 --runs 5` do `.exe` release em `C:\` (texto) com mediana < 15 s; tempo e tamanho da saída `--json`; `--stats` com bytes por Item < 64 e pico de memória; varredura de `C:\` sem admin com código 0 e `inaccessible > 0` registrados; comparação com o benchmark-base de T019. Se alguma meta falhar, registrar e abrir tarefa nova em vez de otimizar sem medição.
- [ ] T041 [P] Atualizar `README.md` com instruções de build (`cargo build --release --target x86_64-pc-windows-gnu`) e uso de `disk-analyst scan`, e `docs/planning/01-roadmap.md` marcando Fases 0 e 1.
- [ ] T042 Executar o roteiro completo de [quickstart.md](quickstart.md) e `cargo xtask check`; registrar o resultado de cada seção no próprio quickstart.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Fase 1)**: sem dependências.
- **Foundational (Fase 2)**: depende da Fase 1; bloqueia todas as histórias.
- **US1 (Fase 3)**: depende da Fase 2. **MVP.**
- **US2 (Fase 4)**: depende de T018 (varredor) e T022 (CLI ligada).
- **US3 (Fase 5)**: depende de T018 e de T019 (comando-padrão de benchmark para as tarefas ⏱); T032 depende de T009.
- **US4 (Fase 6)**: depende de T018, T019 e T022.
- **US5 (Fase 7)**: depende de T022.
- **Polish (Fase 8)**: depende das histórias desejadas; T040 depende de T039 e de US3.

### Within Each Phase

- Fase 2: T004, T005, T006, T014 em paralelo; T007 → T008 → T009 → T010; T011 → T012 → T013.
- US1: (T015 ∥ T016) → T017 → T018 → **T019 (benchmark-base)** → (T020 ∥ T021) → T022 → T023 → T024.
- US3: T028 → T029 → (T030 ∥ T031) → T032 → T033 → T034 → T035.

### Parallel Opportunities

```text
Fase 2:  T004 names.rs | T005 tree.rs | T006 control.rs | T014 sysinfo.rs  (T011 synth em paralelo a T007–T010)
US1:     T015 unix.rs  | T016 windows/buffer.rs ; depois T020 args.rs | T021 text.rs
US3:     T030 unix.rs  | T031 windows/mod.rs
Após US1: US2, US3, US4 e US5 podem andar em paralelo (arquivos majoritariamente distintos; main.rs
         é ponto de conflito: serializar T026, T037 e T038; tarefas ⏱ não rodam em paralelo entre si,
         para não contaminar as medições).
```

## Implementation Strategy

### MVP first

1. Fases 1 e 2.
2. Fase 3 (US1), incluindo o benchmark-base T019 logo após o primeiro varredor (T018).
3. **Parar e validar**: árvore synth com diferença zero nos dois alvos; primeiro tempo em `C:\`.

### Incremental delivery

US2 (contrato JSON) → US3 (correção em disco real) → US4 (cancelamento) → US5 (progresso) →
Polish (benchmark final contra as metas). Cada fase termina com `cargo xtask check` verde e pausa
para revisão do mantenedor.

## Notes

- Toda tarefa: teste primeiro, vermelho, depois verde; `cargo xtask check` antes de marcar.
- Tarefas ⏱: benchmark antes e depois com o comando-padrão de T019 (Princípio I).
- `unsafe` só em `crates/core/src/scan/windows/ffi.rs` e `crates/synth/src/ffi_windows.rs`.
- Nenhuma otimização além do desenho do plano sem benchmark antes/depois.
