# RETOMADA — VGP L2 budget + held-out 20261101c (01/11/2026)

## Estado fechado

- J15: `6fec8958`; perícia do falso deny: `8014e346`.
- Reconstrutor Git local: `16192943` (RED 3→GREEN 4; 56 testes; 23/23 mutantes).
- Held-out `20261101c`: `35d6e1c7`, selo `b5a819f38c57d227`; 971 entradas, 1.439 overlays, zero incompletos e sobreposição zero. **Não julgar antes do J16, após o release de 04/11.**
- Orçamento no produto (env, flag, veredito, sombra e juiz): `04aee323`.
- Retroativo final pelo produto: `ded923cf` — 119 alteradas, 109 comparadas; **104 mérito×mérito = 103 estáveis + 1 deny→não-deny**, zero timeouts e 5 `unknown_outro` por ausência de pyright. Controle `10607309d143` passa por mérito com 30.000 ms nos dois lados.
- Patches `wasmtime` 36.0.19→36.0.20 e 48.0.7→48.0.8 exigidos pelo gate F2.5: `170f1759`.
- DAG `task_1793550203045418497`: `finalized`, subtarefa `orcamento-l2` completed, qualidade 0,95.

## Implementação entregue

- `crates/touring-pyright/src/pyright.rs`: `L2_BUDGET_DEFAULT_MS=1000`, `L2_BUDGET_ENV=TOURING_VGP_L2_BUDGET_MS`, resolvedores `l2_budget_ms_com`, `l2_budget_ms`, `l2_budget`; `discover` usa o resolvedor.
- `crates/touring-pyright/src/pool.rs`: `Config.l2_budget`; `verificar` recebe o valor; `checar_lang_com_orcamento` público.
- `crates/touring-code/src/vgp/{mod.rs,sombra.rs}`: `l2_budget_ms` propagado ao veredito e à sombra.
- `crates/touring-server/src/cli/vgp.rs`: flag `--l2-budget-ms`, precedência flag→env→default e valor efetivo no veredito.
- `crates/touring-hook-handlers/src/hooks/vgp_l2.rs`: valor efetivo na sombra.
- `scripts/juiz_motor_vgp.py`: flag repassada ao motor; teste de argv.
- `docs/vgp-motor.md`: env, flag e campo documentados.
- Retroativo sem instrumento externo: `diagnosticar_dependencias.py` chama `target/debug/touring` com orçamento de 30 s.

## Evidência final

- `cargo fmt --all -- --check` e `git diff --check`: verdes.
- `touring doctor -j`: todos os componentes `ok`, binário 30.4.102.
- Pytest do retroativo + juiz: **11 passed in 22.35s**.
- Suítes dos quatro crates (`touring-pyright`, `touring-code`, `touring-server`, `touring-hook-handlers` com `pre-hooks,post-hooks`), repetidas após o patch do lockfile: exit 0, 69 linhas `test result`, zero falhas.
- Clippy dos quatro crates com `--all-targets -D warnings`, repetido após o lockfile: exit 0.
- F2.5 no bundle: 1.000, Diamond, zero blockers.
- Convergência em `docs/plans/2026-10-08-vgp-motor-base`: **exit 0, CONVERGED**, Diamond 0,9506476, DAG 1/1, nenhum P0.
- Nenhum deploy, restart de daemon ou build release; held-out não julgado.

## Limites fora do escopo convergido

- A tentativa de convergência com `--scope` no workspace inteiro também acusou F2.1 e F2.4 em fixtures de teste fora desta tarefa; esses blockers não foram alterados.
- O `cargo audit` direto listou outros 10 advisories além dos quatro de `wasmtime`, com correções major em `quick-xml`, `zip`, `zstd` e `zstd-safe`. Não foram misturados a este patch. O gate F2.5 do bundle ficou verde após os patches de `wasmtime`.

## Próxima fronteira

Julgar o held-out `20261101c` somente no J16, após o release de 04/11. Não executar esse julgamento nesta retomada.
