# Ground Truth Protocol

> **Read when**: building or extending `ground_truth_collector.py`, or
> debugging why a plan's evidence is incomplete.

## Why ground truth first

A plan that begins from memory is a hallucination. Every plan claim is either
verifiable against the live codebase or it is speculation. The Stage-1
ground-truth sweep is the mechanism that supplies the verification corpus the
rest of the toolkit uses.

## The sweep

`ground_truth_collector.py` executes the following Touring commands in parallel
(via `ThreadPoolExecutor`, I/O-bound) and merges their JSON outputs into a
single `ground_truth.json`:

| Command | Purpose | Field in ground_truth.json |
|---------|---------|----------------------------|
| `touring doctor -j` | system health | `doctor` |
| `touring status -j` | composite health + index/wiring/RL stats | `status` |
| `touring e2e --depth standard -j` | composite E2E score | `e2e` |
| `touring wiring audit -j` | full module + orphan audit | `wiring_audit` |
| `touring wiring orphans -j` | bare orphan list (canonical) | `wiring_orphans` |
| `touring evolution drift -j` | degrading metrics + alert level | `evolution_drift` |
| `touring memory recall "<keywords>"` | past lessons matching intent | `memory_lessons` |
| `touring gotcha match <each target file>` | known pitfalls for each file | `gotcha_per_file` |
| `touring index find <each cited symbol>` | VGP verification | `vgp_verifications` |
| `touring ast overview <each target file>` | symbol map of each touched file | `ast_overviews` |
| `touring ast blast <each target file>` | dependency tree | `ast_blasts` |

The `--intent` flag is required — the collector cannot decide which symbols /
files to verify without it. Heuristics extract candidate symbols and files from
the intent string (PascalCase, snake_case, paths with `/` or `.rs`/`.py`).

## The envelope

```json
{
  "status": "OK | DEGRADED | FAIL",
  "script": "ground_truth_collector",
  "timestamp": "2026-05-23T15:30:00Z",
  "intent": "implement async write-back cache",
  "duration_ms": 1840,
  "daemon_degraded": false,
  "doctor": { ... },
  "status": { ... },
  "e2e": { "composite_score": 0.74, ... },
  "wiring_audit": { "orphan_count": 42, ... },
  "wiring_orphans": [ ... ],
  "evolution_drift": { "alert_level": "none", ... },
  "memory_lessons": [ ... ],
  "gotcha_per_file": { "crates/cache/src/lib.rs": [ ... ] },
  "vgp_verifications": [
    { "symbol": "TokenCache", "verified": true, "file": "crates/cache/src/token.rs", "line": 88 },
    { "symbol": "WriteBackPolicy", "verified": false, "suggestion": "WriteBackQueue" }
  ],
  "ast_overviews": { "crates/cache/src/lib.rs": { ... } },
  "ast_blasts": { "crates/cache/src/lib.rs": { "file_count": 12, ... } },
  "summary": {
    "verified_symbols": 8,
    "unverified_symbols": 2,
    "gotchas_found": 3,
    "lessons_applied": 5
  }
}
```

The envelope is read by every downstream script:
- `dimension_scorer.py` reads `vgp_verifications` to score dimension **a**.
- `gap_detector.py` reads `wiring_orphans` to flag missing wiring coverage.
- `confidence_tagger.py` reads `daemon_degraded` to downgrade affected claims.
- `plan_scaffolder.py` reads the whole envelope to seed the plan skeleton.

## Failure modes

| Mode | Trigger | Behavior |
|------|---------|----------|
| `DEGRADED` | `touring doctor` shows any non-ok component | sweep continues but flags `daemon_degraded: true`; VGP fallback to `grep` |
| `FAIL` | `doctor` cannot be invoked at all | sweep aborts; collector emits `status: FAIL` + structural exit code 3 |
| `partial vgp` | `touring index find <S>` returns 0 results | record `verified: false` + `suggestion: <closest>`; downstream gates surface this |
| `timeout` | any single command exceeds `--per-command-timeout` (default 30s) | timeout recorded; sweep continues; other fields filled |

## Daemon-down fallback

When `touring doctor -j` reports degraded `daemon_socket` or `daemon_health`,
the collector switches into fallback:

| Touring command | Fallback |
|-----------------|----------|
| `touring index find <S>` | `grep -rn '<S>' <root>` (file-level only) |
| `touring ast overview <F>` | `python3 -c "import ast; ast.parse(open('<F>').read())"` for .py; `cargo check --message-format=json` for .rs |
| `touring wiring audit -j` | best-effort `grep` for `pub fn`, `pub struct`, `pub mod` declarations |
| `touring memory recall` | empty list — no fallback (this is a true loss) |
| `touring gotcha match` | empty list — no fallback |

The collector emits `daemon_degraded: true`. The amplifier and the validator
treat affected fields with reduced weight (downgrade FACT → INFERENCE).

## Intent → symbol extraction heuristics

Given `--intent "implement async write-back cache for TokenCache"`, the
collector extracts:

| Extracted | Pattern |
|-----------|---------|
| `TokenCache` | PascalCase word ≥ 2 capital segments |
| `write-back`, `write_back` | kebab/snake_case hyphenated word |
| paths (none in this intent) | regex `[a-z_]+/[a-z_.]+\.(rs\|py\|ts)` |
| feature flags (e.g. `feature="async-fs"`) | quoted strings near `feature` |

For each extracted symbol, `touring index find` is invoked. For each path,
`touring ast overview` + `touring ast blast` are invoked.

When the intent is ambiguous (e.g. "improve performance"), the collector
returns a minimal sweep and emits `summary.intent_too_vague: true` — the author
must refine the intent before proceeding.

## Caching

The envelope is keyed by `blake2b(canonical(intent) + repo_head_sha)` (lib.py:
`compute_intent_cache_key`). A second invocation within 10 minutes returns the
cached envelope unless `--no-cache` is passed. This matters because the sweep
can take 1-3 seconds; iterating the plan should not re-sweep every minute.

## Performance budget

| Phase | Budget |
|-------|--------|
| Total wall time | ≤ 5 seconds for typical intent (5 symbols, 3 files) |
| Per-command timeout | 30s (configurable via `--per-command-timeout`) |
| Parallelism | 8 concurrent ThreadPoolExecutor workers (configurable via `--workers`) |
| Cache TTL | 600 seconds (configurable via `--cache-ttl`) |

If the budget is exceeded, the script still emits whatever it collected and
flags the missing fields. A partial ground_truth is better than none.
