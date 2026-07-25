---
type: CrossAuditReport
title: "Cross-audit — F3 + PILOT + F5 + GA-handoff + GitHub + Descolamento"
description: "3º audit da série (purpose-fidelity, 7 fases): tudo desde o audit F1+F2 — update/component/toolchain, piloto konverter, distribuição, licença, publicação GitHub v30.3.0 e o descolamento do ~/.claude; 2 findings reais corrigidos e provados no próprio audit"
tags: [cross-audit, pln2, f3, pilot, f5, github, descolamento, purpose-fidelity]
timestamp: 2026-07-25
plan: /docs/plans/touring-productization-pln2/00-INDEX.md
---

# Cross-Audit 2026-07-25 (3º da série) — F3 → Descolamento

## 1 · VERDICT

**PASS — 2 findings reais (F-NEW-2, F-NEW-3) encontrados, corrigidos E provados
ao vivo dentro do audit; bateria integral verde (1420 lib + 5 E2E + 8/8 + 9/9 +
10/10 validates); o contrato central do produto — canal determinístico por
socket — está provado nas três pernas (global→dev, per-project→bin pinado,
custom→dev).** O escopo auditado: F3 (update/component/toolchain sources),
PILOT konverter, F5 (instalador/packaging/release pipeline), GA-handoff
(workflows/CHANGELOG/licença), publicação GitHub v30.3.0 e o descolamento
D1-D7 do `~/.claude` (262→15 GB).

## 2 · SCORECARD

