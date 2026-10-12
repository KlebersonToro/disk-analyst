# Feature Specification: Varredura de diretórios e relatório na CLI

**Feature Branch**: `001-directory-scan-cli` (sem branch dedicada ainda; trabalho em `main`)

**Created**: 2026-10-12

**Status**: Draft

**Input**: User description: "Feature: varredura de diretórios e relatório na CLI (Fases 0 e 1 do
docs/planning/01-roadmap.md). O usuário aponta um caminho (pasta ou raiz de unidade) e recebe, o
mais rápido possível: tamanho total (lógico e alocado), número de arquivos e de pastas; as N maiores
pastas e os N maiores arquivos (N configurável, padrão 20); saída legível no terminal ou JSON estável.
Comportamentos obrigatórios: não seguir junctions, symlinks nem pontos de montagem; contar hard links
uma vez; arquivos só na nuvem marcados, com tamanho alocado local zero; itens sem permissão contados
como inacessíveis sem abortar; cancelamento com Ctrl+C deixando resultado parcial consistente;
progresso opcional em stderr, desligado no modo JSON. Critérios: totais idênticos aos de uma árvore
sintética; tempo e memória registrados em C:\ (meta < 15 s com cache quente); memória por nó medida
(meta < 64 bytes sem o nome). Fora: MFT, categorias, duplicatas, exclusão, snapshots, interface."

Vocabulário conforme [GLOSSARY.md](../../GLOSSARY.md). Decisões de base: ADR-0001, 0003, 0005,
0006 e 0007 em [docs/adr/](../../docs/adr/).

## Clarifications

### Session 2026-10-12

- Q: Quais pastas entram no ranking das maiores? → A: qualquer profundidade, ordenadas pelo tamanho
  próprio (só arquivos diretos), exibindo também o agregado.
- Q: A qual pasta atribuir um arquivo com hard links em várias pastas? → A: ao primeiro caminho
  em ordem lexicográfica; os demais com tamanho zero, marcados.
- Q: Como representar no JSON um nome que não é texto válido? → A: texto com U+FFFD e marca
  `name_lossy: true` no Item.
- Q: O JSON deve conter só o relatório ou também a árvore completa de Itens? → A: sempre o
  relatório mais a árvore completa.
- Q: Em que idioma a CLI escreve mensagens, rótulos e erros? → A: português, idioma único na v1;
  chaves do JSON em inglês.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Descobrir o que ocupa espaço num caminho (Priority: P1)

O usuário aponta uma pasta ou a raiz de um Volume e, em poucos segundos, vê no terminal o Tamanho
alocado e o Tamanho lógico totais, quantos arquivos e pastas existem, e as N maiores pastas e os N
maiores arquivos, ordenados pelo Tamanho alocado.

**Why this priority**: é o valor central do produto; sem isso nada mais faz sentido.

**Independent Test**: varrer uma árvore sintética com tamanhos conhecidos e conferir que totais,
contagens e rankings exibidos batem exatamente com os esperados.

**Acceptance Scenarios**:

1. **Given** uma árvore com 3 pastas e 10 arquivos de tamanhos conhecidos, **When** o usuário varre
   a raiz dela, **Then** o relatório mostra os totais lógico e alocado, 10 arquivos e 3 pastas,
   exatamente como esperado.
2. **Given** uma árvore com mais de 20 arquivos, **When** o usuário varre sem informar N, **Then**
   aparecem os 20 maiores arquivos e as 20 maiores pastas, em ordem decrescente de Tamanho alocado
   (pastas pelo tamanho próprio, FR-003).
3. **Given** a mesma árvore, **When** o usuário informa N = 5, **Then** aparecem exatamente 5 de
   cada.
4. **Given** um caminho inexistente, **When** o usuário varre, **Then** recebe uma mensagem de erro
   clara e um código de saída de erro, sem relatório.

---

### User Story 2 - Obter o resultado em formato estável para máquinas (Priority: P1)

O usuário (ou um script, teste ou a futura interface) pede o resultado em JSON e recebe um documento
com versão de esquema, que pode ser processado sem interpretar texto de terminal.

**Why this priority**: é o contrato usado pelos testes de aceitação, pela futura interface e pelos
agentes de IA (Princípio X); sem ele não há como verificar os demais critérios automaticamente.

**Independent Test**: varrer a árvore sintética em modo JSON, validar o documento contra o esquema
publicado e comparar os valores com os esperados.

**Acceptance Scenarios**:

