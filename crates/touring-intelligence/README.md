# touring-intelligence

> The unified code-intelligence layer: indexing, reasoning, learning, and ANN
> search. Master Plan D.W3.P2.

## Purpose

`touring-intelligence` is the **L1+L4 substrate** (see
`docs/explanation/architecture.md`): the symbol index and AST that everything
navigates, plus the reasoning and reinforcement-learning engines that make the
system converge. It is the W6 fusion of four formerly-separate crates into one
coherent layer.

## Architecture (fused subsystems)

| Subsystem | Role |
|---|---|
| **index** | Incremental symbol indexing (the navigation substrate) |
| **cognitive** | Reasoning engine — MCTS, Graph-of-Thought, ACO, pensieve, BM25 |
| **learning** | RL — contextual bandit (LinUCB), clustering, online-RL, ranking, semantic |
| **antt** | Approximate nearest-neighbour index + reranker |

Heavy backends are **opt-in** behind `intel-*` Cargo features so a minimal build
stays light.

## Key capabilities (via the `touring` CLI)

```bash
touring index find <Symbol>          # exact symbol lookup (the VGP primitive)
touring index rebuild "$PWD"         # (re)build the local index
touring wiring impact <Symbol>       # transitive consumers (BFS)
touring tantivy search "<query>"     # BM25 ranked search
touring learning status              # RL bandit arms + EMA reward
```

## Example

```bash
# Does this symbol exist before I generate code that uses it?
touring index find HookRuntime -j

# Reward a tool outcome → updates LinUCB + Q-table
touring learning reward edit 1.0 "tests passed"
```

## Caveats

- The index is **local** (no cloud round-trip) and must be (re)built per
  workspace; a new project needs `touring index rebuild` before queries are
  meaningful.
- Symbol resolution is **syntactic** (index/AST), not type-inferred across files;
  full LSP-grade inference is roadmap (Master Plan A.W4, salsa-backed).
- The wiring cache can lag edits by seconds — confirm "orphan" claims with a
  `grep` before trusting them (VP-Scout Chain 7).
