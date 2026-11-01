# RETOMADA 01/11/2026 — meta "resolva tudo de pendencias-consolidadas.md" (orquestrador touring-d1)

Estado canônico: `docs/plans/2026-10-29-pendencias-consolidadas.md` (tabelas "Feito", decisões 31/10 e 01/11,
"Achados corrigidos em 01/11"). Este arquivo só guarda o que está EM VOO.

## Em voo (01/11 ~17h)

| Frente | Quem | Estado / próximo passo |
|---|---|---|
| D-7b dedup `scripts/` | subagente sonnet, worktree `/home/gabrielgadea/projects/touring-wt-d7b` (branch `feat/2026-11-01-d7b-dedup`, base `d13fd22d`) | F1.3 1,000 (Type-1 2,8%, combinado 9,9%) no commit `1f2bed81`; comparando 38 falhas + 2 erros da suíte inteira de `scripts/` com a linha de base. Merge só se as falhas forem pré-existentes. Depois: `git worktree remove` + `cargo clean` lá |
| Publicar a `main` (Gabriel aprovou "Atualizar a main") | touring-d1 | `origin/main` = `24e901a7` (1 commit CI fora do HEAD); local 451 commits à frente de `origin/feat/...`. Pré-voo: maior blob 1,1 MB; segredos = fixtures sintéticas; 151 `.db` só como DELEÇÃO (já públicos no histórico antigo — exposição anterior, avisar Gabriel). Auditoria de conteúdo dos arquivos ADICIONADOS (65 jsonl/corpus) em curso por subagente; push SÓ com veredito PUBLICAR. Plano: merge `origin/main` no HEAD (ou main ← HEAD), push sem force de `main` e da branch |
| F2.1 barra fixture de teste TS (`href="javascript:"`) | touring-d1 | achado da analise-2a; `retain_exploitable` em `crates/touring-analysis/src/quality/security.rs:143` sem exclusão de caminho de teste para TS/JS |
| Orçamento da camada 2 na CLI (`TOURING_VGP_L2_BUDGET_MS`) | touring-45 (Kimi) | iniciado após `d1fae79e` |
| V-8 re-rotulação js/tsx/ts + adjudicador html (D-8) | touring-a4 (MiniMax) | saída em `~/.claude/touring/vgp/oss_rotulado_20261101/` |
| J16 | touring-45 | held-out `20261101c` SELADO (`1a84e197`, sha_tree `b5a819f38c57d227`, 600+371); roda depois do release de 04/11 |

## Datados

- 03/11: M-1, M-2, 3.5, 3.6. 04/11 00:00: ÚNICA janela de release do AB34 (`scripts/propagate-release.sh`,
  depois `python3 docs/plans/2026-10-31-ab34-compactador-eviccao/ab34_arm.py arm --yes`). 08/11 00:10: AB34 termina
  (`analyze --json`), depois 3.11 (A3). Gabriel: "Manter até 08/11".
- AB34 armado: nada de `update-touring`/`daemon-ctl`/settings/hooks/plugins; nada de `cargo build --release` no
  `target/` principal.

## Lições do dia (memória)
`licao-holdout-dependencia-atual-2026-11-01`, `licao-isolation-worktree-usa-origin-main`, `ab34-armado-2026-10-31`.
