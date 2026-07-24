---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
type: "metrics"
created: "2026-05-11"
---
# 06-METRICS — KPIs & Quality Gates

> **Source of truth** for measurable success criteria across the refactor
> and product lifecycle. Each metric has owner, baseline, target, and
> verification command.

## 1. Product KPIs by horizon

| KPI | T0 | M3 | M6 | M12 | M24 | Unit | Owner |
|---|---|---|---|---|---|---|---|
| GitHub stars | 1k | 5k | 15k | 35k | 80k | count | Marketing |
| DAU active installs | 100 | 1k | 5k | 20k | 60k | count | Product |
| Free → Premium conversion | — | 0.5% | 1% | 1.5% | 2% | % | Growth |
| Premium MRR | $0 | $5k | $25k | $120k | $400k | $ | Sales |
| Enterprise ARR | $0 | $0 | $300k | $1.5M | $5M | $ | Sales |
| **Total ARR** | $0 | $60k | $600k | **$2.9M** | **$9.8M** | $ | CFO |
| Premium subs | 0 | 150 | 700 | 3,000 | 9,000 | count | Sales |
| Enterprise accounts | 0 | 0 | 1 | 5 | 17 | count | Sales |
| Monthly churn | — | 8% | 5% | 3% | 2% | % | CS |
| NPS | — | 30 | 45 | 55 | 60+ | score | CS |
| Mean time to value (install → ROI) | — | 30 min | 15 min | 5 min | <5 min | duration | Product |
| External contributors | 1 | 10 | 50 | 200 | 500 | count | DevRel |
| Conference talks | 0 | 2 | 5 | 12 | 25 | count | Marketing |

## 2. Engineering KPIs per wave

