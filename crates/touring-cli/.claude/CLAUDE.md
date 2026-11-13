---
okf_version: "1.0"
type: CLAUDE
title: "touring-cli — CLI subcommand surface + KPI/handlers library"
description: "Library that powers the `touring` binary's CLI surface + the 86 MCP tools consumed by the daemon. Auto-loaded by CC when sessions touch this crate."
plan_id: docs
tags: [rules, crate, cli, kpi, mcp, handlers]
timestamp: 2026-09-01T11:52:00-03:00
---

# touring-cli — Crate Instructions

## What this crate does

The library that powers the `touring` binary:
1. **CLI subcommand surface** (`src/cli/*.rs`) — 50+ subcommands
2. **MCP server** (`src/mcp_server.rs`) — 86 tools exposed via `rmcp`
3. **Hook handlers** (`src/cli_handlers*.rs`) — invoked by the daemon
   via the hook registry (`touring-dispatch/src/hook_registry.rs`)
4. **Code-mode gates** (`src/code_mode_gates/`) — the decision to let a tool call run, speak beside
   it, rewrite it or deny it: `evaluate(&GateState, &ToolEvent, &GateConfig) -> Decision`. `chain.rs`
   runs G1–G11 in order, `arm.rs`/`mode.rs` resolve the presentation (`native|both|code`), `shell.rs`
   classifies Bash, `remedy.rs` builds the route back a deny names, `g10_route.rs` the exec-burst
   route, `state.rs` the per-daemon ledgers, `spec.rs` the declared gate table. Extracted from
   `cli_suggester.rs`; the module doc (`code_mode_gates/mod.rs`) is the contract. The workspace
   CLAUDE.md §10 lists the gates and their windows.

## Modules (`src/cli/`)

| File | Purpose |
|------|---------|
| `kpi.rs` | `cli_kpi` — the Falsifiable Commitments Dashboard + `code_mode_signal_use` (F6) |
| `ast.rs`, `wire.rs`, `cascade.rs` | AST + wiring commands |
| `decompose.rs` | DAG primitives (claim/ready/ticket/frontier) |
| `cognitive.rs`, `evolution.rs` | learning + refinement loops |
| `calibrate.rs`, `contract.rs` | quality + contract gates |
| `gate.rs`, `metrics.rs` | composite health |
| `cli_handlers*.rs` | RPC handlers (`cli-gate-metrics`, `cli-wiring-status`, etc.) |
| `cli_suggester.rs` | the pillar-induction hook (`cli_suggester` argv from pretooluse) |
| `context_cost.rs`, `surface_usage.rs`, `transcript_scan.rs` | B5/B3 (01/11/2026) — `kpi` sections `context_cost` and `surface_usage`, read-only, one transcript walk |
| `family_gate.rs`, `lesson_lifecycle.rs` | B2/B1 (01/11/2026) — SHADOW `kpi` sections `family_gate` and `lesson_lifecycle`, read-only, reuse `transcript_scan` |

## `context_cost` and `surface_usage` (B5/B3, 01/11/2026)

Siblings of `context_budget` (same transcripts, same caps), wired in `cli_kpi`. `context_budget`
counts BYTES per hook family; these read what the provider BILLED and what was ANNOUNCED × USED.

- **`context_cost`** — `usage` per assistant call (deduped by message id) split by kind; the
  re-sent window attributed to hook families, the announcement surface, tool results and the rest
  by `bytes × TOKENS_PER_BYTE` (declared constant, 0.25 — an ESTIMATE; the bill is measured).
  `rest.parts.unseen_prefix` is the RESIDUAL (system prompt, tool schemas, and whatever the constant
  under-counts), so `reconciliation_error` = Σ max(0, estimated − billed) ÷ Σ billed — the one way
  the identity fails — and `unseen_share` shows how much of the bill no transcript byte explains.
  Dollars come only from `PRICES` (exact model id); an unknown model or non-standard speed makes
  `cost.usd` null and is listed in `unpriced_models` — never guessed. `cost_state` (Claude Code's own
  total) is the independent check; it is booked under the alias asked for, so use its all-model
  `input_total_ratio`, not the per-model one, across backends.
- **`surface_usage`** — per skill / agent type / deferred tool: announced (attachments) × used
  (`Skill`/`Agent`/tool calls, typed `/command`, `invoked_skills`), announcement bytes and estimated
  tokens, top unused. Report only: nothing is pruned or written.
