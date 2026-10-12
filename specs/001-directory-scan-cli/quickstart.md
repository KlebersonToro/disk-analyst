# Quickstart: validar a feature 001

Roteiro de validação ponta a ponta. Comandos rodam no WSL, na raiz do repositório, salvo indicação.
Contratos: [contracts/cli.md](contracts/cli.md), [scan-report-v1.schema.json](contracts/scan-report-v1.schema.json).
Modelo: [data-model.md](data-model.md).

## Pré-requisitos (já presentes, nada a instalar)

- Rust 1.99 com alvos `x86_64-unknown-linux-gnu` e `x86_64-pc-windows-gnu`; `x86_64-w64-mingw32-gcc`.
- `hyperfine` no WSL.
- Windows com Modo de Desenvolvedor ativo (para symlinks sem admin nos testes).

## 1. Portões de qualidade (US6, FR-017)

```bash
cargo xtask check
```

Esperado: `fmt`, `clippy -D warnings` e testes passam no Linux e no Windows; qualquer falha
interrompe com código ≠ 0.

## 2. Totais exatos numa árvore sintética (US1, SC-001)

```bash
cargo xtask synth tests/fixtures/basic.json /tmp/da-basic
cargo run -p disk-analyst -- scan /tmp/da-basic --json > /tmp/da-basic.out.json
```

Esperado: os testes de integração comparam `/tmp/da-basic.out.json` com
`/tmp/da-basic/expected.json` e obtêm diferença zero em totais, contagens e rankings.
No Windows, o mesmo com a árvore criada em `C:\Users\Public\da-basic` e o `.exe`.

## 3. Links, hard links, pasta negada, nuvem (US3, SC-004, SC-005)

```bash
cargo xtask synth tests/fixtures/links.json <destino>   # no Windows: destino em C:\...
```

Esperado: conteúdo de junction e symlink não somado; hard link contado uma vez e atribuído ao
caminho de menor ordem lexicográfica; pasta negada contada em `inaccessible`; arquivo `OFFLINE`
marcado `cloud` com alocado zero.

**Manual (OneDrive real)**: no PowerShell, `disk-analyst.exe scan "$env:USERPROFILE\OneDrive" --top 5`.
Esperado: a pasta é percorrida (não tratada como link), arquivos só online aparecem como "na
nuvem" com alocado zero. Valida o risco de R3.

**Manual (ponto de montagem, requer admin)**: teste `#[ignore]` documentado no próprio arquivo de
teste; esperado: o volume montado aparece como link não seguido.

## 4. JSON estável (US2, SC-007)

Esperado nos testes de contrato: toda saída valida contra o esquema; duas varreduras da mesma
árvore produzem JSON idêntico exceto `duration_ms`; com `--json --progress`, stderr não recebe
progresso.

## 5. Cancelamento (US4, SC-006)

Teste de integração: varrer uma árvore sintética grande, enviar Ctrl+C (SIGINT no Linux;
`GenerateConsoleCtrlEvent` não é usado: no Windows o teste é manual no terminal).
Esperado: relatório marcado `partial: true` em até 1 s, código 130, e todo total de pasta igual à
soma dos filhos incluídos.

## 6. Desempenho em `C:\` (SC-002, SC-003)

```bash
cargo build --release --target x86_64-pc-windows-gnu
cp target/x86_64-pc-windows-gnu/release/disk-analyst.exe /mnt/c/Users/Public/
hyperfine --warmup 1 --runs 5 "/mnt/c/Users/Public/disk-analyst.exe scan 'C:\\' "
/mnt/c/Users/Public/disk-analyst.exe scan 'C:\' --stats > /dev/null
```

Esperado: mediana < 15 s; `--stats` mostra bytes por Item < 64 (meta de design: ≤ 40) e o pico
de memória. Resultados, hardware e custo fixo do interop registrados em `docs/benchmarks/`.
