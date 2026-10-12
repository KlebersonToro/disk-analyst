---
status: accepted
---

# Testes de NTFS/MFT em três níveis, sem ferramentas extras

O shell do WSL executa `.exe` pelo interop, mas sem privilégio de administrador, e queremos um
protótipo sem pacotes extras. Por isso os testes se dividem em: (1) o parser da MFT testado em
Linux sobre registros sintéticos montados em código e testes por propriedade (`proptest`);
(2) a Varredura de diretórios testada nos dois alvos (`cargo test` e
`cargo test --target x86_64-pc-windows-gnu`, executado pelo interop); (3) testes sobre volume
real marcados `#[ignore = "requer admin + NTFS"]`, rodados à mão num terminal Windows elevado com
o comando documentado.

## Considered Options

- Imagens NTFS geradas com `mkntfs`/`ntfs-3g` como fixtures: dariam um oráculo realista sem admin,
  mas exigem instalar `ntfs-3g` e `sudo`. Adiado; reavaliar no início da Fase 2 se os registros
  sintéticos não cobrirem os casos (listas de atributos, `$MFT` fragmentado).
- VM dedicada: configuração pesada demais para um projeto de estudo.

## Consequences

- O fuzzing exigido pelo Princípio III da constituição precisa de toolchain nightly e
  `cargo-fuzz`; a instalação acontece no início da Fase 2, não antes.
