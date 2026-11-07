---
okf_version: "1.0"
type: CrossAudit
title: "Cross-audit — D07 riscos aceitos no F2.5"
description: "Fidelidade entre o aceite escrito e o comportamento executado do backend Python de dependências."
plan_id: 2026-11-05-d07-risco-aceito
timestamp: "2026-11-05T15:05:00-03:00"
links:
  - /RETOMADA.md
---

# Cross-audit — D07 riscos aceitos no F2.5

## Veredito

**FULFILLS após correção dos três achados da primeira rodada independente.** A reauditoria confirmou as correções e não deixou findings abertos.

## 1. Mapa e propósito

O backend F2.5 resolve `python_root`, lê a auditoria OSV em cache ou ao vivo e classifica o relatório `pip-audit`. A mudança adiciona uma camada de aceite temporal entre o relatório e o score, sem alterar a obtenção da auditoria nem o F4.5.

| Símbolo | Contrato auditado | Evidência estática |
|---|---|---|
| `AcceptedRisk` | Guarda ids/aliases, expiração e motivo | `python_deps.rs:119-125` |
| `load_accepted_risks` | Lê somente `[[aceito]]`; descarta entradas sem id ou data | `python_deps.rs:156-190` |
| `open_dependencies` | Aceite vigente sai do conjunto aberto, mas permanece na evidência; vencido permanece aberto | `python_deps.rs:243-285` |
| `score_pip_audit_with_risks` | Score deriva somente das vulnerabilidades abertas | `python_deps.rs:293-363` |
| `score_report` | Sem riscos carregados, delega ao wrapper anterior | `python_deps.rs:397-403` |
| `f2_5` | Usa o mesmo filtro na cache e no live audit, com data derivada de `now_secs` | `python_deps.rs:405-429` e rota live subsequente |

## 2. Controles e bordas

Os testes cobrem: vigente ignorado e listado; alias; vencido reprovado com id/data; último dia inclusivo; id errado sem supressão; arquivo ausente com veredito anterior; parser inerte para tabelas não aceitas ou sem data. As mutações de id e data invertem o veredito, portanto o controle não é apenas um caso limpo.

O RED foi medido antes da implementação: 11 erros de compilação por ausência da nova API. O GREEN não é uma reexecução de testes que já passavam.

## 3. Dívida e harmonia

Não foi introduzido `allow(dead_code)`, `TODO`, `FIXME` ou supressão de lint. O wrapper antigo continua com consumidor real em `score_report`, preservando compatibilidade e evitando código morto. `cargo fmt --check` e `git diff --check` não apresentaram falhas. Nenhum arquivo fora de `python_deps.rs` e do bundle D07 foi modificado nesta worktree.

## 4. Provas executadas

| Prova | Resultado observado |
|---|---|
| `cargo test --manifest-path crates/touring-quality/Cargo.toml` na `cargo-build.slice` | 518 testes unitários + 27 de integração/documentação; 0 falhas |
| `cargo clippy --manifest-path crates/touring-quality/Cargo.toml --all-targets -- -D warnings` | 0 avisos |
| `target/debug/touring-quality check --gate F2.5 --target /home/gabrielgadea/projects/analise` | 1.000 (Diamond); 470 dependências; 0 vulnerabilidades abertas |
| Evidência do risco vigente | `diskcache@5.6.3 PYSEC-2026-2447 accepted risk until 2026-12-05`, com motivo |
| Procedência da auditoria | Cache da mesma `uv.lock`, 18 h; sem refresh de rede |
| `touring doctor -j` | 9/9 componentes com status `ok` |
| `touring decompose validate task_1793900961378191950` | Comando exit 0; log em `diagnostics/dag_validate_final.log` |

## 5. Limites

A prova real usou a cache OSV válida, não uma nova consulta à rede. A medição de órfãos do projeto `analise` é um snapshot de contexto, não uma linha de base atribuível ao D07; o diff desta frente não cria símbolos públicos. A suíte executada é a do crate `touring-quality`, não a de todos os crates do workspace.

## 6. Reconciliação independente

A primeira rodada do auditor `d07-audit-final` devolveu **VIOLATES** com três achados:

1. **Alta** — `[em_onda]` não encerrava o `[[aceito]]` anterior e podia estender sua data. Correção: qualquer cabeçalho iniciado por `[` encerra a tabela corrente; somente `[[aceito]]` inicia outra.
2. **Média** — aceite sem `controle`/`descricao` produzia evidência sem motivo. Correção: o parser descarta entradas com `reason.trim()` vazio; sem motivo, nada é ignorado.
3. **Média** — a data UTC podia antecipar o vencimento em até 3 h. Correção: `today_from_secs` aplica o offset fixo de São Paulo (UTC−03:00, sem horário de verão desde 2019) antes de obter a data.

Os três testes novos reprovaram o código anterior: **16 passed, 3 failed**, exatamente nos casos semeados (`diagnostics/red_auditoria_independente.log`). Depois das correções: **19 passed, 0 failed** (`diagnostics/green_auditoria_independente.log`). A reauditoria estática focada devolveu **FULFILLS, findings=[]**, confirmando código e logs; não executou uma nova suíte. A suíte completa e os demais gates foram então reexecutados pelo orquestrador com exit 0.
