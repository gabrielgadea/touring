# Sessão 2026-08-28 — estreia do KPI mutation, force-gate, RCA do lifecycle, deploy 30.4.18

> **Escopo**: relatório da sessão que fechou 4 frentes + deploy. O veredito
> auditado com toda a proveniência executada está em
> `docs/audits/cross-audit-2026-08-28-kpi-mutation-lifecycle.md`; a entrada
> curada do release está no `CHANGELOG.md` (`[30.4.18]`). Este doc é o índice
> narrativo que liga os artefatos.

## As 4 frentes (commits)

| Commit | Frente | Entregável |
|---|---|---|
| `e7195cd` | propagate-release | gate 5.5 com retry-once anti-transiente (`wait_doctor_clean` + `run_prova`) |
| `36faca8` | KPI | estreia `touring.mutation.kill_rate` 98,15% + gramática `@<scope>` + canal `external:` real + guard contrato×executor bidirecional + commitment `inspect_burst_share` |
| `fdc9126` | mutation hardening | force-gate workspace (`workspace_requires_force`) + probe `$CARGO_HOME/bin` + floor 1800 s no client + `is_heavy` no daemon + `scripts/diag_lifecycle_hang.sh` |
| `476cbba` | RCA lifecycle | fixtures sem `.git` alcançavam o índice tantivy GLOBAL real → starvation no writer Mutex; marcador + guard `fixture_root_normalizes_to_itself_not_home` |
| `a52858a` | auditoria | cross-audit do delta — Diamond nos 7 arquivos, 0 P0, 0 órfãos, FASE 5 vazia |
| `792cdff` | release | bump 30.4.17 → 30.4.18 |

## A classe de defeito unificadora

**"Declaração ≠ executor"**, quatro encarnações no mesmo dia: fonte KPI declarada
sem braço no resolvedor; probe de binário mais estreito que a resolução real do
cargo; client com floor menor que o budget do server; fixture "isolada" cujo
resolvedor de path tinha fallback global. Remédio institucional idêntico nos
quatro: um guard que lê os dois lados (D8) —
`every_declared_source_has_an_arm_and_every_derived_arm_a_commitment`,
`mutation_test_is_classified_heavy`, `fixture_root_normalizes_to_itself_not_home`.

## Deploy 30.4.18

`scripts/propagate-release.sh 30.4.18` — gates (incl. estreia do retry do 5.5)
→ `update-touring` → toolchain install → default → `touring update` por projeto
pinado (analise, konverter; a fonte é pulada) → verify. Prova por projeto é
**comportamental** (git hash do binário), nunca por rótulo (regra 2/2.1 do
CLAUDE.md). Pós-deploy obrigatório: restart final com
`PATH="$HOME/.cargo/bin:$PATH" TOURING_PILLAR_INDUCTION_ARMED=1 touring
daemon-ctl restart` — todo restart desarma o pillar induction e estreita o PATH
(o probe do cargo-mutants depende dele), provado por `/proc/<pid>/environ`.

**Desfecho (23:35 BRT, exit 0, log inteiro lido — sem pipe)**: 6/6 estágios
verdes. Gates (check dual-graph, espelho CLEAN 323, pytest 10/10) → build
release 14m02s + doctor 5/5 → toolchain 30.4.18 freeze + default → analise e
konverter 30.4.17→30.4.18 (daemons per-project restartados) → **prova 35/35 de
primeira** (a re-espera do retry novo chegou a ser anunciada — "doctor ainda
degradado após 20s" — mas não foi consumida) → verify lock=30.4.18 nos dois.
Provas pós-deploy executadas: (a) **deny de rajada ao vivo no analise** com o
binário DO projeto (1ª chamada allow, 2ª deny com rota derivada — o predicado
S3 novo em comportamento, não em rótulo); (b) daemon global re-armado —
`/proc/<pid>/environ` com `TOURING_PILLAR_INDUCTION_ARMED=1` + PATH com
`~/.cargo/bin` (o probe do cargo-mutants depende dele); (c) doctor 7/7 OK; (d) o
`PID running deleted binary` do update-touring morreu sozinho (processo efêmero
segurando o inode velho).

## Documentação atualizada nesta sessão (a ordem "atualize tudo")

- `docs/mutation-testing.md` — **criado** (era referência fantasma na tabela
  TIER 4 do client desde a Wave T1/T2)
- `docs/kpi/README.md` — **criado** (gramática dos 3 canais, canal external,
  guard bidirecional, snapshots)
- `CHANGELOG.md` — entrada curada `[30.4.18]`
- `docs/2026-06-27-coupling-telemetry-infrastructure.md` §1.1 — drift corrigido
  (`external:` deixou de ser STUB; gramática `@<scope>`)
- `CLAUDE.md` regra 2.1(c) — retry-once do gate 5.5 + lição `${PIPESTATUS[0]}`
- `~/.claude/skills/Touring/references/touring-cli-tiers-4-9.md` (live) +
  espelho `client/` sincronizado — linha do mutation-test com force-gate,
  budget heavy e probe CARGO_HOME
- Memórias: `lifecycle-trava-sob-concorrencia` (RESOLVIDO),
  `kpi-fonte-declarada-sem-resolvedor`,
  `teste-unitario-dispara-corrida-de-producao`,
  `prova-comportamental-code-mode-ceg` (retry), backlog mutation regravado
  como CONCLUÍDO

## Pendências que ficam com Gabriel

1. Rotação da GEMINI_API_KEY (segurança, pré-existente).
2. Limpeza dos docs de teste residuais no índice tantivy global (reindex
   apagaria event docs legítimos junto — decisão humana).
3. Débito pré-existente: grafo release-TEST do touring-server (E0460).
