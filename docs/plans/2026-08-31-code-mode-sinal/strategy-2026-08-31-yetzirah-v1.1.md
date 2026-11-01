---
type: Strategy
title: "Strategy Complementar v1.1 — deltas pós-exploração profunda (4 blocos)"
description: "Complementa strategy-2026-08-31-yetzirah.md com 5 deltas concretos, 4 gaps resolvidos, 3 novos riscos e 2 paralelos externos. Invariante: NÃO invalida v1.0, REFINA."
plan_id: 2026-08-31-code-mode-sinal
bundle: docs/plans/2026-08-31-code-mode-sinal
okf_version: "0.1"
version: "1.1"
based_on: "strategy-2026-08-31-yetzirah.md"
tags: [strategy, yetzirah, complement, delta, exploration-deep, ultrathink]
timestamp: 2026-08-31T13:48:00-03:00
---

# Strategy Complementar v1.1 — deltas pós-exploração profunda

> **Invariante**: este documento **NÃO invalida** v1.0 — **refina**. Cada delta vira um PR ou um patch cirúrgico na v1.0. Quando Gabriel aprovar, v1.0 absorve os deltas e v1.1 vira git history.

## TL;DR dos deltas

| Delta | Mudança | Justificativa (evidência) |
|---|---|---|
| **D1** | Inventário real: **27 eventos × 55 handlers** (não 24) | `settings.json` parseado (Bloco 2 — código executável) |
| **D2** | Tier classification por **capability** (não latência) | `touring-language` crate (W14.1) é o pattern canônico |
| **D3** | SDK surface gerada **do journal**, não hardcoded | `run_journal.jsonl` já existe (BestPracticesGate lê) |
| **D4** | PostToolUse + `prompt-enhance` (UserPromptSubmit) = paralelo exato | 13 PostToolUse + 2 UserPromptSubmit handlers observados |
| **D5** | BestPracticesGate é **Warn** (não fail-closed) — F5 vira "promover para Warn-severo" não "fail-closed" | Source `best_practices.rs:72-73` — `severity: Warn` |

## Exploração executada (4 blocos paralelos)

### Bloco 1 — Inventário + SDK source

**P1 — `~/.claude/settings.json` parseado:**
- **27 tipos de eventos** (CC 2.1.x registra todos os hooks da Anthropic, não 24 como chutei)
- **55 handlers** total entries
- **Top eventos por volume**: PostToolUse=13, PreToolUse=12, SessionStart/Stop/PreCompact/PostCompact/UserPromptSubmit=2 cada, resto=1

**P3 — `crates/touring-code/src/` mapeado:**
- Não há `sdk.rs` dedicado
- SDK vive em `crates/touring-code/src/ast/` (api_cascade, code_gen_workflow, speculate, rust_semantic, etc.)
- O "orchestrate" do code mode (SDK in-sandbox) é um **prelude tipado** dentro do prompt do `--code` mode

### Bloco 2 — Payload shape + SEG-2 + CEG profile

**P2 — handlers mais ricos identificados:**

| Evento | Matcher | Comando |
|---|---|---|
| PreToolUse | Read / Edit / Write | `$HOME/.claude/hooks/touring-hook pre-{read,edit,write}` |
| PostToolUse | Read / Edit / Write | `$HOME/.claude/hooks/touring-hook post-{read,edit,write}` |
| UserPromptSubmit | — | `touring-hook prompt-enhance` + `loop_outer_arm.py` |
| Stop | — | `auto_compact.sh` + `loop_stop_guard.py` |
| SessionStart | — | `session_env_setup.sh` + `loop_resume.py` |

**`prompt-enhance` (UserPromptSubmit) é o paralelo exato do que o canal precisa fazer** — injeta `additionalContext` no prompt do modelo antes do processamento. É literalmente o mesmo contrato que o canal teria com o programa in-sandbox.

**P4 — SEG-2 paths confirmados:** todos os hooks vivem em `/home/gabrielgadea/.claude/skills/...` (dentro do allowlist SEG-2). SEG-2 é o allowlist real (não `~/.claude/{skills,rules,agents,commands}` como eu disse — é mais restrito: apenas `skills/`).

