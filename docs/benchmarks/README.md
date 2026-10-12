# Benchmarks

Registros de desempenho do Disk Analyst. Regra da constituição (Princípio I): toda mudança em caminho
crítico (varredura, leitura da MFT, agregação de tamanhos) vem com benchmark **antes e depois**, e
afirmação de velocidade sem comando e resultado é inválida.

## Regra para tarefas ⏱

- Tarefa marcada **⏱** em `tasks.md` registra **dois** resultados: um antes de começar e outro depois
  de terminar, ambos no formato abaixo e medidos com o mesmo comando.
- **Regressão sem justificativa registrada bloqueia a tarefa.** A justificativa vai no campo
  "Observações" do registro "depois".
- Otimização sem medição não entra.
- **O que se compara (decisão de 2026-10-12, após a T019):** o tempo de parede do comando-padrão
  inclui ≈ 213 ms fixos do interop do WSL e, na árvore grande, só ≈ 65 ms são varredura. Por isso
  toda tarefa ⏱ registra **os dois**: (1) o comando-padrão (parede, `hyperfine`) e (2) o **tempo
  interno** — mediana de 5 execuções diretas do `scan_totals.exe`, linha `tempo_ms`, depois de 1 de
  aquecimento. **A decisão de regressão ou ganho usa o tempo interno**; a parede é registrada para
  detectar efeitos fora da varredura (início do processo, saída). Mudanças que afetem o tempo de
  iniciar ou encerrar o processo (ex.: escrita da saída) são julgadas também pela parede.

## Comando-padrão

Definido na T019. É o comando que toda tarefa ⏱ roda **antes e depois** (a medição é feita pelo
`.exe` do Windows, chamado do WSL, sobre a árvore sintética grande):

```bash
hyperfine -N --warmup 1 --runs 5 \
  'target/x86_64-pc-windows-gnu/release/examples/scan_totals.exe D:/Archive/Projects/disk-analyst-test/tree'
```

- `-N` (sem shell) tira o ruído de um `sh -c` do tempo medido. O caminho do Windows usa `/` porque,
  sem shell, o `hyperfine` interpretaria `\` como escape.
- Cache quente: `--warmup 1` faz uma execução de aquecimento antes das 5 medidas.
- Registre mediana, mínimo e máximo (campo "Resultado" do formato abaixo). O `hyperfine` mede o
  tempo de parede, que **inclui o custo fixo de iniciar um `.exe` do Windows pelo interop do WSL**
  (medido com o exemplo `noop`; ver `2026-10-baseline.md`). O tempo interno da varredura, sem esse
  custo, é a linha `tempo_ms` que o próprio `scan_totals` imprime; rode-o uma vez a mais e anote.

### Pré-requisitos

1. Build de release dos exemplos, do repositório:

   ```bash
   cargo build --release --target x86_64-pc-windows-gnu -p disk-analyst-core \
     --example scan_totals --example noop
   ```

2. A árvore grande, gerada **uma vez** com o fixture `tests/fixtures/large.json` (≈ 200 mil
   arquivos, ≈ 0,8 GB, NTFS com cluster de 4 KiB) em `D:\Archive\Projects\disk-analyst-test`, por
   autorização do usuário em 2026-10-12. A árvore fica em `...\tree`, e o `expected.json` ao lado
   dela. **Não apague** a pasta: ela é a base de todas as tarefas ⏱ até o usuário decidir ao fim
   da feature.

   ```bash
   cargo build --release --target x86_64-pc-windows-gnu -p xtask
   target/x86_64-pc-windows-gnu/release/xtask.exe synth \
     "$(wslpath -w tests/fixtures/large.json)" 'D:\Archive\Projects\disk-analyst-test'
   ```

   O gerador recusa destino não vazio, então não sobrescreve a árvore existente. A geração é
   lenta (≈ 44 arquivos/s no NTFS com antivírus em tempo real: 75 min na primeira vez): é um custo de
   uma vez só.

### Variações usadas na linha de base

- Uma thread: `RAYON_NUM_THREADS=1 WSLENV=RAYON_NUM_THREADS hyperfine -N ...`. A variável do WSL só
  chega ao `.exe` do Windows se estiver em `WSLENV`.
- Custo fixo do interop: o mesmo `hyperfine -N --warmup 1 --runs 5` sobre
  `target/x86_64-pc-windows-gnu/release/examples/noop.exe`.
- Núcleo em memória: `cargo bench -p disk-analyst-core --bench baseline` (criterion, Linux).

## Formato obrigatório de cada registro

Todo registro tem estes seis campos:

| Campo | O que registrar |
|---|---|
| Comando exato | A linha de comando completa, copiável, incluindo variáveis de ambiente (ex.: `RAYON_NUM_THREADS`) |
| Commit | Hash do commit medido (`git rev-parse --short HEAD`), com a árvore de trabalho limpa |
| Hardware | O hardware de referência abaixo; qualquer diferença deve ser dita |
| Data | Data da medição (AAAA-MM-DD) |
| Mediana, mínimo e máximo | Os três valores, com unidade (s ou ms) e número de execuções |
| Observações | Estado do cache (quente/frio), ruído conhecido, justificativa de regressão (Princípio I) |

O padrão para a CLI é `hyperfine --warmup 1 --runs 5`; no núcleo, `criterion`.

### Hardware de referência

Resumo da seção "Hardware de referência para benchmarks" de
[`docs/planning/00-visao.md`](../planning/00-visao.md), que é a fonte:

- CPU: 8 threads lógicas
- RAM: 31 GiB
- `C:`: NTFS, 235 GB (sistema)
- `D:`: NTFS, 1 TB
- Ambiente de build: WSL2 (Ubuntu 26.04), Rust 1.99, alvo `x86_64-pc-windows-gnu`

### Modelo de registro

Copie, preencha e salve como `docs/benchmarks/AAAA-MM-<assunto>.md`.

```markdown
# <Assunto> — <antes | depois>

- **Comando exato**: `<comando completo>`
- **Commit**: `<hash>`
- **Hardware**: referência de `docs/planning/00-visao.md` (8 threads, 31 GiB, WSL2, alvo
  `x86_64-pc-windows-gnu`); diferenças: <nenhuma | quais>
- **Data**: AAAA-MM-DD
- **Resultado** (<N> execuções): mediana <valor> <unidade> · mínimo <valor> <unidade> · máximo <valor> <unidade>
- **Observações**: <cache quente/frio; ruído; justificativa de regressão, se houver>
```

## Registros

- [`2026-10-baseline.md`](2026-10-baseline.md): linha de base da T019 (varredura de diretórios da Fase 3,
  árvore large no `D:`, `C:\` inteiro, threads 1 × 8, interop e criterion).
