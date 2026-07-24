# Roadmap Mestre — Coupling LLM↔Touring → Productization (cross-reference dos 9 nós)

> **Data**: 2026-06-27 | **Tipo**: índice mestre / cross-reference | **Autoridade**: Gabriel Gadea
> **Origem**: referência cruzada solicitada por Gabriel sobre os 9 documentos do arco, com verificação
> de status **real contra o código** (VP-Scout Cadeia 5/6 — 4 Explore agents paralelos + CLI audit já em contexto).
> **Marco final do arco**: `~/.claude/plans/giggly-drifting-kahn.md` (Touring Productization — Pln2).
> **Doc ⑥ (R1–R6)**: `2026-06-27-coupling-codemode-cli-and-master-commands.md` — ✅ **R1–R6 COMPLETO** (2026-06-28).
> **🔄 Reconciliado 2026-06-28**: status real re-verificado contra o código pós-entrega de R1–R6 + `audit`→CLI
> + consumer-wiring fail-soft. **O ponto de retomada do arco avançou de ⑥ → ⑦ → ⑨: com F6/F7 SHIPPED (2026-06-29) o ⑦ está FECHADO em M4 (causal/auto — loop L5 runtime-proven); retomada agora é ⑨ (productization Fase 0) ou F4 (counters opcionais).**
> Ledger de drift completo na **Seção 1.5**.
> **🔎 Auditoria cruzada 2026-06-28** (`2026-06-28-coupling-arc-cross-audit.md`): ⑥⑦③⑤ **PROVADOS em runtime**
> (14/14 claimed-done; code-mode sans-MCP + 7 KPIs `coupling.*`; callers ③ wired; debt 0; 50-dim ≥Gold; 6 P0 pass).
> Fix REGRA #21: `cargo fmt` (drift de imports em `touring-analysis`). Oportunidade REGRA #0: `--brief` global-first
> (`crates/touring-server/src/main.rs:176`) — recomendada, **não** aplicada (hot-path de dispatch).
> **🔧 Fixes 2026-06-29** (`fix:brief-global+f5-flush+daemon-reaper`): a oportunidade REGRA #0 acima foi
> **aplicada e provada no binário de produção** — `--brief`/`-j`/`-v`/`--timeout` agora funcionam em posição
> global-first (`main.rs::resolve_subcommand` via `parse_global_flags` antes do dispatch). + **F5 default-ON**
> (`gate_metrics_daily_enabled() != "0"`, opt-out). + **flush-on-shutdown corrigido** (roteava por
> `project_root=""` → `HookRuntime::new("")` pesado/frágil; agora projeto warm do `RuntimeMap` — provado:
> snapshot reescrito, ns-precision + trace `flush OK`). + **daemon split-brain reaper** (`all_daemon_pids()`
> reapa todos em stop/restart/reset; REGRA #19-safe — comm distinto de mcp/hook/cli). Gates verdes (fmt/check/
> clippy/test 0, 50-dim ≥Gold, 6 P0). Gotchas-teste: mtime granularidade 1s (use `stat %y` ns); `daemon-ctl`
> spawn `stderr=null` não loga.
> **🚀 Avanço 2026-07-01** (`harness-premium:onda-a+fase0-productization:2026-07-01`): **⑨ Fase 0 ENTREGUE**
> (`validate_phase0.sh` 5/5 PASS): 5 sites de hardcode → `TOURING_WORKSPACE_ROOT` env→fallback (os 4 do plano
> + `PARCER_SCHEMA` descoberto além); `[toolchain] channel="30.3.0"` no init-project + `ToolchainPin` lido por
> `detect_layered` (teste); **versão única 30.3.0** (workspace.package; server herda; drift 0.1.0↔30.0.0 morto;
> `touring --version` deriva — NB: emite em **stderr**). **Task #6 pillar induction ARMADO** (settings.json env
> + daemon herda) e **provado vivo** (1ª emissão real: `touring scout HookRuntime` arg real); +4 fixes densidade
> no `cli_suggester` (specific-or-absent estrutural, `carries_input_specific_signal()` anti-dedupe de code-mode,
> `stale_index_hint` com root real, template loop runnable). Gates: check/tests/clippy/fmt 0 · 50-dim 8/8 ≥Platinum ·
> update-touring exit 0. **Retomada agora = ⑨ Fase 1 (daemon multi-instância)**. Índice de fases:
> `docs/plans/touring-productization-pln2/00-INDEX.md`.

