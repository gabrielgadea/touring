---
okf_version: "1.0"
type: CLAUDE
title: "touring-code — Unified code intelligence + Code-mode SDK surface"
description: "Crate that fused touring-ast + polyglot + language + semantics (W4) plus the code-mode-sinal F1-F6 SDK surface (journal.rs + sdk.rs + sdk_signal_mirror.rs). Auto-loaded by CC when sessions open files under this crate."
plan_id: docs
tags: [rules, crate, code-intelligence, code-mode, sdk]
timestamp: 2026-09-01T11:50:00-03:00
---

# touring-code — Crate Instructions

## What this crate does

Two responsibilities in one namespace:

1. **Unified code intelligence** (W4.2–W4.5): fuses 4 historically-separate
   crates under one tree:
   - `ast` (ex-touring-ast) — tree-sitter parsing + symbol store + surgery + call graph
   - `polyglot` (ex-touring-ast-polyglot) — ast-grep structural search
   - `languages` (ex-touring-language) — `Lang` enum + tier matrix
   - `semantics` (ex-touring-semantics) — Definition resolver + source-to-def

2. **Code-mode-sinal F1-F6 SDK surface** (01/09/2026): hybrid mirror+junction
   that materialises the in-sandbox SDK into a `SignalReport` consumable by
   the daemon and `touring kpi`.

## Modules (post code-mode-sinal)

| Module | Origin | Purpose |
|--------|--------|---------|
| `ast` | (ex-touring-ast) W4.2 | tree-sitter parsing + symbol store + surgery + call graph |
| `polyglot` | (ex-touring-ast-polyglot) W4.3 | ast-grep structural search |
| `languages` | (ex-touring-language) W4.4 | `Lang` enum + tier matrix |
| `semantics` | (ex-touring-semantics) W4.5 | Definition resolver + source-to-def |
| `journal` | code-mode-sinal F2 (NEW) | Aggregate stats over `~/.claude/touring/run_journal.jsonl`; the path, the line parsers, the writer and the failure taxonomy / exit status live in `touring_foundation::run_outcome` (02/11/2026) |
| `sdk` | code-mode-sinal F2 (NEW) | 8 hardcoded hook names + `SignalReport` JSON shape |
| `sdk_signal_mirror` | code-mode-sinal F3 (NEW) | Append-only JSONL sink of hook calls behind ONE interface (`Mirror::record` / `Mirror::read`); the line format, the default path, the canonical set and the percentiles live behind it (C5, 12/11/2026) |
| `types` | (ex-touring-types) | shared type aliases |
| `error` | (ex-touring-errors) | `Result<T>` + `Error` re-export |

## Code-mode-sinal F1-F6 (DAG `task_1788196388043002698`)

### F2 — S4 hybrid SDK surface (`sdk.rs` + `journal.rs`)
8 hooks hardcoded in `HookName::ALL` (names frozen by strategy v1.1 §D3):
`ast_meta`, `ast_blast`, `index_find`, `wiring_orphans`, `memory_recall`,
`pre_edit`, `tantivy_search`, `parallel`. Types are derived from the journal
via `signal_report_from_journal(path)` — fail-soft if the journal is
absent (returns `default` aggregate).

Public surface:
- `pub fn signal_report_from_journal(path: &Path) -> Result<SignalReport, SdkError>`
- `pub fn load_signal_report(path: &Path) -> Result<SignalReport, SdkError>`
- `pub fn parse_hook(s: &str) -> Result<HookName, SdkError>`
- `pub fn classify_bash_command(command: &str) -> Option<HookName>` — wave
  PostToolUse-wiring P3 (01/09): maps a real bash command line (`touring ast
  meta …`, env-prefix and binary-path tolerated) to its canonical hook.
  Precision-first: any ambiguity returns `None` (a false positive would
  inflate `signal_use`). Consumed by the `post_bash` handler in
  `touring-hook-handlers` to feed the mirror from CLI traffic.
- (`SdkValue`, an alias of `serde_json::Value` that no function returned, was
  removed in the cross-audit R2 orphan census, 15/09/2026.)

### F3 — PostToolUse sync (`sdk_signal_mirror.rs`)
Append-only JSONL sink at `~/.claude/touring/sdk_signal_mirror.jsonl`, behind ONE
interface (C5, 12/11/2026 — the sibling of `touring_foundation::run_outcome::Journal`).
Crash-safe (one torn line is recoverable: the reader works on BYTES, one line at a time, and SKIPS
a line that is not UTF-8, not JSON or has no string `hook_name`, COUNTING it in `skipped_lines`),
and "no file yet" (`None`) is information — not an empty aggregate, and not an unreadable file (`Err`).

