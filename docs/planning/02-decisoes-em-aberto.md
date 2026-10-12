# Disk Analyst — Decisões em aberto

Entrada para o `/grill-with-docs` e o `/speckit-clarify`. Cada decisão fechada vira um ADR em `docs/adr/`.

| # | Decisão | Opções | Recomendação inicial |
|---|---|---|---|
| D1 ✅ [ADR-0001](../adr/0001-alvo-windows-gnu.md) | Alvo de compilação Windows | `x86_64-pc-windows-gnu` (já funciona no WSL) · `x86_64-pc-windows-msvc` via `cargo-xwin` | GNU para começar; reavaliar se precisarmos de PDB/ETW/WPA para perfilar |
| D2 ✅ [ADR-0002](../adr/0002-niveis-de-teste-ntfs.md) | Onde rodar testes de NTFS/MFT | WSL executando `.exe` pelo interop · terminal Windows nativo · VM | Interop do WSL; testes de MFT marcados `#[ignore]` e rodados como administrador |
| D3 ✅ [ADR-0003](../adr/0003-privilegio-asinvoker-sem-elevate.md) | Privilégio de administrador | Manifesto `requireAdministrator` · `asInvoker` com reinício elevado sob demanda · só detectar e usar a alternativa | `asInvoker` + flag `--elevate`; sem administrador, cai na varredura comum |
| D4 ✅ [ADR-0004](../adr/0004-parser-mft-proprio.md) | MFT: parser próprio ou crate `ntfs` | Parser próprio (máximo aprendizado e controle) · crate `ntfs` (mais rápido de entregar) | Parser próprio só para `FILE_RECORD`/`$FILE_NAME`/`$DATA`; `ntfs` como oráculo nos testes |
| D5 ✅ [ADR-0005](../adr/0005-paralelismo-rayon.md) | Paralelismo da varredura comum | `rayon` (work-stealing) · fila MPMC própria · `jwalk` | `rayon`; comparar com `jwalk` no benchmark da Fase 1 |
| D6 ✅ [ADR-0006](../adr/0006-nomes-utf16-buffer-continuo.md) | Representação dos nomes | `String` por nó · buffer UTF-16 contínuo + offsets · interning | Buffer UTF-16 contínuo (é o formato nativo do NTFS, sem conversão na leitura) |
| D7 ✅ [ADR-0007](../adr/0007-tamanho-alocado-e-enumeracao.md) | Tamanho exibido | Tamanho lógico · tamanho alocado em disco · ambos | Ambos; o padrão é o alocado (é o que libera espaço) |
| D8 ✅ [ADR-0008](../adr/0008-snapshot-rkyv.md) | Formato do snapshot | `bincode` · `rkyv` (zero-copy) · próprio | `rkyv`, pelo estudo de zero-copy; versionado desde o início |
| D9 | Interface gráfica | Tauri + impeccable · egui | Tauri (o impeccable gera interface web); decidir na Fase 6 |
| D10 | Nome do binário | `disk-analyst` · `da` · outro | `disk-analyst` |
| D11 | Licença (se publicar) | MIT · Apache-2.0 · MIT OR Apache-2.0 | `MIT OR Apache-2.0` (padrão do ecossistema Rust) |
| D12 | A IA pode excluir arquivos via MCP? | Não, só propõe plano · sim, com confirmação via elicitation do MCP · sim, livre | **Não** na v1: plano revisado e aplicado pelo usuário (`disk-analyst apply`) |
| D13 | Estado do servidor MCP | Varre a cada pedido · mantém a última varredura em memória · lê snapshot salvo | Mantém em memória durante a sessão; lê snapshot se existir (após F5) |
| D14 | Formato do relatório para IA | Markdown agregado · JSON compacto · ambos | Markdown agregado com orçamento de tokens; JSON compacto para quem usar via API |
| D15 | Privacidade dos caminhos no relatório | Caminhos completos · anonimizar só o nome do usuário · hash de todos os segmentos | Completos por padrão (uso local); `--anonymize` troca o usuário e segmentos fora das raízes conhecidas |
| D16 | Regras de classificação de risco | Embutidas no código · arquivo TOML embutido e sobrescrevível · baixadas da internet | TOML embutido, com sobrescrita local em `%APPDATA%`; nunca da internet (constituição: sem rede) |
| D17 | Transporte do MCP | stdio · HTTP local (streamable) · ambos | stdio (mais simples e sem porta aberta); HTTP só se a interface Tauri precisar |
| D18 | CSV para Excel | UTF-8 puro · UTF-8 com BOM · separador `,` ou `;` | UTF-8 com BOM; separador configurável, padrão `;` se a localidade usar vírgula decimal (pt-BR) |