---

## 0. A pergunta unificadora

Todo o arco responde a **uma** pergunta: *por que a LLM não adota o caminho Touring, e como fazer com que
adote por construção?* A resposta evolui de **diagnóstico** → **inventário** → **fixes** → **evidência empírica**
→ **proposta de canal** → **medição** → **arquitetura premium** → **produto instalável**. A tese que costura tudo:

> **Mudar `U(a) = P(sucesso)·V − C(tokens)` por afordância estrutural barata, não por persuasão.**
> A productization é a forma máxima disso: defaults per-project que tornam o caminho acoplado o de **menor
> resistência por construção**, não por instrução.

---

## 1. Os 9 nós — papel no arco + status real verificado

| # | Nó (doc) | Papel no arco | Status REAL (verificado) |
|---|---|---|---|
| ① | `2026-06-26-touring-llm-coupling-strategy.md` | **A TESE**: U(a), afordância > indução, hierarquia de força | ✅ tese sustentada; Touring investiu invertido (110K em rules, camada mais fraca) |
| ② | `2026-06-26-touring-capability-map.md` | **O INVENTÁRIO**: 50 caps, 5 modos de acesso | ✅ "não falta capacidade, falta alcance" (linha 140); alto-valor em canais de baixa adesão |
| ③ | `2026-06-26-coupling-backlog.md` | **OS FIXES**: C1–C14 + MT-1/Gap2/Gap4 | ✅ 14 DONE-verif + 3 Gaps DONE; **C11/C13/C14 callers wired** (C13→`ceg_adapter`, C11/C14→`reason_tools.rs`, cross-audit 2026-06-27); **C12 caller = won't-do** (engine pura, `touring plan-chain` já existe) |
| ④ | `2026-06-27-coupling-adoption-failure-diagnosis.md` | **O DIAGNÓSTICO VIVO**: 27 sugestões → 0 adoção | ✅ modelo CONSTRUÍDO→ATIVADO→**ADOTADO❌**; causa B = MCP *server* desconectado (não tool ausente) |
| ⑤ | `2026-06-27-touring-cli-command-audit.md` | **A EVIDÊNCIA**: 297 cmds, defeitos de envelope | ✅ mean 0.9688, 0 P0; A1–A12 remediados em runtime; **A1 `--brief` já em prod** (`main.rs:252`) |
| ⑥ | `2026-06-27-coupling-codemode-cli-and-master-commands.md` | **O CANAL CODE-MODE** (entregue): R1–R6 | ✅ **R1–R6 COMPLETO+DEPLOYED+PROVEN sans-MCP** (2026-06-28): `run`+6 master+`investigate`+`audit`→CLI (`command_table.rs:131-184`); +consumer-wiring fail-soft |
| ⑦ | `2026-06-27-coupling-telemetry-infrastructure.md` | **A MEDIÇÃO**: uptake/adesão (posterior a ⑥) | ✅ **F1+F2+F3+F5+F6+F7 SHIPPED — ⑦ FECHADO EM M4** (F6/F7 2026-06-29): **7 KPIs** vivos + scheduler daemon-interno + **loop L5 fechado** — A/B atribuível (`run_bench --compare` → control 0.8763 vs treatment 1.0000, Δ**+0.1237** `coupling_helps`) + bloco `ab` no snapshot + `touring kpi --refine` (motor §12 A/B-gated) + atuador `hint_demotion_bump` graduado **default-OFF**. Runtime-proven: `--refine` emitiu `tighten_elision` actionable. F4 (counters G3/G4/G7) = único opcional restante |
| ⑧ | `2026-06-25-harness-consolidation-*` + `plans/touring-47-to-13-residual` | **A ARQUITETURA PREMIUM**: enxugar shims | ⚠ harness ✅ Diamond 0.989; crates 47→13 **parou em 36** (48 físicos, 16 shims, 4550 orphans, 9 cycles) |
| ⑨ | `plans/giggly-drifting-kahn.md` | **O MARCO FINAL**: instalável/versionado per-project | 🟢 **Fase 0 ✅ (2026-07-01**, `validate_phase0.sh` 5/5): fonte desacoplada (`TOURING_WORKSPACE_ROOT`) + pin `[toolchain]` + versão única 30.3.0; restam Fases 1–5; gap nuclear = `touring update` (W12.3, Fase 3) |

