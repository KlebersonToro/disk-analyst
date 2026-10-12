---
status: accepted
---

# Hard links: deduplicação por ordenação após a varredura

Como `FILE_ID_BOTH_DIR_INFO` traz o `FileId` mas não o número de links, cada thread guarda pares
`(FileId, NodeId)` de todos os arquivos; após a varredura o vetor é ordenado e cada grupo repetido
atribui o tamanho ao caminho de menor ordem lexicográfica (spec 001, FR-008), zerando os demais. O
vetor (16 bytes por arquivo) é temporário e não conta na memória da arena; o pico é medido.

## Considered Options

- `HashSet<FileId>` concorrente: memória permanente e contenção entre threads.
- Abrir cada arquivo para ler `NumberOfLinks`: ordens de grandeza mais lento.
- Atribuir ao primeiro caminho encontrado: barato, mas não determinístico com paralelismo.

## Adendo (2026-10-12): o que é "menor ordem lexicográfica"

O caminho é comparado como **sequência de componentes** a partir da raiz da varredura, componente a
componente, cada um em ordinal UTF-16; um caminho que é prefixo de outro vem antes. Não se compara
a string com separador. Exemplo: `a\x` vence `a1\y` (componente `a` < `a1`); como string venceria
`a1\y`, porque `1` (0x31) < `\` (0x5C).

Motivos: o resultado não depende do separador (`\` no Windows, `/` no Linux), então é o mesmo nos
dois alvos; e coincide com a ordem de exibição da árvore, com filhos ordenados por nome. Como a
montagem já ordena os filhos, essa ordem é a pré-ordem da árvore, usada também no desempate dos
rankings (`ScanTree::preorder_ranks`).