Public surface (the Rust code of the workspace reaches the file only through this module — two
named exceptions, below):
- `Mirror::at(path)` / `Mirror::from_env()` (`$HOME/.claude/touring/sdk_signal_mirror.jsonl`) / `path()`
- `Mirror::record(HookName, duration_ms, success, MirrorOrigin) -> io::Result<()>`
- `Mirror::read(since_epoch) -> io::Result<Option<MirrorAggregate>>` (`0` = everything)
- `MirrorAggregate` — plain data: `total_calls`, `canonical_hooks_used`, `non_canonical_calls`,
  `post_bash_calls`/`sdk_calls`/`unknown_origin_calls`, `duration_ms_p50`/`duration_ms_p99`,
  `skipped_lines` (non-blank lines that could not be read, whatever the window);
  `canonical_calls()` = `total_calls - non_canonical_calls` (the deliveries the KPIs credit)
- `MirrorOrigin::{PostBash, Sdk}` + `as_str` (the wire value; `touring-server`'s Python SDK
  writes the same line itself and a test in `run.rs` pins `origin`)

Consumers: the `post_bash` handler (record), `touring run --orchestrate` (the path handed to the
sandbox), `touring kpi` (`code_mode_signal_use`, `hooks_complement` — one read each), the
`kpi_f6_smoke` example.

Exceptions — copies that still know the line format or the path, on purpose:
- `touring-server/src/cli/run.rs` (the Python SDK template, ~309-370): the sandbox writes the same
  line itself, in another language; `orchestrate_python_sdk_mirror_lines_declare_sdk_origin` pins
  the `origin` literal to `MirrorOrigin::as_str`.
- `touring-quality/src/builtins/best_practices.rs` (`signal_use_counts` + the path at ~243): builds
  the path and reads `hook_name` by hand because that crate must stay standalone-buildable
  (`--no-default-features`, no dependency on `touring-code`). Its `SDK_HOOK_NAMES` is a copy of
  `HookName::ALL`, held in step by `touring-server/tests/sdk_hook_names_parity.rs`.

### Incremental pipeline — heat and memory budget (`ast/incremental_pipeline.rs`, C6/C7, 12/11/2026)
The daemon's `SharedPipeline` (`touring-hook-runtime`, `infra.pipeline`) has ONE eviction
policy, applied by the caller that just fed it a file and holds the heat map:
- `SharedPipeline::with_memory_budget_mb(mb)` — ceiling for the ROPE bytes of loaded documents
  (knob `TOURING_PIPELINE_MEMORY_MB`, read at daemon boot: unset = `max(64, floor((H/8 - 260)/1,43))`
  with H = `rebuild_memory_hard_mb()`, one measurement of one project; `0` = unlimited, the kill
  switch; an invalid value = the default plus a log warning; `touring status -j` prints
  `pipeline.budget_source` = `env|default|disabled`). `SharedPipeline` is `Send + Sync` by
  construction (no `unsafe impl`); `AsyncSharedPipeline` and `clear_cache` had no consumer and are gone.
- `enforce_budget(&HeatMap, keep: Option<&str>)` — coldest first (no heat entry < lowest score < path order),
  never `keep`; the victim loses document, parse tree AND symbol cache. `memory_usage_mb` is rope
  bytes, not RSS: the parse-tree cache is capped on its own (LRU, `MAX_TREE_CACHE` = 128 trees), and
  `examples/pipeline_memory_probe.rs` measured on this workspace (11.579 files, 214 MB of rope)
  RssAnon ~= 260 MB fixed + ~1,43 MB per rope MB. `process_file`/`process_edit` do NOT evict by themselves.
