---
status: accepted
---

# Alvo de compilação Windows: x86_64-pc-windows-gnu

Compilamos o binário Windows com `x86_64-pc-windows-gnu` a partir do WSL, porque ele já funciona
sem instalar nada além do que existe: o `.exe` gerado roda direto no Windows 11 e importa só DLLs
do sistema (`KERNEL32`, `ntdll`, `msvcrt`…), sem runtime do MinGW. O custo é que o binário traz
símbolos DWARF em vez de PDB, o que enfraquece o perfilamento com ETW/WPA no caminho Windows
(Leitura da MFT, Win32).

## Considered Options

- `x86_64-pc-windows-msvc` via `cargo-xwin`: gera PDB, mas exige baixar o SDK do Windows e trocar
  o linker. Fica como evolução.

## Consequences

- Gatilho de revisão: quando um perfil do caminho Windows for necessário e o DWARF não bastar,
  avaliar MSVC num novo ADR que substitua este.
- `cargo-flamegraph` no WSL perfila só o build Linux (Varredura de diretórios via `std::fs`).
