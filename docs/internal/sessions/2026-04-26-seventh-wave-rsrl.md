# Seventh Wave — rsrl Comparative RL Analysis (Documentation-Only)

**Date**: 2026-04-26 | **Session**: TACO L3 (no engineer phase) | **Skill**: Touring v4.19.0

## Objetivo

Análise profunda do crate [rsrl](https://crates.io/crates/rsrl) (Rust Reinforcement Learning,
v0.8.1) + extração de insights/estratégias para potencializar Touring's RL stack.

## Verdict: SKIP rsrl as workspace dep

| Criterion | Verdict | Razão |
|-----------|---------|-------|
| Maintenance | **FAIL** | Último commit 2020-06-18 (~6 anos), 21 open issues, 0 PRs |
| Compat workspace | **FAIL** | ndarray 0.13/0.14 era vs workspace pin 0.16 (incompatível) |
| System reqs | **FAIL** | BLAS (openblas-sys) — Touring atualmente sem |
| Algorithm gap fill | **PARTIAL** | Eligibility traces JÁ EXISTEM em qtable.rs (Replacing) |
| Touring RL stack richness | **PASS (Touring wins)** | Touring tem 9 RL + 8 bandit files vs rsrl's generic toolkit |

## Sumário Executivo

| ID | Deliverable | Arquivo | LOC |
|----|-------------|---------|-----|
| D1 | Reference doc completa | `~/.claude/skills/Touring/references/touring-cli-rl-stack.md` | ~310 |
| D2 | SKILL.md addendum v4.19.0 | `~/.claude/skills/Touring/SKILL.md` | ~50 |
| D3 | Bug fix collateral (REGRA #0) | `crates/touring-server/src/cli/tasksfile.rs` | 3 sites |
| **TOTAL** | | **3 arquivos, 1 fonte de bug** | **~360 LOC docs** |

## Resultados

- `cargo build -p touring-server`: EXIT:0 (após fix tasksfile.rs)
- Tests: 3785 baseline preservado (zero código RL modificado)
- Orphan baseline: 9106 preservado
- 6 erros pré-existentes resolvidos colateralmente

## Análise Detalhada

### rsrl Status (verificado 2026-04-26)

| Métrica | Valor |
|---------|-------|
| Latest version | 0.8.1 |
| Last commit | June 18, 2020 (~6 anos stale) |
| Stars / Forks | 205 / 15 |
| Open issues / PRs | 21 / 0 |
| Documentation coverage | 22.99% |
| CI infrastructure | Travis CI (deprecated) |
| BLAS dependency | Required (openblas-sys) |
| ndarray version | 0.13/0.14 era — incompatível com workspace 0.16 |
| License | MIT |

### Modules e Algoritmos rsrl

```
rsrl::control::td::*    → QLearning, SARSA, ExpectedSARSA, QLambda, SARSALambda, QSigma, GreedyGQ, PAL
rsrl::control::ac::*    → Actor-Critic variants
rsrl::control::nac      → Natural Actor-Critic
rsrl::control::cacla    → Continuous Actor-Critic Learning Automata
rsrl::control::mc       → Monte-Carlo policy gradient
rsrl::fa::*             → Linear, Fourier basis, Tile coding (function approximators)
rsrl::policies::*       → Greedy, etc
rsrl::traces::*         → Accumulating, Replacing, Dutch (eligibility traces)
rsrl::prediction::*     → Prediction agents
rsrl::params, linalg, logging
```

### Touring RL Stack (mapeado via grep)

```
touring-learning/src/
├── bandit/                    # 8 files — production bandits
│   ├── linucb.rs              # LinUCBBandit (1163 LOC, 8 arms × 25 dims)
│   ├── granularity.rs         # GranularityBandit (CILA-aware)
│   ├── transfer.rs            # TransferLinUCB
│   ├── reminder_bandit.rs     # ReminderBandit
│   ├── ast_enriched.rs        # AstEnrichedBandit
│   ├── ast_features.rs        # AST feature extraction
│   ├── adaptive_alpha.rs      # AdaptiveAlpha exploration tuning
│   └── mod.rs                 # ContextualBandit trait
├── rl/                        # 9 files — value/policy-based RL
│   ├── qtable.rs              # Q-learning + Replacing traces (λ=0.9)
│   ├── double_qtable.rs       # Double Q-learning
│   ├── actor_critic.rs        # Actor-Critic
│   ├── ndarray_mlp.rs         # MLP function approximator
│   ├── tiny_transformer.rs    # Markov + transformer for tool prediction
│   ├── burn_transformer.rs    # Burn deep learning (feature-gated)
│   ├── per_buffer.rs          # Prioritized Experience Replay (Schaul 2016)
│   ├── curiosity.rs           # Count-based curiosity bonus
│   └── risk_adjusted.rs       # Blast-aware Q-learning
└── online_learning/
    └── ftrl.rs                # Follow The Regularized Leader

touring-cognitive/src/
├── mcts.rs                    # MCTSEngine + PheromoneMCTS (657 LOC)
└── cognitive_mcts.rs          # GraphInformedMCTS
```

### Gap Analysis — Algoritmos rsrl que Touring NÃO tem

Apenas estes não têm equivalente em Touring (mas nenhum tem driver atual):

| Algorithm | Categoria | Driver Touring? |
|-----------|-----------|-----------------|
| QSigma (n-step interpolation) | TD control | None |
| GreedyGQ (gradient TD) | Off-policy convergence | None (research-level) |
| Natural Actor-Critic | Variance reduction | None |
| CACLA | Continuous actions | Touring é discrete |
| Fourier basis | Smooth FA | Touring usa MLP |
| Tile coding | Discrete state generalization | Touring usa sparse Q-table |
| Generic Trace<K> (Accumulating, Dutch) | A/B test trace kinds | None observed |

**Decision**: Sem driver = sem implementação. Se emerger driver futuro, implementar nativamente (~50-150 LOC each); NÃO adicionar rsrl como dep.

## Discovery Crítica via VP-Scout

Antes de scout pesado, grep revelou em `touring-learning/src/rl/qtable.rs:1`:
```
//! Q-table implementation with eligibility traces.
```

Linha 533 mostra implementation **REPLACING** traces:
```rust
self.traces.insert(sa, 1.0);
```

**Impact**: Hipótese inicial "Touring needs eligibility traces" foi INVALIDADA — Touring já tem.
PIVOT do plan para DOCS-ONLY (escopo conservador, sem over-engineering).

## Bug Fix Collateral (REGRA #0 Potencialização)

`cargo check --workspace` retornou 6 erros em `tasksfile.rs:38, 45, 87, 94, 96, 97`:
- E0599: `cloned()` em `Option<&str>` (str não tem método `cloned`)
- E0277: usar `path: &str` em format requer `Sized`

**Root cause**: `args.iter().find(|a| ...).and_then(|s| s.strip_prefix(...))` retorna `Option<&str>`. `.cloned()` em `Option<&str>` tenta clonar `str` (unsized).

**Fix**: substituir `.cloned()` → `.map(str::to_string)`. 3 sites em sequência.

Esta crate `tasksfile` foi adicionada recentemente ao workspace (linha 23 root Cargo.toml) mas nunca passou cargo check completo. Fix permitiu Wave 7 build.

## Lessons Learned

### Memory Pattern (memorize for future waves)

Quando análise de crate revela:
1. **Crate último commit > 2 anos** → abandonment risk alto
2. **Workspace tem stack mais rica que crate** → integration cost > benefit
3. **Version conflicts** (ndarray, syn, etc) → hard blocker

**Default verdict**: **DOCS-ONLY** (Wave 6 BugStalker + Wave 7 rsrl). Documentation evita re-evaluation em waves futuras.

### Methodology — Pre-Scout Ultrathink (3rd consecutive wave)

Padrão refinado em Waves 6 + 7:
- WebFetch + grep paralelos **antes** de scout agent
- Sequential-thinking para hipótese refinement
- Verdict baseado em ground truth, não scout output

Saving: ~30-45min por wave vs scout pesado paralelo.

### Trade-off documentado

**Não-deliverables intencionais**:
- Adicionar rsrl como dep: 6 anos abandono + ndarray conflict + BLAS req + 0 touring commands ativados
- Re-implementar Accumulating/Dutch traces: zero use case driver
- Refactor qtable.rs para `Trace<K>` genérica: scope L3 sem driver

**Inspiração arquitetural** (sem implementação):
- `Policy` trait abstraction (rsrl's `policies::*`)
- `FunctionApproximator` trait (Touring tem 3 implementations sem trait shared)
- Algorithm taxonomy modules (`control::{td, ac, mc}` mirroring textbook RL)

## Comparison Wave 6 vs Wave 7

| Aspect | Wave 6 (BugStalker) | Wave 7 (rsrl) |
|--------|---------------------|---------------|
| Target | GitHub repo (binary CLI) | crates.io crate (library) |
| Verdict | INTEGRATE-AS-DOCS | SKIP-WITH-DOCS |
| Reason | Orthogonal mission (debugger vs static analysis) | Touring stack richer + abandonment + version conflict |
| Deliverables | 3 (reference + helper script + SKILL section) | 3 (reference + SKILL section + bug fix) |
| Code mods Touring | 0 | 0 (RL); 3 sites collateral fix tasksfile |
| Docs LOC | ~445 | ~360 |
| Methodology | Pre-scout ultrathink | Pre-scout ultrathink + grep discovery pivot |

## See Also

- Reference doc completa: `~/.claude/skills/Touring/references/touring-cli-rl-stack.md`
- rsrl upstream: https://github.com/tspooner/rsrl (dormant since 2020-06)
- Sutton & Barto, "Reinforcement Learning: An Introduction" (canonical RL reference)
- Wave 6 (BugStalker): `~/.claude/rust/docs/2026-04-26-sixth-wave-bugstalker.md`
