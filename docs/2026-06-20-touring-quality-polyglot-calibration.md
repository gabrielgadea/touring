# touring-quality — Polyglot Calibration & f2_4 Backlog Hardening (2026-06-20)

> Code-first verified. Two tracks delivered in `crates/touring-quality`; one daemon-level
> gap diagnosed and deferred (needs Gabriel's go-ahead — new scope).

## TRACK A — f2_4_secrets hardening (7 backlog items, all shipped + tested)

| # | Gap | Fix | Approach |
|---|-----|-----|----------|
| 1 | JWT (`eyJ…`) | `is_jwt()` structural (3 base64url segments, `eyJ` header, ≥8 chars) | token-scan, near-zero FP |
| 2 | Conn-strings (`scheme://u:pw@host`) | `has_connstring_creds()` — authority `user:password@`, literal pw only | per-line; `$VAR`/`${VAR}` excluded |
| 3 | New providers | STRONG_MARKERS += `dop_v1_`/`doo_v1_`/`dor_v1_`/`glpat-`/`shpat_`/`shpss_`/`shpca_`/`sq0atp-`/`sq0csp-`/`pypi-AgEI`; token-scan `sk-proj-`/`sk-ant-api`/`sk-ant-`/`xapp-` + SendGrid `SG.…` | substring for distinctive prefixes; whole-token check for kebab-collision-prone ones |
| 4 | Entropy floor (4.5) misses base64url/short | name-context path relaxes to 3.5 + accepts hex (gitleaks keyword-prefilter pattern) | only when `names_secret(lhs)` |
| 5 | Unquoted `KEY=eyJ…`/hex env-style | path-2b `looks_like_secret_value_named()` on unquoted RHS | `.env`/shell/YAML |
| 6 | ARN entropy FP | `has_non_secret_markers` += `starts_with("arn:")` | identifiers, not secrets |
| 7 | Hex tokens by-design excluded | accepted ONLY under name-context (hex ≥32) | generic path stays hex-safe (no SHA FP) |

**Verified on deployed binary**: 9/9 secrets BLOCK (0.0), 6/6 benign SAFE (URL/connstr-env/kebab/ARN=1.0,
env::var/placeholder=0.5). 151 tests pass, clippy `-D warnings`=0, build=0.

## TRACK B — polyglot dir-scan (Rust-bias fix)

**Root cause**: all 50 verifiers inlined `if target.is_dir() { … e == "rs" … }` — `score <dir>`/`--workspace`
on a Python/TS project read only the first top-level `.rs` (none → empty → meaningless scores).

**Fix**: shared `verifications::read_target_source()` — recursive, polyglot (24 source exts), deterministic
(sorted), skips vendor/build/venv/.git, capped 2 MB. Migrated **48** content-based verifiers via exact-block
replacement; left the 2 manifest-readers (`f2_5_dep_cves`, `f4_5_pkg_mgmt`) untouched.

**Verified**: `score apps/backend` (Python dir) now reads real content (`CC≈874 loc=58438`, was empty);
50-dim composite 0.63 Bronze with real blockers incl. F2.4 (a real secret exists in the scanned window — investigate per-file).

## Corrected calibration recipe (the report's `--lang` flag does NOT exist)

```bash
# 1. PER-FILE calibration — correct granularity for count-based dims (F1.1/F1.2/F1.4)
#    (dir/workspace concatenates → inflates complexity; score files for per-function signal)
touring-quality score apps/backend/<file>.py --dims F1.1,F1.2,F1.4 --format json

# 2. Dir/workspace scan (NOW polyglot) — best for presence-based dims (secrets, OWASP, error-handling)
touring-quality score apps/backend --format json | jq '{composite,tier,blockers}'
touring-quality score . --workspace --fail-below 0.80          # delivery gate

# 3. Locate the F2.4 secret finding (per-file, since dir aggregates)
for f in $(find apps/backend -name '*.py'); do
  v=$(touring-quality score "$f" --dims F2.4 --format json | jq -r '.dimensions.F2_4.value')
  [ "$v" = "0" ] && echo "SECRET: $f"
done

# 4. Compare 50-dim vs daemon TDG (both polyglot for metrics)
touring ast tdg apps/backend/<file>.py        # daemon: language=python, grade, composite ✓
```

## "Daemon Python/TS symbol gap" — INVESTIGATED & REFUTED (2026-06-21, FACT)

The earlier inference (`ast overview <file>.py` → `symbol_count:0` ⇒ daemon can't extract Python symbols)
was a **false positive**: it was drawn from one trivial 9-line file (`celery_worker.py`) that genuinely has
no `def`/`class` (only an import + `if __name__` guard). Verified code-first against symbol-rich files:

| Check | Result |
|-------|--------|
| `ast overview apps/backend/conftest.py` (Python) | **13 symbols** — `{function:10, class:1, assignment:2}`, correct lines |
| `ast overview …/plans.test.tsx` (TS) | **3 functions** (TS works; `next-env.d.ts`=0 is correct — no symbols) |
| `index find db_session` | **found** at `conftest.py:40` (Python def IS indexed) |
| `index find test_engine` | **3 defs** — Python `conftest.py:23` + Rust `.rs` |
| `ast overview …/languages.rs` (Rust) | 36 symbols — `{enum, function, impl, method, module, type_alias}` |

**Root cause of the original report's claim**: `non_rust=1.012.783` (99.96% of rows) was read as "low quality",
but transferegov *is* 99.96% Python/TS (49,012 `.py` vs 53 `.rs`) — the ratio is **correct and expected**, not a
defect. The daemon detects, extracts (on-demand + index), and scores Python/TS first-class via tree-sitter
`.scm` queries (`crates/touring-code/src/ast/queries/python.scm`, etc.). `Lang::from_path("py")→Python`,
`tree_sitter_python`, and the Python query are all wired and NOT behind the `more-languages` feature gate.

**Elite benchmark (context7 `/tree-sitter/tree-sitter` code-navigation/tags):** the gold-standard tagging
taxonomy is `@definition.<kind>` + `@name` (+ `@reference.call` for find-references). Touring's queries capture
`@name` and derive kind from `parent.kind()` (`node_kind_to_symbol_kind`) — a functionally-equivalent variant
of the canonical convention, already covering Python/TS/Rust/JS with rich kinds. **No change required.**

### Optional refinements (Gabriel's decision — NOT defects)
1. **Module-level `assignment` capture** (`python.scm` last rule) goes *beyond* the tree-sitter-tags standard
   (which captures only def/class/method, not `x = value`). It adds coverage but also index noise on generic
   names (`app`, `config`). Keep (more coverage) or drop (align to tags standard) — judgment call.
2. **`@reference.call` captures** for go-to-references are not in the `.scm` queries; call relationships are
   handled separately in `ast/call_graph.rs`. Adding reference tags would align 1:1 with the GitHub-semantic
   tags model — only worth it if reference-navigation via the index is desired.

**Net: no daemon work performed — it would have been modifying code that already works (REGRA: never modify
code you don't need to). VP-Scout "verify before report" caught the false positive before any wasted L4 effort.**

---

## Authorized enhancements (2026-06-21) — Gabriel: "manter é obrigatório, aperfeiçoar organização/classificação/utilização" + "@reference.call 1:1 GitHub-semantic"

### ✅ Phase A1 — SHIPPED & VERIFIED: module-binding classification
`crates/touring-code/src/ast/symbols.rs`: Python `"assignment" => Variable` mapping + new
`refine_binding_kind(name, kind)` → SCREAMING_SNAKE_CASE / dunder metadata (`__all__`, `__version__`) become
`Constant` (`as_str()`="const"), ordinary lowercase stay `Variable`. Applied in `extract_symbols_from_tree`
after the async/method upgrades. **No symbol dropped (REGRA #0) — only sharpened.** 47 ast::symbols tests pass
(+1 new `test_module_binding_constant_vs_variable_classification`), full `touring-code` suite green except one
PRE-EXISTING stale test (`workflow_step5_…touring-ast` — asserts a package fused into `touring-code`, unrelated).
`ast overview <py>` now shows `const`/`variable` instead of generic `assignment`. **Live after next `update-touring`.**

### ✅ Phase A2 — SHIPPED & DEPLOYED & VERIFIED (2026-06-21): index kind persistence
`kind` now persists in the symbol store end-to-end; `index find` returns the rich classification (was `kind:null`).
- **`SymbolLocation` (`ast/graph/mod.rs`)**: added `kind: Option<String>` (`#[serde(default, skip_serializing_if)]`
  → RPC/snapshot payloads carry it for free, backward/forward compatible); `::new(..5)` sets `kind:None` (callers
  unaffected); `with_kind(self, k)` builder. Producers `index_file`/parallel-merge call `.with_kind(sym.kind.as_str())`.
- **`ast/store.rs`**: idempotent `ALTER TABLE symbols ADD COLUMN kind TEXT` (`.ok()` like `co_edit_weight`, order-/
  version-independent across all project DBs). `row_to_symbol` reads kind **defensively** via
  `get::<_,Option<String>>(5).unwrap_or(None)` (mirrors `row_to_dep`) — SELECTs without kind degrade to `None`, no
  panic. 4 INSERTs: `+kind` col, `+?6`, `+sym.kind` param, `DO UPDATE kind=COALESCE(?6,kind)` (a kind-less re-upsert
  never wipes an existing kind). 6 SELECTs surface kind.
- **`ast/incremental_pipeline.rs`**: `run_query` threads `lang`; classifies via `Symbol::node_kind_to_symbol_kind` +
  `refine_binding_kind` (A1 classifiers made `pub(crate)`). **`touring-cortex` `pure_ast_extract`** preserves `s.kind`.
- **CLI**: `cli_index_find`/`cli_index_search`/`cli_ast_find` emit `kind`; `fallback_overview_from_store` `Null→loc.kind`.
- **~21 `SymbolLocation` struct-literals** workspace-wide got `kind: None` (src + tests + benches across touring-code /
  hook-handlers / hooks-core / hooks). Field-on-struct + serde(default) = the right additive shape.
- **Gate**: `cargo check --workspace --all-targets`=0 · `cargo test -p touring-code`=635 pass + new
  `test_symbol_kind_round_trips_through_store` (round-trip + None + COALESCE) + doctests 11 pass. **Deployed**:
  `update-touring` + `touring daemon-ctl restart` + `index rebuild`. **Verified live**: `index find SymbolLocation` →
  `kind=struct`/`impl`; `with_kind` → `function`.
- **Incidental co-evolution repairs** (the ast→code fusion left these stale, surfaced by my gate): fixed 1 workspace
  unit test (`touring-ast`→`touring-code`) + migrated 13 files of doc-comment crate refs (`touring_ast::`→
  `touring_code::ast::` per the `lib.rs:28-31` mapping). Not A2 regressions — pre-existing fusion debt.

### ✅ Phase B — SHIPPED & DEPLOYED & SCALE-MEASURED (2026-06-21): polyglot `@reference.call` (find-references)
`index find <sym>` now doubles as find-references, 1:1 with the GitHub-semantic tags model (`@definition` + `@reference.call`).
- **Reuse win**: `ast/call_graph.rs::build_call_graph(source, lang)` was ALREADY polyglot (`RUST/PYTHON/TS_CALL_QUERY`
  → `CallGraph{sites:Vec<CallSite{caller,callee,line,args_count}>}`); `SymbolLocation.is_definition` + the CLI
  `definitions_only` filter ALREADY existed. So B = extract + persist + split output only.
- **`cli_index_rebuild`** (`touring-cli/.../index.rs`): **flag-gated** — when `TOURING_INDEX_REFERENCES=1`,
  `build_call_graph(content, lang).sites` → `SymbolLocation::new(rel_path,callee,line,0,false).with_kind(Some("call"))`
  appended to the per-file set persisted by `replace_file_symbols` (DELETE-then-INSERT handles refresh). **Default OFF**
  (scale safety + zero VGP behavior change).
- **`cli_index_find`**: partitions `find_symbol` locations into `definitions[]` (is_def=true) + `references[]`
  (is_def=false, `kind="call"`); `references` suppressed under `definitions_only` (VGP-safe); `count`/`reference_count`
  report true totals before output caps (10 defs / 50 refs).
- **Scale-gate data** (rust workspace, mostly Rust): defs **73,882** → refs **164,786** = **2.23× defs** (total 238,668
  rows, 3.2× defs); DB **76.8→123.9 MB** (+61%); reindex **101s→113s** (+12%). Well under the conservative 5–10× estimate.
  For transferegov (~1M defs): est. ~2.2M refs, ~3.2M rows, ~+60% DB — feasible for SQLite.
- **Gotcha**: the rebuild's `symbols_added` counter only counts *definitions* (refs go into a separate vec) — measure
  references via `index status` `symbol_count` + `sqlite GROUP BY is_definition`, not the rebuild JSON.

### ✅ Phase B — DEFAULT-ON, SAFE-BY-CONSTRUCTION (Gabriel 2026-06-21)
References flipped to **default ON** (disable per-project with `TOURING_INDEX_REFERENCES=0`). A naive flip would break
the 7+ `find_symbol` consumers (VGP false-positives on called-but-undefined symbols, inflated `cli_suggester`
definition counts, polluted in-memory wiring/blast). Instead, references were made **invisible to every existing
consumer**: all definition-oriented store queries (`find_symbol`, `find_symbols_in_file`, `search_symbols`,
`get_hot_symbols`, `symbols_page`, `load_into_index`) now filter `AND is_definition = 1`; references are reachable
**only** via the new explicit `find_references(name)` (`is_definition = 0`). `cli_index_find` reads `definitions` from
`find_symbol` and `references` from `find_references`. This also makes incremental edits ref-safe: `apply_change_set`
diffs defs-only sets, so a hot edit never removes a file's references.
- **Gate**: `cargo test -p touring-code`=636 (+`test_find_symbol_excludes_references_find_references_returns_them`);
  per-crate `clippy --all-targets -D warnings` clean across **all 45 workspace crates + touring-quality**;
  `clippy --workspace`=0. **Verified live (default-on reindex, no flag)**: 73,886 defs + 164,817 refs; `index find clone`
  → `count(defs)=14 reference_count=2670` (find_symbol is defs-only; references separate).
- **Error sweep** (Gabriel: "fix all errors regardless of origin"): a per-crate clippy pass surfaced 10 pre-existing
  feature-gated latents (hidden by workspace feature-unification) — fixed: `cli/tantivy.rs` ×4 + `cli/handlers/mcp.rs`
  (unused feature-gated params), `touring-quality/verifications/mod.rs` (`let_and_return`), `touring-hook-handlers`
  ×3 (unused-mut + missing-doc on `not(tantivy-fts)`/`not(pre-hooks)` stubs).

### To use find-references (default-on — no flag needed)
```bash
touring index find <symbol> -j | jq '{count, reference_count, references}'   # count=defs, references[]=call-sites
# Polyglot project: export TF_WORKSPACE="$PWD"; touring index rebuild --dir "$PWD"   (references indexed by default)
# To opt OUT per-project: TOURING_INDEX_REFERENCES=0 touring index rebuild --dir "$PWD"
```
