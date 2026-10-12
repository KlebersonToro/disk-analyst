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

## Comando-padrão

A definir em T019.

Enquanto isso, não há comando-padrão: nenhum registro deste diretório vale como linha de base.

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
