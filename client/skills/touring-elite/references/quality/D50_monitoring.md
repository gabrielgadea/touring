# D50 — Monitoring & Observability (F4.10)

**Phase**: 4 (Best Practices & CI/CD) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f4_10_monitoring`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/open-telemetry/opentelemetry-rust` · Prometheus · Datadog · Grafana

## Definition

Avalia observabilidade: logging estruturado, métricas (counters/gauges/histograms), tracing distribuído, SLI/SLO definidos, e alerting. "Não se pode consertar o que não se pode ver." Os três pilares: logs, métricas, traces. Touring já expõe `gate-metrics` com counters (USP de observabilidade interna).

## Why it matters

Sem observabilidade, incidentes são debugados às cegas (MTTR alto), regressões de performance passam despercebidas, e SLOs não podem ser medidos. Telemetria estruturada transforma "está lento" em "p99 do endpoint X subiu 3× às 14h após deploy Y".

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | logs+métricas+traces+SLO |
| 0.5–0.8 | ⚠ Warn | observabilidade parcial |
| <0.5 | ❌ Fail | sem telemetria |

## MUST

```bash
touring-quality check --gate F4.10 --target <FILE>
touring-quality score <FILE> --dims F4.10 --format json
```

## SHOULD

```bash
touring gate-metrics -j                                 # exemplo real: counters de observabilidade do Touring
touring ast grep <FILE> 'println!'                      # anti-pattern: println! em vez de tracing
```

## MAY

```bash
touring memory recall "quality:F4.10"
```

## Elite best practices (context7 — `/open-telemetry/opentelemetry-rust`)

1. **OpenTelemetry como padrão único** — instrumentar com OTel (traces+metrics+logs) → exportar para qualquer backend (Prometheus/Datadog/Grafana) sem lock-in. Fonte: `/open-telemetry/opentelemetry-rust`.
2. **Logging estruturado (`tracing`), nunca `println!`** — `tracing::info!(user_id, latency_ms, "request done")` com campos; spans para contexto async; correlação por trace-id. Fonte: tracing + OTel.
3. **Os 3 pilares + correlação** — logs (eventos), métricas (agregados/counters como Touring `gate-metrics`), traces (fluxo distribuído); correlacionados por trace-id. Fonte: OTel.
4. **SLI/SLO explícitos + alerting baseado neles** — definir o indicador (p99 latência, error-rate) e o objetivo; alertar no error-budget burn, não em CPU bruta. Fonte: Prometheus/SRE.
5. **Histograms para latência (não média)** — Prometheus histogram/hdrhistogram para p50/p95/p99; média esconde a cauda (ver D33). Fonte: Prometheus + Touring (hdrhistogram P99).

## Common pitfalls

- `println!`/`eprintln!` em vez de `tracing` (sem estrutura, sem nível, sem correlação).
- Métricas só de infra (CPU/mem), nenhuma de negócio/SLI.
- Sem trace-id → impossível correlacionar logs de um request.
- Alertar em métricas brutas em vez de SLO/error-budget (fadiga de alerta).

## Remediation

1. `touring ast grep 'println!'` → trocar por `tracing` estruturado.
2. Instrumentar com OTel (métricas+spans), definir SLI/SLO via `Edit tool`.
3. `Write tool --path src/exporter.rs --intent "Prometheus exporter" --kind RustModule` (Prometheus/OpenTelemetry; REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 7)

## Cross-references

- Decision matrix: **C12 SYSTEM-HEALTH** + **C09 DEBUG-ROOT-CAUSE**
- Dims relacionadas: D33 (perf tests), D51 (incident), D26 (scalability)
- Keystone: `~/.claude/rules/elite-50-quality.md` (architect-owned, USP gate-metrics)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /open-telemetry/opentelemetry-rust + Prometheus) — maintained by touring-quality_
