# Disk Analyst

Analisador de uso de disco para Windows: descobre o que ocupa espaço num volume e ajuda o usuário
a decidir o que liberar, sem nunca apagar nada por conta própria.

## Varredura

**Volume**:
Uma unidade montada com sistema de arquivos (por exemplo `C:`), alvo de uma varredura.
_Evitar_: disco, partição, drive

**Motor de varredura**:
O mecanismo que percorre um volume ou pasta e produz a árvore de itens: Varredura de diretórios
ou Leitura da MFT.
_Evitar_: scanner, engine, modo

**Varredura de diretórios**:
Motor que percorre as pastas uma a uma pelas APIs do sistema operacional; funciona em qualquer
sistema de arquivos e sem privilégio de administrador.
_Evitar_: varredura comum, scan normal, modo lento

**Leitura da MFT**:
Motor que lê diretamente a tabela de arquivos de um volume NTFS; exige administrador.
_Evitar_: varredura MFT, modo rápido, modo NTFS

**Item**:
Um arquivo ou uma pasta encontrado na varredura.
_Evitar_: entrada, nó, objeto

**Item inacessível**:
Item que a varredura encontrou mas não pôde ler por falta de permissão; é contado à parte e não
interrompe a varredura.
_Evitar_: erro, negado, item com falha

**Link não seguido**:
Item que aponta para outro lugar (junction, symlink, ponto de montagem) e é registrado sem que a
varredura percorra o destino; ocupa zero no relatório.
_Evitar_: atalho, link quebrado, reparse point

**Arquivo na nuvem**:
Arquivo sincronizado (por exemplo OneDrive sob demanda) cujo conteúdo não está armazenado
localmente; ocupa zero de Tamanho alocado.
_Evitar_: placeholder, arquivo online, arquivo virtual

## Tamanhos

**Tamanho lógico**:
Quantidade de bytes de conteúdo de um arquivo, como o usuário o vê.
_Evitar_: tamanho do arquivo, tamanho real

**Tamanho alocado**:
Espaço que o arquivo de fato ocupa no volume; é o que se libera ao excluí-lo. É o tamanho
exibido por padrão.
_Evitar_: tamanho em disco, espaço ocupado

**Tamanho próprio**:
Soma dos tamanhos dos arquivos que estão diretamente dentro de uma pasta, sem contar subpastas;
usado para ranquear pastas.
_Evitar_: tamanho local, tamanho direto

**Tamanho agregado**:
Soma dos tamanhos de tudo o que está abaixo de uma pasta, em qualquer profundidade.
_Evitar_: tamanho total da pasta, tamanho recursivo

## Decisão de limpeza

**Nível de risco**:
Classificação de um item quanto à segurança de removê-lo: Seguro, Revisar ou Protegido, sempre
acompanhada de um motivo.
_Evitar_: perigo, severidade, cor

**Item protegido**:
Item com Nível de risco Protegido; nunca é sugerido para remoção, por ninguém.
_Evitar_: item de sistema, item bloqueado

**Plano de limpeza**:
Lista de itens propostos para remoção, cada um com motivo, que só o usuário pode aplicar.
_Evitar_: sugestão, lista de exclusão, recomendação

**Snapshot**:
Registro salvo do resultado de uma varredura, usado para comparar o crescimento ao longo do tempo.
_Evitar_: histórico, captura, backup
