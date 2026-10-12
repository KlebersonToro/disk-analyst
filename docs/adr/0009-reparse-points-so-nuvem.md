---
status: accepted
---

# Reparse points: percorrer só os de nuvem

A Varredura de diretórios não percorre Itens com `FILE_ATTRIBUTE_REPARSE_POINT` (junctions,
symlinks, pontos de montagem e tags desconhecidas viram "link não seguido", tamanho zero), **exceto**
os de tag `IO_REPARSE_TAG_CLOUD*`, que são percorridos. Pastas do OneDrive sob demanda são reparse
points; tratá-las como links esconderia o OneDrive inteiro do relatório. A tag é lida do campo
`EaSize` de `FILE_ID_BOTH_DIR_INFO`, sem chamada extra (MS-FSCC). A raiz pedida pelo usuário é
sempre resolvida.

## Consequences

- Depende de confirmar a leitura da tag numa pasta OneDrive real (cenário manual do quickstart da
  spec 001). Se falhar, usar `FSCTL_GET_REPARSE_POINT` só nos diretórios com o atributo.