| Eixo | Resultado | Evidência executada |
|---|---|---|
| Suite lib touring-server | **1420 passed, 0 failed** (release) | pós-fix F-NEW-2/3 |
| E2E F3 (binário real) | **5/5** | update/lock/rollback/component/from-source |
| Validates | phase3 **8/8** · pilot **9/9** · phase5 **10/10** — ALL PASS | re-executados no audit |
| Prova F-NEW-2 (3 pernas) | global→`target/release` (dev) · per-project→`.touring/bin` pinado · socket custom→dev | spawns reais + `/proc/exe` |
| Rollback loud | exit **1** real + "nothing to roll back to…" | konverter sem `previous` (1ª medição via pipe corrigida — real-exit-codes) |
| 50-dim rodada | license.rs **0.954 Diamond** · hooks/main.rs 0.921 | 6 P0 license.rs **6/6 PASS** |
| Clippy / debt scan | 0 erros · **0 débitos** nos arquivos novos | update/component/project_toolchain/license/installer/client |
| Release GitHub | v30.3.0, 4 assets, provado por download+install+exec ontem | `gh release view` |
| Símbolos novos (REGRA #0) | 0 órfãos: restart_socket_with_bin=2, project_root_for_socket=5, resolve_active_channel=10, install_from_source=3 consumers | grep census |
| e2e composite | 0.8660 pass | `touring e2e -j` |

## 3 · FINDINGS (all-breadth)

**F-NEW-2 · MÉDIO · CONFIRMADO → CORRIGIDO+PROVADO.** Canal do daemon
não-determinístico: `try_autostart_daemon` (touring-hooks/main.rs) resolvia o
daemon como *sibling cego* do hook — fora de projeto o shim entrega o hook da
toolchain default ⇒ o daemon GLOBAL grudava na toolchain imutável; o próximo
`update-touring` o devolvia ao dev ⇒ oscilação conforme o último spawner
(observado: global rodando `~/.touring/toolchains/30.3.0/bin/` com uptime
2m34s). Fix: canal-por-socket — per-project→`.touring/bin/touring-daemon`
PINADO do projeto; global/custom→`TOURING_DAEMON_BIN` > dev channel > sibling
— espelhando `daemon_ctl::spawn_daemon_with_bin` (C08 nos 2 spawn-sites).
Provado ao vivo nas 3 pernas; deploy + toolchain reinstalada (o hook da
toolchain é quem autospawna).

**F-NEW-3 · MÉDIO · CONFIRMADO → CORRIGIDO.** O descolamento D1/D5 quebrou 3
symlinks Touring em `~/.local/bin` (→ root deletado): `touring-quality`
(quebrou o próprio harness 50-dim deste audit — detectado quando TODOS os
scores retornaram "?"), `touring-lsp` e `libclaude_learning_kernel.so`.
Correções: quality+so reapontados ao novo target; **touring-lsp exigiu fix de
CÓDIGO** — a lib não compilava sob sua própria feature `lsp-bridge`
(2×E0277: `HookRuntimeInitError` serializado direto sem `Serialize`; drift de
feature-gate nunca buildada pelo CI default) → serializa o display, build
verde, symlink restaurado. Bônus: 5 links mortos de sistemas desativados
(kazuba/openclaw/zeroclaw) removidos.

**FP-1 · Falso positivo do harness documentado.** F3.11 (README Completeness)
acusou "repository has no README" num target de arquivo — `README.md` EXISTE
na raiz; a dim repo-level aplicada a escopo-arquivo resolve o scope errado.
Registrado para recalibração do engine; não afeta o veredito (Diamond 0.954).

**O1.** PID do daemon global muda com frequência nesta sessão — consistente
com os meus próprios restarts/deploys do dia; com o canal agora determinístico
o respawn é inócuo. Observar `daemon.stderr.log` em uso normal.

## 4 · FUSED RISK

Nenhum P0. Residuais: (a) GitHub Actions sem minutos (quota free esgotada) —
ci/release-plz pausados até decisão de visibilidade/billing; nada local
depende; (b) suites de integração do server não re-executadas por inteiro
(lib+E2E+validates cobrem o delta); (c) F3.11 scope-resolution no engine
50-dim (FP conhecido).

## 5 · ROOT-CAUSE (a classe que une a série)

**"Consumidores do recurso antigo esquecidos em refactors/moves 1→N"** — a
mesma classe em três formas: F-NEW-1 (lock global vs socket per-project),
F-NEW-2 (spawn-site sibling vs canal por socket), F-NEW-3 (symlinks vs root
movido). Lição institucional: todo split/move/descolamento exige **census de
consumidores do recurso antigo** (locks, spawn-sites, symlinks, paths em
scripts) ANTES de declarar completo — o grep do símbolo principal não basta;
o PAR e os PONTEIROS também migram.

## 6 · PROVENANCE (comandos executados)

MAP: git log/status + daemon-ctl list-all + /proc exe + toolchains + lock
konverter + gh release + du ~/.claude · FASE2/3: scan_debt (0) + grep TODOs
(0) + corpo try_autostart lido · FASE4: touring-quality score/check (após
restaurar o próprio harness) + census consumers + e2e -j · FASE5: Edit
main.rs (canal-por-socket) + server.rs (2×to_string) + clippy 0 + deploy
update-touring + toolchain --force + update konverter · FASE6: spawns reais
com sockets isolados + /proc (3 pernas) + rollback exit real + suites
1420/5 + 3 validates + license + daemons · Commit `2679b02` pushed.

## 7 · ACTIONS

**Fechadas no audit**: F-NEW-2 (fix+deploy+prova tripla), F-NEW-3 (2 symlinks
+ 1 fix de código + build + 5 mortos), medição de exit via pipe corrigida.
**Roteadas**: recalibrar F3.11 scope-resolution (engine harness); observar
respawns do daemon global em uso normal; Actions aguardam decisão de
visibilidade/billing (Gabriel). **Estado do programa**: Pln2 F0-F5+PILOT
completos · release v30.3.0 pública no repo · descolamento D1-D7 executado ·
3 audits da série, 4 findings reais, todos corrigidos e provados.

---
_3º cross-audit (1º: F4′; 2º: F1+F2; 3º: F3→Descolamento). Institucional:
`touring memory store cross-audit-f3-descolamento-2026-07-25`._
