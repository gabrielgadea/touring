---
type: Report
title: "Conserto do falso deny de `exit` — builtins do site e controles"
plan_id: 2026-10-08-vgp-motor-base
subtask: task_1793487928776429345::MOTOR
okf_version: 1
timestamp: 2026-10-31
tags: ["#kind:report", "#domain:vgp", "#status:verified"]
---

# Conserto do falso deny de `exit`

## 1. Causa

A sonda sintética (`sonda_antes.json`) mostrou que o binário atual negava `exit()` com
`nome=exit`, enquanto `exec` e `return` passavam e `trap()` era corretamente negado.
`crates/touring-code/src/vgp/lang/python/stdlib_model.rs` continha `exec` em
`PYTHON_BUILTINS`, mas não `exit` nem `quit`; `scope.rs:220,231,247` usa essa tabela ao
resolver nomes. O runtime (`python3 -I`) confirma `exit` e `quit` como `Quitter`, injetados
pela inicialização normal de `site`.

O conteúdo do corpus gasto não foi aberto. Todas as provas usam fixtures sintéticas.

## 2. Correção

- `stdlib_model.rs`: adicionados `exit` e `quit` como helpers de `site`.
- `tests.rs`: três regressões:
  - `exit` e `quit` não podem gerar deny;
  - `exec` e `return` continuam sem falso deny;
  - `trap()` continua deny, como controle positivo de nome inexistente.

Nenhum outro símbolo foi adicionado à allowlist.

## 3. Evidência

| Prova | Resultado | Arquivo |
|---|---|---|
| Sonda antes | `exit`: deny; `exec`/`return`: allow; `trap`: deny | `sonda_antes.json` |
| RED | 1 falha, `nome=exit`, 977 filtrados | `red_exit.log` |
| GREEN `exit`/`quit` | passa | `green_builtins.log` |
| GREEN `exec`/`return` | passa | `green_exec_return.log` |
| GREEN `trap` | deny correto preservado | `green_trap.log` |
| Mutante sem `exit`/`quit` | **morto por asserção, rc=101**; green após restaurar rc=0 | `mutante_exit.{log,json}`, `green_depois_mutante.log` |
| Suíte completa | **978 testes, 0 falhas** | `test_crate.log` |
| Clippy | `cargo clippy -p touring-code --all-targets -- -D warnings`, rc=0 | `clippy.log` |
| Format | `cargo fmt --check -p touring-code`, rc=0 | `fmt.log` |

A primeira execução de `cargo fmt --check` reprovou apenas a forma do `assert_eq!` de `trap`;
a linha foi formatada e o gate repetido com sucesso.

## 4. Arquivos tocados para a fusão (touring-65)

Somente estes arquivos Rust foram alterados no motor:

1. `crates/touring-code/src/vgp/lang/python/stdlib_model.rs` (+3 linhas);
2. `crates/touring-code/src/vgp/lang/python/tests.rs` (+47 linhas, depois −3 da formatação do assert).

Os demais artefatos são evidências em `diagnostics/fix-builtins/`. O trabalho foi feito na
branch principal, separado do worktree `touring-wt-intadapt`.

## 5. Prova comportamental do 30.4.102 instalado

Binário congelado: `~/.claude/touring/vgp/binarios/30.4.102/touring`, sha256
`404106ccdb32edb4329036291cb8cb55660adbbc9c7a8fc835237a9f148e3ef7` (versão 30.4.102,
git `9ab1723f72160e75fc08ab9a34244b80b5b04362`). Nenhum daemon foi reiniciado.

| Caminho | `2106f0ebf279` | `exit()` sintético | `quit()` sintético |
|---|---|---|---|
| CLI instalada, camadas 1+2 | `allow`, `findings=[]` | `allow`, `findings=[]` | `allow`, `findings=[]` |
| Daemon global pelo hook | `deny` | `allow` | `allow` |

O run quente teve 3/3 medições com sombra e camada 2, **6/6 rotas daemon**, zero timeouts e
p95 54,10 ms (`probe_30_4_102_hot*.json`). O caso real diverge entre CLI e hook porque o hook
não aceita `--overlay` nem `--disk-file`; a divergência fica registrada e não é apresentada como
equivalência semântica. A condição pré-anunciada para liberar o J15 era `exit` sem deny, e ela
foi satisfeita nos dois caminhos.

## 6. Regressões dos instrumentos e qualidade

- A preparação quente passou a resolver snapshots relativos ao corpus e absolutos das fixtures;
  o teste `test_main_prepara_mini_corpus_sem_apagar_o_caso` cobre o fluxo inteiro com dados
  sintéticos, sem abrir conteúdo do corpus.
- Os testes pareados usam imports estáticos e cobrem `main`, agregação de entradas, construção
  da chamada com overlays, restauração byte a byte do mutante e as cinco fixtures.
- Suíte deste escopo mais o consolidador J15: **19 passed**.
- `quality_final.json`: **Diamond 0,9784039**, 12 arquivos, **0 blockers**; F3.1 0,99462366.
  Uma medição intermediária com `--workspace` (4.154 arquivos) foi descartada por estar fora do
  escopo; o valor final foi medido sem essa flag.
