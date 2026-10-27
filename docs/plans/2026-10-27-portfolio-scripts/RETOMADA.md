# RETOMADA — Portfólio de Scripts (parada de cota 27/10/2026 ~16:40)

> Parada obrigatória por cota semanal (82% > teto 78%). Estado completo para retomada.

## Feito (tudo commitado e deployado/entregue)

| Item | Hash/Local | Estado |
|---|---|---|
| Diagnóstico (41.923 scripts minerados, 2.420 transcripts) | `diagnostico-portfolio-scripts.md` (commit b19d3353) | ✅ entregue |
| Análise aproveitamento CLI/masters | `analise-aproveitamento-touring-cli.md` (b19d3353) | ✅ |
| SEG-3 sandbox lê ~/.claude/projects+hooks | b19d3353, em produção (30.4.98) | ✅ deployado |
| P0 hook `script_portfolio_gate.py` (sombra, settings.json) | `~/.claude/skills/Touring/scripts/hooks/` + espelho client/ (sync pela touring-44) | ✅ ativo |
| P1 `~/.claude/scripts-portfolio/` (README+MANIFEST+3 canônicos) | fora do repo, em disco; 3 snippets via sync-tags | ✅ |
| P2-v2 gate de semelhança (SimHash τ=10, bloqueia 2ª inline) | adbeea9a, em produção (30.4.98 = adbeea9a) | ✅ deployado |
| C1b F2.5 (wasmtime 48.0.7, z3 0.21.1, candle 0.9.2) | `74d7e966` branch `feat/c1b-cves` (worktree `../touring-wt-c1b`) | ✅ verde, **aguardando merge** |

## Pendente (ordem)

1. **Merge de feat/c1b-cves** no tree principal pela touring-44 (aditivo) + 3 ignores do zip no deny.toml (decisão do Gabriel: aceitar risco documentado) + `propagate-release 30.4.99` (resolve o rótulo duplo do 30.4.98). Tudo parado até Gabriel liberar a cota.
2. **Residual zip** (RUSTSEC-2026-0367/0368/0369, patched só ≥9.0.0, nenhum candle aceita): risco aceito; reavaliar quando sair candle com zip 9.
3. **Medição do portfólio**: `python3 ~/.claude/skills/Touring/scripts/hooks/script_portfolio_gate.py --stats` após 1 semana (critério de promoção do hook à la prose gate); `touring kpi -j` → code_mode_reuse; os `repeat_blocked` jornalizados medem o gate P2-v2.
4. **Calibração fina do τ**: observado em produção que rename com mudança de comprimento (media→media_final) passa de 10 bits — avaliar τ=12-14 com os repeat_blocked acumulados.
5. **F2.4** apareceu na última leitura do gate de convergência (não existia antes) — verificar pós-merge se é oscilação do tree ou real.

## Coordenação vigente

- touring-44: merge + propagate 30.4.99 são dela; sem cargo/update-touring meu sem aviso cruzado.
- Worktree `../touring-wt-c1b` pode ser removido APÓS o merge (não antes — o 74d7e966 mora lá).