**P6 — CEG profile do code mode:**
- `profile: Sandboxed` (não Trusted)
- `Landlock: KernelEnforced`
- `cgroup v2: group-level capping available`
- `rlimit: 3 per-process classes capped`
- `subprocess capability: DENIED por padrão, waived para shell/python via decisão Gabriel 30/08/2026`

### Bloco 3 — Gate design + F2.1 + KPIs existentes

**P5 — `BestPracticesGate` deep:**
- Source: `crates/touring-quality/src/builtins/best_practices.rs`
- Trait: `impl Gate for BestPracticesGate`
- Signature: `fn check(&self, change: &Change) -> GateOutcome`
- Helpers: `declaration_site()` (lê docs/code-mode.md ou rule global), `adherence_counts()` (lê `~/.claude/touring/run_journal.jsonl`)
- **Severidade: `Warn`** (não bloqueia; só alerta)
- ID: `GateId::BestPractices`
- Constantes: `ADHERENCE_FLOOR = 0.8`, `MIN_RUNS = 20`

**Insight:** o BestPracticesGate JÁ lê o journal. Adicionar contador de `signal_use` é trivial — só estender `adherence_counts()` para contar calls ao SDK tipado.

**P7 — F2.1 OWASP:** roda em outro crate (`touring-analysis/src/quality/` provavelmente). Para adicionar check do SDK, criar regra no BestPracticesGate ou estender F2.1 com regex `subprocess.run(shell=True)` específica para o SDK.

**P10 — KPIs REUTILIZÁVEIS:**

| KPI | Fonte | Baseline | Threshold | Status |
|---|---|---|---|---|
| `touring.code_mode.adoption_ratio` | `gate-metrics` | 1.6% | ≥ 0.10 | ADVISORY |
| `touring.code_mode.economy_ratio` | `run_journal.jsonl` | 84% | ≥ 0.50 | **PASS** |
| `touring.code_mode.adoption_ratio` | `gate-metrics` | 1.6% | ≥ 0.10 | ADVISORY |
| `touring.code_mode.inspect_burst_share` | `gate-metrics` | 0.21 | ≤ 0.75 | **PASS** |
| `touring.code_mode.economy_ratio` | `run_journal.jsonl` | 0.84 | ≥ 0.50 | **PASS** |
| `touring.code_mode.arm_native` | `code_mode_arm.json` | — | ≥ 0.50 | STUB |
| `touring.code_mode.arm_both` | `code_mode_arm.json` | — | ≥ 0.50 | STUB |
| `touring.code_mode.arm_code` | `code_mode_arm.json` | 0.34 | ≥ 0.50 | ADVISORY |
| `touring.code_mode.parallel_runs` | `run_subcalls.jsonl` | 5 | ≥ 1 | **PASS** |

**Falta criar:** `touring.code_mode.signal_use.composite` (aderência específica ao uso do canal).

**Composição proposta:**
```
composite = 0.5 * (signal_use_runs / total_runs)
          + 0.3 * (economy_ratio já medido)
          + 0.2 * (adoption_ratio já medido)
```

### Bloco 4 — Tier prior-art + paralelos externos

**P8 — `touring memory recall` retorna 20 entries relevantes:**

| Pattern | Local | Aplicação |
|---|---|---|
| **Tier-by-capability (1-4)** | `touring-language` crate, W14.1 | Pattern canônico para nosso tier |
| **Tier-by-latency** | `touring-hooks/src/lib.rs:70 HOOKS_MODE` | NÃO é o que queremos |
| **SignalDiff comparison** | `touring-analysis/src/quality/signal/diff.rs` (W2 P4) | Trending + classification — útil para "tier subiu/desceu" |
| **LatencyMarker morto** | `context-mode:think-in-code:98pct-reduction` | REGRA #0 — potencializar infraestrutura morta |
| **PreRead think-in-code** | `context-mode:think-in-code` (W24) | **PRIOR-ART DIRETO**: 5+ reads consecutivas → injeta sugestões |

**P9 — Anthropic Programmatic Tool Calling (paralelo canônico):**

| Citação verbatim | Implicação para nossa estratégia |
|---|---|
| "11% performance improvement, 24% fewer tokens (BrowseComp, DeepSearchQA)" | **Argumento de venda para F6** — o canal paga o custo em 24% de economia de tokens |
| "single script runs all + filters + returns only relevant" | Confirma nossa arquitetura — o programa in-sandbox é o filtro |
| "shrinking what Claude needs to reason over from hundreds of kilobytes down to a handful of lines" | O canal **é exatamente isso** para o code mode: filtra o sinal antes de o modelo processar |
| "allows Claude to write code that calls your tools programmatically within a code execution container" | O sandbox do code mode É essa ferramenta |

