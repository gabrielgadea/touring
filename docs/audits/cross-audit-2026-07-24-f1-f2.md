---
type: CrossAuditReport
title: "Cross-audit — F1 daemon multi-instância + F1-ROUTING + F2 install lifecycle"
description: "Auditoria purpose-fidelity das 7 fases sobre tudo entregue em F1/F1-ROUTING/F2 (24/07/2026); 1 finding real corrigido no audit; 3829 testes 0 falhas; evidência executada em cada claim"
tags: [cross-audit, productization, pln2, w12-5, f1, f2, purpose-fidelity]
timestamp: 2026-07-24
plan: /docs/plans/touring-productization-pln2/00-INDEX.md
---

# Cross-Audit 2026-07-24 (2º do dia) — F1 + F1-ROUTING + F2

## 1 · VERDICT

**PASS — 1 finding real (F-NEW-1) encontrado E corrigido no próprio audit; 0
regressões nas 5 suites completas (3829 testes); todos os fluxos novos provados
ao vivo em produção.** O propósito auditado — "N daemons per-project coexistem,
todo comando acha o task/binário/socket CERTO de qualquer cwd, e o ciclo
init→bin→shim→daemon fecha ponta a ponta" — verificado por execução, incluindo
os vetores adversariais que o walk-up recém-ativado poderia ter aberto no
filesystem real (nenhum se materializou). O audit foi pausado por Gabriel após
a FASE 5 e retomado sem perda (checkpoint semântico).

## 2 · SCORECARD

| Eixo | Resultado | Evidência executada |
|---|---|---|
| Suites completas | foundation **422** · hooks-core **437** · cli **268** · server **1392** · dispatch **1310** = **3829 passed, 0 failed** | `cargo test --release --lib` ×5 |
| E2E W12.5 | **3/3** (coexistência multi-daemon · race same-socket idempotente · roteamento decompose cross-cwd) | run + re-run pós-fix |
| Gates de fase | `validate_phase1.sh` 8/8 · `validate_phase2.sh` 7/7 (ambos ALL PASS) | execuções anteriores no mesmo dia |
| Clippy | 0 em todos os crates tocados (incl. touring-hooks pós-fix) | `-D warnings` |
| 50-dim | decompose 0.8840 (Gold) · init_project 0.9441 · ipc 0.8942 · shim 0.9299 · +5 arquivos F1 ≥0.9128 | `touring-quality score` |
| 6 P0 BLOCK | Pass/N-A no decompose.rs (o arquivo com SQL) | `touring-quality check` ×6 |
| Perf | shim **+3,5 ms/evento** (5,7 vs 2,2 ms) · resolver c/ walk-up+toml ~10 ms de cwd profundo | `time` ×10 e ×20 |
| Idempotência | 2º `update-touring`: shim sha256 **idêntico**, segue arquivo (não symlink), hook exit 0, daemon PID 2324240 no binário canônico | sha256sum antes/depois |
| Débito | **0 itens nos arquivos F1/F2**; pré-existentes classificados (§3 O4) | `scan_debt.py` ×5 dirs |

## 3 · FINDINGS (all-breadth — 1 corrigido + 4 observações)

**F-NEW-1 · MÉDIO · CONFIRMADO → CORRIGIDO.** `cleanup_orphan_daemon_state`
(`touring-hooks/src/main.rs:813`, caminho de auto-spawn por hooks) resolvia o
socket per-project-aware mas limpava o lock **uid-global** — descasamento que
podia tocar o lock do daemon GLOBAL ao limpar um daemon per-project crashado
(mitigado pelo alive-check e por flock morrer com o processo, mas incoerente).
Fix: `daemon_lock_path_for(&socket_path)`. Rebuild + clippy 0 + 3829 testes
verdes + E2E 3/3 pós-fix.

**O1 · Vetores adversariais do walk-up — verificados LIMPOS.** (a) O toolchain
home `~/.touring` não colide com o walk-up do shim (não há `~/.touring/bin/
touring-hook`); (b) o `.touring/` herdado pelo rsync em `~/projects/touring`
tem `touring.toml` antigo **sem** `[toolchain]`/`[daemon]`, `bin/` **vazio** e
nenhum `daemon.sock` — resolver e shim seguem o global por construção (provado:
`daemon-ctl status` sem env resolve o socket global).