- Pure entry points (no daemon): `CostTracker`, `SurfaceTracker`, `cost_payload`, `surface_report`;
  `cargo run -p touring-cli --example context_cost_probe -- --sessions 10` prints both over a seeded
  sample of real sessions (counts and names only). Payloads never carry transcript text.

## `family_gate` and `lesson_lifecycle` (B2/B1, 01/11/2026, SHADOW)

Read-only, wired in `cli_kpi` after `surface_usage`; nothing writes a policy or a memory.

- **`family_gate`** (ADR-002) — per injection family (`context_budget`'s classifier): MUSTs emitted
  and followed (the global `injection_follow_ratio` rule, attributed to the family of the block that
  emitted it; denied MUSTs leave the denominator), injected tokens (bytes x `TOKENS_PER_BYTE`), and
  the recommended level `full|brief|off` from declared constants `N_MIN`/`TAU`/`TAU_OFF`. A family
  with no MUST, only denied MUSTs, or n < `N_MIN` is `not_measured` with the reason. Constants come
  from `calibrate` run on sessions before `APPLICATION_START`; the payload shows both windows and
  `calibration.constants_match_chosen`. Re-run `shadow_probe` and update the constants as the
  application window grows (today one family is observable out of sample).
- **`lesson_lifecycle`** (ADR-001) — a faithful PORT of `touring-server`'s
  `extract_error_resolution_pairs` (touring-cli cannot call it: dependency cycle) with the key
  normalised (`<path>`/`<hex>`/`<n>`), distinct sessions per key, `active` (k >= 3) vs `candidate`,
  and the overlap with the project's `memory.db` (read-only open). Parity with `f4a_lessons.py`:
  identical once the script reads only user/assistant records, as the miner does.
- Shell caps are `SHADOW_CAPS` (24 newest transcripts); the whole history comes from
  `cargo run -p touring-cli --example shadow_probe` (links every transcript into a temp HOME).

## `code_mode_signal_use()` (NEW 01/09/2026, F6)

In `src/cli/kpi/code_mode.rs` (`pub(crate) fn code_mode_signal_use`; the kpi module was split into
`src/cli/kpi/`), the function:
- Reads the F3 sink (`~/.claude/touring/sdk_signal_mirror.jsonl`) through
  `touring_code::sdk_signal_mirror::Mirror` (C5, 12/11/2026): `code_mode_signal_use_at(&Mirror)` is ONE
  `Mirror::read(0)`, so counts and latency come from the same lines and a torn line is skipped, never
  fatal. `hooks_complement` reads it once more through `hooks_complement_from(.., Option<&Mirror>, ..)`;
  no code in this crate parses a mirror line or builds the path
- Counts distinct canonical SDK hooks (8 total) + total call count (aliases like `cli-index-find` count
  in `non_canonical_calls`, never in `used`)
- Returns `{available, used, total, ratio, total_calls, non_canonical_calls, duration_ms_p50, duration_ms_p99}`
  (+ `skipped_lines` only when the reader skipped a line). A mirror that exists and cannot be read
  is `{available: false, reason: "unreadable: <why>"}`, distinct from `"no mirror yet"`;
  `hooks_complement` states the same two cases as `mirror_absent: true` / `mirror_unreadable: "<why>"`
  and leaves the mirror-derived numbers (`mirror_deliveries_*`, `post_bash_delivery_ratio`) `null`,
  never 0
- Wired into `cli_kpi()` (`src/cli/kpi.rs`) so `touring kpi -j` includes it

Public surface:
```rust
fn code_mode_signal_use() -> serde_json::Value
```

Returns fail-open shape when mirror is missing:
`{"available": false, "reason": "no mirror yet"}`.

## Example: `kpi_f6_smoke.rs`

`cargo run --example kpi_f6_smoke --` runs the F6 6-criteria-AND smoke
test directly against the journal + mirror (no daemon, no RPC) and
prints the verdict. Used as a regression test for the signal_use
gate pipeline without booting the full daemon stack.

## How to run

```bash
cargo build -p touring-cli                            # Build
cargo test -p touring-cli --lib                       # 2170+ unit tests
cargo run --example kpi_f6_smoke                     # F6 e2e
```

## CLI subcommands surfaced

- `touring kpi -j` — Falsifiable Commitments Dashboard (41 checks)
- `touring kpi --check` — exit 1 on any FAIL
- `touring kpi --snapshot` — persist docs/kpi/YYYY-MM/YYYY-MM-DD.json
- `touring kpi --refine` — F7 refinement actuators
- `touring doctor -j` — health dashboard
- `touring quality {score,check,list}` — 50-dim scoring
- `touring adw {new,run,explain,list,fragments}` — software factory
- `touring decompose {claim,ready,ticket,frontier}` — DAG primitives
- `touring index why <path>` — why a file is, or is not, in the index (I13)
- …50+ more in `src/cli/`

## Critical invariants

- **RPC handler registry**: every `cli-<name>` RPC MUST be declared in
  `build_dispatch_table` of `crates/touring-dispatch/src/hook_registry.rs` (and inverse: the
  the `every_declared_source_has_an_arm_and_every_derived_arm_a_commitment`
  test in kpi.rs cross-checks YAML contracts vs handler arms).
- **Handler return is JSON Value, not String** — the daemon wraps and
  returns to the RPC client. Returning `Value::Null` from any helper
  surfaces as `null` in the kpi payload (this was the F6 bug).
- **BestPracticesGate enrichment** (F5): the `code_mode_signal_use`
  field is added by `cli_kpi` AFTER the commitments load — never before
  (the yaml_path loader short-circuits with error JSON on failure).
- **Index exclusions are never silent (I13, 2026-09-13)**: the walker's three
  rule tables (`INDEX_SUPPORTED_EXTS`, `INDEX_SKIP_DIRS`,
  `INDEX_SKIP_SUBPROJECTS`), `dir_skip_reason`, the size ceiling and the
  per-path verdict live in `touring_hooks_shared::index_policy` and are the
  ONLY copy (re-exported by `src/cli/handlers/index.rs`) — the rebuild walker,
  the rebuild's sweep, the hook writers (`reindex_file_with_old` refuses what
  the walker refuses — and `touring index ingest` now REPORTS the refusal as
  `status: refused` instead of answering ok while writing nothing) and
  `cli-index-why` (`touring index why <path>`) all ask ONE
  `IndexPolicy::verdict_for_key` (`skipped_dir` / `skipped_subproject` /
  `unsupported_extension` / `oversized` / `unreadable` / `missing` / `gitignored` /
  `outside_root` / `companion_not_configured` / `indexed` /
  `eligible_not_indexed`). `why` also reports `residue: true` when the store
  still holds rows for a refused path. A new exclusion rule goes into the policy
  module, never inline in the walk, or `why` starts lying. Origin: the Graft
  analysis (`docs/plans/2026-09-12-graft-analysis/analysis.md` §22/§27/§29)
  found Graft dropping >1 MB files with `grep` claiming "all indexed code was
  searched" — and, in the live workspace, 5.865 files of residue that no
  rebuild removed.
