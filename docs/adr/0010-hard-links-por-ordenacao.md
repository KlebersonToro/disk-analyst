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
