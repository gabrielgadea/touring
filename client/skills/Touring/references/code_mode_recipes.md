# Code Mode Recipes — `touring_ctx_execute` as Programmatic Tool-Caller

> **Status**: canonical | **Date**: 2026-05-23 | **Pattern**: MCP spec "Programmatic Tool Calling / Code Mode"
> **Companion**: `references/mcp_tools.md` (per-tool catalog) + `Reflex #8` in CLAUDE.md (Compute-in-Code)
> **Spec authority**: [modelcontextprotocol.org/docs/develop/clients/client-best-practices](https://modelcontextprotocol.org/docs/develop/clients/client-best-practices) — "Code Mode" + "Combining Both Patterns"

## Why this exists

Calling N MCP tools in sequence forces **every intermediate result through the model's context**. The MCP spec recommends "Code Mode": the model writes a script that calls the tools in a sandbox, and only the final summary returns. Touring already ships the sandbox — `touring_ctx_execute` — supporting 13 runtimes (js/node, python3, ts/bun, ruby, go, rustc, perl, R, elixir, php, bash, sh).

**Token math**: a typical Touring MCP response carries ~500-2000 tokens of envelope + diagnostics + `_next_tools`. Doing 5 calls back-to-back = 2.5-10K tokens through the model. The same 5 operations in one `ctx_execute` script = **one** ~500-token response. **80-90% reduction** on multi-step workflows.

## Sandbox surface (quick reference)

| Param | Type | Notes |
|---|---|---|
| `language` | required string | `js`/`node`/`bun` · `python` · `ts` · `ruby` · `go` · `rust` · `perl` · `r` · `elixir` · `php` · `sh`/`bash` |
| `code` | required string | source body — runs in a fresh sandbox |
| `args` | `string[]?` | `sys.argv[1:]` (Python) · `__ctx_args` global (JS) |
| `timeout_ms` | `u64?` | default 30000, max 120000 |
| `cwd` | `string?` | working directory (defaults to workspace root) |
| `allow_forbidden` | `bool?` | override the forbidden-call policy (default off — fs.write*, subprocess.run, eval, etc. are blocked) |

**Output envelope**: `{stdout, stderr, exit_code, duration_ms, forbidden_calls, stdout_truncated, stderr_truncated}` plus graph-svc context injection. `stdout` truncated at 1 MB.

**Important**: the sandbox CAN spawn `touring` (the CLI) — that is the *whole point* of Code Mode. Forbidden-call policy targets dangerous primitives (writes, eval, network), NOT child processes to local CLIs you control.

---

## Recipe 1 — Wiring Forensics in one shot (orphans + chain + impact)

**Naive (3-5 MCP calls)**:
```
touring_wiring_audit                       → ~1500 tok response
touring_wiring                             → ~800 tok
touring_wiring_suggest                     → ~1200 tok
touring_wiring_purpose × symbol_of_interest → ~600 tok each
```
≈ **4-6K tokens** through the model just to see the wiring landscape.

**Code Mode (1 call, ~400 tok response)**:
```python
touring_ctx_execute(
  language="python",
  code='''
import json, subprocess

def t(*args):
    r = subprocess.run(["touring", *args, "-j"], capture_output=True, text=True, timeout=15)
    try: return json.loads(r.stdout)
    except: return {"raw": r.stdout[:500], "err": r.stderr[:200]}

orphans = t("wiring", "orphans")
audit   = t("wiring", "audit")
suggest = t("wiring", "suggest", "--top", "5")

# Cross-correlate: which top-suggested orphans have low-quality consumers?
out = {
    "orphan_total": len(orphans.get("orphans", [])),
    "audit_low_score_modules": [
        m for m in audit.get("modules", []) if m.get("score", 1.0) < 0.5
    ][:10],
    "top_5_suggestions": suggest.get("suggestions", [])[:5],
    "actionable": [
        s for s in suggest.get("suggestions", [])
        if s.get("confidence", 0) > 0.7
    ][:3],
}
print(json.dumps(out, indent=2))
'''
)
```

**Savings**: 4-6K → ~400 tok ≈ **90%**. Single round-trip. Cross-correlation logic stays in the sandbox, not in your context.

---

## Recipe 2 — Symbol Verification Batch (VGP for the Constitutional Table)

REGRA #15 (Symbol Verification Table) demands every cited symbol carry `touring index find` evidence. Verifying 10-20 symbols one-by-one is the canonical context tax.

**Naive (N × 2 MCP calls)**:
```
for sym in symbols:
    touring_index_find(symbol=sym)   → ~300 tok
    touring_ast_find(symbol=sym)     → ~400 tok
```
20 symbols × 700 tok = **14K tokens**.

**Code Mode (1 call, ~600 tok response with full evidence)**:
```python
touring_ctx_execute(
  language="python",
  code='''
import json, subprocess, sys

symbols = sys.argv[1:]   # passed via args

def find(sym):
    r = subprocess.run(["touring", "index", "find", sym, "-j"], capture_output=True, text=True, timeout=5)
    try: hits = json.loads(r.stdout)
    except: hits = []
    return {"symbol": sym, "defs": len(hits), "files": [h.get("file_path") for h in hits[:3]]}

results = [find(s) for s in symbols]
table = {
    "verified_existing": [r for r in results if r["defs"] > 0],
    "not_found":         [r["symbol"] for r in results if r["defs"] == 0],
    "ambiguous":         [r for r in results if r["defs"] > 1],
}
print(json.dumps(table, indent=2))
''',
  args=["CompositeHealthScore", "WiredPair", "ActivityVerify", "GateMetrics", "..."]
)
```

**Savings**: 14K → ~600 tok ≈ **96%**. And the output is *already structured* for the Symbol Verification Table — copy-paste into the JSON output of the TACO phase.

---

## Recipe 3 — Health Snapshot for FASE 0 Gate

FASE 0 (TACO Phase Protocol) requires `cargo check` + `touring doctor` + `touring status` + `touring e2e`. Each call costs tokens; cross-correlation costs more.

**Naive (4-5 MCP calls)**:
```
touring_health        → ~800 tok
touring_metrics       → ~1200 tok
touring_evolution_status → ~600 tok
touring_index_status  → ~400 tok
```
≈ **3K tokens** for a binary "go/no-go" decision.

**Code Mode (1 call, exit-code semantics)**:
```bash
touring_ctx_execute(
  language="bash",
  code='''
set -u
status=0
touring doctor -j > /tmp/doctor.json 2> /dev/null || status=1
touring status -j > /tmp/status.json 2> /dev/null || status=1
cargo check --workspace --message-format short 2>&1 | tail -5 > /tmp/cargo.log
cargo_status=$?

jq -n \
  --slurpfile d /tmp/doctor.json \
  --slurpfile s /tmp/status.json \
  --arg cargo_exit "$cargo_status" \
  --rawfile cargo_log /tmp/cargo.log \
  "{
    doctor_components: ([\$d[0][] | select(.status != \"ok\") | .name] // []),
    composite_health:  \$s[0].composite_health_score,
    cargo_exit:        (\$cargo_exit | tonumber),
    cargo_tail:        \$cargo_log,
    gate_decision:     (if (\$cargo_exit | tonumber) == 0 and \$s[0].composite_health_score > 0.5 then \"CONTINUE\" else \"BLOCK\" end)
  }"
''',
  timeout_ms=120000
)
```

**Savings**: 3K → ~250 tok (just the gate verdict + minimal evidence). The 95-byte verdict is what FASE 0 actually needs to decide whether to proceed.

---

## Recipe 4 — Multi-file Metadata Aggregation (REGRA #1: File Metadata First)

When you're about to edit 5 files, running `touring_ast_meta` once per file is the textbook anti-pattern — pure waste of model tokens.

**Naive (N × 1 MCP call)**:
```
for f in files:
    touring_ast_meta(file=f)   → ~500 tok each
```
8 files × 500 = **4K tokens**.

**Code Mode (1 call, ranked verdict)**:
```python
touring_ctx_execute(
  language="python",
  code='''
import json, subprocess, sys

def meta(f):
    r = subprocess.run(["touring", "ast", "meta", f, "--depth", "summary", "-j"], capture_output=True, text=True, timeout=8)
    try: return json.loads(r.stdout)
    except: return {"file": f, "error": "no meta"}

files = sys.argv[1:]
metas = [meta(f) for f in files]

# Triage by blast_radius + quality_score (REGRA #1 thresholds)
hi_risk = [m for m in metas if m.get("blast_radius", 0) > 10 or m.get("quality_score", 1.0) < 0.5]
ok = [m for m in metas if m not in hi_risk]

print(json.dumps({
    "files_checked": len(metas),
    "high_risk": [{"file": m["file_path"], "blast": m["blast_radius"], "q": m["quality_score"]} for m in hi_risk],
    "safe_to_edit": [m["file_path"] for m in ok],
    "verdict": "STOP_AND_PLAN" if hi_risk else "PROCEED",
}, indent=2))
''',
  args=["crates/touring-server/src/server/mod.rs", "crates/touring-hooks/src/pre_edit.rs", "..."]
)
```

**Savings**: 4K → ~400 tok ≈ **90%**. And you get a single GO/NO-GO verdict instead of 8 raw metadata blobs to manually correlate.

---

## Recipe 5 — Memory Recall Fusion + Re-ranking

When investigating an unfamiliar topic, the natural reflex is `touring memory recall "<query>"` — but a single query may miss synonyms. The right approach is 3-5 queries with cosine-fused ranking — which is exactly the *kind* of work that should NEVER touch the model context.

**Naive (5 MCP calls + manual ranking)**:
```
touring_memory_recall(query="wiring orphan")  → ~1200 tok
touring_memory_recall(query="pub unused")     → ~1000 tok
touring_memory_recall(query="REGRA #0")       → ~1000 tok
touring_memory_recall(query="dead code")      → ~900 tok
touring_memory_recall(query="potencializar")  → ~800 tok
```
≈ **5K tokens** of overlapping recall hits.

**Code Mode (1 call, fused top-N)**:
```python
touring_ctx_execute(
  language="python",
  code='''
import json, subprocess
from collections import Counter

queries = ["wiring orphan", "pub unused", "REGRA #0", "dead code", "potencializar"]
all_hits = []
for q in queries:
    r = subprocess.run(["touring", "memory", "recall", q, "-j"], capture_output=True, text=True, timeout=8)
    try:
        for e in json.loads(r.stdout).get("entries", []):
            all_hits.append((e.get("key"), e.get("value", "")[:200], q))
    except: pass

# RRF-style fusion: rank by appearance count across queries
key_count = Counter(k for k, _, _ in all_hits)
fused = [
    {"key": k, "appeared_in": c, "preview": next(v for kk, v, _ in all_hits if kk == k)}
    for k, c in key_count.most_common(8) if c >= 2
]
print(json.dumps({"queries": queries, "fused_top_recalls": fused}, indent=2))
''',
  timeout_ms=45000
)
```

**Savings**: 5K → ~600 tok ≈ **88%**. Plus you get **deduplicated + re-ranked** results — better signal than raw recall.

---

## When NOT to use Code Mode

Code Mode is for **multi-call workflows with mechanical aggregation**. Don't use it for:

| Situation | Use instead |
|---|---|
| Single MCP call | Just call the tool directly — `ctx_execute` overhead > tool overhead |
| The result needs LLM reasoning *during* aggregation | Chain MCP calls — model needs to see intermediate values |
| Anything requiring forbidden primitives (file writes, eval) | Direct tool calls (or `Edit tool` for writes) |
| Cold-start exploration (you don't know which tools yet) | `touring_minimal_context` first — discover then `ctx_execute` |

## Performance + safety guarantees

- **Sandbox isolation**: each call gets a fresh process tree; no state leaks between calls
- **Timeout enforcement**: hard kill at `timeout_ms` (default 30s, max 120s)
- **Output truncation**: `stdout` at 1MB, `stderr` at smaller — `stdout_truncated`/`stderr_truncated` flags signal when this happened
- **Forbidden-call policy**: AST-detected blocks (since CEG P1.3/P1.4) for `fs.write*`, `subprocess.run` (Python), `eval`, etc. — returned in `forbidden_calls[]` with policy = `Warn` (default) or `Block`
- **Currently supported AST forbidden-detection**: JS, TS, Python, Ruby, Rust, PHP, Elixir. Substring fallback for Go, Shell, Perl, R (tree-sitter ABI 14 grammars — upgrade to ABI 15 unlocks Go/Shell AST per Wave 5 plan)

## Reflex #8 alignment

CLAUDE.md Reflex #8 (Compute-in-Code) already canonicalizes this pattern:

> Default: `touring inferlets run` (ou `ctx_execute`) para count/filter/aggregate em ≥3 arquivos
> Skip: query exata em 1 arquivo já carregado

The recipes above are the operationalization of that reflex for the 5 most common multi-call patterns. Read `~/.claude/rules/touring-decision-matrix.md` for the full task→tool matrix.

## Counter inspection

Live usage of this pattern is observable:

```bash
touring gate-metrics -j | jq '{
  ctx_execute_invocations: .ctx_execute_invocations_count,
  ctx_execute_forbidden_blocked: .ctx_execute_forbidden_blocked_count,
  ctx_execute_timeouts: .ctx_execute_timeout_count
}'
```

If `ctx_execute_invocations` is near zero while sessions show >5 MCP calls/turn on average, you are leaving compression on the table.

---

## Cross-references

| Topic | File |
|---|---|
| Tool catalog (per-tool descriptions) | [mcp_tools.md](mcp_tools.md) |
| Decision matrix (task → which tool) | `~/.claude/rules/touring-decision-matrix.md` |
| Reflex #8 canonical statement | `~/.claude/CLAUDE.md` (section "Os 9 Reflexos do TACO") |
| ctx_execute source | `crates/touring-server/src/server/tools_ctx_execute.rs` + `crates/touring-server/src/tools/ctx_execute_tools.rs` |
| Forbidden-call AST detection | `crates/touring-hooks/src/forbidden_patterns.rs` (CEG P1.3) |
| CEG sandbox stage | `crates/touring-hooks/src/gateway/sandbox_stage.rs` |
| MCP spec — Code Mode | https://modelcontextprotocol.org/docs/develop/clients/client-best-practices |

---

_v1.0 — 2026-05-23 — Code Mode discipline as cookbook. Materializes MCP spec "Programmatic Tool Calling" + CLAUDE.md Reflex #8 in 5 concrete patterns._
