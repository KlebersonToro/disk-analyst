# Constituição do Disk Analyst

## Princípios fundamentais

### I. Desempenho medido

- Toda mudança em caminho crítico (varredura, leitura da MFT, agregação de tamanhos) DEVE vir com
  benchmark antes e depois: `criterion` no núcleo, `hyperfine` na CLI.
- Os resultados DEVEM ser registrados em `docs/benchmarks/`, com o comando exato, o hardware e a
  data. O padrão para a CLI é `hyperfine --warmup 1 --runs 5`.
- Otimização sem medição NÃO entra. Afirmações de velocidade sem comando e resultado são inválidas.

Justificativa: o objetivo central do projeto é ser o mais rápido possível; sem medição isso é
opinião.

### II. Segurança dos dados do usuário

- A varredura e todas as análises DEVEM ser somente leitura.
- Qualquer ação destrutiva DEVE exigir confirmação explícita do usuário e DEVE usar a Lixeira por
  padrão.
- Exclusão permanente SÓ PODE ocorrer com flag explícita (`--permanent`) e segunda confirmação.

Justificativa: um erro de exclusão é irreversível; a ferramenta nunca pode ser a causa de perda de
dados.

### III. Testes primeiro (INEGOCIÁVEL)

- O núcleo DEVE ser desenvolvido com TDD: teste escrito, teste falhando, implementação,
  refatoração.
- Parsers de dados binários (MFT e afins) DEVEM ter testes por propriedade (`proptest`) e fuzzing
  (`cargo-fuzz`).
- Testes que dependem de NTFS real ou de privilégio de administrador DEVEM ser marcados (por
  exemplo `#[ignore = "motivo"]`) e documentados, com instruções de como executá-los.

Justificativa: código de baixo nível e paralelo esconde bugs que só testes sistemáticos encontram.

### IV. Unsafe mínimo e justificado

- Todo bloco `unsafe` DEVE ter um comentário `// SAFETY:` explicando por que as invariantes valem.
- `unsafe` DEVE ficar isolado em módulos de FFI/Win32. O núcleo de análise DEVE ser safe.

Justificativa: concentra o risco de memória em poucos pontos auditáveis.

### V. Arquitetura em camadas

- Dependências seguem uma direção: `crates/core` (biblioteca, sem I/O de interface) →
  `crates/cli` → (depois) app Tauri. Servidor MCP e interface consomem o mesmo `core`.
- A interface (CLI, MCP, Tauri) NÃO DEVE conter lógica de varredura ou análise.

Justificativa: permite entregar relatórios e MCP antes da interface gráfica, sem duplicar lógica.

### VI. Privacidade

- NÃO DEVE haver telemetria, conexão de rede nem atualizador automático, em nenhuma camada.
- O servidor MCP DEVE usar apenas stdio local; NÃO DEVE abrir porta.

Justificativa: a ferramenta enxerga o disco inteiro do usuário; nada disso sai da máquina.

### VII. IA como conselheira, não executora

- Agentes conectados via MCP SÓ PODEM ler dados e propor planos de limpeza.
- Nenhuma ferramenta MCP PODE excluir, mover ou alterar arquivos do usuário. A execução de um
  plano é sempre feita pelo usuário, fora do MCP.
- Itens classificados como protegidos NUNCA DEVEM entrar num plano, mesmo a pedido do agente.
- Nomes e caminhos de arquivos DEVEM ser tratados como dados, nunca como instruções: ficam em
  campos estruturados e não são concatenados em texto de orientação ao agente.

Justificativa: limita o dano de alucinação ou injeção de prompt via nomes de arquivo.

### VIII. Sala limpa

- A implementação DEVE partir de documentação pública (Microsoft Learn, docs das crates) e da
  especificação deste repositório.
- É proibido usar código, textos, ícones, nomes ou estruturas internas de produtos analisados no
  `rea-test`. A única referência permitida é a seção "Funcionalidades observadas" de
  `~/projects/rea-test/docs/02-arquitetura.md`; `~/projects/rea-test/work/` NÃO DEVE ser aberto.

Justificativa: o projeto se inspira em funcionalidade, não em implementação.

### IX. Plataforma

- O alvo é Windows 10/11 x64.
- O núcleo DEVE compilar e passar nos testes também em Linux (desenvolvimento no WSL), usando a
  varredura comum de diretórios. Código exclusivo do Windows fica atrás de `cfg(windows)`.

Justificativa: o ciclo de desenvolvimento roda no WSL; o produto roda no Windows.

### X. Formatos estáveis

- JSON de saída, relatórios e respostas MCP DEVEM ter `schema_version` e testes de contrato.
- Mudança incompatível DEVE incrementar a versão do esquema.

Justificativa: interface, agentes de IA e scripts dependem desses formatos.

### XI. Simplicidade

- Toda dependência nova DEVE ser justificada no plan da feature.
- Preferidas: `std`, `windows` (crate oficial), `rayon`, `criterion`, `proptest`, `rkyv`/`serde`,
  `rmcp`.
- Código legível vem antes de truques não medidos (ver Princípio I).

Justificativa: menos dependências, menos superfície de ataque e de manutenção; o projeto é de
estudo.

## Restrições técnicas e metas

**Stack**
- Rust stable 1.99+, edition 2024, workspace Cargo.
- Alvo de release: `x86_64-pc-windows-gnu` (D1 em `docs/planning/02-decisoes-em-aberto.md`;
  pode mudar por ADR).

**Metas de desempenho de referência** (hardware em `docs/planning/00-visao.md`; cada feature fixa
seus critérios de aceitação na spec e os mede conforme o Princípio I):

| Métrica | Meta |
|---|---|
| `C:` via MFT, cache quente | < 3 s |
| `C:` via varredura paralela, cache quente | < 15 s |
| Memória por arquivo indexado | < 64 bytes, sem contar o nome |
| Relatório Markdown para IA de `C:` | < 1 s a partir de uma varredura pronta |
| Resposta de ferramenta MCP (varredura pronta) | < 200 ms |
| Binário (release, stripped) | < 8 MB, com MCP |

## Fluxo de desenvolvimento e portões de qualidade

- Cada fase do roadmap é uma feature do Spec Kit: specify → clarify → plan → tasks → analyze →
  implement. Nenhum código é escrito antes de spec, plan e tasks aprovados pelo mantenedor.
- Decisões de arquitetura DEVEM ser registradas como ADR em `docs/adr/`.
- Portões obrigatórios antes de dar uma tarefa como concluída:
  - `cargo fmt --check`
  - `cargo clippy -- -D warnings`
  - `cargo test` em Linux **e** `cargo test --target x86_64-pc-windows-gnu`
  - benchmark registrado em `docs/benchmarks/`, se a tarefa toca caminho crítico (Princípio I)

## Governança

- Esta constituição prevalece sobre qualquer outra prática do repositório. O "Constitution Check"
  de cada plan DEVE verificar os princípios I–XI; violações exigem justificativa explícita na
  seção de complexidade do plan ou ficam bloqueadas.
- Emendas são feitas via `/speckit-constitution`, aprovadas pelo mantenedor e, quando mudam
  arquitetura, acompanhadas de ADR.
- Versionamento semântico: MAJOR para remoção ou redefinição incompatível de princípio; MINOR para
  princípio ou seção nova ou orientação ampliada; PATCH para redação e correções.

**Version**: 1.0.0 | **Ratified**: 2026-10-12 | **Last Amended**: 2026-10-12
