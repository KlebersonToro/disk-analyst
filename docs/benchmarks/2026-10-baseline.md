# Linha de base de desempenho (T019)

Primeira medição do projeto: a referência contra a qual toda tarefa ⏱ compara o antes e o depois.
Formato de [`README.md`](README.md). Nada aqui é otimização: é o código como estava no fim da Fase 3.

## Condições comuns

| Item | Valor |
|---|---|
| Data | 2026-10-12 |
| Commit medido | `0df3fdb` (os `.exe` foram compilados nele). O núcleo (`crates/core`) é **idêntico** ao do commit `e942f09`, HEAD no momento da medição: `git diff --stat 0df3fdb..HEAD -- crates/core` é vazio; mudaram só a CLI e a documentação. |
| Hardware | O de [`docs/planning/00-visao.md`](../planning/00-visao.md): 8 threads lógicas, 31 GiB de RAM, `C:` NTFS 235 GB (sistema), `D:` NTFS 1 TB, WSL2 (Ubuntu 26.04), Rust 1.99. Observado nesta máquina: Intel Core i3-1215U (12ª geração, 6 núcleos, 8 threads). Nenhuma diferença em relação ao hardware de referência. |
| Compilação | `cargo build --release --target x86_64-pc-windows-gnu -p disk-analyst-core --example scan_totals --example noop` |
| Execução | Os `.exe` do Windows, chamados do WSL (interop). Medição de varredura sempre pelo `.exe`; o criterion (item e) roda no Linux. |
| Estado da máquina | **Ociosa** durante as medições a, c e g: carga média de 0,01 a 0,19, nenhum `cargo`/`rustc` e a geração da árvore já terminada. A geração da árvore (75 min) e o build dos programas auxiliares foram feitos **antes**, nunca durante uma medição. |
| Cache | **Quente**: `hyperfine --warmup 1` faz uma execução de aquecimento antes das 5 medidas. |

### Árvore sintética grande (a, c, g)

Gerada **uma vez** com `tests/fixtures/large.json` em `D:\Archive\Projects\disk-analyst-test` (autorização
direta do usuário em 2026-10-12; a pasta fica para as tarefas ⏱ seguintes):

```bash
target/x86_64-pc-windows-gnu/release/xtask.exe synth \
  "$(wslpath -w tests/fixtures/large.json)" 'D:\Archive\Projects\disk-analyst-test'
```

