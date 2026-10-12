---
status: accepted
---

# Nomes como UTF-16 bruto num buffer contínuo

Os nomes dos Itens ficam num único buffer de `u16`; cada nó guarda offset `u32` e comprimento
`u16`. Não há `String` por nó. UTF-16 é o formato nativo do NTFS e do Win32, então no Windows a
ingestão é uma cópia; no Linux os nomes são convertidos na ingestão (custo só de desenvolvimento).

## Consequences

- O NTFS aceita nomes com surrogates soltos, que não são UTF-16 válido: por isso guardamos `u16`
  brutos e só convertemos para texto, com perdas, na borda (exportação, terminal).
- Como representar no JSON um nome que não é UTF-16 válido, sem perder a identidade do Item,
  fica para a D14 (Fase 3).
