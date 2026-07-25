# Amplification Strategies — From Pln1 to Pln2

> **Read when**: a dimension scored < 7 and you need to lift it. Each strategy
> is concrete, mechanical, and emits the diff the author should apply.

`dimension_amplifier.py` reads the dimension report, identifies dimensions below
the threshold (default 7.0), and emits one or more amplification actions per
dimension. The strategies below are the catalog the amplifier draws from.

## a — Precision: Replace prose with `file:LINE` citations

| Trigger | Before | After |
|---------|--------|-------|
| "the auth module" | "modify the auth module to add rate limiting" | `crates/auth/src/middleware.rs:142 — fn validate_token` |
| "around line 200" | "around line 200, the parser fails" | `crates/parser/src/lib.rs:212 — match arm Token::Eof returns Err` |
| no symbol cited | "make handler async" | `touring ast find handle_request` → embed signature |

**Mechanical action**: `dimension_amplifier.py --dim a --suggest <draft>` emits
the list of unverified citations to replace.

## b — Scalability: Replace one-offs with patterns

| Trigger | Before | After |
|---------|--------|-------|
| `if tenant == "X"` | special-cased branch | `TenantPolicy` trait + registry lookup |
| copy-paste between 2 callers | duplicated logic | extract helper; both callers consume |
| hard-coded list | inline `vec![a, b, c]` | move to `Cargo.toml`'s `[features]` or a config file |

**Mechanical action**: identify any subtask whose code lives in `match` /
`if/else` on a value enumerable at compile time — extract into a trait.

## c — Performance: Numbers, not adjectives

| Trigger | Before | After |
|---------|--------|-------|
| "fast" | "should be fast" | `P99 < 50ms under 10 RPS` (with bench name) |
| "low memory" | "low memory" | `< 20MB RSS at idle, < 100MB under 1000 concurrent` |
| missing complexity | "iterate over users" | `O(n) over users where n = active session count (~10k typical)` |

**Mechanical action**: insist on a number with a unit and a workload. The
amplifier suggests "what would you bench? against what load? what is the SLO?"

## d — Functionality: Wire the orphans

| Trigger | Source | Action |
|---------|--------|--------|
| Plan does not mention orphan symbols | `touring wiring orphans -j` | for each orphan, add a subtask connecting it to a consumer or document why it stays |
| Plan removes a `pub` symbol | wiring audit | check who depends on it (`touring wiring impact <sym>`); add migration step |
| Plan adds new `pub` symbol | wiring audit | declare at least one consumer; otherwise prepare for it to be orphaned at audit |

**Mechanical action**: `dimension_amplifier.py --dim d` lists every orphan in the
target tree the plan does not touch.

## e — Quality: Name tests + handle errors

| Trigger | Before | After |
|---------|--------|-------|
| "add tests" | generic | name each test: `test_validate_token_rejects_expired`, with the assertion: `assert resp.status_code == 401` |
| `unwrap()` in the diff | unwrap | replace with `?` or `unwrap_or_else(\|e\| log::error!("..."); default)` |
| no error path | only happy path | add the error branch in the same subtask |

**Mechanical action**: every code change in the plan must point to a test name
+ assertion AND show the error branch.

## f — Detail: Schemas and exact code

| Trigger | Before | After |
|---------|--------|-------|
| "the API takes a JSON" | prose | paste the Pydantic / serde struct; enumerate optional fields |
| "edge cases handled" | empty | enumerate the edges (null, empty array, oversize, timeout) |
| "see diagram" | no diagram | inline Mermaid or ASCII; for cross-module flows |

**Mechanical action**: for every API or schema referenced, embed it inline.

## g — Integration: Map every connection

| Trigger | Action |
|---------|--------|
| New component introduced | enumerate who calls it (which hook, which CLI command, which test fixture) |
| New trait introduced | name at least one implementor that exists already or is in the plan |
| Cross-module data flow | draw the flow (A → B → C) with the actual function names |

**Mechanical action**: `dimension_amplifier.py --dim g` runs `touring wiring
audit` and asks "where does this new code plug in?"

## h — Dependencies: Pin everything

| Trigger | Before | After |
|---------|--------|-------|
| `tokio = "*"` | wildcard | `tokio = { version = "1.42", features = ["sync", "rt-multi-thread"] }` |
| "use the latest pydantic" | imprecise | `pydantic = ">=2.5,<3"` with the rationale |
| Python only | no MSRV note | document MSRV (Min Supported Rust Version) or Python version range |

**Mechanical action**: for every new dependency line, embed the version
constraint AND say why that range (compat with X, requires feature Y from
version Z).

## i — Potentiation: Empty Enables column triggers rewrite

This is the most distinctive amplifier — REGRA #0 made operational.

| Trigger | Before | After |
|---------|--------|-------|
| `Enables: —` | empty | rewrite the subtask so something it does makes future work easier |
| One-off patch | dead-end | turn into a hook / extension point others can build on |
| Removes capability | shrinking | replace with a flag or feature toggle so capability stays available |

**Mechanical action**: `dimension_amplifier.py --dim i` lists every subtask with
empty `Enables`. The author must either fill the row or rewrite. No subtask
ships with an empty Enables column.

## The amplifier's output

```
{
  "dim": "i",
  "score": 4.2,
  "threshold": 7.0,
  "delta": 2.8,
  "amplifications": [
    {
      "subtask": "S-3",
      "issue": "Enables column is empty",
      "strategy": "REGRA #0 rewrite",
      "specific_action": "S-3 introduces `TokenCache`. Rewrite so it is consumed by an `AuthMiddleware` trait, enabling per-tenant cache policy in W04.",
      "rescore_after": 7.5
    }
  ]
}
```

The author reads the amplifications, applies the diffs, and re-runs the
scorer. Iterate until every dimension ≥ 7 (target 8).