---

## 1.5 · Reconciliação 2026-06-28 — ledger de drift (doc 2026-06-27 → realidade verificada)

> Re-verificação VP-Scout Cadeia 5/6 **contra o código** (não inferência). O doc original foi escrito no
> **ponto de partida** de R1–R6; a realidade avançou. Evidência file:line em cada linha.

| # | Claim no doc (2026-06-27) | Realidade (2026-06-28, verificada) | Verdict |
|---|---|---|---|
| 1 | ⑥ "🟡 proposta; R1 destrava o canal" | **R1–R6 todos SHIPPED+DEPLOYED+PROVEN sans-MCP** | 🔄 DRIFT → ✅ |
| 2 | R1 `touring run` = quick-win a arrancar | `cli/run.rs` → `command_table.rs:131`; adaptador sobre `ctx_execute_impl` | ✅ DONE |
| 3 | "C12 caller é **pré-requisito** de R4 `--orchestrate`" | R4 entregue via **SDK socket-RPC**; o sandbox já alcança o socket → C12 **não** era prereq; C12-caller **avaliado e recomendado NÃO-fazer** (`plan_tool_chain` é pura, já tem `touring plan-chain`) | 🔄 DRIFT-CORRECTED |
| 4 | R3 master commands = proposta | `cli/master.rs` 6 wrappers + `command_table.rs:139-171` (scout/read/health/guard/map/blast); R5 `investigate:176` | ✅ DONE |
| 5 | `audit` MCP-only ("promover MT-1") | `cli/audit.rs` adapter sobre `run_audit` → `command_table.rs:184`; Diamond 0.9757; runtime-proven | ✅ DONE |
| 6 | ③ "C11–C14 engines prontas, callers follow-up" | callers **wired**: C13→`ceg_adapter`, C11/C12/C14→`reason_tools.rs` (cross-audit 2026-06-27, 2362 tests) | 🔄 PARTIAL → ✅ |
| 7 | §7 âncora `ctx_execute_tools.rs:176` | **correto** — é `crates/touring-server/src/tools/ctx_execute_tools.rs:176` (`fn ctx_execute_impl`) | ✅ MATCHES |
| 8 | ⑦ telemetria F1/G2/F5 abertos | intacto — **não tocado** | ⏸ MATCHES (aberto) |
| 9 | ⑧ consolidação 36→13, 4550 orphans, 9 cycles | intacto — **não tocado** (re-medir antes de agir, lição L6) | ⏸ MATCHES (aberto) |
| 10 | ⑨ productization (`touring update` W12.3) | Pln2 aprovado; Fases 0-5 pendentes → **agora é o marco de retomada** | ⏸ MATCHES (aberto) |

**Descoberta arquitetural (co-evolução)**: os master commands **não reimplementam** lógica — `master.rs` faz
*forward* para os scripts Layer-3 canônicos em `~/.claude/skills/Touring/scripts/` (resolvido por
`$TOURING_SKILL_SCRIPTS` → `$HOME/.claude/skills/Touring/scripts`); `run.rs`/`audit.rs` são nativos
(padrão **MT-1** = 1 engine, 2 adaptadores MCP+CLI). Reforça a tese ①: a superfície CLI é **afordância fina**
sobre engines já existentes, não capacidade nova.

