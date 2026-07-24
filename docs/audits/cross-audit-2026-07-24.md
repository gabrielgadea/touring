---
type: CrossAuditReport
title: "Cross-audit — Fase 4′ move-first (fonte canônica em ~/projects/touring)"
description: "Auditoria de purpose-fidelity das 7 fases sobre TUDO da migração de 24/07/2026; 3 findings reais encontrados E corrigidos no próprio audit; evidência executada em cada claim"
tags: [cross-audit, productization, pln2, f4-move-first, purpose-fidelity]
timestamp: 2026-07-24
plan: /docs/plans/touring-productization-pln2/00-INDEX.md
---

# Cross-Audit 2026-07-24 — Fase 4′ Move-First

## 1 · VERDICT

**PASS — com 3 findings reais (2 ALTOS + 1 baixo), TODOS corrigidos e provados
no próprio audit.** O propósito da fase ("a fonte canônica vive em
`~/projects/touring` e o sistema inteiro opera dela") estava **parcialmente
violado de forma silenciosa** quando o audit começou: o daemon vivo e os units
systemd ainda resolviam partes do runtime para a cópia congelada. Após os fixes,
toda a cadeia (symlinks → daemon → env → units → configs → index) opera do novo
root, com prova forense em cada elo. 3 itens permanecem honestamente
`UNVERIFIED` (§4).

## 2 · SCORECARD

| Eixo | Resultado | Evidência executada |
|---|---|---|
| Gate da fase | `validate_phase4.sh` **8/8 PASS ×2** (pré e pós-fixes do audit) | run + re-run, exit 0 |
| Sistema | e2e **0.8749 pass ×2** do novo root (idêntico ao root velho — zero regressão) | `touring e2e -j`, target confirmado |
| Integridade da cópia | digests sha256 idênticos (amostra Cargo.toml/lock/config.rs); **0 source perdido** — os 6 .rs "faltantes" são cache serde de um `target/` ANINHADO espúrio no root velho | comm + sha256 + inspeção |
| 50-dim (tocados) | validate 0.9401 · coevolve 0.9514 · loop_marker 0.9497 · test_flow_guard 0.9433 · pre_read_tests 0.9128 — **≥ Platinum ×4, todos ≥ Gold** | `touring-quality score` ×5 |
| 6 P0 BLOCK | **Pass/N-A 6/6** no coevolve (o script de maior privilégio de escrita) | `touring-quality check --gate` ×6 |
| Testes | flow_guard **25/25** · plan_scope **6/6** (feature `pre-hooks`) · foundation smoke exit 0 | pytest / cargo test |
| Débito | **0 markers** no bundle da fase ("the tree is clean") | `scan_debt.py` |
| Co-evolução | 93 aplicados, settings.json válido, **0 JSON quebrado PELO apply**, hooks registrados 0 refs velhos, crontab limpo, backup tar.gz íntegro | scan determinístico + jq |
| Index novo root | symbols.db mtime 08:32:40 (pós-rebuild server-side), paths **relativos** (portáveis), doctor **6/6** do novo cwd | stat + `index find` + doctor |
| DAG | F4-1..F4-6 completed íntegros **pós-restarts do audit** (read-after-write) | `decompose get` |

## 3 · FINDINGS (all-breadth — 3 corrigidos + 5 observações)

**F-A · ALTA · CONFIRMADO → CORRIGIDO (3 camadas).** O daemon vivo NÃO tinha
`TOURING_WORKSPACE_ROOT` no environ e `workspace_root_marker()` resolve apenas
env → fallback hardcoded `~/.claude/rust` (a camada config `[paths]` planejada
na Fase 0 **não existe no código**) → o knowledge-wiring do daemon operava
sobre a **cópia congelada**. Fixes: (1) `settings.json` env +=
`TOURING_WORKSPACE_ROOT` (cobre toda sessão CC nova e auto-spawns);
(2) `update-touring` agora `export TOURING_WORKSPACE_ROOT="$RUST_WORKSPACE"`
(todo daemon spawnado pelo pipeline herda o root com que foi instalado);
(3) daemon reiniciado com a env — **prova forense**: `/proc/1514405/environ`
contém `TOURING_WORKSPACE_ROOT=/home/gabrielgadea/projects/touring`, exe =
binário novo. Pendência estrutural roteada para F1: flip do fallback hardcoded
+ camada config real (exige rebuild — F1 já rebuilda).

**F-D · ALTA · CONFIRMADO → CORRIGIDO.** `touring-daemon.service` e
`touring-resource-monitor.service` (**enabled**) tinham `ExecStart` no binário
congelado — no próximo boot o systemd subiria o daemon VELHO (drift garantido na
primeira release divergente). Fix: ExecStart → `~/projects/touring/target/release/`
+ `Environment="TOURING_WORKSPACE_ROOT=…"` no daemon unit + `daemon-reload`
(backups `.bak-audit-20260724` sidecar). Provado por `grep` pós-reload.

**F-C · BAIXA → CORRIGIDO/POTENCIALIZADO.** `touring-bootstrap`
`discover_scan_dirs` cobria o layout antigo (`.claude/rust/crates`) mas não
`crates/` na raiz — o layout do novo root e de qualquer workspace Rust padrão.
Fix aditivo (REGRA #0): `"crates"` incluído, legado mantido. Prova: dry-run no
novo root agora descobre `scan: crates`.

**O1.** Os 6 .rs a mais no root velho são `crates/touring-quality/target/debug/build/serde-*`
— um `target/` aninhado espúrio (anti-padrão REGRA #12 #1) que o rsync
corretamente não copiou. Cópia íntegra. **O2.** As refs `claude/rust` em
`agents/_shared-touring-base.md`/`touring-architect.md` são **contra-exemplos
documentais** de entradas FP corrompidas — falso positivo do grep; intocadas
corretamente (reescrever mudaria a semântica do exemplo). **O3.** 5+ JSONs
corrompidos em `data/`/`checkpoints/` com mtime abril/maio — pré-existentes,
fora do escopo do coevolve; recomendação: quarentena. **O4.** Baseline de
orphans do novo root: **27453** (o "168239" dos hooks é outro contador);
sem baseline comparável limpa do root velho — fica registrada como baseline
inicial REGRA #0 do novo root. **O5.** Daemon respawna com frequência e um
`daemon-ctl restart` foi no-op silencioso (PID inalterado); somado ao PID file
canônico vazio e ao gotcha "decompose updates perdidos em restart" — três
sintomas do mesmo tema: lifecycle/flush determinístico, **núcleo da F1/W12.5**.

## 4 · FUSED RISK

Nenhum P0 de segurança (6 BLOCK Pass/N-A). Risco residual concentrado no tema
lifecycle (O5, roteado para F1). `UNVERIFIED` honestos: (1) **boot real via
systemd** — provado `daemon-reload` + conteúdo do unit, não um start real (daria
colisão de socket com o daemon vivo; confirmação natural no próximo boot);
(2) **sessão CC nova** herdando `TOURING_WORKSPACE_ROOT` do settings — hooks só
carregam em sessão nova (gotcha provado em 23/07); primeira sessão nova confirma;
(3) 4 advisory pré-existentes do e2e (candle_bge CC=17 etc.) — já na fila ADW
`task_1784818024931392697`, fora desta árvore.

## 5 · ROOT-CAUSE (dos 3 findings)

Classe única: **cutover cobriu os consumidores DECLARADOS, não os caminhos de
spawn implícitos**. O coevolve reescreveu o que o grep mapeou (93 configs), mas
o estado efetivo de runtime vive em três lugares que nenhum grep de configs
alcança: o environ do processo daemon já em execução (F-A), unit files fora de
`~/.claude` (F-D) e heurísticas de descoberta embutidas em scripts (F-C). É o
mesmo padrão-raiz já catalogado como "âncora de contexto errada" — o sistema
resolve um root implícito quando o explícito não chega ao ponto de uso. A defesa
estrutural definitiva é a F1 (config em camadas + daemon per-project), que
substitui env-implícito por pin explícito por projeto.

## 6 · PROVENANCE (comandos executados)

`loop_marker.py write` (marker cross-audit, scope novo root) · `touring doctor -j`
×3 (6/6 do novo cwd) · `/proc/<pid>/environ` ×3 (F-A antes/depois) · `crontab -l`
· `grep ~/.local/bin` + `systemctl --user is-enabled/cat` (F-D antes/depois +
`daemon-reload`) · scan determinístico python (digests, contagens .rs, residuais,
hooks registrados, JSONs, backup) · `comm` dos .rs (6 = target aninhado) ·
`stat` symbols.db · `scan_debt.py` (0) · `touring-quality score` ×5 + `check` ×6
· `touring status -j` (orphans 27453) · fixes: settings.json (backup
`.bak-audit-20260724`), update-touring export (bash -n OK), units sed+reload,
touring-bootstrap (py_compile + dry-run com `scan: crates`) · `daemon-ctl
stop` + auto-spawn com env (ENV_PROVADA) · re-provas: `validate_phase4.sh` 8/8
×2 · pytest 25/25 · `decompose get` (DAG íntegro) · `touring e2e -j` 0.8749 pass.

## 7 · ACTIONS

**Fechadas neste audit**: F-A (3 camadas + prova forense), F-D (units + reload),
F-C (potencializado + dry-run). **Roteadas (fila F1/W12.5, sem pendência de
código nesta fase)**: flip do fallback hardcoded + camada config `[paths]`;
PID file canônico; flush determinístico do decompose; investigação do restart
no-op. **Recomendação sem urgência**: quarentena dos JSONs históricos corrompidos
(O3); limpeza do target aninhado no congelado quando o descarte (D4) for decidido.
**Confirmações naturais**: próximo boot (units) e próxima sessão CC (env settings).

---
_Auditoria executada sob o próprio contrato auditado (marker `cross-audit`
armado na invocação; este arquivo é o artefato que o Stop guard exige).
Institucional: `touring memory store cross-audit-f4-move-2026-07-24`._
