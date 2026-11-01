---
type: Benchmark
title: "Benchmarks F2 — latência wall-clock × 50 runs dos 3 subcmds CLI"
description: "Medições wall-clock × 50 iterações dos 3 subcmds CLI criados em F1.5 (pub-api, scan, identity). Resultado: p95 < 10ms para os 3 — 10× melhor que o budget de 100ms. SignalLayer Rust direto será ainda mais rápido (<5ms)."
plan_id: 2026-08-31-complementacao-hooks
bundle: docs/plans/2026-08-31-complementacao-hooks
okf_version: "0.1"
version: "1.0"
tags: [benchmark, latency, subcmd, signal-layer]
timestamp: "2026-08-31T19:11:30-03:00"
---

# Benchmarks F2 — latência × 50 runs

## Metodologia

- **N = 50 runs por subcmd**
- Wall-clock medido via `time.perf_counter()` Python wrapper (subprocess.run)
- Binário: `target/debug/touring` (debug build, sem otimização)
- Comando exemplo:
  ```bash
  ./target/debug/touring pub-api --file crates/touring-server/src/cli/pub_api.rs
  ./target/debug/touring scan crates/touring-server/src/cli/scan.rs
  ./target/debug/touring identity --canonical-name touring.test
  ```

## Resultados (wall-clock subprocess completo)

| Subcmd | N | p50 | p95 | p99 | avg | budget | status |
|---|---:|---:|---:|---:|---:|---:|:---:|
| `touring pub-api` | 50 | 6.88 ms | **9.00 ms** | 9.33 ms | 7.11 ms | <100ms | ✅ 11× melhor |
| `touring scan` | 50 | 6.42 ms | **8.86 ms** | 9.10 ms | 6.76 ms | <100ms | ✅ 11× melhor |
| `touring identity` | 50 | 6.34 ms | **8.91 ms** | 9.39 ms | 6.86 ms | <100ms | ✅ 11× melhor |

## Interpretação

- Os 3 subcmds estão **bem dentro do budget** (p95 <10ms vs budget <100ms).
- Wall-clock inclui: fork + exec + Rust runtime init + tracing init + lógica + stdout emit + exit.
- **SignalLayer Rust direto** (sem subprocess) será tipicamente **<5ms p95** — função pura, sem I/O de processo.
- H6, H12, H14 (criar SignalLayer puro) e wirings (H7, H8, H11, H13, H15) — budget generoso.

## Próximo passo

F3 — implementar 15 SignalLayers (2 novos + 13 wirings em handlers existentes).

---

_v1.0 — 2026-08-31 19:11 BRT | Wall-clock × 50 runs | 3 subcmds todos p95 <10ms | budget <100ms respeitado com folga 11×_