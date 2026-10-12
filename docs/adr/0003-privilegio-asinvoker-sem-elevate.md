---
status: accepted
---

# Privilégio: asInvoker, sem auto-elevação na v1

O binário roda com o manifesto `asInvoker`. Ele detecta se está elevado e se o Volume é NTFS:
se sim, usa a Leitura da MFT; se não, usa a Varredura de diretórios e avisa em stderr.
`--engine auto|mft|dir` força o Motor de varredura; pedir `mft` sem privilégio é erro explícito,
não queda silenciosa.

## Considered Options

- `requireAdministrator`: obriga UAC até para uso trivial.
- `asInvoker` com `--elevate` (relançar via `ShellExecuteW("runas")`): numa CLI, o processo
  elevado abre outro console, então a saída (inclusive `--json`) não volta ao terminal original e
  pipes quebram. Resolver isso exige comunicação entre processos. Rejeitado na v1; reavaliar na
  Fase 6, onde a interface Tauri pode elevar de outra forma.
