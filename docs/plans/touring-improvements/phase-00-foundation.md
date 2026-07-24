---
plan_id: touring-improvements-2026
phase: 0
title: 'Foundation: DISCOVER Protocol, VGP Cache, Checkpoint Setup'
status: completed
created: '2026-03-25'
horizon: H0
insights_covered: []
depends_on_phases: []
validates_with: validate_phase00.py
estimated_effort: 2h
vgp_verified_structs: []
---

# Foundation: DISCOVER Protocol, VGP Cache, Checkpoint Setup

## Objective

Establish the infrastructure needed before any code changes: verify daemon health, cache VGP schemas, set up .toon checkpoint format using msgpack, and create the DISCOVER protocol template.

## Final Result

VGP schemas cached for all 24 verified structs. Checkpoint directory ready with msgpack-based .toon format. DISCOVER protocol documented.

## Impacts

| Dimension | Assessment |
|-----------|------------|
| performance | No runtime impact (setup only). |
| maintainability | VGP cache prevents field hallucination in all subsequent phases. |
| reliability | Checkpoint infrastructure enables rollback on failure. |
| daemon_latency | No change (read-only operations). |

## Subtasks

- [ ] S0.1: Verify touring-daemon socket at /tmp/touring-daemon-1000.sock
- [ ] S0.2: Run cargo check --workspace to verify baseline compiles
- [ ] S0.3: Run cargo test --workspace --exclude touring-python to verify baseline passes
- [ ] S0.4: Cache all VGP schemas from generate_plan.py VGP_SCHEMAS dict
- [ ] S0.5: Create checkpoint writer using msgpack (.toon format)
- [ ] S0.6: Write initial checkpoint with plan metadata

## DISCOVER Protocol

```
1. ls /tmp/touring-daemon-*.sock -> verify daemon
2. cargo check --workspace -> verify compilation
3. cargo test --workspace --exclude touring-python -> verify test baseline
4. Record test count for regression detection in subsequent phases
```

## TDD Plan

No Rust code changes in this phase. Validation only:
- cargo check passes
- cargo test passes with expected count
- .toon checkpoint file created and readable

## Checkpoint (.toon)

```
Save: plan_metadata + vgp_schema_count + baseline_test_count
File: checkpoints/phase-00-foundation.toon
```

## Validation Criteria

- [ ] Daemon socket exists
- [ ] cargo check --workspace exits 0
- [ ] cargo test --workspace --exclude touring-python exits 0
- [ ] VGP schemas file exists with 24+ entries
- [ ] Checkpoint file checkpoints/phase-00-foundation.toon exists
