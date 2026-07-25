# D26 — Scalability (F2.13)

**Phase**: 2 (Security & Performance) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f2_13_scalability`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/grafana/k6` · Locust · ChaosBlade

## Definition

Avalia capacidade de escalar: design stateless (escala horizontal), eliminação de SPOF (single point of failure), estado compartilhado externalizado, e comportamento sob carga. **Dim do architect** — `wiring audit` revela acoplamentos que viram SPOF.

## Why it matters

Arquitetura que não escala horizontalmente tem teto rígido — só dá para crescer comprando máquina maior (caro, finito). Estado in-process não-externalizado impede rodar N réplicas. SPOF derruba o sistema inteiro. Escalabilidade é decisão de design difícil de retrofitar.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | stateless, sem SPOF |
| 0.5–0.8 | ⚠ Warn | estado in-process / SPOF potencial |
| <0.5 | ❌ Fail | re-arquitetar para escala |

## MUST

```bash
touring-quality check --gate F2.13 --target <FILE>
touring-quality score <FILE> --dims F2.13 --format json
```

## SHOULD

```bash
touring wiring audit -j                                  # acoplamentos centrais = SPOF candidato
touring wiring impact <central_symbol> --depth 3        # fan-in alto = ponto de gargalo/SPOF
# Load test: k6 / Locust para validar comportamento sob carga
```

## MAY

```bash
touring memory recall "quality:F2.13"
```

## Elite best practices (context7)

1. **Stateless por design, estado externalizado** — sessão/estado em store compartilhado (Redis/DB), não em memória do processo; permite N réplicas atrás de LB. [training-data: 12-factor].
2. **Eliminar SPOF — redundância + degradação graciosa** — sem componente único cuja queda derruba tudo; circuit breaker + fallback (Touring tem circuit_breaker nativo). [training-data: resilience].
3. **Load test com thresholds, não só média** — k6 com `thresholds` em p95/p99 (`http_req_duration: ['p(99)<500']`); a cauda é o que importa sob carga. Fonte: `/grafana/k6` (thresholds).
4. **Backpressure, não buffer infinito** — bounded queues/channels (ver D21); sob sobrecarga, rejeitar/degradar em vez de OOM. [training-data: tokio backpressure].
5. **Chaos testing** — injetar falhas (ChaosBlade) para validar que a degradação graciosa funciona de verdade. [training-data: chaos engineering].

## Common pitfalls

- Estado de sessão in-process → impossível escalar horizontalmente.
- SPOF: um serviço/lock central cuja queda para tudo.
- Fila/buffer unbounded sob carga → OOM em vez de backpressure.
- Otimizar média ignorando p99 (a cauda derruba SLO).

## Remediation

1. `touring wiring audit`/`impact` → identificar SPOF e estado in-process.
2. Externalizar estado, adicionar fallback/circuit-breaker, bounded queues via `Edit tool`.
3. `Write tool --path <FILE> --intent "stateless service + pool" --kind ServiceModule` (REGRA #2 canonical workflows — stateless; ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 7)

## Cross-references

- Decision matrix: **C10 ARCHITECTURAL** + **C11 DEPENDENCY-FLOW**
- Dims relacionadas: D21 (memory), D22 (caching), D33 (F3.7 perf tests)
- Keystone: `~/.claude/rules/elite-50-quality.md` (architect-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /grafana/k6) — maintained by touring-quality_
