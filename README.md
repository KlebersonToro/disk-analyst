# Disk Analyst

Analisador de uso de disco para Windows, escrito em Rust, com um objetivo principal: ser o mais
rápido possível, com cada ganho de velocidade comprovado por benchmark.

> **Status: em especificação.** Ainda não há código. O projeto segue um fluxo orientado a
> especificação ([Spec Kit](https://github.com/github/spec-kit)): constituição, spec, plano e
> tarefas são aprovados antes de qualquer implementação. Acompanhe em [`specs/`](specs/).

## O que ele vai fazer

- **Varredura rápida** de pastas e volumes inteiros: varredura paralela de diretórios e, com
  privilégio de administrador, leitura direta da MFT do NTFS.
- **Tamanho lógico e alocado** de cada item, com arquivos do OneDrive sob demanda identificados.
- **Contagem correta**: hard links contados uma vez; junctions, symlinks e pontos de montagem não
  são seguidos.
- **Análises**: maiores pastas e arquivos, categorias, duplicatas por conteúdo, lixo de
  desenvolvedor (`node_modules`, `target/`, caches) e classificação de risco 🟢/🟡/🔴 com motivo.
- **Relatórios** em CSV, JSON versionado e Markdown resumido para IA.
- **Servidor MCP** (stdio) para que agentes como Claude e Gemini ajudem a decidir o que remover.
  A IA só **propõe** planos de limpeza; quem aplica é sempre o usuário, e a exclusão vai para a
  Lixeira por padrão.

## Princípios

- **Sem rede**: nenhuma telemetria, conexão de rede ou atualizador automático.
- **Somente leitura** por padrão: nada é apagado sem confirmação explícita.
- **Desempenho medido**: toda otimização vem com `hyperfine`/`criterion` e resultado registrado.
- **Sala limpa**: implementação feita do zero a partir de documentação pública.

A lista completa está na [constituição do projeto](.specify/memory/constitution.md).

## Roadmap

| Fase | Entrega |
|---|---|
| 0 | Fundação: workspace, portões de qualidade, gerador de árvore sintética |
| 1 | Varredura de diretórios e relatório na CLI (MVP) |
| 2 | Leitura direta da MFT (NTFS) |
| 3 | Análises, classificação de risco e exportação |
| 4 | Servidor MCP para agentes de IA |
| 5 | Ações (envio para a Lixeira) e snapshots |
| 6 | Interface gráfica |

Detalhes e metas de desempenho em [docs/planning/01-roadmap.md](docs/planning/01-roadmap.md).

## Metas de desempenho

| Métrica | Meta |
|---|---|
| `C:` via MFT, cache quente | < 3 s |
| `C:` via varredura paralela, cache quente | < 15 s |
| Memória por arquivo indexado | < 64 bytes, sem contar o nome |

Os resultados medidos serão publicados em `docs/benchmarks/`.

## Documentação

| Onde | O quê |
|---|---|
| [docs/planning/](docs/planning/) | Visão, roadmap, decisões em aberto |
| [docs/adr/](docs/adr/) | Registros de decisões de arquitetura |
| [GLOSSARY.md](GLOSSARY.md) | Vocabulário do domínio |
| [specs/](specs/) | Especificações das features (Spec Kit) |

## Plataforma e build

- Alvo: Windows 10/11 x64. O núcleo também compila e roda em Linux, para desenvolvimento no WSL.
- Rust stable 1.99+ (edition 2024), compilado para `x86_64-pc-windows-gnu` a partir do WSL
  ([ADR-0001](docs/adr/0001-alvo-windows-gnu.md)).

Instruções de build e uso serão adicionadas quando houver código.

## Licença

Ainda não definida. Até lá, todos os direitos são reservados.
