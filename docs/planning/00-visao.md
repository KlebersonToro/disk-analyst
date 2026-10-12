# Disk Analyst — Visão

## Propósito

Projeto de estudo para aprender **Rust de baixo nível**, **desenvolvimento orientado a especificação** (Spec Kit) e **engenharia reversa com o REA**, construindo um analisador de uso de disco para Windows que seja o **mais rápido possível**.

Pode virar open-source no futuro, mas isso não é prioridade.

## Objetivos

1. **Velocidade:** listar todos os arquivos de um volume NTFS em segundos, lendo a MFT diretamente, com varredura paralela de diretórios como alternativa.
2. **Memória:** representar milhões de arquivos com pouca memória (árvore em arena, nomes compactados).
3. **Segurança dos dados:** nunca apagar nada sem confirmação; exclusão sempre para a Lixeira por padrão.
4. **Medição:** cada otimização é comprovada com benchmark reproduzível (`hyperfine`, `criterion`).
5. **Aprendizado:** decisões registradas em ADRs; código legível acima de truques não medidos.
6. **Decisão assistida por IA:** relatórios exportáveis (CSV, JSON, Markdown resumido para IA) e um servidor MCP para que agentes (Claude, Gemini) ajudem a decidir o que apagar. A IA **propõe** planos de limpeza; quem executa é sempre o usuário.
7. **Explicabilidade:** toda sugestão de limpeza diz o motivo (categoria, regra de risco, duplicata, idade).

## Diferenças em relação ao original

| Disk Analyzer Pro | Disk Analyst |
|---|---|
| Telemetria com o MAC, atualizador por HTTP, ofertas de terceiros | Sem rede, sem telemetria, sem ofertas |
| Só tamanho lógico | Tamanho lógico **e** alocado; arquivos só na nuvem identificados |
| Duplicatas por nome, tamanho e data | Duplicatas por conteúdo (hash) |
| Sem noção de risco | Classificação 🟢/🟡/🔴 com motivo; caminhos protegidos nunca sugeridos |
| Sem histórico | Snapshots e comparação de crescimento |
| Relatório interno do app | CSV, JSON versionado e Markdown para IA, mais servidor MCP |
| Genérico | Detectores de lixo de desenvolvedor e de recuperáveis do Windows |

## Fora do escopo (por enquanto)

- Interface gráfica: vem depois (Fase 6), com o impeccable. A tendência é Tauri, para usar interface web; ver [decisões em aberto](02-decisoes-em-aberto.md).
- Telemetria de qualquer tipo, conexões de rede, atualizador automático, licenciamento, ofertas de terceiros.
- Exclusão de arquivos feita por agentes de IA (ver D12).
- Linux e macOS. O núcleo pode ser portável, mas a leitura da MFT é só para Windows/NTFS.
- Sistemas de arquivos além do NTFS (FAT32, exFAT, ReFS), exceto pela varredura comum de diretórios.

## Regra de "sala limpa"

Este projeto se inspira na **funcionalidade** do Disk Analyzer Pro (Systweak), analisado no repositório `rea-test`.

- **Pode** usar como referência: `~/projects/rea-test/docs/02-arquitetura.md`, seção "Funcionalidades observadas" (lista de recursos, categorias de arquivo, comportamento de exclusão). Nenhuma outra parte dos documentos de análise serve como especificação.
- **Não pode** abrir nem copiar `~/projects/rea-test/work/` (binários e código decompilado), nem textos, ícones, nomes ou estruturas internas do produto original.
- Toda implementação é escrita do zero a partir de documentação pública (Microsoft Learn, docs das crates) e da especificação deste repositório.

## Hardware de referência para benchmarks

| Item | Valor |
|---|---|
| CPU | 8 threads lógicas |
| RAM | 31 GiB |
| `C:` | NTFS, 235 GB (sistema) |
| `D:` | NTFS, 1 TB |
| Ambiente de build | WSL2 (Ubuntu 26.04), Rust 1.99, alvo `x86_64-pc-windows-gnu` |

## Ferramentas e skills disponíveis

- **Spec Kit** (`specify` 1.1.3): constitution → specify → clarify → plan → tasks → implement.
- **Skills:** `rust-engineer`, `rust-testing`, `cargo-fuzz`, `documentation-and-adrs`, `/grill-with-docs` (glossário + ADRs), superpowers (brainstorming, TDD, verificação, subagentes).
- **Medição:** `hyperfine`, `cargo-flamegraph`, `criterion` (crate).
- **REA + Ghidra:** para analisar o próprio binário compilado e comparar com o original.
