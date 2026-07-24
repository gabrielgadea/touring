# touring-analysis — Architecture

> **Version**: v0.1.0 | **Updated**: 2026-05-11 | **LOC**: 14723

## Overview

Code quality analysis and metrics engine — provides Halstead metrics, cyclomatic complexity, maintainability index (MI), cognitive complexity, blast radius computation, and TDG grade letter scoring. 56 modules covering quality, temporal, wiring, and blast analysis.

## Key Types

`AnalysisConfig` | `Depth` | `AnalysisPipeline` | `AnalysisPipelineBuilder` | `OtelConfig` | `AnalysisInsights` | `TdgGrade` | `TdgReport` | `QualityReport` | `QualityPipeline` | `ComplexityMetrics` | `HalsteadMetrics` | `Antipattern` | `QualityFinding` | `UnwrapAudit` | `ErrorCoverage` | `HealthStatus` | `HealthDimension` | `CodeHealthReport` | `CodeHealthReportEnriched` | `WiringReport` | `OrphanResult` | `WiringFinding` | `CyclePath` | `ChainResult` | `TrendReport` | `TrendDirection` | `LearningReport` | `RewardTrend` | `SecurityReport` | `SecurityAdvisory` | `SecurityAnalyzer` | `RustQualitySignals` | `BlastRadiusEngine` | `BlastRadiusResult` | `BlastWarning` | `AffectedFile` | `LatencyTier` | `AnalysisError` | `AnalysisResult` | `RulesError` | `E2eConfig` | `AnalysisSummary`

## Module Map

| File | LOC | Responsibility |
|------|-----|----------------|
| `src/lib.rs` | 154 | Library entry point, public API, re-exports |
| `src/quality/complexity.rs` | 849 | — |
| `src/pipeline.rs` | 848 | — |
| `src/quality/tdg.rs` | 749 | — |
| `src/temporal/trends.rs` | 670 | — |
| `src/quality/mod.rs` | 660 | — |
| `src/wiring/orphan.rs` | 561 | — |
| `src/report.rs` | 475 | — |
| `src/quality/signal/types.rs` | 448 | — |
| `src/blast_radius/mod.rs` | 393 | — |
| `src/learning/mod.rs` | 367 | — |
| `src/cache.rs` | 354 | — |
| `src/e2e/schema_guard.rs` | 345 | — |
| `src/quality/signal/workspace_io.rs` | 345 | — |
| `src/rules/evaluator.rs` | 329 | — |
| `src/wiring/finding.rs` | 328 | — |
| `src/wiring/mod.rs` | 321 | — |
| `src/quality/signal/diff.rs` | 318 | — |
| `src/blast_radius/warning.rs` | 305 | — |
| `src/rules/types.rs` | 280 | — |
| `src/knowledge/mod.rs` | 275 | — |

## Key Features

- **Halstead metrics**: n1/n2/N1/N2/V/D/E/B/T operators and operands
- **Cyclomatic complexity**: CC per function
- **Maintainability Index (MI)**: SEI/Mozilla formula with 0-100 range
- **Cognitive complexity**: Deep-nesting and structural penalties
- **Blast radius**: Transitive dependency tree for impact analysis
- **TDG Grade**: A+..F letter grade across 6 dimensions
- **Temporal trends**: Historical quality tracking
- **Wiring orphan analysis**: Pub symbols without consumers

## Integration Points

- touring-hooks: quality signals for post-edit quality tracking
- touring-server: quality reports via CLI ast quality
- touring-learning: blast radius feeding RL reward
- touring-generator: TDG grade gate in plan pipeline

## Technology

Pure Rust. syn for Rust parsing. No unsafe at crate level.