**O2 · `ipc::daemon_lock_path` (legado) mantido como compat-API.** Após o
F-NEW-1 seu último consumer interno migrou; permanece como nome documentado do
lock global (REGRA #19, consumível por scripts) — decisão registrada, não
órfão acidental.

**O3 · `backup_symlink` não faz backup do shim** (só de symlinks): o 1º cutover
fez backup do symlink ✓; re-runs regeneram o shim da fonte versionada —
idempotente (provado por sha256). Perda de backup do shim é inócua.

**O4 · Débito pré-existente classificado (fora do escopo F1/F2):** ~26
"TODO: Audit env access" da migração edition-2024 (auto-gerados; os 3 do
arquivo tocado foram convertidos em SAFETY docs neste trabalho) — recomendada
wave mecânica dedicada; docstrings do extrator de TODOs do produto = falsos
positivos do scanner; `#[ignore] // requires graphviz` = testes condicionais a
dependência externa, legítimos.

**O5 · e2e composite 0.8749 idêntico entre runs** — determinístico por
construção (checks estáticos compostos), não cache; nota informativa.

## 4 · FUSED RISK

Nenhum P0. Riscos residuais: (a) suites de **integração** do touring-server
(binary_e2e etc.) não re-executadas por inteiro neste audit — as lib suites
(3829) + E2E W12.5 + gates de fase cobrem o delta F1/F2; (b) grafo de testes
**release** do touring-server segue quebrado (feature unification capnp/
bindings — pré-existente, perfil nunca usado; registrado na F1); (c) primeira
sessão CC totalmente nova confirmará o shim no hot path em carga real (a
invocação sh -c formato-CC exit 0 já foi provada ×3).

## 5 · ROOT-CAUSE (do F-NEW-1)

Mesma classe da F1 inteira: **recursos pareados (socket↔lock) atualizados em
lados diferentes de um refactor** — o resolver de socket evoluiu para
per-project e um dos três call-sites do lock ficou no nome antigo. O grep de
unificação da F1 cobriu `daemon_socket_path`; o pareamento lock↔socket só
aflorou ao auditar consumers de `daemon_lock_path()`. Lição: ao dividir um
conceito 1→N (lock global → lock por socket), auditar TODOS os consumers do
símbolo antigo antes de declarar a divisão completa (C08 aplicado ao par).

## 6 · PROVENANCE (comandos executados)

MAP: `ls ~/.touring` + `ls .touring/` (workspace e root congelado) + `cat
touring.toml` + `env -u TOURING_DAEMON_SOCKET daemon-ctl status` · PERF:
`time` shim ×10 vs binário ×10; resolver ×20 de cwd profundo · DEBT:
`scan_debt.py` ×5 dirs (detalhe por item) · HARMONY: `touring-quality score`
×4 + `check` P0 ×6 + grep consumers `daemon_lock_path()` · FIX: Edit main.rs
+ rebuild + clippy · E2E: 5 suites lib completas (3829) + E2E W12.5 3/3 (re-run
pós-fix) · Idempotência: sha256 antes/depois do 2º `update-touring` + hook
sh -c exit 0 + `daemon-ctl status` (PID 2324240, exe canônico) · Pausa/retomada:
checkpoint `cross-audit-f1-f2:pausado:2026-07-24` + marker arquivado/renovado.

## 7 · ACTIONS

**Fechadas neste audit**: F-NEW-1 corrigido + regressão completa verde.
**Roteadas (sem pendência de código nesta fase)**: wave mecânica dos
edition-TODOs (O4); grafo release-test do server (pré-existente, fila F1);
confirmação do shim em sessão CC nova (natural). **Prontas para seguir**: F3
(`touring update` + `component`) é a próxima fase do DAG — aguarda o human
gate.

---
_2º cross-audit do dia (o 1º cobriu a Fase 4′). Executado sob pausa/retomada de
Gabriel com checkpoint semântico. Institucional: `touring memory store
cross-audit-f1-f2-2026-07-24`._