**Saldo**: nós ①–⑥ **fechados** (arco coupling → canal code-mode entregue); abertos: **⑦** (medição),
**⑧** (consolidação premium), **⑨** (productization — objetivo nuclear).

---

## 2. O arco causal (a narrativa que liga os nós)

```
①TESE ─────────► ②INVENTÁRIO ─────► ③FIXES ─────► ④DIAGNÓSTICO VIVO
(U(a) negativo;   (50 caps existem;   (C1-C14:       (27 sugestões → 0;
 afordância >      falta ALCANCE,      --brief, MCP    CONSTRUÍDO✅→
 persuasão)        não capacidade)     curation,       ATIVADO🟡→ADOTADO❌)
                                       search/route,         │
                                       code-mode)            ▼
                                          │          ⑤EVIDÊNCIA EMPÍRICA
                                          │          (297 cmds: o caminho
                                          │           PUNE — anti-STR;
                                          │           A1-A12 remediados)
                                          ▼                  │
                                   ⑥PROPOSTA ATIVA ◄─────────┘
                                   (R1 touring run = canal;
                                    R2-R3 master commands = U(a)+;
                                    R4 SDK/orchestrate)
                                          │
                          ┌───────────────┼───────────────┐
                          ▼               ▼               ▼
                   ⑦MEDIÇÃO        ⑧ARQUITETURA      (defaults
                   (mede o delta   (48→13 crates,     adotáveis)
                    DEPOIS de ⑥;   shims, 4550             │
                    F1 quick-win)  orphans → premium)      │
                          └───────────────┼───────────────┘
                                          ▼
                                  ⑨PRODUCTIZATION
                          (instalável, versionado, per-project —
                           afordância máxima por construção)
```

**Por que esta ordem é causal, não arbitrária**: medir adoção (⑦) **antes** de tornar o caminho adotável (⑥)
mede a escolha **racional** de evitá-lo. Consolidar a arquitetura (⑧) e empacotar o produto (⑨) sobre uma base
que **pune quem a usa** (⑤) propagaria o defeito. O fio: ⑥ muda `U(a)`; ⑦ prova que mudou; ⑧ deixa a base
profissional; ⑨ torna o `U(a)+` o default de todo projeto.

---

## 3. Caminho crítico (o que destrava o quê)

```
C12 caller (indutor code-mode no cli-suggester)        F1 telemetria (KPIs coupling.*,
        │  [③ backlog — engine pronta, caller follow-up] │  zero instrumentação nova) [⑦]
        ▼                                                ▼
R1 `touring run` ──► R2 invariantes ──► R3 master ──► R4 SDK/--orchestrate
(Camada 1: S,        (densidade/         commands      (Camada 2: L,
 NÃO depende de C12)  fail-soft/correção) (M)           PRECISA de C12 + cap daemon-socket)
        │                                                        │
        └──────────────────────┬─────────────────────────────────┘
                               ▼
          ⑧ consolidação 48→13 (paralelo) ──► ⑨ Productization Pln2 (Fase 0→5)
```

> **✅ ATUALIZAÇÃO 2026-06-28**: a linha `R1 → R2 → R3 → R4` está **100% entregue** (ver Seção 1.5). O que
> resta do diagrama é o ramo de baixo: **⑦ consolidação/telemetria → ⑨ Productization**.

- ✅ **R1–R4 ENTREGUES**: `touring run --code/--file/--stdin` sobre `ctx_execute_impl`
  (`tools/ctx_execute_tools.rs:176`) → `command_table.rs:131`; R3 master commands + `audit`→CLI; R4
  `--orchestrate` via **SDK socket-RPC**. Resolve a causa B de ④ (canal de code-mode utilizável).