- **The extension list is the parser's table seen twice (cross-audit 12/11/2026, F1)**:
  `INDEX_SUPPORTED_EXTS` admits every extension `Lang::from_path` reads — `.mts`/`.cts`,
  `.htm`, `.zsh` and `.jsonc` joined the list that day — except those named WITH A REASON in
  `PARSER_READS_INDEX_REFUSES` (`go`, `java`: `more-languages` grammars, admission a product
  decision). `the_index_admits_every_extension_the_parser_reads_or_says_why_not`
  (`index_policy.rs`) sweeps every `[a-z0-9]` extension of 1-4 characters through the public
  `Lang::from_path` and fails on a divergence in either direction, so a new grammar cannot
  reach the parser without the index being told. Gap: a 5+ character row is only seen if
  named in the test's `LONGER` (no public enumerator in `touring-code`). The same table now
  backs `cli-shadow-validate` (`rlsearch.rs`): an extension the parser does not read answers
  `error: unsupported_extension` instead of being judged as Rust. So do the AST and semantics
  handlers (`cli-ast-quality|calls|scope|imports`, `cli-resolve-def|find-references|rename`), all
  through ONE helper, `cli::shared::lang_or_error(file_path, neutral)` (the error plus the handler's
  own envelope), and `touring ast importance` (an error, exit 1). A `None` from `Lang::from_path` is
  never turned into a chosen language: `touring-code/tests/lang_fallback_guard.rs` scans
  `crates/*/src` and fails on a new `unwrap_or(Lang::X)` / `map_or(Lang::X, ..)` outside its
  written allow-list (empty). The one alias kept is `.markdown` in `shadow_lang` (the old private
  table knew it; the parser's table reads `.md`/`.mdx` only).
- **Git's ignore rules are part of the policy (cross-audit 14/09/2026, R2-2)**:
  `IndexPolicy::gitignored` asks `touring_foundation::gitignore::GitIgnoreRules`
  (`.git/info/exclude` + every `.gitignore` down to the path, deeper files win,
  nothing under an ignored directory comes back, global excludes left out on
  purpose). Both walkers (rebuild and `metadata backfill`) skip what git ignores,
  `verdict_for_key` reports `gitignored` naming the file and the rule, and the
  sweep purges stored rows by that reason. Companion roots are not the project's
  working tree and are never judged by it. The MkDocs build under `site/` had put
  47.877 stale symbols in the index. The sweep also judges CONSUMER files that hold
  no symbols (`consumers_purged`): 342 legacy edges whose consumers were gone from
  disk kept 72 producers wired.
- **`.claude/` is walked; only its runtime state is refused (2026-09-13)**:
  the hidden-directory rule has ONE named exemption — `.claude/` — because
  memories, rules, skills, plans and `CLAUDE.md` live there and "memórias e
  regras devem sim ser buscáveis" (Gabriel). Inside it, `CLAUDE_STATE_DIRS`
  (`touring/`, `session_summaries/`, `cipher_queue/`) are refused by name with
  the detail "holds Touring runtime state, not documents"; any other hidden
  directory stays refused. `dir_skip_reason(components)` sees the directory's
  ancestry, so the rule is decided in one place for the walker, the sweep and
  `why`.
- **Companion roots — the index carries directories OUTSIDE the project under
  stable keys (2026-09-13)**: `TouringConfig::companion_roots_for(root)` reads
  `.touring/touring.toml` (`[index] companion_defaults = true|false`,
  `[index.companion_roots] name = "path"` with `~` and `{slug}` expansion) and
  yields, by default, `rules`/`commands`/`agents`/`skills` under `~/.claude`,
  `memory` (this project's auto-memory, keyed by its Claude Code slug) and
  `memory-home` (the memory of `~` as a project). A full rebuild walks each
  companion with the SAME `walk`, contained to that companion (a symlinked
  skill pointing into another project is an escape and is not followed), and
  stores every file under `@companion/<name>/<rel>` (`IndexPolicy::key_for`);
  `why`, `ingest`, `find`, both searches answer under that key; `path_for_key`
  resolves it back to disk; the sweep purges `companion_not_configured` rows
  the moment a name leaves the configuration. Companion files are SEARCH, never
  WIRING (decision 1-A, 14/09/2026): the same skills sit in every project's
  index, so their producers would be orphans in every project's judge and their
  imports resolve to absolute paths. `touring_foundation::config::
  is_companion_key` is the one predicate; the `touring-storage` write gate
  refuses producer, consumer and unresolved-import rows under the key, and the
  rebuild skips the wiring work (asserted through `wiring_entries`, because the
  end-of-walk phantom sweep deletes the rows either way). A root WITHOUT `.touring/` has
  no companions — that keeps every scratch root (`rebuild --dir /tmp/x`, every
  test fixture) from walking `~/.claude/skills` by default. The rebuild payload
  reports `companion_roots: {name: files}`. Proof: `companion_roots_are_indexed_
  under_stable_keys_and_swept_by_the_same_policy` in `src/cli/handlers/index.rs`.
- **Projects declare their own exclusions (2026-09-13)**: `[index] exclude_dirs
  = ["client"]` in `.touring/touring.toml` lists directories RELATIVE to the root
  (never a bare name matched at any depth — a web app's `client/` is code).
  `TouringConfig::index_excluded_dirs_for` reads it; `IndexPolicy` judges it
  (`excluded_dir_reason`, `skipped_dir` with the detail "excluded by
  `[index] exclude_dirs`"), and the rebuild walker, the metadata backfill walker,
  the sweep and `why` all ask it. The touring workspace excludes `client/`, the
  GENERATED mirror of `~/.claude` that the companions already index from the
  original — two copies of every rule and skill took two places of every fused
  result. The mirror is also incomplete (16 of 17 rules, 304 of 809 skill
  files), which is why the companions, not the mirror, carry that content.
- **Search sees text, and the rebuild feeds it (2026-09-13)**: the rebuild writes
  each walked file's search documents right after its symbols
  (`shared::tantivy_docs::docs_for_file` + `refresh_file`, one commit at the
  end, `search_documents` / `search_refresh_failures` in the payload); the hook
  writer (`reindex_file_with_old`) and `touring tantivy reindex` use the SAME
  builder. Until then the rebuild never wrote to tantivy and `search` answered
  from the last manual reindex. Documents carry: file-path words
  (`module_path`), doc comments of code items and one module document per file
  (`ast::doc_text`), and, for markdown, a `document` symbol per file (frontmatter
  `name`, else first heading, else the directory for `SKILL.md`/`mod.rs`-like
  stems) plus 600-character chunks covering the whole body (`ast::markdown_text`
  — 84 of 98 memories had no heading and were invisible). The `document` symbol
  also carries description + body up to 20 000 characters: a chunk answers a
  localized phrase, the whole-file document answers words spread across the
  file. A title rides on every chunk under it, so titles are capped at 200
  characters and index names at 256: an 841.812-character heading in a converted
  PDF made 1,2 GB of text from 7,7 MB and aborted the analise rebuild (13/09/2026).
  Markdown text is
  indexed as written (`code_only` is for code). The `en_stem` analyzer folds
  diacritics and drops English/Portuguese stopwords, positions kept. Ranking is
  ONE builder (`TantivyIndex::ranked_query`) for `search` and the `tantivy search`
  route: names, text (0.25), path words (1.0), a proximity clause on names and
  one on text (6.0, off for identifier-shaped queries) built from the field's own
  analyzer, a constant COVERAGE bonus over text (5 for every query word present,
  1.25 per all-but-one subset; queries of 3-8 words, off for identifiers —
  length normalization otherwise lets a short chunk with two rare words beat the
  file holding all five; tantivy 0.22 has no minimum-should-match, so the
  conjunctions are built by `coverage_clauses`; a word counts in whichever field
  holds it), an IDF-weighted WORD coverage (12 spread over the distinct words, a
  constant per word present, so one name match of one rare word no longer buries
  the document holding the whole question), a 0.6 factor on the name and path
  boosts for a MIXED query (a command or identifier token among prose words,
  `is_mixed_query`), plus the file's best other
  candidate (λ 0.25) over a FIXED pool of 400
  candidates (sized by the request, the order changed with `--limit`). A commit
  invalidates the query cache, and every full write (rebuild, finished reindex)
  ends with `TantivyIndex::compact`: BM25 statistics count deleted documents until
  their segment merges, and a live index with 12 segments ranked the same
  questions worse than a fresh one. `search unified` fuses the `tantivy search`
  ranking (docs mode `ranked`, weight 1) with the BM25 docs lane (weight 0.05),
  one vote per file per lane — 50 of 55 against 27 for the old equal-weight
  fusion of the LIKE, BM25 and text lanes, which lowered every mix they joined;
  `search exact` and `search text` remain their own subcommands. A question is
  WORDS, never query syntax: `plain_words` quotes any token carrying a parser
  character before `parse_query` (14/09/2026: `rm -rf` excluded every document
  holding `rf`, `open()` failed the whole search). Every weight is an env knob
  (`TOURING_TANTIVY_*`) and every value was chosen by `examples/search_eval.rs`
  over 93 questions (train, held-out, two keyword-bag sets generated by
  `bench/make_bag_set.py` and three mixed sets, the held-out ones never used for
  tuning; a miss prints the truth's rank and score) — sets and grids in
  `docs/plans/2026-09-12-graft-analysis/bench/`. Re-measure there before changing one.
- **The rebuild's memory guard budgets ANONYMOUS memory (13/09/2026)**: resident
  size also counts mapped search-index segments the kernel reclaims first (up to
  1,1 GB during merges), so `MemorySnapshot::pressure_mb` is the guard's number
  and the payload reports `max_rss_mb`, `max_anon_mb`, `baseline_anon_mb` (what
  the daemon held before the walk) and `memory_hard_mb`. The budget counts the
  WHOLE daemon, so it follows the machine: a quarter of `MemTotal`, clamped to
  3000..=16000 MB, unless `TOURING_REBUILD_MEMORY_HARD_MB` is set in the DAEMON's
  environment (floor 500). The fixed 3000 MB stopped the analise rebuild (51.727
  files) twice — 18.821 files at 3.215 MB, then 17.052 at 3.064 MB with 1,3 GB
  already held by the idle daemon (14/09/2026). The daemon's allocator is jemalloc
  (`heap-profile` feature), not mimalloc; tree-sitter and SQLite allocate from glibc.
  `TOURING_REBUILD_SEARCH_DOCS=0` leaves search documents to `tantivy reindex`
  (measured cost on a 25.189-file walk: +181 MB anonymous). Measure memory with
  `examples/rebuild_memory_probe.rs` under `systemd-run --user --scope -p
  MemoryMax=8G`, and find a pathological input first with `search_eval --census`.
- **The rebuild is sealed, atomic per file and deterministic (I16, 2026-09-13)**:
  `cli-index-rebuild` opens an `index_generation` row (`building`) before its
  first write and seals it (`complete` / `aborted`) after the last; every
  reader — `index status`, `index find`, both searches, `why`, `doctor` —
  reports `partial` when the latest generation was left `building` by a dead
  daemon or `aborted`, with `remedy: touring index rebuild`. Each file's wiring
  rows (clear + producers + consumers) commit as ONE transaction, and the
  consumer kinds are resolved in ONE deterministic pass from the final producer
  table (`resolve_consumer_kinds_from_producers`), so a cold rebuild is
  byte-identical to a warm one. Acceptance is the measurement that found the
  defects: `kill -9` mid-rebuild loses no wiring row and is reported, and the
  cold/warm dumps diff to zero (analysis §28/§29). Since 12/11/2026 a test holds it:
  `cli::shared::heat_invariant::assert_tables_independent_of_heat` rebuilds the same tree with
  an empty heat map and with the heat ranking the files both ways, and the tables (rowids
  included, volatile cells found by a control run) must not move. It caught C7's
  "hottest first" walk — the rowids of `graph.db`/`symbols` diverged — and that reordering
  left BOTH batch walks (the rebuild and `file-knowledge populate`, whose test is
  `populate_leaves_the_same_tables_whatever_the_heat`): the walk's order is the walker's
  (path order), whatever the daemon's heat. A batch writer that wants heat must not let it
  decide the ORDER; heat decides eviction and prefetch only.
- **`index status` is answered off the actor (decision 3-A, 14/09/2026)**:
  `index_status_from_disk` reads committed state through READ-ONLY, NO_MUTEX
  connections and the dispatch serves it before queueing (`serve_status_off_actor`),
  so it answers during a rebuild's sealing phase, where no yield point exists. It
  returns `None` (the actor answers) when the databases are missing or the
  generation cannot be read, never caches `building`, and the rebuild invalidates
  the cache after the seal (cross-audit A6/A7).
- **Derived projections carry their provenance (I12, design rule, 2026-09-13)**:
  any human-readable projection of the index that is PERSISTED (cards, maps,
  `INDEX.md`-like files) must embed the fingerprint of the index that produced
  it and be marked stale when the index changes — never let a `grep` over a
  projection answer with the past. Today the index has no persisted projection
  (`map`/`overview` are computed on demand), so this is a rule, not a phase;
  it becomes a phase the day one is written. Origin: Graft's cards stayed stale
  after its incremental refresh (analysis §23, upstream #371).

## Cross-emitter invariant (Python ↔ Rust)

`scripts/gen_sdk.py` produces the same `SignalReport` JSON as
`touring_code::sdk::signal_report_from_journal`, and **Rust is the reference**.
What FAILS on drift is `crates/touring-code/tests/sdk_cross_emitter.rs` (it runs the
script over a journal with every family of line — runs, `{"repeat_blocked":…}`, non-object
and torn lines — and compares the parsed JSON, minus `generated_at_unix`); the
`sdk_smoke` example only WRITES a report and compares nothing (the old promise that it
failed on drift was false; the two had drifted, 57.405 × 57.539 runs, 12/11/2026). A "run"
is what the strict `JournalEntry` reader counts. `_extras` (byte totals) is the one
declared Python-only extension: opt-in with `gen_sdk.py --extras`, absent by default.

## See also

- Bundle code-mode-sinal F1-F6: `docs/plans/2026-08-31-code-mode-sinal/`
- F6 spec: `phases/F6-criterios-and-6.md` in that bundle
- Subcommand dispatcher (RPC handler registry): `crates/touring-dispatch/src/hook_registry.rs`
- The standalone CLI subcommand for kpi: `crates/touring-server/src/cli/kpi.rs` (calls `daemon_query("cli-kpi", ...)` → routed here)
- Elicitation script: `crates/touring-server/.claude/CLAUDE.md`