| Wave | Metric | Baseline (T0) | Gate Target | Verification |
|---|---|---|---|---|
| W0 | Baselines captured | 0/5 artifacts | 5/5 | ls docs/baselines/ |
| W1 | Dead crates | 4 | 0 | grep workspace.members Cargo.toml |
| W1 | Cycle count | 2 | 1 (Cycle #1 GONE) | touring wiring cycles |
| W2 | Centralized deps | 0 | 60+ | grep [workspace.dependencies] Cargo.toml |
| W3 | Foundation test ratio | ~9% | ≥ 25% | cargo llvm-cov -p touring-foundation |
| W4 | touring-code created | NO | YES (26k LOC) | wc -l crates/touring-code/src/ |
| W4 | Parsing bench delta | 0% | ≥ -5% | cargo bench compare baseline |
| W4 | touring-ast consumers updated | 38 | 0 (or shim-only) | grep 'use touring_ast::' crates/ |
| W5 | touring-storage features | 0 | 11 | grep [features] crates/touring-storage/Cargo.toml |
| W6 | Cortex test ratio (pre-fusion) | 0.56% | ≥ 15% | cargo llvm-cov -p touring-cortex |
| W6 | touring-intelligence created | NO | YES (90k LOC) | wc -l crates/touring-intelligence/src/ |
| W6 | Macrociclo depth max | 618 | 0 or <10 | touring wiring cycles --min-depth 2 |
| W6 | MCTS/ANN/bandit bench delta | 0% | ≥ -5% | cargo bench compare baseline |
| W7 | touring-bindings default features | n/a | empty | grep 'default =' touring-bindings/Cargo.toml |
| W8 | hook hot-path P99 | n/a | < 5 ms | hdrhistogram bench |
| W8 | Cycle count post-split | 1 | 0 | touring wiring cycles --min-depth 2 |
| W8 | 24 hook events smoke | n/a | 24/24 PASS | TACO E2E test |
| W9 | CLI dispatch P99 | n/a | < 10 ms | criterion bench |
| W9 | 82 CLI commands smoke | n/a | 82/82 PASS | for cmd in $(touring --help); do touring $cmd --help; done |
| W11 | Min test ratio per crate | — | ≥ 20% (NO exceptions) | cargo llvm-cov per crate |
| W11 | Mutation kill rate | ~50% | ≥ 80% | cargo mutants |
| W11 | Proptest properties | 0 | ≥ 50 | grep proptest! crates/ |
| W11 | Fuzz targets | 0 | ≥ 8 | cargo fuzz list |
| W12 | Pilot projects per-project | 0 | 2 (konverter+analise) | ls ~/projects/{konverter,analise}/.touring/ |
| W12 | Migration tool functional | NO | YES | touring migrate --dry-run |
| W13 | docs.rs build green | n/a | 100% | docs.rs status all crates |
| W13 | RC1 published | NO | YES (1.0.0-rc.1) | curl install.touring.dev sees rc1 |
| W14 | Tiers active | 0/4 | 4/4 | touring login --tier-test all |
| W14 | install.touring.dev functional | NO | YES | curl pipe sh succeeds |
| W14 | 1.0.0 GA published | NO | YES | git tag v1.0.0 |

## 3. Quality gates (cross-cutting, every wave)

| Gate | Threshold | Verification | Blocking? |
|---|---|---|---|
| cargo check --workspace | exit 0 | bash | ✓ |
| cargo test --workspace --no-fail-fast | exit 0 | bash | ✓ |
| cargo clippy --workspace -- -D warnings | clean | bash | ✓ |
| cargo doc --workspace --no-deps --warnings-as-errors | clean | bash | ✓ |
| touring wiring cycles --min-depth 2 | monotonic non-increasing | jq cycle_count | ✓ |
| touring wiring orphans -j | monotonic non-increasing per wave | jq count | ⚠ warn |
| cargo deny check | clean | bash | ✓ (from W2) |
| cargo audit | 0 vulns | bash | ✓ (from W2) |
| cargo machete | 0 unused | bash | ✓ (from W2) |
| Bench regression vs baseline | ≥ -5% | criterion compare | ✓ (from W2) |
| Test ratio per touched crate | ≥ 20% | cargo llvm-cov | ✓ (from W11) |
| Mutation kill rate per crate | ≥ 80% | cargo mutants | ✓ (from W11) |
| Memory lesson persisted | ≥ 1 per wave | touring memory recall | ⚠ warn |
| RL reward injected | ≥ 1 per wave | touring learning status | ⚠ warn |

## 4. Composite health score formula

```
composite_health =
    0.20 * acyclicity        # 1 - (cycles / total_crates)
  + 0.15 * test_ratio_avg     # avg test/src LOC ratio
  + 0.15 * mutation_kill_rate
  + 0.10 * doc_coverage       # pub items with docstrings / total pub
  + 0.10 * supply_chain       # 1 if deny+audit+vet clean else 0
  + 0.10 * api_stability      # 1 - (breaking_changes / pub_surface)
  + 0.10 * perf_budget        # 1 - max(0, regression - 5%) / 100
  + 0.05 * deployment         # 1 if touring init works else 0
  + 0.05 * tiers              # active tiers / 4
```

## 5. Unit economics (commercial)

| Metric | Value | Source |
|---|---|---|
| Cost per premium user/month | ~$11 | infra $0.30 + license $0.05 + support $1.50 + sales $4 + marketing CAC $5 |
| Revenue per premium | $29/mo | Stripe pricing |
| **Gross margin** | **~62%** | (29 - 11) / 29 |
| LTV premium individual | $740 | $290 ARR × 0.85 retention × 3 yr |
| LTV premium team | $5,300/seat | $1,440 ARR × 0.92 × 4 yr |
| LTV enterprise | $1.38M/account | $290k × 0.95 × 5 yr |
| LTV/CAC premium | **9.3×** | vs benchmark 3× |
| LTV/CAC enterprise | **34.5×** | vs benchmark 3-5× |

## 6. References

- Cross-audit dimensions: `CROSS-AUDIT.md` (10 weighted dims, composite ≥ 0.95)
- Per-wave gates: each `WX-*.md` "Gate de Saída" section
- Premium commercial: `03-COMMERCIAL.md`
