---
status: accepted
---

# Paralelismo da Varredura de diretórios com rayon

A Varredura de diretórios é paralelizada com `rayon` (work-stealing), uma tarefa por pasta via
`rayon::scope`. Uma fila MPMC própria só entra se um benchmark mostrar que o `rayon` perde de
forma mensurável, e então com novo ADR.

## Considered Options

- `jwalk`: descontinuado (o registro indica `dua-core`) e, por usar `std::fs`, não daria o
  Tamanho alocado no Windows (ver ADR-0007).
- Fila MPMC própria: mais controle, mais código; sem evidência de necessidade.

## Consequences

- O benchmark-base da Fase 1 compara: uma thread, `rayon` e o `dua` como referência externa via
  `hyperfine`. O `dua` não é dependência do projeto.
