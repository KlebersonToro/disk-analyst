# Research: Varredura de diretórios e relatório na CLI

Fase 0 do plano. Cada item: decisão, justificativa, alternativas. Fatos marcados **[verificado]**
foram conferidos neste ambiente em 2026-10-12.

## R1. Execução dos testes Windows a partir do WSL

- **Decisão**: `cargo test --target x86_64-pc-windows-gnu` executa os `.exe` de teste direto pelo
  interop do WSL, sem runner configurado.
- **Justificativa**: **[verificado]** um teste com `assert_eq!(std::env::consts::OS, "windows")`
  passou assim. O `.exe` GNU só importa DLLs do sistema (ADR-0001).
- **Alternativas**: runner explícito em `.cargo/config.toml` (desnecessário); `cargo-xwin`
  (ADR-0001, adiado).

## R2. API de enumeração no Windows

- **Decisão**: abrir cada pasta com `CreateFileW(FILE_LIST_DIRECTORY, FILE_FLAG_BACKUP_SEMANTICS |
  FILE_FLAG_OPEN_REPARSE_POINT)` e ler lotes com `GetFileInformationByHandleEx`
  (`FileIdBothDirectoryRestartInfo` na primeira chamada, `FileIdBothDirectoryInfo` nas seguintes),
  buffer de 64 KiB. Caminhos com prefixo `\\?\` (suporta > 260 caracteres). A raiz é aberta sem
  `FILE_FLAG_OPEN_REPARSE_POINT` (segue o link que o usuário pediu); as demais pastas, com a flag.
  Os registros do buffer são lidos por uma função pura e safe sobre `&[u8]`, que valida offsets e
  comprimentos (é parser binário, Princípio III); o `unsafe` fica restrito às chamadas de sistema.
- **Justificativa**: ADR-0007. Cada registro `FILE_ID_BOTH_DIR_INFO` traz `EndOfFile` (Tamanho
  lógico), `AllocationSize` (Tamanho alocado), `FileAttributes`, `FileId` e o nome em UTF-16, sem
  abrir os arquivos. Arquivos nunca são abertos, logo nunca ficam inacessíveis nesta feature: só
  pastas podem ser Itens inacessíveis.
- **Alternativas**: `FindFirstFileExW` (sem tamanho alocado); `NtQueryDirectoryFile` + abertura
  relativa ao handle do pai via `NtCreateFile` (evita montar caminhos, mas é API do ntdll menos
  documentada). Fica como otimização candidata, só com benchmark (Princípio I).

## R3. Links, pontos de montagem e pastas da nuvem

- **Decisão**: um Item com `FILE_ATTRIBUTE_REPARSE_POINT` não é percorrido, **exceto** quando a tag
  de reparse é da família de arquivos na nuvem (`IO_REPARSE_TAG_CLOUD*`). Para registros de diretório
  com esse atributo, o campo `EaSize` contém a tag de reparse (MS-FSCC). Junctions, pontos de
  montagem de volume (`IO_REPARSE_TAG_MOUNT_POINT`), symlinks (`IO_REPARSE_TAG_SYMLINK`) e demais
  tags viram Itens do tipo "link não seguido", com tamanho zero.
- **Justificativa**: pastas do OneDrive sob demanda são reparse points; ignorar todo reparse point
  esconderia o OneDrive inteiro. A raiz pedida pelo usuário é aberta seguindo o link (edge case da
  spec).
- **Risco**: a leitura da tag via `EaSize` precisa ser confirmada numa pasta OneDrive real
  (cenário manual no quickstart). Alternativa se falhar: `FSCTL_GET_REPARSE_POINT` só para os
  diretórios com o atributo, que são poucos.
- **Fato [verificado]**: o Modo de Desenvolvedor está ativo; `mklink /J` e `mklink /D` funcionam sem
  administrador. Existe `C:\Users\klebi\OneDrive` para o teste manual.

## R4. Arquivo na nuvem

- **Decisão**: um arquivo é Arquivo na nuvem se tiver `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`,
  `FILE_ATTRIBUTE_RECALL_ON_OPEN` ou `FILE_ATTRIBUTE_OFFLINE`. O Tamanho alocado dele é forçado a
  zero; o lógico vem de `EndOfFile`.
- **Justificativa**: atributos documentados para placeholders do Cloud Files API. `OFFLINE` pode ser
  aplicado por qualquer usuário, o que permite teste automatizado; os outros dois só existem em
  provedores reais (teste manual).
- **Alternativas**: consultar o estado de hidratação com a Cloud Files API (mais caro, sem ganho
  para esta feature).

## R5. Hard links contados uma vez

- **Decisão**: deduplicação por ordenação após a varredura. Cada thread acumula pares
  `(FileId: u64, NodeId: u32)` de arquivos; no fim, os vetores são concatenados, ordenados por
  `FileId`, e cada grupo com mais de um elemento é resolvido: o caminho de menor ordem
  lexicográfica (comparação de UTF-16 código a código) recebe o tamanho; os demais recebem zero e a
  marca `hardlink`. No Linux, só entram arquivos com `st_nlink > 1`, usando `(st_dev, st_ino)`.
- **Justificativa**: `FILE_ID_BOTH_DIR_INFO` traz o `FileId`, mas não o número de links; descobrir
  `NumberOfLinks` exigiria abrir cada arquivo. O vetor temporário custa 16 bytes por arquivo e é
  liberado antes do relatório; ordenar 2 milhões de pares leva décimos de segundo (a medir no
  benchmark-base). Caminhos completos só são montados para os poucos grupos duplicados.
- **Alternativas**: `HashSet<FileId>` concorrente (memória permanente e contenção); abrir arquivos
  para ler `NumberOfLinks` (lento demais).

## R6. Estrutura do varredor paralelo

- **Decisão**: `rayon::scope` com uma tarefa por pasta (ADR-0005). Cada tarefa produz um
  `DirChunk` local (registros brutos e nomes UTF-16 da pasta) e o guarda num `Mutex<Vec<…>>`
  indexado por um id temporário atribuído por `AtomicU32`. Após o fim da varredura, uma etapa
  sequencial monta a arena em ordem determinística (filhos de cada pasta ordenados por nome,
  ordinal UTF-16), agrega tamanhos de baixo para cima e libera os chunks.
- **Justificativa**: a montagem sequencial dá ids e saída determinísticos (US2, cenário 3) e
  mantém a arena compacta (filhos contíguos). Um lock por pasta, não por arquivo, tem contenção
  desprezível.
- **Alternativas**: inserir direto numa arena compartilhada (ordem não determinística, mais
  sincronização); canal MPMC (dependência extra sem ganho).

## R7. Cancelamento (Ctrl+C)

- **Decisão**: crate `ctrlc` na CLI. O primeiro sinal liga um `AtomicBool` compartilhado com o
  núcleo; o segundo encerra o processo com código 130. Cada tarefa confere a flag antes de abrir
  uma pasta e entre lotes de 64 KiB; pastas não iniciadas viram Itens com a marca `not_scanned`.
  Uma pasta é incluída inteira ou não é incluída, então FR-011 vale no relatório parcial.
- **Justificativa**: `ctrlc` cobre `SetConsoleCtrlHandler` e sinais Unix sem `unsafe` nosso.
  Conferir a flag entre lotes limita o atraso a uma chamada de sistema (SC-006, ≤ 1 s).
- **Alternativas**: `SetConsoleCtrlHandler` direto via `windows` + `libc` no Linux (mais `unsafe` e
  código duplicado por plataforma).

## R8. Argumentos da CLI

- **Decisão**: `lexopt`.
- **Justificativa**: sem dependências transitivas, e permite mensagens de ajuda e erro inteiramente
  em português (FR-019); o `clap` gera textos em inglês e é bem maior (Princípio XI, meta de
  binário < 8 MB).
- **Alternativas**: `clap` (mensagens em inglês, binário maior); parser manual (reinventa o
  `lexopt`).

## R9. JSON, esquema e testes de contrato

- **Decisão**: `serde` + `serde_json`. O relatório é serializado por derive; a árvore de Itens
  (milhões) é escrita em streaming, como lista plana com `id` e `parent`, num `BufWriter`. Esquema
  JSON Schema 2020-12 publicado em `docs/schemas/scan-report-v1.schema.json` (cópia do contrato
  deste plano). Testes de contrato validam a saída com a crate `jsonschema` como
  `dev-dependency`, sem features padrão.
- **Justificativa**: Princípio X. Lista plana é mais barata de gerar e de consumir que aninhamento
  profundo, e evita estouro de pilha em leitores recursivos.
- **Alternativas**: árvore aninhada (recursão profunda, maior); validação só por golden files (não
  cumpre SC-007 literalmente).

## R10. Oráculo para árvores sintéticas

- **Decisão**: o gerador (crate de biblioteca `crates/synth`, também exposto como `cargo xtask synth`)
  cria a árvore a partir de uma descrição JSON e grava um
  arquivo de totais esperados. O Tamanho lógico esperado vem da descrição. O Tamanho alocado esperado
  é medido arquivo a arquivo **depois** de criado, por um método independente e lento
  (`st_blocks * 512` no Linux; `GetFileInformationByHandleEx(FileStandardInfo)` por arquivo no
  Windows). Por ser biblioteca, roda dentro dos testes `.exe` no Windows, onde a medição é nativa (o
  `/mnt/c` visto pelo WSL não informa a alocação real com confiança). Não usa código do núcleo.
- **Justificativa**: o alocado depende do sistema de arquivos (cluster, arquivos pequenos
  residentes na MFT ocupam zero, compressão), então não pode ser previsto pela descrição; e um
  oráculo que reutiliza o código testado seria tautológico. Assim SC-001 (diferença zero) é
  verificável.
- **Links na árvore sintética**: hard links via `std::fs::hard_link`; symlinks via
  `std::os::{unix,windows}::fs`; junctions via `cmd /c mklink /J` (só no Windows, sem `unsafe`);
  pasta inacessível via `chmod 000` no Linux e `icacls /deny` para o usuário atual no Windows;
  `OFFLINE` via `attrib +O` no Windows. Pontos de montagem exigem admin: teste manual `#[ignore]`.

