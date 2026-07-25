# MCTS Planning — Multi-Path Decisions

> **Read when**: a plan has 2+ valid architectural paths with significant
> consequences, and you want a data-grounded comparison before committing.
> `mcts_wrapper.py` is the toolkit's interface to `touring mcts search`.

## When to use MCTS

MCTS (Monte Carlo Tree Search) is **not** the default planning mode. The
default is the 4-stage protocol (Ground Truth → 9-Dim → Plan Structure →
Amplification). MCTS is a specialized tool for **multi-path decisions** where:

| Signal | Reason to use MCTS |
|--------|---------------------|
| 2+ paths with comparable cost/risk | the team disagrees on direction |
| High blast-radius reversal | rolling back is expensive — choose right the first time |
| Past `touring evolution drift -j` shows similar decisions caused rework | history is a signal; let MCTS exploit it |
| Cross-cutting choice (architecture, schema, protocol) | downstream cascades amplify the choice |

When **none** of these apply, the 4-stage protocol is faster and clearer. Do
not invoke MCTS for trivial decisions.

## What `touring mcts search` does

It runs a Monte Carlo Tree Search over `candidate_actions` rooted at
`root_state`, using `num_rollouts` simulations of depth `max_depth`. The
output is `MCTSResult { best_action, confidence, value, tree_depth, total_rollouts }`.

```bash
touring mcts search '<root_state>' --candidate-actions <C> --num-rollouts <R> --max-depth <D> -j
```

The `root_state` is a JSON object describing the current planning state:

```json
{
  "intent": "implement async write-back cache",
  "candidates": [
    {"id": "A", "description": "tokio::sync::Mutex<HashMap>", "blast_radius": 12},
    {"id": "B", "description": "dashmap::DashMap with background flush task", "blast_radius": 6},
    {"id": "C", "description": "moka::Cache with built-in eviction", "blast_radius": 4}
  ],
  "context": {
    "current_score": {"performance": 5.2, "scalability": 6.8, "code_quality": 7.0},
    "constraints": ["MSRV 1.75", "no unsafe", "P99 < 10ms"],
    "past_drift": ["pattern:custom-mutex-becomes-contention-point"]
  }
}
```

`touring mcts search` returns:

```json
{
  "best_action": "C",
  "confidence": 0.78,
  "value": 0.84,
  "tree_depth": 5,
  "total_rollouts": 200,
  "alternative_actions": [
    {"id": "B", "value": 0.79, "rationale": "second-best — lower built-in surface area"},
    {"id": "A", "value": 0.61, "rationale": "third — known to become contention point per past drift"}
  ]
}
```

## What `mcts_wrapper.py` adds

The Touring command returns raw MCTS metrics. The wrapper:

1. **Composes a root_state** from a Pydantic `MCTSRootState` model
   (validates the input before the Touring command runs).
2. **Embeds context** from `ground_truth.json` automatically when present.
3. **Parses + types** the result as `MCTSResult` (Pydantic).
4. **Emits a markdown-friendly comparison table** to embed in the plan.
5. **Caches** by `blake2b(canonical(root_state))` (10-min TTL).

## Example: cache-backend choice

```bash
python3 mcts_wrapper.py \
  --intent "implement async write-back cache" \
  --ground-truth data/ground_truth.json \
  --candidates "tokio-mutex-hashmap;dashmap-bg-flush;moka-builtin-eviction" \
  --rollouts 200 \
  --max-depth 5 \
  --emit-markdown
```

Emits to `data/mcts_cache_backend.json` + a markdown table that the plan
embeds verbatim:

```markdown
## MCTS Decision: cache backend

| Action | Value | Confidence | Why |
|--------|------:|------------:|-----|
| **C — moka** | **0.84** | **0.78** | lowest blast (4); built-in eviction; matches MSRV |
| B — dashmap | 0.79 | 0.72 | bg flush adds complexity but composes well |
| A — tokio mutex + HashMap | 0.61 | 0.59 | past drift: contention point in 3 prior modules |

**Decision**: C (moka). Confidence 0.78.
```

## Constraints on MCTS use

| Constraint | Reason |
|------------|--------|
| Max 5 candidates per call | combinatorial explosion; force the author to pre-filter |
| `--rollouts` between 50-500 | below 50 = noisy; above 500 = slow without benefit |
| `--max-depth` between 3-7 | beyond 7 = irrelevant (planning horizon issue) |
| Result expires after the plan's `data/ground_truth.json` mtime changes | because the codebase moved; the decision may need re-evaluation |

## Daemon-down behavior

If `touring mcts search` is unavailable (daemon socket error), the wrapper
returns:

```json
{
  "mode": "skip",
  "reason": "daemon_unavailable",
  "fallback_recommendation": "decide via pros/cons table; record decision in plan with confidence INFERENCE [0.7]"
}
```

The author then fills in a pros/cons table manually, tagged INFERENCE.

## Limitations

- MCTS does not understand semantic correctness — it operates on metric proxies.
- Bad inputs produce bad outputs: garbage `candidates` → garbage best_action.
- It is a **decision support** tool, not an autonomous decider. The author
  reads the result and decides.

For 80% of planning decisions, the 4-stage protocol gives a clearer outcome
faster. Reserve MCTS for the 20% where the choice is consequential and
multiple sane paths exist.