- 🔄 **Correção (fidelidade)**: o **C12 caller NÃO era pré-requisito de R4**. R4 foi entregue via socket SDK —
  o sandbox **já alcança o socket** (não-forbidden + landlock permite `/tmp`). O C12-caller (indutor code-mode
  automático) foi **avaliado e recomendado NÃO-fazer**: `plan_tool_chain()` é função **pura**, já exposta via
  `touring plan-chain` (`reason_tools.rs`), e o LLM já planeja cadeias via decision-matrix.
- 🟡 **F1 telemetria** (ABERTO — próximo do arco): liga `coupling.*` KPIs (ema_reward, health_delta_net,
  str_bytes/emit) sobre sources já verificados — só YAML em `commitments.yaml` + derivador, **zero instrumentação
  nova**. Agora **tem o que medir**: o delta de adoção que R1–R6 acabaram de produzir.

---

## 4. Status real consolidado — pronto vs gaps abertos (priorizado)

### ✅ Pronto e verificado (a base sólida a preservar)
- **③** 14/14 C-items + MT-1/Gap2/Gap4 DONE; **⑤** A1–A12 remediados em runtime (`verify_remediation.sh` 13/13);
  **A1 `--brief`** em prod (`main.rs:252`, `wiring audit` 1.248.275→477 B); **⑧** harness Diamond 0.989;
  **⑦** τ-bench Diamond 1.0 (`docs/agentic-bench/run_bench.py`) = instrumento A/B pronto.
- **⑥ R1–R6 (2026-06-28)**: canal code-mode sans-MCP completo — `touring run` (`command_table.rs:131`),
  6 master commands (`cli/master.rs` → skill scripts), `touring investigate` + SessionStart hook,
  `audit`→CLI (`cli/audit.rs`, Diamond 0.9757); +C11/C13/C14 callers wired; +consumer-wiring fail-soft.