## R11. Medição (Princípio I, FR-018)

- **Decisão**:
  - Tempo da CLI: `hyperfine --warmup 1 --runs 5`, executado no WSL chamando o `.exe` pelo interop
    (o `hyperfine` já está instalado; nada a instalar no Windows). O custo fixo do interop é medido
    à parte com um `.exe` vazio e registrado junto.
  - Memória: a CLI ganha `--stats`, que escreve em stderr o pico de memória do processo
    (`GetProcessMemoryInfo` no Windows, `VmHWM` de `/proc/self/status` no Linux), o número de Itens
    e bytes por Item da arena.
  - Núcleo: `criterion` para montagem da arena, agregação e deduplicação de hard links sobre
    árvores sintéticas em memória; e uma asserção em tempo de compilação de
    `size_of::<Node>() <= 40`.
  - Referências externas no benchmark-base: varredura com 1 thread e o `dua`, se instalado
    (ADR-0005); se não estiver, o benchmark registra a ausência, sem instalar.
- **Justificativa**: só ferramentas já presentes, como pedido.

## R12. Verificação de qualidade única (FR-017)

- **Decisão**: `cargo xtask check` (alias em `.cargo/config.toml`), que roda em sequência e para
  na primeira falha: `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings` nos dois alvos;
  `cargo test` no Linux e com `--target x86_64-pc-windows-gnu`.
- **Justificativa**: comando único, multiplataforma, escrito em Rust, sem `make` ou scripts de
  shell.

## R13. Dependências (Princípio XI)

| Crate | Versão | Onde | Justificativa |
|---|---|---|---|
| `windows` | 0.62 | core e synth (só `cfg(windows)`) | Win32 oficial; preferida pela constituição. No synth, só para o oráculo de alocado (R10) |
| `rayon` | 1.12 | core | Paralelismo (ADR-0005) |
| `serde`, `serde_json` | 1.0 | core, synth, xtask | Contrato JSON (Princípio X); no synth, leitura da descrição e escrita do `expected.json` (R10) |
| `lexopt` | 0.3 | cli | Argumentos com mensagens em português (R8) |
| `ctrlc` | 3.5 | cli | Ctrl+C sem `unsafe` próprio (R7) |
| `proptest` | 1.12 | dev | Testes por propriedade (Princípio III) |
| `criterion` | 0.8 | dev | Microbenchmarks (Princípio I) |
| `jsonschema` | 0.58 | dev, sem features padrão | Validação de contrato (SC-007) |

Versões **[verificadas]** no crates.io em 2026-10-12.