## Decisões aprovadas por Gabriel (31/08 13:50 BRT)

| # | Decisão | Aprovação | Impacto |
|---|---|---|---|
| **D5** | BestPracticesGate Warn + KPI advisory (promover após 7-14d) | ✅ aprovado | Gate continua Warn; KPI `signal_use.composite` advisory; promoção só após calibração |
| **D2** | 3 tiers + 4 eventos movidos para Tier 1 Essencial | ✅ aprovado com ajuste | post-edit, post-write, post-read, cli-suggest SOBEM para Essencial; validar com 7d journal |
| **D3** | SDK híbrido (nomes hardcoded + tipos gerados) | ✅ aprovado | Nomes em `sdk.rs`; tipos via `gen_sdk.py` do journal em CI |
| **P9** | 2 KPIs secundários (≥20% tokens / ≥50% rounds) | ✅ aprovado | F6 vira 6 critérios AND |

### Tier classification FINAL (D2 com ajuste de Gabriel)

| Tier | Eventos | Por quê |
|---|---|---|
| **Tier 1 — Essencial** (9) | `pre-edit`, `pre-write`, `pre-read`, `prompt-enhance`, `loop_resume`, **`post-edit`**, **`post-write`**, **`post-read`**, **`cli-suggest`** | Sempre invocado; sem eles o modelo opera cego. **Gabriel moveu os 4 post-* + cli-suggest para Essencial** |
| **Tier 2 — Útil** (1) | `loop_outer_arm` | Invocado sob demanda; ativa o work-outer quando sessão é substantiva |
| **Tier 3 — Opcional** (17) | `pre-compact`, `post-compact`, `stop:*`, `subagent-*`, `file-changed`, `task-*`, todos os 1-handlers restantes | Audit/instrumentation; latência > 10ms ou raramente úteis |

**Nota:** a assimetria 9+1+17 reflete a observação de Gabriel: Tier 2 fica enxuto após os movimentos (só loop_outer_arm). Validação 7d journal pode redistribuir.

---

## Cadeia de injeção F1.5 (atualização 31/08 14:25 BRT)

`settings.json` é handshake, não a cadeia completa. A injeção real passa por 7 camadas:

```
Claude Code tool call
    ↓
[1] ~/.claude/hooks/cc-*.sh (wrappers bash, 1 linha, setam PROFILE)
    ↓
[2] ~/.claude/hooks/touring-hook (shim bash 4-layer: project→toolchain→dev→fail-open)
    ↓
[3] ~/projects/touring/target/release/touring-hook (binário Rust v11.0, 24 subcommands)
    ├─ Fast-path: Unix socket → touring-daemon (circuit breaker)
    └─ Standalone: HookRuntime + bridges
    ↓
[4] Bridges em touring-hooks-core/src/bridges/ (ast/aco/cognitive/knowledge_symbol/nlp)
    ↓
[5] stdout JSON: {"hookSpecificOutput": {"hookEventName": "...", "additionalContext": "..."}}
    ↓
[6] Claude Code consome + injeta no contexto do modelo
    ↓
[7] Modelo recebe o sinal ANTES da tool call (PreToolUse) ou como observação (PostToolUse)
```

**Tier 1 Essencial CORRIGIDO (F1.5)**: 15 subcommands (não 9 como v1 catalogou) — inclui pre-bash, post-bash, pre-grep, pre-glob, pre-edit-prevention, post-tool-rl, session-start.

**Implicação para F3 (Yetzirah)**: o canal espelha essa cadeia, mas o programa in-sandbox consome o JSON direto (sem passar pelo modelo). F2 (S4 surface híbrido) precisa ter 15 funções hardcoded, não 8.

Detalhamento completo dos bridges: `inventory-hooks.md` v1.5 (F1.5 cadeia de injeção).

---

## DELTAS concretos sobre v1.0

### D1 — F1 inventário: 27 × 55 (não 24)