### ✅ Fechados desde 2026-06-27 (eram 🔴 P1 / 🟡 P2)
**R1 `touring run`** · **R4 `--orchestrate`** (SDK socket-RPC) · **R3 master commands + `audit`→CLI** ·
**C11/C13/C14 callers wired** · **C12 caller = won't-do** (engine pura; `touring plan-chain` já existe).
**⑦ F1 coupling KPIs** (2026-06-28): família `touring.coupling.*` + mecanismo **advisory** + **derivador** (`kpi.rs`), 5 commitments runtime-proven (Diamond 0.9787, `world_model_success`=0.998).
**⑦ F2 suggestion-uptake** (2026-06-28): `pending_suggestion` DashMap **in-daemon** (`cli_suggester`) + 2 counters + 6º KPI `derived:suggestion_uptake`; **offline era inviável** (VP-Scout: `activity.jsonl` só `hook_fired`, zero `tool_invoked`). Runtime-proven emitted=6/followed=1, KPI=0.167 ADVISORY, cross-call persistence.
**⑦ F3 adoption_ratio — a métrica-mãe** (2026-06-28): helper `classify_adoption` no `cli_suggester` (reusa `action_is_touring_redirect`+`detect_antipattern` shared, gated `tool_class=="bash"`) + 2 counters `adoption_{touring,antipattern}` + 7º KPI `derived:adoption_ratio` (threshold 0.50 = ponto de inversão prior-bash→prior-touring). Mesma via **online** do F2 (offline = mesma premissa falsa). Runtime-proven touring=3/antipattern=1 → **ratio=0.75 PASS**, family=7 KPIs, regressions=0.
**⑦ F5 scheduler** (2026-06-28): o writer (`touring kpi --snapshot`) já existia, faltava o gatilho. Scheduler **daemon-interno** em `daemon.rs` — task periódica (6h) + flush no `graceful_shutdown` (captura pré-reset) — reusa `dispatch_request_async("cli-kpi")`, completa o órfão `record_gate_metrics_daily_flush` (REGRA #0). Gated `TOURING_GATE_METRICS_DAILY=1` (opt-in). Runtime-proven: `daemon-ctl restart`→mtime do snapshot 20:34→20:54 **sem** `--snapshot` manual. **Arco ⑦ telemetria fechado (F1+F2+F3+F5).**

### ⚠ Gaps abertos (ordenados por desbloqueio)
| Pri | Gap | Nó | Evidência | Desbloqueia |
|---|---|---|---|---|
| ✅ | **F6/F7 loop L5 SHIPPED 2026-06-29** (⑦ em M4 — causal/auto) | ⑦ | A/B atribuível runtime-proven (`run_bench --compare` Δ**+0.1237** `coupling_helps`) + `touring kpi --refine` (motor §12 A/B-gated, emitiu `tighten_elision` actionable) + atuador `hint_demotion_bump` graduado **default-OFF** (arming vivo = decisão humana pós-A/B) | refinamento auto fechado |
| ✅ | **Task #6 compounding SHIPPED 2026-06-29 → ARMADO+VIVO 2026-07-01** (⑦ — indução ATIVA por pilar) | ⑦ | pillar induction **armado** (`TOURING_PILLAR_INDUCTION_ARMED=1` no settings.json env; daemon herda) e **runtime-proven** (1ª emissão: `touring scout HookRuntime` arg real + C03/C04/Anthropic nomeados; learning-memory idem); specific-or-absent **estrutural** (`classify_pillar` exige topic derivável — probe não-derivável → EMPTY provado); 8º KPI `pillar_induction_ratio` agora tem sinal vivo. F7 *actuator* (supressor) segue default-OFF (decisão humana pós-A/B) | mede se persuasão funciona → afordância/⑨ |
| 🟠 P2 | **Consolidação 36→13 crates** | ⑧ | 16 shims <30 LOC; high-risk: `ast` 405/`learning` 209/`cognitive` 132 refs (re-medir, lição L6) | release limpo (⑨) |
| 🟠 P3 | **~4994 wiring orphans + 9 cycles** (re-medido 2026-07-01, lição L6 aplicada) | ⑧ | Medidor corrigido: `count_orphans` (touring-analysis) tinha bug de placeholder (par consumido contado como orphan; teste blindava o bug) — fixado e alinhado à `orphan_symbols` (touring-storage, já correta, fonte do KPI). Pós-rebuild: 4994 elegíveis; spot-check da lista real 2/4 orphan-confirmado, 2/4 borderline → FP baixo-moderado. KPI `touring.wiring.orphans` → advisory até FP quantificado | composite ≥ 0.90 (gate de productization) |
| 🔵 — | **`touring update`/`component` (W12.3)** | ⑨ | ausente do command_table | propagação de update per-project (**objetivo nuclear / marco de retomada**) |

---

## 5. Correções VP-Scout aplicadas (honestidade — REGRA #21)

Os agents verificaram claims contra o código e corrigiram imprecisões da narrativa inicial:
1. **"+16-36pp harness>modelo"** NÃO vem da ① coupling-strategy (que cita +22pp §9.4) — vem do doc externo
   `2026-06-26-harness-architecture-insights.md`. Não atribuir a ①.
2. **"MCP off"** (causa B de ④) = o MCP **server** não estava conectado naquela sessão, **não** o tool ausente:
   `ctx_execute_impl` existe compilado (`:176`), está em `CURATED_TOOLS` (`mod.rs:73-104`) sem cfg guard.
   → muda o fix de R1: o problema é **canal ergonômico** (CLI), não código faltante.
3. **"4/15 systemic failures"** NÃO aparece em ④ (63 linhas) — conflação de memória; retirado até confirmar.
4. **Duas camadas de curadoria MCP**: (a) `CURATED_TOOLS` filter em `list_tools` (já funciona); (b)
   `#[cfg(feature = "mcp-curated")]` (compila W2 tools). Propostas devem ser precisas sobre qual camada tocam.

---

## 6. Ponto de retomada — ⑥ concluído, arco avança para ⑦ / ⑨

**⑥ R1–R6 está 100% entregue** (2026-06-28, sans-MCP). O ponto de retomada avançou:

1. **⑦ telemetria — FECHADO** (F1+F2+F3+F5 ✅ 2026-06-28): família `touring.coupling.*` (**7 KPIs**) + uptake/adoption
   online no `cli_suggester` + scheduler **daemon-interno** (periódica + shutdown-flush, runtime-proven). A medição de
   efetividade do canal code-mode está completa; só resta o loop L5 opcional (F6/F7).
   **Próximo nuclear: ⑨ Productization Pln2 Fase 0** (desacoplar fonte + version-pin → daemon per-project → `touring update`).
2. **⑨ Productization Pln2 — Fase 0 ✅ ENTREGUE (2026-07-01**, `validate_phase0.sh` 5/5 PASS; índice
   `docs/plans/touring-productization-pln2/00-INDEX.md`): fonte desacoplada + version-pin + versão única 30.3.0.
   **Retomada = Fase 1** (daemon per-project: RED test `w12_5` → lock per-socket `ipc.rs:60-63`) → Fase 3
   `touring update` (o gap W12.3). Endereça a **raiz** do consumer-wiring fail-soft (walk-up de project-DB
   tratando qualquer `.claude/` como boundary).
3. **⑧ Consolidação 36→13** (premium, paralelo) — re-medir antes de agir (lição L6).

A productization ⑨ e a consolidação ⑧ **agora seguem** o arco R1→R6 já concluído — não o precedem.

---

## 7. Referências (file:line âncora)

| Nó | Doc | Âncora de código |
|---|---|---|
| ① | `2026-06-26-touring-llm-coupling-strategy.md` | `ctx_execute_tools.rs:176`; `cli_suggester.rs:1823` (past-failures 57%) |
| ② | `2026-06-26-touring-capability-map.md:140` | 48 crates / 636.937 LOC / 5 modos (CLI 114, MCP 171, inferlets 17, hooks 416) |
| ③ | `2026-06-26-coupling-backlog.md` | C12 `tool_planning.rs` + `reason_tools.rs:16`; C5 `learn.rs:37`; MT-1 `tools_workflow.rs` |
| ④ | `2026-06-27-coupling-adoption-failure-diagnosis.md` | `gate_metrics.rs:655,661` (ceg_captured/sandboxed) |
| ⑤ | `2026-06-27-touring-cli-command-audit.md` §10.1/10.2 | `main.rs:252` (`apply_heavy_brief_default`) |
| ⑥ | `2026-06-27-coupling-codemode-cli-and-master-commands.md` | R1 `cli/run.rs`→`command_table.rs:131`; R3 `cli/master.rs`+`cli/audit.rs`→`:139-184`; R5 `investigate:176`; scripts `~/.claude/skills/Touring/scripts/`; engine `tools/ctx_execute_tools.rs:176` |
| ⑦ | `2026-06-27-coupling-telemetry-infrastructure.md` | `gate_metrics.rs` (142 counters); `kpi.rs:231`; `activity.jsonl` |
| ⑧ | `2026-06-25-harness-consolidation-master-plan-v3.md` + `plans/touring-47-to-13-residual/plan.md` | 48 crates; 16 shims `crates/*/src/lib.rs` |
| ⑨ | `plans/giggly-drifting-kahn.md` | `init_project.rs:113`; `ipc.rs:60-63` (lock global); `config.rs:403-460` (resolver) |

> **Nota (cross-audit 2026-06-28)**: o *base path* dos bare-filenames de ⑥/⑤ (`command_table.rs`, `cli/run.rs`,
> `cli/master.rs`, `cli/audit.rs`, `main.rs`) é **`crates/touring-server/src/`** — **não** `touring-cli` (que só
> abriga `cli_suggester.rs`). Anchors de linha verificados exatos: run:131 · scout:139 · read:145 · health:151 ·
> guard:157 · map:163 · blast:169 · investigate:176 · audit:184; A1 brief `main.rs:252` (`apply_heavy_brief_default`).

---

_Cross-reference gerado por 4 Explore agents paralelos (status real verificado) + CLI audit em contexto._
_A tese: afordância estrutural muda `U(a)`; a productization é a afordância levada ao default per-project._
