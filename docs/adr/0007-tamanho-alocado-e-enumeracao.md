---
status: accepted
---

# Dois tamanhos, alocado por padrão; enumeração por FileIdBothDirectoryInfo

Medimos Tamanho lógico e Tamanho alocado, exibindo o alocado por padrão, porque é o que se libera
ao excluir. Como `FindFirstFileExW` só devolve o tamanho lógico, a Varredura de diretórios no
Windows abre cada pasta e chama `GetFileInformationByHandleEx` com `FileIdBothDirectoryInfo`, que
devolve em lote `EndOfFile`, `AllocationSize`, atributos e `FileId`. Isso desvia do roadmap da
Fase 1, que citava `FindFirstFileExW`.

## Considered Options

- `FindFirstFileExW` + um handle por arquivo para obter o alocado: abrir um handle por arquivo
  tornaria a varredura ordens de grandeza mais lenta.
- Só tamanho lógico: esconde compressão, arquivos esparsos e Arquivos na nuvem, justamente o que
  diferencia o projeto.

## Consequences

- O `FileId` é a base para contar cada hard link uma vez; a estratégia exata (conjunto de todos
  os `FileId` ou só dos candidatos) é decidida no plan da Fase 1, com custo de memória medido.
- No Linux o alocado vem de `st_blocks * 512`.
