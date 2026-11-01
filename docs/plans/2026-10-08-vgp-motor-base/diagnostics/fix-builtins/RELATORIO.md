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