**v1.0 dizia:** "24 hooks"  
**v1.1 corrige para:** "27 eventos × 55 handlers"  
**Impacto:** F2 (S4 surface) precisa cobrir 55 handlers read-only, não 24

### D2 — F1 tier classification: by-capability

**v1.0 dizia:** Essencial / Útil / Opcional  
**v1.1 mantém** mas adiciona:

| Tier | Capability | Eventos |
|---|---|---|
| **Tier 1 — Essencial** | sempre disponível, latência < 1ms | `pre-edit`, `pre-write`, `pre-read`, `user-prompt-submit:prompt-enhance`, `session-start:loop_resume` |
| **Tier 2 — Útil** | sob demanda, latência 1-10ms | `post-edit`, `post-write`, `post-read`, `cli-suggest` (pillar induction), `user-prompt-submit:loop_outer_arm` |
| **Tier 3 — Opcional** | audit/instrumentation, latência > 10ms | `pre-compact`, `post-compact`, `stop:*`, `subagent-*`, `file-changed`, `task-*`, todos os 1-handlers restantes |

**Justificativa:** pattern canônico é `touring-language` Tier1-4 por **capability**, não por latência (`LatencyTier` é diferente — tier por QUANTO TEMPO leva, não por O QUÊ faz).

### D3 — F2 SDK surface: gerar do journal

**v1.0 dizia:** "S4 surface gerada (8 → N hooks)"  
**v1.1 refina para:** "SDK gerada DO journal (`~/.claude/touring/run_journal.jsonl`) — lista tipada emerge dos calls observados"

**Razão:** o BestPracticesGate já lê o journal (`adherence_counts()`); adicionar `signal_use_counts()` é trivial. SDK tipada = lista derivada do journal, não inventada.

### D4 — F3 hook PostToolUse + prompt-enhance

**v1.0 dizia:** "Hook PostToolUse + arquivo espelho"  
**v1.1 adiciona:** "Replicar o contrato de `prompt-enhance` (UserPromptSubmit) — injeta `additionalContext` antes do modelo processar"

**Paralelo exato:** `prompt-enhance` é literalmente o que queremos que o programa in-sandbox faça — ler sinais e devolver contexto adicional antes do modelo raciocinar. Anthropic chama isso de "filter-before-context" (P9).

### D5 — F5 BestPracticesGate: Warn-severo, NÃO fail-closed

**v1.0 dizia:** "gate fail-closed (bloqueia se aderência < 1.0)"  
**v1.1 corrige para:** "gate Warn-severo com threshold ≥ 0.8 (similar ao BestPracticesGate atual com adherence_counts)"

**Razão técnica:** o BestPracticesGate tem `severity: Warn` por design (linha 73). Promover para fail-closed exigiria shift de policy que afetaria o 13-gate aggregate (elite_aggregate.py). Isso é uma **decisão Gabriel** — não posso tomá-la sozinho.

**Workaround:** o `touring.kpi.code_mode.signal_use.composite` (criado em F6) pode ficar **advisory** no `elite_aggregate.py` (similar a `code_mode.adoption_ratio`). Quando passar ≥ 0.80 por 7 dias, **proposta** de promoção a fail-closed (não auto-aplicada).

## Gaps RESOLVIDOS pela exploração

| Gap da v1.0 | Resolvido por |
|---|---|
| "24 hooks" (chute) | P1 — inventário real 27 × 55 |
| "8 → N hooks no S4" (chute) | P1 —55 handlers; SDK gerada do journal, não hardcoded |
| "SEG-2 concede `~/.claude/{skills,rules,agents,commands}`" (chute) | P4 — SEG-2 = apenas `~/.claude/skills/` (mais restrito) |
| "BestPracticesGate estensível" (chute) | P5 — confirmado: trait-based + Warn + já lê journal |
| "Tier Essencial/Útil/Opcional" (chute) | P8 — `touring-language` Tier1-4 é pattern canônico |
| "Argumento para F6 (>0.80 ≥7d)" (sem base) | P9 — Anthropic: 11% improvement + 24% fewer tokens é prova externa |

## NOVOS riscos identificados

### R-NOVO-1 — BestPracticesGate Warn não bloqueia

**Severidade:** média  
**Mitigação:** complementar com KPI `touring.code_mode.signal_use.composite` advisory; promover só após 7 dias estável. **Decisão Gabriel** sobre fail-closed.