| | Valor |
|---|---|
| Arquivos / pastas (com a raiz `tree\`) | 199.980 / 1.112 |
| Itens na árvore do núcleo | 201.092 (arquivos + 1.111 pastas + a raiz) |
| Tamanho lógico | 820.571.035 B (≈ 782,6 MiB) |
| Tamanho alocado | 1.163.989.984 B (≈ 1,08 GiB), NTFS com cluster de 4 KiB |
| Tempo para gerar | **75 min 7 s** (≈ 44 arquivos/s). É o custo de criar 200 mil arquivos no NTFS com antivírus em tempo real, não do gerador; custo único. |

A varredura mede `D:\Archive\Projects\disk-analyst-test\tree`. Já o item (b) mede o `C:\` **no `C:`**:
a árvore large está no `D:`, o `C:` é um volume diferente.

## Resultados, lado a lado

Tempos de parede são do `hyperfine`; "interno" é a linha `tempo_ms` que o próprio `scan_totals` imprime
(só a varredura, sem iniciar o processo). Todos com 5 execuções.

| Medição | Mediana | Mínimo | Máximo |
|---|---|---|---|
| **d)** custo fixo do interop (`noop.exe`, parede) | 213,0 ms | 206,2 ms | 239,5 ms |
| **a)** large, 8 threads, **parede** (comando-padrão) | **332 ms** | 317 ms | 352 ms |
| **a)** large, 8 threads, **interno** | **65,4 ms** | 61,6 ms | 81,8 ms |
| **c)** large, 1 thread, parede | 371 ms | 250 ms | 401 ms |
| **c)** large, 1 thread, interno | 125,5 ms | 118,1 ms | 170,9 ms |
| **b)** `C:\` inteiro, 8 threads, parede, cache quente | **13,057 s** | 10,347 s | 14,803 s |
| **b)** `C:\` inteiro, 1ª execução, **cache frio** (uma só) | 43,0 s | n/a | n/a |
| **e)** criterion A: montagem + agregação de 1.000.001 Nodes (Linux) | 41,103 ms | 38,648 ms | 44,722 ms |
| **e)** criterion B: dedup de 1 milhão de pares (Linux) | 59,467 ms | 58,051 ms | 60,773 ms |

No criterion, "mediana" é a estimativa central que ele imprime, entre os limites inferior e superior do intervalo
de confiança (`[mín  centro  máx]`); `sample_size` 10.

## Comandos exatos

**a) comando-padrão** (parede, 8 threads):

```bash
hyperfine -N --warmup 1 --runs 5 \
  'target/x86_64-pc-windows-gnu/release/examples/scan_totals.exe D:/Archive/Projects/disk-analyst-test/tree'
```

Tempos de cada execução: 340, 332, 317, 352, 327 ms. Saída de uma execução direta do mesmo `.exe`
(uma das 5 do "interno"): `itens: 201092`, `tempo_ms: 65.4`, `pico_bytes: 41111552`, `bytes_por_item: 204`.
Interno das 5 execuções: 81,8 / 65,4 / 61,6 / 62,1 / 67,9 ms; pico entre 41.111.552 e 42.475.520 B.

**c) uma thread** (a variável do WSL só chega ao `.exe` com `WSLENV`):

```bash
RAYON_NUM_THREADS=1 WSLENV=RAYON_NUM_THREADS hyperfine -N --warmup 1 --runs 5 \
  'target/x86_64-pc-windows-gnu/release/examples/scan_totals.exe D:/Archive/Projects/disk-analyst-test/tree'
```

Tempos de parede: 371, 377, 361, 250, 401 ms. Interno das 5 execuções diretas: 118,1 / 125,5 / 126,5 /
170,9 / 122,5 ms; pico entre 38.547.456 e 39.362.560 B.

**d) interop:**

```bash
hyperfine -N --warmup 1 --runs 5 'target/x86_64-pc-windows-gnu/release/examples/noop.exe'
```

**b) `C:\`:**

```bash
hyperfine -N --warmup 1 --runs 5 'target/x86_64-pc-windows-gnu/release/examples/scan_totals.exe C:/'
```

Tempos: 10,347 / 12,489 / 13,057 / 14,563 / 14,803 s. Uma execução direta quente deu `itens: 1311972`,
`tempo_ms: 10176.2`, `pico_bytes: 505827328`, `bytes_por_item: 385`. A 1ª execução da sessão (cache frio) deu
`itens: 1311920`, `tempo_ms: 43023.0`, `pico_bytes: 504438784`, `bytes_por_item: 384`.

**e) criterion** (Linux, WSL2; `sample_size` 10):

```bash
cargo bench -p disk-analyst-core --bench baseline
```

A: `montagem/assemble_aggregate_1M_nodes`, B: `hardlinks/dedup_1M_pairs` (500 mil `file_id`, cada um em
2 caminhos; a dedup inclui o `aggregate`, como documentado na T009).

**f) `dua`:** **não medido: `dua` ausente.** A tarefa diz "só se já instalado"; não foi instalado (o config exige
avisar antes de instalar ferramenta). `hyperfine` 1.19.0 já estava instalado.

## g) Memória: bytes por item

Duas medidas **diferentes**. A oficial da SC-003 é a segunda e será medida pelo `--stats` na T039.

| | Árvore large (D:) | `C:\` |
|---|---|---|
| Itens | 201.092 | 1.313.528 |
| **Pico do processo ÷ itens** (`bytes_por_item` do `scan_totals`) | 204 a 211 (mediana 207) | 384 a 385 |
| Pico do processo | ≈ 41,6 MB (mediana) | ≈ 505 MB |
| **Arena** = `size_of::<Node>()` (40 B) × itens + nomes UTF-16 | 48,0 B/item (40 de Node + 8,0 de nomes) | 101,9 B/item (40 + 61,9) |
| **Arena sem os nomes** (SC-003) | **40 B/item** | **40 B/item** |

- **SC-003** ("memória por Item indexado, sem contar o nome, abaixo de 64 bytes"): a arena tem **40 B por item**,
  dentro da meta com folga (a conta é o tamanho do `Node`, garantido em compilação por
  `assert!(size_of::<Node>() <= 40)`). O pico do `C:\` (≈ 505 MB) é o que a SC-003 manda registrar.
- O "pico ÷ itens" (≈ 207 e 384 B) **não** é a métrica da SC-003: inclui o piso do processo, os chunks
  temporários da varredura (copiados na montagem), a folga de capacidade dos `Vec` e os nomes. O piso do processo
  é ≈ 6,0 MB: varrer uma subárvore de 181 itens da própria árvore large deu pico de 5.943.296 a 6.094.848 B (3
  execuções, `scan_totals` de release). Descontado esse piso, a árvore large fica em ≈ 177 B/item
  ((41,6 − 6,0) MB ÷ 201.092) e o `C:\` em ≈ 380 B/item.
- A conta da arena é `itens × 40 + Σ(name_len) × 2`, feita por um programa temporário fora do repositório;
  ela não inclui a folga de capacidade do `Vec<Node>`. A medição oficial é a da T039.

## SC-002 (preliminar)

A meta "varredura completa de `C:\` em menos de 15 s com cache quente (mediana de 5 execuções após 1 de
aquecimento)" fica **atendida nesta linha de base**: mediana de parede de 13,057 s (uma execução direta quente deu
10,2 s de tempo interno de varredura, sem o interop). Mas o máximo (14,803 s) já está perto do limite, e a SC-002 é validada na T040.

## Observações

1. **Interop.** Iniciar um `.exe` do Windows pelo WSL custa ≈ 213 ms (item d). Todo tempo de parede do `hyperfine`
   inclui isso. Na árvore large, o "tempo de parede" de 332 ms são ≈ 213 ms de interop + ≈ 65 ms de varredura +
   o restante de inicialização/encerramento do processo. **Para comparar antes × depois de uma otimização, use o
   `tempo_ms` interno junto com a parede**: a parede esconde o ganho.
2. **Threads: 1 × 8.** Conferi que o `RAYON_NUM_THREADS` chega ao `.exe`: `WSLENV=RAYON_NUM_THREADS` faz o rayon
   ver 1 thread (sem o `WSLENV` ele vê 8, mesmo com a variável exportada). Pelo tempo de parede a diferença é
   pequena (371 × 332 ms, medianas), **porque** o interop de ≈ 213 ms e o ruído do `hyperfine` (±15 a ±60 ms)
   dominam. Pelo tempo interno ela é clara: **125,5 ms com 1 thread contra 65,4 ms com 8, ≈ 1,9 vez mais rápido.**
   O ganho de ≈ 1,9× com 8 threads (num CPU de 6 núcleos/8 threads, com núcleos de desempenho e de eficiência)
   é uma medida; por que ele não chega perto de 8× não foi investigado.
3. **Cache.** Quente em todas as medidas, exceto a linha explícita de cache frio (43,0 s no `C:\`). A diferença
   entre a 1ª execução (43,0 s) e a mediana quente (13,1 s) mostra que a primeira varredura de um volume depende
   do cache do sistema operacional.
4. **`C:\` está em uso.** O número de itens muda entre execuções (1.311.920, 1.311.972, 1.313.528) porque o
   volume do sistema muda enquanto se mede. Rodou sem administrador; o `scan_totals` não imprime quantas pastas
   ficaram inacessíveis (isso entra no relatório de texto da CLI e na T040).
5. **Ruído.** O desvio do `hyperfine` foi pequeno na árvore large com 8 threads (σ ≈ 13 ms em 333 ms) e grande com
   1 thread (σ ≈ 59 ms, mínimo 250 ms contra máximo 401 ms) e no `C:\` (σ ≈ 1,8 s). Cinco execuções dão uma
   mediana útil, mas não eliminam isso.
6. **Onde cada coisa foi medida.** A árvore large está no `D:`; o `C:\` foi medido no `C:`; o criterion rodou no
   Linux (WSL2), não no Windows: ele mede a montagem e a agregação em memória, que não dependem do sistema de
   arquivos.
7. **Candidatos a otimização já registrados** (nada foi otimizado aqui): G1 (`buf.fill(0)` por lote), G4 (contadores
   incrementados por entrada), G5 (geração lenta da árvore), G6 (`bytes_por_item`). Cada um só entra com antes e
   depois pelo comando-padrão.
