---
status: accepted
---

# Parser próprio da MFT, puro e safe

Escrevemos nosso próprio parser da MFT: uma função de `&[u8]` para registros, sem Win32 e sem
`unsafe`, para aprender o formato e controlar desempenho. Escopo da v1: `FILE_RECORD` com fixups
(update sequence), atributos residentes e não residentes, data runs (o próprio `$MFT` pode estar
fragmentado), `$STANDARD_INFORMATION`, `$FILE_NAME`, `$DATA` e `$ATTRIBUTE_LIST`. Sem listas de
atributos, arquivos muito fragmentados ou com muitos hard links sairiam com tamanho errado.

## Considered Options

- Crate `ntfs` (0.4) como implementação: mais rápido de entregar, mas tira o objetivo de estudo
  e o controle sobre alocação. Pode entrar como `dev-dependency` de oráculo se a Fase 2 adotar
  imagens NTFS como fixtures (ver ADR-0002).

## Consequences

- Por ser puro, o parser roda, é testado e passa por fuzzing no Linux.
- A abertura e leitura de `\\.\C:` fica num módulo Win32 separado, único lugar com `unsafe`
  nesse caminho (Princípio IV).
