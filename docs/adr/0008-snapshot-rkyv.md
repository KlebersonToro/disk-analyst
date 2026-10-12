---
status: accepted
---

# Snapshot em rkyv com cabeçalho versionado

Snapshots são serializados com `rkyv` (zero-copy), precedidos por um cabeçalho próprio: bytes
mágicos, `format_version` e hash do esquema. Versão incompatível é recusada com erro claro.

## Considered Options

- `bincode`: a versão 3.0.0 é uma lápide (só um `compile_error!`); o projeto não é mais mantido.
- `postcard`: simples e estável, mas sem leitura zero-copy, que é parte do estudo.
- Formato próprio: trabalho sem ganho claro.

## Consequences

- O uso real só chega na Fase 5. Reavaliar se a arena (ADR-0006, ADR-0007) não puder ser
  arquivada sem cópia.