### R-NOVO-2 — LatencyMarker morto

**Severidade:** baixa (infraestrutura existente não usada)  
**Mitigação:** REGRA #0 — potencializar o LatencyMarker como **counter de latência** dentro do canal (não escrever do zero).

### R-NOVO-3 — Adoption_ratio mascara (sobe igual para usos triviais)

**Severidade:** média  
**Mitigação:** já existe `economy_ratio` (PASS em 84%) para detectar isso. Usar **composite** = adoption + economy + signal_use (calibrado).

## NOVOS argumentos para defender F6 (Anthropic P9)

| Argumento | Fonte |
|---|---|
| **24% fewer tokens em benchmarks** | BrowseComp/DeepSearchQA — redução mensurável |
| **11% performance improvement** | Mesmo benchmark |
| **Filter-before-context** | "shrinking what Claude needs to reason over from hundreds of kilobytes down to a handful of lines" |
| **Single script > N round-trips** | "20 employees = 1 script vs 20 round-trips" |

**Adicionar ao F6 como KPIs secundários:**
- `code_mode_signal_token_reduction_pct` (target ≥ 20% — metade do ganho Anthropic)
- `code_mode_signal_rounds_reduction_pct` (target ≥ 50% — N round-trips → 1 script)

## PRIORIZAÇÃO REVISADA (F1-F6 ordem inalterada, conteúdo refinado)

| Fase | Conteúdo refinado | Dependência |
|---|---|---|
| **F1** | Inventário 27 × 55 + tier by-capability (3 tiers) | nenhuma |
| **F2** | SDK tipada gerada do journal (não hardcoded) | F1 |
| **F3** | Hook PostToolUse (13 handlers) + replicar `prompt-enhance` contrato | F2 |
| **F4** | SDK Protocol Python tipado + LatencyMarker potencializado | F2 |
| **F5** | BestPracticesGate estendido com regra signal_use (Warn-severo) | F4 |
| **F6** | 4 critérios AND originais + 2 KPIs secundários (token_reduction, rounds_reduction) | F5 |

## Próximo passo (gate humano revisado)

Gabriel, **5 deltas + 4 gaps + 3 riscos + 2 paralelos** refinam a v1.0. Decisões pedidas:

1. **D5 — BestPracticesGate Warn vs fail-closed?**  
   v1.0 assumia fail-closed; v1.1 corrige para Warn-severo (gate atual é Warn por design). Quer promover para fail-closed (afeta 13-gate aggregate) ou aceitar Warn + KPI advisory?

2. **D2 — Tier by-capability (3 tiers) OK?**  
   Tier 1 Essencial (5 eventos) · Tier 2 Útil (5 eventos) · Tier 3 Opcional (17 eventos). Outra classificação?

3. **D3 — SDK gerada do journal OK?**  
   Ou prefere SDK hardcoded para tipagem explícita?

4. **P9 — Paralelos Anthropic contam como argumento?**  
   Adiciono os 2 KPIs secundários (token_reduction_pct, rounds_reduction_pct) ao F6?

Quando aprovar, v1.1 vira o strategy-doc canônico e v1.0 vira git history.

---

## Status de execução (31/08 14:15 BRT)

| Fase | Estado | Artefato | Próximo passo |
|---|---|---|---|
| **F1** | ✅ done (quality 0.92) | `inventory-hooks.md` | Validação 7d journal |
| **F2** | ⏳ pending (priority 64 — retomada) | — | `sdk.rs` + `gen_sdk.py` |
| **F3** | ⏳ pending (depends F2) | — | hook PostToolUse sync + prompt-enhance |
| **F4** | ⏳ pending (depends F2) | — | SDK tipada Protocol |
| **F5** | ⏳ pending (depends F4) | — | BestPracticesGate Warn-severo |
| **F6** | ⏳ pending (depends F5) | — | 6 critérios AND |

**Decisão de Gabriel (31/08 14:13 BRT)**: parar em F1 (entrega limpa). F2-F6 aguardam retomada futura. DAG registrada em `task_1788196388043002698` com depends-on chain preservado.

---

_v1.1 — 2026-08-31 13:48 BRT | Autor: TACO work-outer (3a87ce4a) | 4 blocos exploração + 25 findings + Anthropic P9 paralelo | F1 entregue; F2-F6 aguardam retomada_