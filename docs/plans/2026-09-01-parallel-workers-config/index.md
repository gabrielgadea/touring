---
type: LoopBundle
title: "Parallel Workers Configuration (2026-09-01)"
okf_version: "0.1"
tags: [loop-bundle, parallel, workers, tuning]
---

# Parallel Workers Configuration — Bundle Index

## Artefatos

- [strategy.md](strategy.md) — Diagnóstico + 3 fases de tuning (A/B/C) + candidatos ranqueados

## Fases propostas

| # | Status | Owner | Artefato |
|---|---|---|---|
| A — medir contention pipeline_runner (1/2/4/8/16 workers) | pending | TACO | timing data |
| B — TUNAR rayon pool (TOURING_RAYON_THREADS=12) | pending | TACO | ~/.bashrc + verify |
| C — override codegen-units no dev profile | pending | TACO | Cargo.toml + benchmark |