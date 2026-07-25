# D33 — Performance Test Gaps (F3.7)

**Phase**: 3 (Testing & Documentation) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f3_7_perf_tests`
**Enforcement**: ⚠ WARN on PreToolUse:Edit/Write
**Elite reference (context7)**: `/grafana/k6` · Criterion · Gatling

## Definition

Avalia presença de testes de performance: benchmarks (micro), load tests (macro), e guards de regressão (p99/throughput não pode piorar). "Meça antes de otimizar" — sem baseline, não há como saber se uma mudança regrediu a performance.

## Why it matters

Regressões de performance entram silenciosamente (um clone aqui, um lock ali) e só aparecem em prod sob carga. Benchmarks com guard de regressão pegam a degradação no PR. Touring usa hdrhistogram P99 guards exatamente para isso.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | bench + load + regression guard |
| 0.5–0.8 | ⚠ Warn | bench sem guard / sem load |
| <0.5 | ❌ Fail | sem teste de performance |

## MUST

```bash
touring-quality check --gate F3.7 --target <FILE>
touring-quality score <FILE> --dims F3.7 --format json
```

## SHOULD

```bash
cargo bench                                             # Criterion micro-benchmarks
touring gate-metrics -j | jq '.perf'                   # P99 guards do próprio Touring (hdrhistogram)
# Load test macro: k6 com thresholds p95/p99
```

## MAY

```bash
touring memory recall "quality:F3.7"
```

## Elite best practices (context7 — `/grafana/k6`)

1. **Thresholds em p95/p99, não média** — k6 `thresholds: { http_req_duration: ['p(99)<500'] }`; o teste FALHA se a cauda passar do SLO. Média esconde a cauda. Fonte: `/grafana/k6` (thresholds).
2. **Criterion para micro-benchmarks com guard de regressão** — `cargo bench` compara com baseline; CI falha se regredir > X%. [training-data: Criterion + Touring perf_p99_gate].
3. **Load test realista (ramp + soak)** — ramp-up de VUs + soak prolongado para achar leaks/degradação (k6 stages); não só pico instantâneo. Fonte: k6 scenarios.
4. **Baseline versionado** — guardar baseline de perf; regressão = diff contra baseline, não número absoluto (depende da máquina). [training-data: Touring perf baseline].
5. **Profile guia otimização** — flamegraph (pprof) para achar o hot-path real antes de otimizar (princípio operacional 8). [training-data: Touring profile].

## Common pitfalls

- Benchmark sem guard de regressão (mede mas não bloqueia piora).
- Load test só de pico (não pega leak/degradação em soak).
- Threshold em média (cauda p99 passa despercebida).
- Otimizar sem profile (adivinhar o gargalo).

## Remediation

1. `cargo bench` baseline + guard de regressão no CI; k6 load com thresholds p99.
2. Adicionar via `Write tool + touring generate verify`/CI config.
3. `Write tool (script Python) --path tests/load/<scenario>.js --intent "<k6 load test>" --kind LoadTest` (REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 6)

## Cross-references

- Decision matrix: **C06 EDIT-MAJOR** + **C12 SYSTEM-HEALTH**
- Dims relacionadas: D26 (scalability), D20 (DB perf), D04-perf, D50 (monitoring)
- Keystone: `~/.claude/rules/elite-50-quality.md` (auditor-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /grafana/k6 + Criterion) — maintained by touring-quality_
