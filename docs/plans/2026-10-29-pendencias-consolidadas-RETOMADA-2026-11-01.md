# RETOMADA 01/11/2026 — meta "resolva tudo de pendencias-consolidadas.md" (orquestrador touring-d1)

Estado canônico: `docs/plans/2026-10-29-pendencias-consolidadas.md` (tabelas "Feito", decisões 31/10 e 01/11,
"Achados corrigidos em 01/11"). Este arquivo só guarda o que está EM VOO.

## Em voo (01/11 ~17h)

| Frente | Quem | Estado / próximo passo |
|---|---|---|
| D-7b dedup `scripts/` | — | **FEITO** `4c85f572`; worktrees removidos. Suíte de `scripts/` 0 falhas fora do D8 (datado 08/11, patch pronto) e do `test_help_never_executes` (verde após o release de 04/11) |
| Publicar a `main` + purge remoto dos `.db` | touring-d1 | **FEITO** (`487dcf6a`→`40807a81`); resta só `refs/pull/*/head` — pedido ao suporte do GitHub (Gabriel) |
| F2.1 barra fixture de teste TS | touring-d1 | **FEITO** (`is_js_test_file_name` + `/__tests__/`); vai no release de 04/11 |
| Orçamento da camada 2 na CLI (`TOURING_VGP_L2_BUDGET_MS`) | touring-45 (Kimi) | iniciado após `d1fae79e` |
| V-8 re-rotulação js/tsx/ts + adjudicador html (D-8) | touring-a4 (MiniMax) | saída em `~/.claude/touring/vgp/oss_rotulado_20261101/` |
| J16 | touring-45 | held-out `20261101c` SELADO (`1a84e197`, sha_tree `b5a819f38c57d227`, 600+371); roda depois do release de 04/11 |

## Datados

- **08/11 depois de 00:10**: aplicar `docs/plans/2026-10-29-pendencias-d8-secao-code-mode.patch` (hook vivo).

- 03/11: M-1, M-2, 3.5, 3.6. 04/11 00:00: ÚNICA janela de release do AB34 (`scripts/propagate-release.sh`,
  depois `python3 docs/plans/2026-10-31-ab34-compactador-eviccao/ab34_arm.py arm --yes`). 08/11 00:10: AB34 termina
  (`analyze --json`), depois 3.11 (A3). Gabriel: "Manter até 08/11".
- AB34 armado: nada de `update-touring`/`daemon-ctl`/settings/hooks/plugins; nada de `cargo build --release` no
  `target/` principal.

## Lições do dia (memória)
`licao-holdout-dependencia-atual-2026-11-01`, `licao-isolation-worktree-usa-origin-main`, `ab34-armado-2026-10-31`.