1. **Given** qualquer varredura bem-sucedida, **When** o usuário pede JSON, **Then** a saída padrão
   contém só um documento JSON válido com `schema_version`, e nada mais.
2. **Given** o modo JSON, **When** a varredura roda, **Then** nenhum progresso é escrito, mesmo que
   o usuário tenha pedido progresso.
3. **Given** duas varreduras da mesma árvore inalterada, **When** ambas pedem JSON, **Then** os
   documentos são idênticos, exceto por campos de tempo de execução.

---

### User Story 3 - Varredura correta em casos reais do Windows (Priority: P2)

O usuário varre `C:\` ou a pasta de perfil e confia nos números: links não inflam totais, pastas
protegidas não abortam a varredura e arquivos do OneDrive que não estão no disco não contam como
espaço ocupado.

**Why this priority**: sem isso os números de um disco real ficam errados ou a varredura falha,
mas a P1 já entrega valor em pastas comuns.

**Independent Test**: árvores de teste montadas com junction, symlink, hard link, pasta sem
permissão e Arquivo na nuvem, comparando o relatório com os valores esperados.

**Acceptance Scenarios**:

1. **Given** uma pasta com uma junction ou symlink para outra pasta grande, **When** o usuário varre,
   **Then** o conteúdo do destino não é percorrido nem somado através do link.
2. **Given** um arquivo com 3 hard links dentro do caminho varrido, **When** o usuário varre,
   **Then** o tamanho dele é somado uma única vez no total.
3. **Given** um outro Volume montado numa pasta, **When** o usuário varre, **Then** o ponto de
   montagem não é percorrido.
4. **Given** uma pasta sem permissão de leitura, **When** o usuário varre, **Then** a varredura
   termina normalmente e o relatório informa a quantidade de Itens inacessíveis.
5. **Given** um Arquivo na nuvem de 1 GB, **When** o usuário varre, **Then** ele conta 1 GB no
   Tamanho lógico, 0 no Tamanho alocado e aparece marcado como na nuvem.

---

### User Story 4 - Interromper uma varredura longa (Priority: P2)

O usuário percebe que a varredura vai demorar e aperta Ctrl+C; recebe o relatório do que já foi
varrido, claramente marcado como parcial.

**Why this priority**: em volumes grandes interromper é comum; perder tudo ou receber números
incoerentes quebra a confiança.

**Independent Test**: interromper a varredura de uma árvore sintética grande e verificar que o
relatório é marcado como parcial e que todo total de pasta é igual à soma de seus filhos
efetivamente varridos.

**Acceptance Scenarios**:

1. **Given** uma varredura em andamento, **When** o usuário aperta Ctrl+C, **Then** em até 1 segundo
   o programa emite o relatório parcial (no mesmo formato pedido) marcado como parcial e encerra com
   código de saída próprio de cancelamento.
2. **Given** um relatório parcial, **When** se inspecionam os totais, **Then** cada total de pasta é
   igual à soma dos Itens daquela pasta incluídos no relatório (sem contagens pela metade).

---

### User Story 5 - Acompanhar o progresso (Priority: P3)

O usuário pede progresso e vê, em stderr, quantos Itens já foram varridos e a taxa (arquivos/s),
sem poluir a saída padrão.

**Why this priority**: conforto; não muda os resultados.

**Independent Test**: varrer com progresso ligado redirecionando a saída padrão para um arquivo e
conferir que o arquivo contém só o relatório e que stderr recebeu atualizações.

**Acceptance Scenarios**:

1. **Given** progresso ligado e saída de texto, **When** a varredura roda, **Then** stderr recebe
   atualizações pelo menos uma vez por segundo e a saída padrão contém só o relatório.
2. **Given** progresso não pedido, **When** a varredura roda, **Then** nada é escrito em stderr além
   de avisos e erros.

---

### User Story 6 - Fundação verificável e medida (Priority: P3)

O mantenedor gera uma árvore sintética descrita com tamanhos conhecidos, roda a verificação de
qualidade completa nos dois alvos (Linux e Windows) com um comando e registra o benchmark-base.

**Why this priority**: é o que torna mensuráveis os critérios das outras histórias (Fase 0), mas não
entrega valor direto ao usuário final.

**Independent Test**: gerar a mesma árvore duas vezes a partir da mesma descrição e obter conteúdo
idêntico; rodar a verificação e ver todos os portões passarem.

**Acceptance Scenarios**:

1. **Given** uma descrição de árvore (pastas, arquivos, tamanhos, links), **When** o mantenedor gera
   a árvore, **Then** ela é criada de forma determinística e vem acompanhada dos totais esperados.
2. **Given** o repositório, **When** o mantenedor roda a verificação de qualidade, **Then** formatação,
   lint e testes rodam no alvo Linux e no alvo Windows, e qualquer falha interrompe a verificação.

---

### Edge Cases

- Caminho que é um arquivo, não uma pasta: o relatório trata esse arquivo como o único Item.
- Caminho raiz que é ele próprio uma junction ou symlink: a raiz é resolvida (o usuário a pediu
  explicitamente), mas links internos continuam não sendo seguidos.
- Pasta vazia: totais zero, rankings vazios, sem erro.
- Árvore com menos Itens que N: os rankings mostram só o que existe.
- Empate de tamanho nos rankings: desempate determinístico pelo caminho.
- Caminhos com mais de 260 caracteres: varridos normalmente.
- Nomes que não são texto válido (surrogates UTF-16 soltos): varridos e contados; exibidos com
  caractere de substituição no terminal e no JSON, onde o Item recebe a marca `name_lossy: true`.
- Hard link com nomes em pastas diferentes do caminho varrido: o total geral conta o arquivo uma vez
  e o tamanho vai para o primeiro caminho em ordem lexicográfica (FR-008), igual em toda execução.
- Pasta que some ou muda durante a varredura: o Item é tratado como inacessível ou com o estado lido,
  sem abortar.
- Arquivos esparsos ou comprimidos: Tamanho alocado menor que o lógico, reportado como medido.
- Ctrl+C pressionado duas vezes: o segundo encerra imediatamente, sem relatório.

## Requirements *(mandatory)*

### Functional Requirements

**Entrada e saída**

- **FR-001**: O usuário MUST poder varrer um caminho (pasta, raiz de Volume ou arquivo) informado na
  linha de comando.
- **FR-002**: O relatório MUST conter: Tamanho lógico total, Tamanho alocado total, número de
  arquivos, número de pastas, número de Itens inacessíveis, número de Arquivos na nuvem e duração da
  varredura.
- **FR-003**: O relatório MUST listar as N maiores pastas e os N maiores arquivos por Tamanho alocado,
  com N configurável pelo usuário e padrão 20. Cada entrada mostra caminho, Tamanho alocado e Tamanho
  lógico. O ranking de pastas considera pastas de qualquer profundidade, ordenadas pelo tamanho
  próprio (soma só dos arquivos diretamente dentro delas, sem subpastas); cada entrada mostra o
  tamanho próprio e o tamanho agregado (com subpastas).
- **FR-004**: O usuário MUST poder escolher entre saída de texto legível (padrão) e JSON.
- **FR-005**: A saída JSON MUST trazer `schema_version`, ter esquema publicado no repositório e
  testes de contrato; em modo JSON, a saída padrão contém só o documento. O documento contém sempre
  o relatório (o mesmo conteúdo da saída de texto) e a árvore completa de Itens varridos, cada um
  com seus tamanhos e marcas e a relação com a pasta-pai.
- **FR-006**: O programa MUST usar códigos de saída distintos para: sucesso (mesmo com Itens
  inacessíveis), erro de uso ou caminho inválido, e varredura cancelada.

**Correção da contagem**

- **FR-007**: A varredura MUST NOT seguir junctions, symlinks nem pontos de montagem encontrados
  dentro do caminho; eles são contados como Itens próprios, sem o conteúdo do destino.
- **FR-008**: Um arquivo com vários hard links dentro do caminho varrido MUST ser contado uma única
  vez nos totais gerais e nas contagens. O tamanho é atribuído ao caminho que vem primeiro em ordem
  lexicográfica entre os caminhos do arquivo dentro da varredura; os demais caminhos aparecem como
  Itens com tamanho zero e marcados como hard link.
- **FR-009**: Arquivos na nuvem MUST ser identificados e marcados, contribuindo com seu Tamanho lógico
  e com Tamanho alocado zero.
- **FR-010**: Itens sem permissão de leitura MUST ser contados como inacessíveis, sem abortar a
  varredura; o relatório informa a quantidade.
- **FR-011**: Para cada pasta, o tamanho agregado MUST ser igual à soma dos tamanhos de todos os
  Itens abaixo dela incluídos no relatório.

**Cancelamento e progresso**

- **FR-012**: Ao receber Ctrl+C, o programa MUST parar de varrer, emitir em até 1 segundo o relatório
  parcial no formato pedido, marcado como parcial, e sair com o código de cancelamento. Um segundo
  Ctrl+C MUST encerrar imediatamente.
- **FR-013**: O usuário MUST poder ligar o progresso, escrito apenas em stderr, com Itens varridos e
  taxa em arquivos por segundo, atualizado pelo menos uma vez por segundo. Desligado por padrão e
  sempre desligado em modo JSON.

**Segurança, plataforma e fundação**

- **FR-014**: A varredura MUST ser somente leitura: não cria, altera, move nem exclui nada no caminho
  varrido, e não faz nenhuma conexão de rede.
- **FR-015**: A funcionalidade MUST rodar no Windows 10/11 x64 sem privilégio de administrador, e
  também em Linux (com a semântica de Tamanho alocado do sistema de arquivos local).
- **FR-016**: O projeto MUST incluir um gerador determinístico de árvore sintética a partir de uma
  descrição (pastas, arquivos, tamanhos, links), que também produz os totais esperados.
- **FR-017**: O projeto MUST oferecer uma verificação de qualidade única que roda formatação, lint e
  testes nos alvos Linux e Windows e falha se qualquer um falhar.
- **FR-018**: O tempo de varredura e a memória usada MUST ser medidos de forma repetível e
  registrados em `docs/benchmarks/`, incluindo um benchmark-base logo após o primeiro varredor
  funcional.
- **FR-019**: Mensagens, rótulos do relatório de texto, progresso e erros MUST estar em português
  (idioma único na v1). Chaves e valores enumerados do JSON MUST estar em inglês, por serem
  contrato para máquinas.

### Key Entities

- **Item**: arquivo ou pasta encontrado; tem caminho, nome, tipo (arquivo, pasta, link não seguido),
  Tamanho lógico, Tamanho alocado e marcas (na nuvem, inacessível, hard link, nome com perdas).
  Pastas têm também tamanho próprio e tamanho agregado.
- **Árvore de varredura**: todos os Itens de uma varredura com relação pai-filho e tamanhos agregados
  por pasta.
- **Relatório de varredura**: totais, rankings, contagens, duração, indicação de parcial e versão do
  esquema; é o que o usuário recebe.
- **Descrição de árvore sintética**: especificação de uma árvore de teste com os totais esperados.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Para 100% das árvores sintéticas de teste, totais, contagens e rankings do relatório
  são idênticos aos valores esperados (diferença de zero byte).
- **SC-002**: A varredura completa de `C:\` no hardware de referência, com saída de texto, leva
  menos de 15 segundos com cache quente (mediana de 5 execuções após 1 de aquecimento), registrada
  em `docs/benchmarks/`. O tempo e o tamanho da saída em modo JSON (com a árvore completa) também
  são medidos e registrados, sem meta nesta feature.
- **SC-003**: A memória por Item indexado, sem contar o nome, fica abaixo de 64 bytes, medida e
  documentada em `docs/benchmarks/`; o pico de memória da varredura de `C:\` também é registrado.
- **SC-004**: Em árvores de teste com junctions, symlinks, hard links e pontos de montagem, nenhum
  byte é contado em dobro nem através de link.
- **SC-005**: Uma varredura de `C:\` sem privilégio de administrador termina sem abortar, com os
  Itens inacessíveis contados no relatório.
- **SC-006**: Após Ctrl+C, o relatório parcial aparece em até 1 segundo e passa na verificação de
  consistência de FR-011 em 100% das execuções de teste.
- **SC-007**: Todo documento JSON emitido nos testes valida contra o esquema publicado, e a árvore
  nele contida reproduz exatamente os totais do relatório.

## Assumptions

- Hardware de referência é o descrito em `docs/planning/00-visao.md` (8 threads, 31 GiB, `C:` NTFS
  de 235 GB).
- "Cache quente" significa uma varredura idêntica executada imediatamente antes (aquecimento).
- As medições de tempo usam `hyperfine --warmup 1 --runs 5` (constituição, Princípio I); a memória
  por Item é medida por microbenchmark no núcleo.
- Tamanhos no texto legível usam unidades binárias (KiB, MiB, GiB); o JSON usa bytes inteiros.
- Varrer compartilhamentos de rede não é proibido, mas não é alvo de teste nem de desempenho.
- Itens inacessíveis entram com tamanho zero, pois não puderam ser lidos.
- O Motor de varredura desta feature é só a Varredura de diretórios; a opção de escolher motor
  (ADR-0003) aceita apenas esse motor até a Fase 2.
- Fora desta feature: Leitura da MFT, categorias, duplicatas, Níveis de risco, exclusão, Snapshots,
  relatórios CSV/Markdown, servidor MCP e interface gráfica.
