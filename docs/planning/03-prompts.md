# Disk Analyst — Prompts

Sequência para uma sessão nova do Claude Code aberta em `~/projects/disk-analyst`.

## 0. Prompt inicial (colar primeiro)

```text
Este é o Disk Analyst, um projeto de estudo: um analisador de uso de disco para
Windows em Rust, focado em ser o mais rápido possível. Antes de qualquer coisa, leia:

- docs/planning/00-visao.md   (propósito, escopo, regra de sala limpa)
- docs/planning/01-roadmap.md (fases e metas de desempenho)
- docs/planning/02-decisoes-em-aberto.md

Regras desta sessão:
1. Sala limpa: a única referência externa permitida é a seção "Funcionalidades
   observadas" de ~/projects/rea-test/docs/02-arquitetura.md. Nunca abra
   ~/projects/rea-test/work/.
2. Fluxo orientado a especificação com o Spec Kit. Nada de código antes de
   spec, plan e tasks aprovados por mim.
3. Desempenho só conta se for medido: toda afirmação de velocidade vem com
   comando hyperfine/criterion e o resultado.
4. Decisões de arquitetura viram ADRs em docs/adr/.

Primeiro passo: inicialize o Spec Kit neste repositório
(`specify init . --integration claude`), confirme quais skills speckit-* foram
instaladas e pare. Não rode nenhuma etapa do Spec Kit ainda.
```

## 1. Constituição (`/speckit-constitution`)

```text
Crie a constituição do Disk Analyst com estes 11 princípios:

1. Desempenho medido: toda mudança em caminho crítico (varredura, MFT, agregação)
   exige benchmark antes e depois (criterion no núcleo; hyperfine na CLI), com
   resultados em docs/benchmarks/. Sem medição, a otimização não entra.
2. Segurança dos dados do usuário: a varredura é só de leitura. Qualquer ação
   destrutiva exige confirmação explícita e usa a Lixeira por padrão; exclusão
   permanente só com flag explícita.
3. Testes primeiro: TDD no núcleo. Parsers de dados binários (MFT) têm testes
   por propriedade e fuzzing (cargo-fuzz). Testes que dependem de NTFS real ou de
   administrador ficam marcados e documentados.
4. Unsafe mínimo e justificado: todo bloco unsafe tem comentário SAFETY e fica
   isolado em módulos de FFI/Win32. O núcleo de análise é safe.
5. Arquitetura em camadas: crates/core (biblioteca, sem I/O de interface) →
   crates/cli → (depois) app Tauri. A interface nunca contém lógica de varredura.
6. Privacidade: nenhuma telemetria, nenhuma conexão de rede, nenhum atualizador
   automático. O servidor MCP usa só stdio local; sem porta aberta.
7. IA como conselheira, não executora: agentes conectados via MCP só leem e
   propõem planos de limpeza. Nenhuma ferramenta MCP exclui, move ou altera
   arquivos do usuário; itens classificados como protegidos nunca entram em plano;
   nomes de arquivos são tratados como dados, nunca como instruções.
8. Sala limpa: implementação a partir de documentação pública e da especificação;
   proibido usar código, textos ou recursos de produtos analisados no rea-test.
9. Plataforma: Windows 10/11 x64 é o alvo; o núcleo compila e testa também em
   Linux (para desenvolvimento no WSL) usando a varredura comum.
10. Formatos estáveis: JSON de saída, relatórios e respostas MCP têm
    schema_version e testes de contrato; mudança incompatível exige nova versão.
11. Simplicidade: dependências novas exigem justificativa no plan. Preferir std,
    windows (crate oficial), rayon, criterion, proptest, rkyv/serde, rmcp.

Stack: Rust stable (1.99+), edition 2024, workspace Cargo, alvo de release
x86_64-pc-windows-gnu (ver docs/planning/02-decisoes-em-aberto.md, D1).
Qualidade obrigatória: cargo fmt --check, cargo clippy -- -D warnings,
cargo test (Linux e --target x86_64-pc-windows-gnu).
```

## 2. Grill (`/grill-with-docs`)

```text
Use docs/planning/02-decisoes-em-aberto.md como frontier inicial. Feche D1–D8
agora; D12–D18 ficam para o início das Fases 3 e 4, e D9–D11 para depois.
Gere GLOSSARY.md e um ADR para cada decisão fechada.
```

## 3. Especificação da Fase 0 + 1 (`/speckit-specify`)

