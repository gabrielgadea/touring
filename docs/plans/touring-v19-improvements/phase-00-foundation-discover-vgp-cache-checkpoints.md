---
plan_id: touring-v19-improvements
phase: 0
title: 'Foundation: DISCOVER, VGP Cache, Checkpoints'
status: planned
created: '2026-03-25'
horizon: H0
insights_covered: []
depends_on_phases:
validates_with: validate_phase00.py
estimated_effort: 2h
vgp_verified_structs: []
---

# Foundation: DISCOVER, VGP Cache, Checkpoints


## Objective

Establish pre-conditions for all implementation phases: verify daemon health (P50=1ms), cache VGP schemas for 10 verified structs, setup .toon checkpoint format using msgpack, and validate test baseline (2,152 passed, 0 failed).

## Final Result

VGP schemas cached for 10 structs. Daemon warm. Checkpoint directory ready with .toon format. Baseline verified: 2,152 tests green.

## Impacts

| Dimension | Assessment |
|-----------|------------|
| performance | No runtime impact (setup only). |
| maintainability | VGP cache prevents field hallucination in all subsequent phases. |
| reliability | Baseline test count verified before any changes. |
| daemon_latency | Daemon pre-warmed — subsequent hook calls at P50=1ms. |

## Subtasks

- [ ]S0.1: Verify daemon: ls /tmp/touring-daemon-$(id -u).sock
- [ ]S0.2: Run baseline: cargo test --workspace --exclude touring-python → 2152 passed
- [ ]S0.3: Cache VGP schemas via touring_ast_find for all 10 structs
- [ ]S0.4: Install msgpack-python: pip install msgpack; verify .toon roundtrip
- [ ]S0.5: Create checkpoints/ directory with .gitkeep
- [ ]S0.6: Run cargo clippy --workspace -- -D warnings → 0 warnings

## DISCOVER Protocol

```
touring_ast_find(symbol_name='LearnedPattern', definitions_only=true)
touring_ast_find(symbol_name='MutableGeneratorGraph', definitions_only=true)
touring_ast_find(symbol_name='TrackerReport', definitions_only=true)
touring_memory_recall(query='touring-learning aco tracker template', top_k=10)
cargo test --workspace --exclude touring-python 2>&1 | tail -3
```

## TDD Plan

No new code. Validation + setup only.

## Checkpoint (.toon)

```python
# Save: {'baseline_tests': 2152, 'daemon_warm': True, 'vgp_cached': 10}
# File: checkpoints/phase-00-foundation.toon
import msgpack, pathlib
pathlib.Path('checkpoints/phase-00-foundation.toon').write_bytes(msgpack.packb({'baseline_tests': 2152, 'daemon_warm': True, 'vgp_cached': 10}))
```

## Validation Criteria

- [ ]cargo test → 2152 passed, 0 failed
- [ ]cargo clippy → 0 warnings
- [ ]Daemon socket exists: /tmp/touring-daemon-$(id -u).sock
- [ ]VGP cache entries > 0 in memory store
- [ ]checkpoints/ directory exists
