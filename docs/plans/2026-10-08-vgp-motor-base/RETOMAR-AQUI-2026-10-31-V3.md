# RETOMADA — V-3 pós-J14 (31/10/2026)

B = `/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base`.

## Estado persistido

- J14 concluído e aceito pela touring-00. Commits: `443564a3`, `1256b7f0` (instrumento), `b4ef21c8` (dados/selo), `0358b517` (J14), `8b717222` (gates). Relatórios em `diagnostics/j14/RELATORIO.md` e `diagnostics/holdout-20261031/RELATORIO-HELDOUT-20261031B.md`.
- Corpus J14: `~/.claude/touring/vgp/corpus_holdout_disjunto_20261031b`, selo `e8d8016f45bff2c5`, 600 arquivo + 400 inline, 30.4.101 NÃO PRONTO. Frio p95 1.014,9 ms; quente consolidado 252,48 ms.
- Pré-registro do novo critério quente já escrito: `B/PREREGISTRO-V3-QUENTE-2026-10-31.md`. Vale do próximo held-out; não reinterpreta J14.
- Nova DAG: `task_1793487928776429345` (criada, sem subtasks ainda).
- Diretório novo do conserto: `B/diagnostics/fix-builtins/`. Fixtures SINTÉTICAS `synthetic/{exit,exec,return,trap}.py`; nunca abrir conteúdo do corpus gasto.
- Sonda atual `sonda_builtins.py` lançada sob `hw-slot --class rust`, background `b96t0i0y4`, saída `sonda_antes.json`. Objetivo: confirmar se `exit()` é negado por não constar em `PYTHON_BUILTINS`.

## Próximos passos

1. Ler `diagnostics/fix-builtins/sonda_antes.json`. Código provável: `crates/touring-code/src/vgp/lang/python/stdlib_model.rs:9-10` e `scope.rs:220,231,247`.
2. Diagnosticar `exit` com a fixture sintética. Escrever testes RED para `exit` e demais builtins, sem conteúdo do corpus; correção por TDD + mutante morto. `exec` é builtin Python; `return` é keyword; `trap` não deve virar allowlist insegura.
3. Commit separado "instrumento/motor", relatando todos os arquivos tocados para a touring-65 integrar (`touring-wt-intadapt`).
4. Construir/selar `corpus_holdout_disjunto_20261101` com semente nova, excluindo também o parcial 20261031 e o 20261031b. NÃO julgar até aviso do release 30.4.102.
5. Enviar relatório à touring-00 (`uds:/run/user/1000/cc-socks/58806.sock`).

## Restrições

- Pré-registro quente já existe; nenhuma nova medição foi feita antes dele.
- `hw-slot` para todo motor/job pesado; nunca kill/pkill Touring; nada de corpus no git; commits só dos paths da tarefa.
- O `20261031b` está gasto: não abrir seu conteúdo para diagnosticar. Usar apenas fixtures sintéticas.

## Atualização após o motor

- `exit`/`quit` corrigido em `stdlib_model.rs`; `exec`/`return` sem falso deny e `trap` preservado como deny verdadeiro. RED 1 falha → GREEN; mutante rc=101 morto; suíte 978, clippy e fmt verdes. Commit `6f72b473`.
- Arquivos Rust tocados para a fusão: `crates/touring-code/src/vgp/lang/python/{stdlib_model.rs,tests.rs}`. Relatório: `diagnostics/fix-builtins/RELATORIO.md`.
- Held-out 20261101 preparado em `diagnostics/holdout-20261101/`, com semente 20261101 e exclusões adicionais do parcial 20261031 e do 20261031b. Construção em background `bmzyutsob` após chmod do runner compartilhado; primeira tentativa rc=126 foi só permissão.
- Depois da construção: `bash diagnostics/holdout-20261101/selar_e_cruzar_20261101.sh`. NÃO julgar o J15 até aviso do release 30.4.102.

## Held-out 20261101 selado

- Construção rc 0 (1.236,7 s): 600 arquivo + 400 inline, 1.000 entradas, 873 overlays, 0 incompletas, 0 `ts=""`, 0 após o corte.
- Selo `6c228591f1f404f5`, 2.478 arquivos, 16.400.909 bytes, conferido intacto. Sobreposição zero nas quatro chaves contra vivo, temporal e todos os anteriores, incluindo parcial 20261031 e 20261031b. Controle positivo ativo.
- Relatório: `diagnostics/holdout-20261101/RELATORIO.md`.
- **Estado atual: aguardando o release 30.4.102. NÃO rodar J15 nem abrir o corpus para julgamento.**
