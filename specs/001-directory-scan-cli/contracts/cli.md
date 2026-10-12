# Contrato da CLI: `disk-analyst scan`

```
disk-analyst scan <CAMINHO> [--top N] [--json] [--progress] [--engine auto|dir|mft] [--stats]
disk-analyst --help | --version
```

| Opção | Padrão | Efeito |
|---|---|---|
| `<CAMINHO>` | obrigatório | Pasta, raiz de Volume ou arquivo. Se for link, a raiz é resolvida |
| `--top N` | 20 | Tamanho dos rankings; inteiro ≥ 1 |
| `--json` | desligado | Escreve em stdout só o documento de [scan-report-v1.schema.json](scan-report-v1.schema.json) |
| `--progress` | desligado | Progresso em stderr (Itens, arquivos/s), ≥ 1 atualização/s; ignorado com `--json` |
| `--engine` | `auto` | `auto` e `dir` usam a Varredura de diretórios. `mft` termina com erro de uso: "Leitura da MFT ainda não disponível" (ADR-0003). O aviso "rode como administrador para o modo MFT" do ADR-0003 só passa a existir na Fase 2, com a Leitura da MFT |
| `--stats` | desligado | Ao final, escreve em stderr: pico de memória, Itens, bytes por Item da arena (R11) |

## Saídas

- **stdout**: relatório de texto em português (padrão) ou documento JSON (`--json`). Nada mais.
- **stderr**: avisos, erros, progresso e `--stats`. Mensagens em português (FR-019).

## Relatório de texto (forma, não literal)

```
Disk Analyst — C:\                                   (PARCIAL, se cancelado)
Tamanho alocado: 182,4 GiB   Tamanho lógico: 190,1 GiB
Arquivos: 1.234.567   Pastas: 234.567   Inacessíveis: 42   Na nuvem: 310   Links não seguidos: 18
Duração: 9,8 s

Maiores pastas (tamanho próprio)
  alocado próprio   alocado total   caminho
  ...
Maiores arquivos
  alocado   lógico   caminho   [na nuvem]
  ...
```

Unidades binárias (KiB, MiB, GiB), separador decimal vírgula. Nomes inválidos com U+FFFD.

## Códigos de saída (FR-006)

| Código | Situação |
|---|---|
| 0 | Varredura concluída (mesmo com Itens inacessíveis) |
| 2 | Erro de uso: opção inválida, caminho inexistente ou raiz ilegível, `--engine mft` |
| 130 | Cancelada por Ctrl+C (relatório parcial emitido no primeiro sinal; nenhum no segundo) |
| 1 | Erro inesperado de E/S ao escrever a saída |
