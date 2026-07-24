# touring-offensive — Architecture

> **Version**: v0.1.0 | **Updated**: 2026-05-11 | **LOC**: 7320 | **Constraints**: `#![forbid(unsafe_code)]`

## Overview

Security vulnerability analysis and exploit tooling for Touring — 14 modules implementing concolic execution, bug bounty tracking, vulnerability CWE classification (CWE-89 SQL injection, CWE-79 XSS, CWE-78 command injection), and SMT solver-based analysis. Built on Erickson NLP framework with CVC5 and Z3 SMT backends.

## Key Types

`ConcolicExecutor` | `BugBountyTracker` | `EricksonExtractor` | `PatternRegistry` | `CVC5SolverBackend`

## Module Map

| File | LOC | Responsibility |
|------|-----|----------------|
| `src/lib.rs` | 49 | Library entry, public re-exports |
| `src/concolic.rs` | 1461 | SymbolExpr, Constraint, ConcolicExecutor, PathExplorer |
| `src/bug_bounty.rs` | 855 | BugBountyTracker, BugStatus, Severity, BugBountyError |
| `src/erickson.rs` | 705 | EricksonExtractor, NLPPattern, EricksonElement |
| `src/erickson/confidence.rs` | 608 | BaseConfidence, ContextBoost, QualifierPattern |
| `src/solver.rs` | 579 | Solver trait + dispatch |
| `src/erickson/relation_population.rs` | 524 | PopulationResult, RelationContext |
| `src/vuln/cwe_patterns.rs` | 445 | PatternRegistry, 10 vulnerability patterns (SQLi, XSS, Cmdi, etc.) |
| `src/solver/cvc5_backend.rs` | 409 | CVC5SolverBackend SMT integration |
| `src/erickson/ptbr_markers.rs` | 404 | PT-BR NLP markers for Brazilian Portuguese |
| `src/erickson/sentence_boundaries.rs` | 398 | Sentence boundary detection |
| `src/solver/z3_backend.rs` | 395 | Z3SolverBackend SMT integration |
| `src/solver/stub_backend.rs` | 333 | StubSolverBackend for testing |
| `src/erickson/rl_feedback.rs` | 119 | EricksonRLAdapter, ImmediateReward |
| `src/vuln/mod.rs` | 36 | Vulnerability pattern registry |

## Key Features

- **Concolic execution**: Concrete + symbolic execution for bug analysis
- **Bug bounty patterns**: Known vulnerability pattern detection
- **CWE classification**: Common Weakness Enumeration categorization
- **SMT solving**: CVC5 backend for constraint solving
- **Erickson framework**: PT-BR markers and sentence boundary detection

## Integration Points

- touring-analysis: vulnerability scanning in quality pipeline
- touring-learning: RL-based exploit prioritization
- Security gate: vulnerability detection in touring hooks

## Technology

Pure Rust. CVC5 SMT solver (optional, via libcvc5). No unsafe at crate level.
