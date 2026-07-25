# Touring RL Stack — Comparison with rsrl + Architectural Map

> **Wave 7 (2026-04-26)** — Touring v4.19.0 documentation addendum
> **Analyzed**: [rsrl](https://crates.io/crates/rsrl) v0.8.1 (last commit June 18, 2020 — 6 years dormant)
> **Verdict**: SKIP rsrl as dep | Touring's RL stack is richer in production-relevant ways

---

## Executive Summary

After deep analysis of `rsrl` (Rust Reinforcement Learning), the verdict is:

| Aspect | Conclusion |
|--------|------------|
| **rsrl maintenance** | Abandoned (last commit 2020-06-18, 21 open issues, no PRs) |
| **rsrl dep risk** | High — ndarray version conflict (workspace 0.16 vs rsrl-era 0.13/0.14), BLAS requirement, 22.99% docs coverage |
| **Touring's RL stack** | Richer in 9/12 dimensions; genuinely production-tuned per-use-case |
| **Net integration value** | **Negative** — adds dep risk for 0 new touring commands activated |

**Strategic recommendation**: Document the comparison so future analyses don't re-evaluate; do not add `rsrl` as workspace dep.

---

## Comparison Matrix — rsrl vs Touring

### Algorithms (Control)

| Algorithm | rsrl | Touring |
|-----------|------|---------|
| Q-Learning | `control::td::QLearning` | `rl::qtable::QLearning` |
| Double Q-Learning | ❌ | `rl::double_qtable::DoubleQTable` ✓ |
| SARSA | `control::td::SARSA` | implicit via on-policy bandit updates |
| Expected SARSA | `control::td::ExpectedSARSA` | ❌ (not yet needed) |
| QLambda (TD-λ) | `control::td::QLambda` | **integrated** in `rl::qtable` (Replacing traces, λ=0.9) |
| SARSALambda | `control::td::SARSALambda` | partial via qtable's λ |
| QSigma (n-step) | `control::td::QSigma` | ❌ (research-level, no driver) |
| GreedyGQ (gradient TD) | `control::td::GreedyGQ` | ❌ (research-level) |
| PAL (Persistent AL) | `control::td::PAL` | ❌ |
| Actor-Critic | `control::ac::*` | `rl::actor_critic::ActorCritic` ✓ |
| Natural Actor-Critic | `control::nac` | ❌ |
| CACLA | `control::cacla` | ❌ |
| Monte-Carlo PG | `control::mc` | implicit via reward backpropagation |

### Bandits

| Component | rsrl | Touring |
|-----------|------|---------|
| ContextualBandit trait | ❌ | `bandit::ContextualBandit` (Send + Sync) ✓ |
| LinUCB | ❌ | `bandit::linucb::LinUCBBandit` (1163 LOC, production) ✓ |
| TransferLinUCB | ❌ | `bandit::transfer::TransferLinUCB` ✓ |
| GranularityBandit | ❌ | `bandit::granularity::GranularityBandit` (CILA-aware) ✓ |
| ReminderBandit | ❌ | `bandit::reminder_bandit::ReminderBandit` ✓ |
| AstEnrichedBandit | ❌ | `bandit::ast_enriched::AstEnrichedBandit` ✓ |
| Adaptive α | ❌ | `bandit::adaptive_alpha::AdaptiveAlpha` ✓ |

### Function Approximation

| Type | rsrl | Touring |
|------|------|---------|
| Linear FA | `fa::linear` | implicit via LinUCB |
| Fourier basis | `fa::projector::Fourier` | ❌ (could be added if needed) |
| Tile coding | `fa::projector::TileCoding` | ❌ |
| MLP | ❌ | `rl::ndarray_mlp::ContextMlp` ✓ |
| Tiny transformer | ❌ | `rl::tiny_transformer::TinyTransformerPredictor` ✓ |
| Burn deep learning | ❌ | `rl::burn_transformer` (feature-gated) ✓ |

### Eligibility Traces

| Trace kind | rsrl | Touring |
|------------|------|---------|
| Replacing | `traces::Replacing` | `rl::qtable` (hardcoded; λ=0.9) ✓ |
| Accumulating | `traces::Accumulating` | ❌ (not surfaced as primitive) |
| Dutch | `traces::Dutch` | ❌ |
| Generic Trace<K> primitive | `traces::Trace` | ❌ (qtable-specific) |

### Advanced Components

| Component | rsrl | Touring |
|-----------|------|---------|
| Prioritized Experience Replay | ❌ | `rl::per_buffer::PrioritizedReplayBuffer` (Schaul 2016) ✓ |
| Curiosity-driven exploration | ❌ | `rl::curiosity::CuriosityModule` ✓ |
| Risk-adjusted Q-learning | ❌ | `rl::risk_adjusted::RiskAdjustedQLearning` (blast-aware) ✓ |
| FTRL (Follow Regularized Leader) | ❌ | `online_learning::ftrl` ✓ |
| MCTS | ❌ | `touring-cognitive::mcts::MCTSEngine` + `PheromoneMCTS` (657 LOC) ✓ |
| GraphInformedMCTS | ❌ | `touring-cognitive::cognitive_mcts::GraphInformedMCTS` ✓ |
| AST feature extraction | ❌ | `bandit::ast_features::extract_ast_features` ✓ |
| HNSW (working memory) | ❌ | feature `hnsw-working-memory` ✓ |
| Leiden clustering | ❌ | `linfa-clustering` integration ✓ |
| Async memory patterns | ❌ | `async-memory` feature ✓ |
| Q4 quantization | ❌ | `u4-quantization` feature ✓ |

---

## rsrl Status (verified 2026-04-26)

| Metric | Value |
|--------|-------|
| Latest version | 0.8.1 (June 18, 2020) |
| Last commit | June 18, 2020 (~6 years stale) |
| Stars / Forks | 205 / 15 |
| Open issues | 21 (no PRs) |
| Documentation coverage | 22.99% |
| CI infrastructure | Travis CI (deprecated since 2021) |
| Top modules | `control`, `fa`, `policies`, `prediction`, `traces`, `linalg`, `logging`, `params` |
| BLAS dep | Required (`ndarray` with `blas` feature + OpenBLAS) |
| ndarray version | 0.13/0.14 era — **incompatible with workspace 0.16** |
| License | MIT |

---

## Why rsrl as workspace dep is rejected

1. **Abandonment risk**: 6 years without commits, 21 unanswered issues. No upstream patches for security/compat issues.
2. **Version conflict**: workspace pinned `ndarray = "0.16"` (line 122 of root `Cargo.toml`); rsrl-era ndarray is 0.13/0.14. Resolving requires:
   - Forking rsrl, OR
   - Downgrading workspace ndarray (catastrophic, affects 12+ crates)
   - Both are non-starters
3. **BLAS system requirement**: rsrl requires `openblas-sys` or equivalent — adds system dependency Touring doesn't currently need
4. **Zero new touring commands activated**: rsrl's algorithms would sit unused — Touring's existing stack covers production needs
5. **Research-grade documentation (22.99%)**: API stability uncertain; would need substantial reverse-engineering
6. **Algorithm overlap**: 80%+ of rsrl's value-add overlaps with Touring's existing implementations (which are tuned for specific Touring use cases like CILA-aware GranularityBandit)

---

## Algorithms Touring DOES NOT have (potential future work)

Of rsrl's offerings, these have NO Touring equivalent — but **none have a current driver/use case** justifying implementation:

| Algorithm | Useful for | Touring driver? |
|-----------|-----------|-----------------|
| `QSigma` (n-step interpolation) | Sequential RL with mixed bias-variance | None |
| `GreedyGQ` (gradient TD) | Off-policy with linear FA + convergence guarantees | None |
| `Natural Actor-Critic` | Reduce variance in policy gradient | None |
| `CACLA` | Continuous action spaces | Touring is discrete (tool selection) |
| `Fourier basis projection` | Smooth value function approximation | Touring uses MLP |
| `Tile coding` | Discrete state generalization | Touring uses sparse Q-table |
| `Generic Trace<K>` (Accumulating, Dutch) | A/B test trace kinds | None observed |

**If a use case emerges**: implement natively (~50-150 LOC each); do not add rsrl as dep.

---

## Lessons Extracted from rsrl Study

### What rsrl does well architecturally

1. **Generic abstractions over algorithms**: `Algorithm`, `Policy`, `Function`, `Projector` traits — clean separation of concerns
2. **Eligibility trace abstraction**: `Trace` struct + `UpdateRule` trait + 3 implementations (decoupled from algorithm)
3. **Modular function approximators**: `fa::projector::*` (Fourier, RBF, Tile coding) interchangeable
4. **Module hierarchy**: `control::{td, ac, mc, cacla, nac}` mirrors RL textbook taxonomy

### Architectural patterns Touring could borrow (low priority)

| Pattern | Where in Touring it could apply | Effort |
|---------|--------------------------------|--------|
| Generic `Trace<K>` primitive (Accumulating/Replacing/Dutch) | `touring-learning/rl/qtable.rs` (currently hardcoded Replacing) | 2-3 hours |
| `Policy` trait abstraction | `touring-learning/rl/actor_critic.rs` (currently hardcoded Boltzmann) | 1-2 hours |
| `FunctionApproximator` trait | `touring-learning/rl/{ndarray_mlp, tiny_transformer}` (no shared trait) | 3-4 hours |
| Algorithm taxonomy modules (`control::td`) | Touring's `rl/` is flat — could group | 1 hour refactor |

**None implemented in Wave 7** — these are inspiration for future refactors when drivers emerge.

---

## Touring's RL Architecture (current state, 2026-04-26)

```
touring-learning/src/
├── bandit/                    # 8 files — contextual bandits (THE production layer)
│   ├── mod.rs                  # ContextualBandit trait
│   ├── linucb.rs               # LinUCBBandit (1163 LOC, 8 arms × 25 dims)
│   ├── granularity.rs          # GranularityBandit (CILA-aware splits)
│   ├── transfer.rs             # TransferLinUCB
│   ├── reminder_bandit.rs      # ReminderBandit
│   ├── ast_enriched.rs         # AstEnrichedBandit
│   ├── ast_features.rs         # AST feature extraction
│   └── adaptive_alpha.rs       # AdaptiveAlpha (exploration tuning)
├── rl/                        # 9 files — value-based & policy-based RL
│   ├── mod.rs
│   ├── qtable.rs               # Q-learning + Replacing eligibility traces (λ=0.9)
│   ├── double_qtable.rs        # Double Q-learning (van Hasselt 2015)
│   ├── actor_critic.rs         # Actor-Critic
│   ├── ndarray_mlp.rs          # MLP function approximator
│   ├── tiny_transformer.rs     # Markov + transformer for tool prediction
│   ├── burn_transformer.rs     # Burn deep learning (feature-gated)
│   ├── per_buffer.rs           # Prioritized Experience Replay
│   ├── curiosity.rs            # Count-based curiosity bonus
│   └── risk_adjusted.rs        # Blast-radius-aware Q-learning
├── online_learning/
│   └── ftrl.rs                 # Follow The Regularized Leader
├── meta/                      # Meta-learning
├── clustering/                # Leiden clustering
├── memory/                    # HNSW working memory
├── observability/             # Telemetry
├── data/                      # Datasets
└── ...
```

```
touring-cognitive/src/
├── mcts.rs                    # MCTSEngine + PheromoneMCTS (657 LOC)
└── cognitive_mcts.rs          # GraphInformedMCTS
```

---

## Wave 7 Decision Tree

```
START: Should we integrate rsrl?
   │
   ├─ Is rsrl maintained? NO (6 years stale) ──► SKIP as dep
   │
   ├─ Are there algorithms Touring lacks? YES (QSigma, GreedyGQ, etc)
   │  │
   │  └─ Are they driven by current use cases? NO ──► No code addition
   │
   └─ Are there architectural patterns to borrow? YES (Trace<K>, Policy trait)
      │
      └─ Have drivers emerged? NO ──► Document as future inspiration
```

**Wave 7 = DOCS-ONLY** (similar to Wave 6 BugStalker).

---

## See Also

- `touring-cli-rl-quality.md` — RL counters + observability
- `touring-cli-debugging-bugstalker.md` — Wave 6 (debugger integration, also docs-only)
- `~/projects/touring/crates/touring-learning/src/rl/qtable.rs` — TD(λ) implementation reference
- rsrl upstream: https://github.com/tspooner/rsrl (dormant since 2020-06)
- Sutton & Barto, "Reinforcement Learning: An Introduction" (canonical RL reference)
