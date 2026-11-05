---
okf_version: "1.0"
type: LoopBundle
title: "RETOMADA — D07 riscos aceitos no F2.5"
description: "Aceite escrito antes do código para ignorar somente vulnerabilidades vigentes, com evidência explícita."
plan_id: 2026-11-05-d07-risco-aceito
timestamp: "2026-11-05T14:45:00-03:00"
links:
  - /diagnostics/outer_strategy.log
---

# RETOMADA — D07 riscos aceitos no F2.5

Worktree `/home/gabrielgadea/projects/touring-d07-risco`; branch `d07-risco-aceito/2026-11-05`; base `be328f6b`. DAG canônica `task_1793900961378191950`, subtask D07 reivindicada por `analise-36`. Sem push, sem `update-touring`, sem instalação de toolchain e sem kill em processo Touring.

## Critério de aceite — escrito antes do código

1. O backend F2.5 lê `scripts/utils/gate_risco_aceito.toml` no `python_root` do projeto alvo, usando somente tabelas `[[aceito]]`; arquivo ausente preserva byte-por-byte o comportamento anterior.
2. Uma vulnerabilidade é ignorada somente quando o `id` ou um alias declarado casa E `hoje <= expira`. O aceite vale inclusive no dia da expiração; no dia seguinte reprova.
3. Toda entrada vigente ignorada aparece na evidência com pacote, id, data e o controle/motivo da aceitação; o número não desaparece silenciosamente. Toda entrada vencida aparece com id e data e mantém score 0.
4. Controles obrigatórios: vigente ignora e lista; vencida reprova; ausente igual a hoje; id errado não ignora nada; alias casa; `[[em_onda]]` e entrada sem data não ignoram. Mutação da data/id deve inverter o veredito.
5. `cargo test -p touring-quality` verde e `cargo clippy -p touring-quality --all-targets -- -D warnings` zero, na `cargo-build.slice`.
6. O binário da worktree, ao medir `/home/gabrielgadea/projects/analise`, passa F2.5 e lista `PYSEC-2026-2447`/`diskcache` como risco aceito até 2026-12-05, sem refresh de rede se a cache atual for válida.
7. RETOMADA, cross-audit, memória e UM relatório final; commits somente de `python_deps.rs` e deste bundle.

## Estado atual

OUTER exit 0. Fonte `python_deps.rs` lida: `score_pip_audit` é pura, `f2_5` usa cache/live OSV, `AuditConfig.now_secs` já permite tempo determinístico e `chrono` já é dependência. Não há convenção prévia em `.touring/touring.toml`; adotar o arquivo existente no projeto alvo é a menor mudança.

RED comprovado: 11 erros por API ausente. Implementação GREEN: 5 testes de vigência/alias/vencimento/fronteira/id errado, mais parser e ausência de arquivo verdes. A auditoria independente encontrou 3 bordas não cobertas — tabela simples sobrescrevendo aceite, motivo ausente e virada UTC; os 3 testes de regressão fizeram RED (**16 passed/3 failed**) e depois GREEN (**19 passed/0 failed**). Reauditoria final: **FULFILLS, findings=[]**. Suíte completa: **518 unitários + 27 integração/doc tests, 0 failed**. Clippy: **0 warnings**. O binário da worktree mediu o `analise` em **Diamond 1.000**, com `diskcache@5.6.3 PYSEC-2026-2447 accepted risk until 2026-12-05` e o controle completo na evidência; usou a cache OSV de 18 h, sem refresh de rede. O verificador completo `audit-plan-completion.sh` terminou exit 0.

## Próximo passo

Commitar `python_deps.rs` e o bundle, fechar D07 na DAG e enviar UM relatório final a `analise-67`. As três lições já foram gravadas no grafo com leitura de volta íntegra. D10 permanece depois do D07.