```text
Feature: varredura de diretórios e relatório na CLI (Fases 0 e 1 do
docs/planning/01-roadmap.md).

O usuário aponta um caminho (pasta ou raiz de unidade) e recebe, o mais rápido
possível:
- tamanho total (lógico e alocado), número de arquivos e de pastas;
- as N maiores pastas e os N maiores arquivos (N configurável, padrão 20);
- saída legível no terminal ou JSON estável (para a futura interface e para testes).

Comportamentos obrigatórios:
- não seguir junctions, symlinks nem pontos de montagem; contar hard links uma vez;
- arquivos só na nuvem (OneDrive sob demanda) marcados como tais, com tamanho
  alocado local zero;
- arquivos e pastas sem permissão de leitura são contados como "inacessíveis" no
  relatório, sem abortar a varredura;
- cancelamento com Ctrl+C deixando resultado parcial consistente;
- progresso opcional em stderr (arquivos/s), desligado no modo JSON.

Critérios de aceitação mensuráveis:
- totais idênticos aos de uma árvore sintética gerada com tamanhos conhecidos;
- tempo e memória registrados em C:\ com hyperfine (meta: < 15 s com cache quente);
- memória por nó medida e documentada (meta: < 64 bytes sem o nome).

Fora desta feature: MFT, categorias, duplicatas, exclusão, snapshots, interface.
```

## 4. Esclarecer, planejar, quebrar em tarefas

```text
/speckit-clarify
```

```text
/speckit-plan
Use a crate `windows` para Win32, `rayon` para paralelismo, árvore em arena
(Vec<Node> com índices u32, nomes UTF-16 num buffer contínuo), `criterion` para
microbenchmarks e `proptest` para testes por propriedade. Respeite os ADRs em docs/adr/.
```

```text
/speckit-tasks
Tarefas pequenas, cada uma com teste que falha primeiro, verificáveis numa sessão.
Inclua uma tarefa de benchmark-base logo após o primeiro scanner funcional.
```

```text
/speckit-analyze
```

O `/speckit-analyze` gera um relatório de consistência entre spec, plan, tasks e constituição. Corrija o que ele apontar antes de implementar.

## 5. Implementação

```text
/speckit-implement
Siga as tarefas em ordem, com TDD (superpowers:test-driven-development e
rust-testing). Ao terminar cada tarefa: cargo fmt, clippy -D warnings, testes em
Linux e Windows (--target x86_64-pc-windows-gnu), e marque a tarefa. Pare ao fim
de cada fase do tasks.md para minha revisão.
```

## Próximas features (mesmo ciclo)

Antes da Fase 3, rode `/grill-with-docs` sobre D12–D18.

### Fase 2 — MFT

```text
/speckit-specify
Leitura direta da MFT de volumes NTFS (Fase 2 do roadmap), com alternativa
automática para a varredura comum sem administrador ou fora do NTFS. Resultado
idêntico ao da varredura comum num volume de teste; < 3 s em C:\ com cache quente;
parser de FILE_RECORD com testes por propriedade e cargo-fuzz.
```

### Fase 3 — Análises, classificação de risco e exportação

```text
/speckit-specify
Fase 3 do docs/planning/01-roadmap.md: categorias configuráveis, duplicatas por
conteúdo (tamanho → xxh3 parcial → BLAKE3), classificação de risco 🟢/🟡/🔴 com
motivo por item (regras TOML, D16), detectores de lixo de desenvolvedor e
recuperáveis do Windows (só recomendação), e `disk-analyst report` em CSV
(UTF-8 com BOM, D18), JSON versionado e Markdown agregado para IA com orçamento
de tokens e truncamento explícito.

Critérios: testes de tabela caminho → risco + motivo; nenhum caminho protegido
classificado como seguro; relatório Markdown de C:\ dentro do orçamento e gerado
em < 1 s a partir de uma varredura pronta.
```

### Fase 4 — Servidor MCP

```text
/speckit-specify
Fase 4 do docs/planning/01-roadmap.md: `disk-analyst mcp`, servidor MCP via stdio
com a crate rmcp, expondo scan, get_scan_status, summarize_volume, list_largest,
find_duplicates, classify_paths, find_dev_artifacts, export_report e
propose_cleanup_plan (compare_snapshots desativada até a Fase 5).

Regras inegociáveis (princípio 7 da constituição): nenhuma ferramenta exclui ou
move arquivos; itens 🔴 nunca entram em planos; raízes permitidas configuráveis;
respostas paginadas e limitadas; nomes de arquivo só em campos estruturados.

Critérios: testes de contrato de cada ferramenta; fluxo "libere 30 GB no C:"
funcionando no Claude Code e no Gemini CLI e gerando plano sem itens 🔴;
resposta < 200 ms com varredura pronta. Inclua instruções de registro:
`claude mcp add disk-analyst -- disk-analyst.exe mcp --allow <raiz>` e o
equivalente em ~/.gemini/settings.json.
```

### Fase 5 — Ações e snapshots

```text
/speckit-specify
Fase 5: `disk-analyst apply <plano>` (mostra, confirma, envia para a Lixeira via
IFileOperation, registra log; --permanent exige segunda confirmação), snapshots
rkyv versionados, `disk-analyst diff` e ativação de compare_snapshots no MCP.
```

### Fase 6 — Interface

Rodar o impeccable antes; depois `/speckit-specify` da app Tauri consumindo
crates/core, com revisão e aplicação de planos de limpeza (inclusive os gerados
por IA).