- `HeatMap` keys are project-relative paths (the index's spelling); `HeatMap::insert`/`get` seed and
  read it. `prefetch_from_heat(root, &HeatMap, n, admit)` warms the `n` hottest files that `admit`
  accepts from under `root` (the runtime passes the index's own admission predicate).
- The heat never orders a batch: the rebuild and `file-knowledge populate` walk in path order whatever the
  heat (I16: a "hottest first" queue, `PrioritizedPipeline`, made `graph.db` rowids diverge between a cold and
  a warm daemon, and left with its only consumer). Heat decides the eviction victim and the session prefetch.
  `FileHeat::blast_radius_weight` has no production producer (always `0.0`; `touring ast heat` says
  `blast_radius_weight_has_producer: false`). `stats()` -> `PipelineMemoryStats` feeds `touring status -j`
  (`pipeline`, `heat`).
- The cortex keeps ANOTHER real `IncrementalPipeline` (static, no budget); `status` says
  `cortex_pipeline: "not_counted"`.

### Examples (use `cargo run --example <name>`)
- `sdk_smoke` — write the Rust `SignalReport` of a journal (compares nothing; the drift guard against
  `gen_sdk.py` is `tests/sdk_cross_emitter.rs`, Rust is the reference)
- `pipeline_memory_probe` — load a tree into a `SharedPipeline` and print rope MB, trees and RssAnon
  before/after (run under `systemd-run --user --scope -p MemoryMax=8G`)
- (kpi examples live in `touring-cli/examples/`, not here)

## Deep modules (C1-C4, C8 slice 1 — 12/11/2026; why and measurements: `docs/plans/2026-11-12-touring-code-aprofundamento/C*-desenho.md`)

### `ast::grammar` — one parse door per language (C1; `src/ast/grammar.rs`, all `pub(crate)`)
- `Grammar::{of_extension(&str, Reach), of_path(&Path, Reach), of_lang(Lang), lang(), name(), from_name(&str),
  language(), parse(&str)}`; `parse_str(&Language, &str, Option<&Tree>)`, `parse_chunks(&Language, usize, &mut F,
  Option<&Tree>)` (rope); all `-> Result<Tree, Refusal>`, `Refusal::{Rejected(String), Halted}`.
- Guards: ONE `Parser::new()`/`set_language` (`new_parser`), ONE extension table (`EXTENSIONS`), ONE per-thread parser
  cache, ONE budget (2 s + 2 s/MiB, progress-callback halt). A second bare `Parser::new()` fails
  `the_crate_builds_and_configures_a_parser_in_exactly_one_place`.
- Trap: `Reach::Exact` (extensions that ARE the language: VGP verdicts, `polyglot::detect_lang`) vs `WithNeighbours`
  (`.scss`/`.mdx`/`.jsonc`: `Lang::from_path`) — a verdict on `WithNeighbours` fails an `.scss` for CSS syntax it never
  had. `.ts` is `TypeScript`, `.tsx` is `Tsx`, but `of_lang(Lang::TypeScript)` has no dialect and reads TSX (a `Lang::Tsx`
  would break other crates' exhaustive `match`es). Extension CASE is the caller's policy. Over budget = `Err(Halted)`.

### `ast::graph::FileReferences` — one "references of a file" pass (C2; `src/ast/graph/references.rs`)
- `FileReferences::new(&str, Lang)` (a borrow and a tag; each method reads the source when asked): `imports()`,
  `consumed_imports()`, `forwarded_paths() -> HashSet<String>`, `inferred() -> InferredReferences`,
  `python_qualified_uses()`, `module_references() -> ModuleReferences`, `module_declarations()`,
  `self_references(&[Symbol]) -> BTreeSet<String>`; `ImportInfo`/`ModuleDeclaration` stay `pub`.
- Guards, decided once: importing is not using (`consumed_imports` drops a Rust `pub use` the file never names in its own
  code and a Python name never touched outside its import; symbols go, modules never); a re-export counts only if used;
  a declaration is not a reference to itself; an unruled language or an unparseable source answers EMPTY, never an
  error. The five extractors (`imports`, `method_calls`, `python_uses`, `module_paths`, `self_refs`) are `pub(crate)`.
- Trap: `imports()` order is unstable outside Rust (the query path accumulates in a `HashMap`, Python included): no
  consumer may depend on it. Each method still parses on its own; persisting the answers, and in what order, is the consumer's.

### One write path for an indexed file (C3; `ast/graph/mod.rs`, `ast/store.rs`)
- `SymbolIndex::{index_file(&mut self, &str, &str, Lang) -> AstResult<()>, index_files, remove_file(&mut self, &str)}`
  write only through `pub(crate) insert_symbol(SymbolLocation)` / `insert_edge(DependencyEdge)` and private `withdraw`;
  `importers_of(&self, &str) -> &[String]` reads. SQLite: free fns `put_symbol_row`, `put_edge_row` (row plus its
  `dependency_symbols`), `delete_file_rows`; a multi-row verb runs in a `rusqlite::Transaction` (rollback on drop).
- Guards: `file_to_symbols[f]` lists the names in `symbols[*]` with `file_path == f`; `reverse_deps[m]` lists the `from`
  of every edge with `to == m`; no map holds an empty list. So index-then-remove restores the index; `index_file` REPLACES.
- Trap: FILE key (the path as indexed: `symbols`, `file_to_symbols`, `dependencies`, SQL `file_path`/`from_file`) vs MODULE
  key (what an import names, `utils`/`crate::a::b`: `DependencyEdge::to`, SQL `to_file`); the only bridge is `module_key`,
  the file STEM, a heuristic (`crate::a::b` never matches `b`). `importers_of` credits a stem only to the ONE indexed file
  owning it; `SymbolStore::get_reverse_deps` has no such rule, so memory and SQLite disagree on `a/utils.py` x `b/utils.py`
  on purpose (`memory_and_sqlite_part_ways_on_a_homonymous_stem`). `reverse_deps[<file path>]` by hand is the wrong key.

### Resolved edges and coverage (C8) — one loader, one writer of edges, loaded in production
- `SymbolIndex::{load_file_edges(edges: impl IntoIterator<Item = (String, String)>, covered: impl IntoIterator<Item =
  String>), replace_edges_of(&mut self, &str, &[String]), apply_co_edit_weights(pairs: impl IntoIterator<Item = (String,
  String, i64)>), importers_status(&self, &str) -> Importers<'_>}`; `enum Importers<'a> { Known(&'a [String]), Isolated,
  Uncovered }`; `ast::store::load_index(&SymbolStore, &FileKnowledgeDB) -> Result<SymbolIndex, rusqlite::Error>`.
- Guards: the wiring is the one writer of resolved file-to-file edges; the index only LOADS them under FILE keys, and a
  file key beats the stem in `importers_of`. A covered file keeps them when reindexed. Absence is not zero: an empty
  list is `Isolated` only where the source looked, else `Uncovered`.
- Edge rule (F13, 13/11/2026): an edge needs IMPORT evidence. `touring-storage`'s `file_edge_filter!` (the ONE rule that
  `file_dependency_edges`, `file_importers` and `file_dependencies` share) keeps a consumer→producer pair only when one of its
  `wiring_map` rows is not `ast_inferred`: a bare-name match (cap 4, same crate first) exists to keep a SYMBOL from reading as an
  orphan and is not a file dependency (measured: 77.572 edges collapsed to 6.722; `domain_circuit.rs` 1.236 → 2 importers).
  So `Isolated` means "no RESOLVED import points here", never "nothing depends on it" — the wording of `blast_radius_signal` ("nenhum
  import resolvido aponta para este arquivo") says exactly that. See `C8-desenho.md` §13.
  - F16 (13/11/2026): a Python alias use (`import m as x` / `from p import m [as x]` + `x.Nome`) IS import evidence — the module path comes
    from the file's own import and goes through the import resolver — so `record_python_qualified_uses` writes `ast_resolved` and, in the
    rebuild, records it in the file's own walk step (it is cleared with the file's other non-inferred rows; deferred to the post-walk pass it
    would be missing until the walk ended). Trap that remains: an import the resolver cannot place (`sys.path` tricks; analise: 80 of 3.094
    inferred-only `.py` pairs) reads as no edge — declared debt, not guessed.
  - F15 (13/11/2026): a raw identifier names the same module as the plain one (`use crate::plugin::r#trait::{..}` is `plugin/trait.rs`);
    `resolve_module_layout` in `touring-hooks-core` drops the `r#` of a segment, the one place a module path becomes a file.
- Density rule (13/11/2026): `Uncovered` is a typed state, not a reason to speak. `SymbolIndex::coverage_expected(file) -> bool` is
  true when the edge source covers at least one file with this file's extension (case-insensitive; derived once from
  `edge_coverage` into a cache that every writer — `load_file_edges`, `replace_edges_of`, `remove_file`, `clear` — drops, never
  serialized; an index with no edge source answers false). The pre_edit/pre_read/pre_write reader (`blast_radius_signal`) says
  "sem cobertura" only for `Uncovered && coverage_expected`: a source file the wiring should have read. A doc, a config, a `.py`
  where only `.rs` is wired, or a project with no coverage at all gets SILENCE (silence, unlike "nenhum import resolvido", claims nothing);
  `detect_changes.coverage` and `file_edges` in `status` still count every uncovered file.
- `SymbolIndex::ranked_importers(&self, &str) -> Vec<&str>` — the SET `importers_of` answers, ordered by co-edit weight of the
  edge toward the file, then the importer's own in-degree (file-key bucket only), then path: the order a text that names only a
  few importers must use. `weighted_blast_radius` ties are now in path order (the list is cut to its first names).
- More on the index (C8 runtime, 12/11/2026): `blast_extent(&str, usize) -> BlastExtent { file_count, max_distance }` (counts
  only; the same walk as `blast_radius_with_depth`, which now shares its private `walk_importers`), `edge_coverage_summary()
  -> EdgeCoverage { edges, covered_files }`, `hotspots(limit) -> Vec<(&str, usize)>` (FILES by importers, never a module key),
  `ast::store::load_index_for(&SymbolStore, &Path)` (the loader for a caller with a store and only the knowledge PATH: no
  wiring store = what `load_into_index` always gave, no resolved edge and no coverage, so the index speaks for the files it
  holds as it always did; the missing store is never created). `replace_edges_of` keeps the co-edit weight of a
  pair the consumer still has (a hook that refreshes an edited file must not zero its importers' ranking).
- PRODUCTION (12/11/2026): the index has ONE loader and it loads ON DEMAND. `HookRuntime::load_symbol_index`
  (`touring-hook-runtime/src/hook_runtime_ext.rs`) calls `load_index`; the daemon does NOT call it when it builds a project's
  runtime (a project nobody reads would pay ~133 MiB and ~259 ms). The first read that needs a graph signal loads it, through
  `HookRuntime::index_generation()` (every hook consumer asks there; one `IndexLoad::describe` log line per load; a failed load is
  marked and not retried per read) — for the runtimes the DAEMON builds, which turn on `infra.index_on_demand`: a one-hook
  `touring-hook` process leaves it off, because loading per invocation costs ~259 ms and ~133 MiB each time and would change what
  the standalone route prints (`touring-hooks/tests/standalone_table_parity.rs` holds it byte for byte); there the graph signals
  stay as before C8, while the symbol queries (`get_symbol_index`, `find_symbol`) load on demand in every runtime. `index rebuild`
  reloads it only if a generation is held
  (`reload_symbol_index_if_loaded`; `index_reload.state` is `loaded`, `not_loaded`, `disabled`, `no_store` or `failed`, never a
  zero count for an unloaded index). Before C8 nothing loaded `infra.symbol_index`
  in production, so every graph signal was mute. The index lives in an `IndexSlot` (`index_slot.rs`): one `Arc<SymbolIndex>`
  plus one HNSW cache per loaded generation, zero copies per read (`IndexSlot::get`), copy-on-write when a hook edits while
  a reader still holds the generation. Hooks keep it in step: after `refresh_file_wiring` the edit, read, file-changed and
  task-output paths call `HookRuntime::sync_index_edges_of` -> `replace_edges_of`, so an import added or removed shows in
  `importers_of` with no rebuild. The server (`graph_service.rs`, `server/mod.rs`, `tools_analysis.rs`, `ast_tools.rs`) loads
  through `load_index_for`; `detect_changes` counts `affected_files` with `importers_of`, its hotspots list FILES, and its
  payload carries `coverage: {changed_files, covered, uncovered, uncovered_files, caveat?}` (`importers_status`), so an empty
  `affected_files` for a file the wiring never looked at reads "not looked at", not "isolated"; the number is untouched.
- Human kill switch `TOURING_FILE_EDGES_DISABLED=1`, read when the runtime is built: the index is NEVER loaded and the graph
  signals stay silent, as before C8 (loading it without the edges would answer "isolated" for every file, a different lie).
  `touring status -j` prints `file_edges: {enabled, source: default|disabled, edges, covered_files}`; the last two are `null`
  with a `reason` while no index is loaded, which for an enabled project is the state right after opening it (status reads the
  slot and never loads it). Knob `TOURING_GRAPH_BLAST_MAX_CHARS` (120..=3200, default 400): the ceiling of the
  blast group's text in the `pre_read` pipeline.
- Guards: `tests/index_loader_guard.rs` — no `.load_into_index(` call in production outside its written allow-list (the loader's
  own body and the server's cross-project merge, symbols only ON PURPOSE: files of different projects share keys), exactly one
  `load_index(` caller in `touring-hook-runtime`, `.load_symbol_index(` called only from `hook_runtime_ext.rs` (the loader's two
  entry points), the daemon's `daemon.rs` touching the index nowhere (an eager load there is the regression) but setting
  `index_on_demand = true` (without it every graph signal goes quiet again), and no
  `Arc::new(<index>.clone())` anywhere. Retired: `pre_read.rs`
  `bfs_hop_signal` (a second BFS over a whole-index clone per read; the pipeline's blast group says the same in one bounded line).
  SQLite (`get_reverse_deps`) still never sees the resolved edges, so it and memory diverge on homonyms (the server's
  `detect_changes` no longer asks it).

### `vgp::lang::vista` — overlays over the disk (C4; `src/vgp/lang/vista.rs`, `pub(crate) mod`)
- `Vista::nova(&HashMap<String, String>)` (Copy): `sobreposto(&Path) -> Option<&str>`, `sondar(&[P], &Path) -> Sonda`,
  `e_arquivo`/`e_diretorio`/`existe(&[P], &Path) -> bool`, `ler(&[P], &Path, Option<u64>) -> Result<String, Recusa>`,
  `outros(&Path)` (every overlay but `p`); `Sonda`/`Recusa` are `caminho`'s.
- Guards: overlay beats disk and ignores the roots; the disk is touched only through `caminho` (`so_o_caminho_toca_o_disco`);
  a directory exists if the disk says so OR an overlay lies under it; the key is the lexically normal absolute path and
  ONLY the key is normalized (the disk gets the caller's path); no read of the map outside `vista.rs`
  (`so_a_vista_consulta_o_mapa_de_overlays`); the empty map the hooks pass costs one `is_empty()`.
- Trap — divergences kept ON PURPOSE (`C4-desenho.md` D2, D4-D7), do not "unify": D2 Rust asks the DISK only whether a
  manifest/`lib.rs`/`main.rs` exists (`caminho::e_arquivo`) but reads the overlay first, so an overlay-only crate is an
  abstention; D4 read cap = `limite` (TS `MAX_LEITURA` 3 MiB, CSS `MAX_BYTES_FOLHA` 4 MiB, Rust/Python none, overlay never);
  D5 HTML asks `existe`, CSS/TS/Rust `e_arquivo`; D6 TS/HTML/CSS normalize the path before the disk, Rust/Python pass it
  raw; D7 only Python memoizes reads (`fs_cache`).

## How to run tests

```bash
cargo build -p touring-code                        # Build (release for kpi examples)
cargo test -p touring-code --lib --tests           # unit tests
cargo test -p touring-code --test sdk_cross_emitter    # Rust x gen_sdk.py must print the same report
```

## Migration from the fused crates (for older callers)

```text
use touring_ast::X         → use touring_code::ast::X
use touring_ast_polyglot::X → use touring_code::polyglot::X
use touring_language::X     → use touring_code::languages::X
use touring_semantics::X    → use touring_code::semantics::X
```

The 3 NEW modules (`journal`, `sdk`, `sdk_signal_mirror`) have no
upstream — they were created in code-mode-sinal F1-F6 (01/09/2026) and
have NO migration path. New callers should import them directly.

## Critical invariants

- **Both modes co-exist**: the `ast/polyglot/languages/semantics` modules
  are unchanged from W4; the new `journal/sdk/sdk_signal_mirror` modules
  are additive. A diff against `main` shows 3 new files, ~750 LOC.
- **No daemon coupling**: `touring-code` is a pure library with zero
  IPC. The daemon lives in `touring-server` and embeds this crate.
- **JSONL invariants**: `Mirror::record` writes `ts`, `hook_name`, `duration_ms`, `success`,
  `origin` (that order); `Mirror::read` skips a line it cannot read — not UTF-8, not JSON, no
  `hook_name`: never an abort — counts it in `skipped_lines`, and degrades the optional fields (no
  `ts` = 0, no duration = no sample). `Err` is only for a file that exists and cannot be read.

## Lint hygiene (matches workspace-wide RBP-01)

```bash
cargo clippy -p touring-code --all-targets -- -D warnings   # 0 warnings
```

`#![cfg_attr(not(test), deny(missing_docs))]` enforces doc comments on
all public items — measured 0 violations across the 3 new modules.

## See also

- Bundle code-mode-sinal F1-F6: `docs/plans/2026-08-31-code-mode-sinal/`
- Strategy-doc D3 (hybrid SDK): `docs/plans/2026-08-31-code-mode-sinal/strategy-2026-08-31-yetzirah-v1.1.md`
- Daemon caller (re-embeds this crate): `crates/touring-server/src/cli/run.rs` (the Python template's `record_hook_call` + `query()` wrap, ~309-370)
- Python emitter (CI without Rust toolchain): `scripts/gen_sdk.py`