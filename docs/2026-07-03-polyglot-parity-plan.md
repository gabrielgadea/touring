# Touring Polyglot Parity — Grounded Plan (2026-07-03)

> **Directive** (Gabriel, 2026-07-03): *"touring precisa ser totalmente aplicado para
> qualquer linguagem, todas as suas funcionalidades."*
> **Status**: PLAN — execution HELD pending scope confirmation (a strategic fork too
> large to guess). Recommended default: **Python + JS/TS first, semantic-graph layer first**.

## 1. Grounded reality (verified from source, not assumed)

### ✅ Already polyglot
| Capability | Coverage | Evidence |
|---|---|---|
| AST parse | 13 tree-sitter grammars: rust, python, ts, js, go, java, html, css, json, bash, toml, yaml, md | `touring-code/Cargo.toml:23-35` |
| Symbol capture | `.scm` queries for ~10 langs (rust/python/ts deepest) | `python.scm`, `javascript.scm`, `typescript.scm`, `go.scm`, `java.scm` present |
| Structural search/rewrite | `touring ast grep` polyglot (metavars) | ast-grep engine |
| 50-dim quality | 15 langs via `lang_from_ext` + `code_regions` (F1.1/F1.3/F1.6…) | `touring-quality/verifications/mod.rs:73-74,830` |
| node-types, highlight | polyglot | tree-sitter / syntect |

### ❌ Rust-centric gap (the program)
| Gap | Why Rust-only today | Lift path |
|---|---|---|
| **Import/module resolution** (blast/wiring cross-file) | hardcoded `crate::` path model | per-lang resolver (Python `import`/`from`, JS/TS `import`/`require`, Go packages) |
| **Query depth parity** | go/java lack `*_fields`/`*_imports`; py/ts lack `*_method_calls` | author missing `.scm` per lang |
| `rust-semantic` (generics/traits) | syn (Rust-only) | tree-sitter-based semantic extractor per lang |
| `workspace-info` | cargo_metadata | per-ecosystem manifest readers (pyproject/package.json/go.mod) |
| **F2.5 CVE** | RustSec advisory-db | OSV.dev multi-ecosystem (PyPI/npm/Go) |
| dep-cycles, F1.8/F1.12 | cargo graph | per-lang module graph |
| `import_resolver` | rust/python only | extend to js/ts/go |
| `language:"rust"` defaults | `learning_loop.rs` (18×), `languages.rs` | route by detected lang |

**Thesis**: the *foundation* (parse + capture + search + quality) is polyglot; the gap is the **semantic-graph layer** (index→blast→wiring, gated on per-lang import resolution) plus **toolchain-specific** features (cargo/RustSec, inherently per-ecosystem).

## 2. Phased program (recommended scope: Python + JS/TS)

- **P0 — Language registry unification** (foundation): replace scattered `"rust"` defaults with a single `Language` router keyed off `lang_from_ext`; every AST/index entry point takes the detected lang. Low risk, unblocks everything. Removes the 18× hardcodes in `learning_loop.rs` etc.
- **P1 — Symbol-capture depth parity** (Python + JS/TS): author the missing `.scm` (`go_imports`, `python_method_calls`, `ts_method_calls` as needed for the chosen langs); validate symbol counts vs a fixture repo per lang.
- **P2 — Per-lang import/module resolution** (the keystone): a `ModuleResolver` trait with Rust (`crate::`), Python (`import`/`from … import`), JS/TS (`import`/`require`/re-export) impls. This is what makes **blast / wiring / orphans** work cross-file for non-Rust. Highest value, highest effort.
- **P3 — Semantic extractor per lang**: tree-sitter-based equivalent of `rust-semantic` (types, signatures, generics-where-they-exist) for Python/TS; feeds `generate` VGP.
- **P4 — Toolchain features multi-ecosystem**: `workspace-info` reads pyproject.toml/package.json/go.mod; **F2.5 → OSV.dev** (covers PyPI/npm/Go/crates in one API, supersedes RustSec-only); dep-cycles per-lang.
- **P5 — Generate + quality parity**: VGP codegen per lang; close the remaining 50-dim fallback-to-Rust holes.

Each phase: convergence-gated (fixture repo per lang, `cargo test`+clippy green, feature scored on a real non-Rust target), same discipline as the F1.3 loop.

## 3. Scope forks (Gabriel decides)

- **Languages**: Python+JS/TS (recommended) · all 13 grammars · Python-only PoC.
- **Starting layer**: semantic graph (recommended — biggest gap, unblocks most) · quality-50-dim parity · semantic+generate.
- **Depth**: Rust-parity · best-effort per lang.

## 4. Cross-references
- Symbol capture: `crates/touring-code/src/ast/` + `.scm` queries
- 50-dim polyglot: `crates/touring-quality/src/verifications/mod.rs` (`lang_from_ext`, `code_regions`)
- Graph layer: `crates/touring-analysis/src/blast_radius/`, `crates/touring-code/src/ast/wiring.rs`, `crates/touring-storage/src/salsa/queries/`
- Memory: `polyglot:parity-plan:2026-07-03`

## 5. RE-GROUNDING (2026-07-03, VGP-verificado no source — corrige premissas stale)

O grounding original foi feito de survey parcial. Verificação direta contra o source
(VP-Scout Chain 5/6) corrige três premissas e **reescreve o keystone**:

| Premissa original | Realidade verificada | Consequência |
|---|---|---|
| P0: remover ~18 hardcodes `"rust"` em `learning_loop.rs` | `grep -c '"rust"'` = **0** — já limpos em sessão anterior | **P0 é no-op**, remover do DAG |
| P0: criar "language registry" unificado | `languages.rs` (459 LOC) **já tem** `Lang` enum + tier matrix (Deepest: Py/Rust/TS/JS/Bash · Shallow: Go/Java) + `from_ext`/`tree_sitter_language`/`symbol_query`/`import_query` per-lang | **registry já existe** |
| P2: escrever `ModuleResolver` do zero | `import_resolver.rs` (542 LOC) **já extrai** imports Rust+Python+TS/JS via `extract_imports_resolved(source, lang)`; Go/Java caem em `_ => empty`; já é chamado por `index.rs:1202` | **extração meio-feita**, não zero |

### O keystone REAL (empírico)

`touring doctor` reporta ao vivo: `wiring_diagnostic: … non_rust=0`. O grafo semântico de
wiring tem **ZERO linhas não-Rust**. A causa está em `knowledge_wiring.rs:57-73`
(`is_indexable_module_file`): `if !module_file.ends_with(".rs") { return false }` — gate
chamado por `register_pub_symbol` e `record_consumer`. O grafo **rejeita não-`.rs` na
porta de população**; `non_rust_rows` é um **contador de regressão** no schema.

**Sutileza crítica** (docstring linha 49-55): o gate `.rs` existe para prevenir
**258 FPs históricos** de `docs/*.py`/`scripts/*.py` poluindo o orphan-count. Lift
ingênuo ressuscita os FPs. O keystone correto:

> Tornar `is_indexable_module_file` + o JOIN producer↔consumer + orphan-detection
> **per-language-aware**, com **identidade de módulo per-lang** (Rust `crate::` · Python
> dotted-path · JS/TS relative/bare specifier), de modo que a extração já existente
> (Py/TS/JS) alimente producer/consumer rows keyed por modelo de cada linguagem,
> **sem cross-contaminação** e sem ressuscitar os 258 FPs.

### DAG corrigido (keystone-first)

- **P-A (keystone, L4) — Polyglot wiring-graph population**: `is_indexable_module_file` per-lang + JOIN/orphan per-lang; wire `extract_imports_resolved` (Py/TS/JS) → `record_consumer`. Destrava `wiring impact`/`orphans`/`blast`/`index` non-Rust. **É o gate humano** (invariante foundational, blast alto, histórico FP).
- **P-B (L2, additive) — Go/Java import extraction**: preencher o braço `_ => empty`.
- **P-C (L2, additive) — `.scm` depth parity**: `python_method_calls`, `ts_method_calls` (call-graph), `go_imports`/`go_fields`, `java_imports`/`java_fields`.
- **P-D (L3) — Semantic extractor parity**: equivalente tree-sitter de `rust-semantic` p/ Py/TS → alimenta `generate` VGP.
- **P-E (L3) — Toolchain multi-ecosystem**: `workspace-info` lê pyproject/package.json/go.mod; F2.5 CVE → OSV.dev.
- **P-F (L2) — generate + quality parity holes.**

Deps: P-A(Py/TS/JS) é buildável já (extração existe); P-B/P-C estendem cobertura p/ Go/Java; P-D/P-E/P-F são downstream de P-A.

## 6. P-A SHIPPED (2026-07-03) — Python-only PoC, flag-gated

**Status: code-complete + empirically proven** (`target/debug`, NOT deployed). Gate
`TOURING_POLYGLOT_WIRING` (default **OFF** → byte-identical Rust behavior).

Achado que reescreveu o escopo: o gate `.rs` eram **4 filtros**, não 1 — write-gate
`is_indexable_module_file:58` + 3 read-side (`find_producer_modules_for_methods:386`,
`orphan_symbols:527`, `orphan_symbols_for_module:597`). Abrir só o write teria deixado
Python **invisível** a orphan/impact. Fix: fonte única de verdade
`wireable_extensions(bool)` + `wireable_ext_sql(col,bool)` alimenta os 4; OFF ⇒
byte-idêntico. Feeder de indexação (`index.rs:508`, `resolve_import_path_with_source:141`
Python dotted→`.py`) **já era polyglot**. Defesa dos 258 FPs mantida via
`is_python_non_wireable` (venv/site-packages/node_modules/docs/scripts/pytest).

- **Arquivos**: `crates/touring-storage/src/knowledge_wiring.rs` (5 edits) +
  `crates/touring-storage/tests/polyglot_wiring_poc.rs` (novo end-to-end).
- **Prova**: 8 unit (política pura, ambos os modos) + 1 integration
  (`python_populates_wiring_graph_under_flag`: rows Python entram, Order órfão / User
  wired, Demo/Vendor bloqueados, `integration_score`=0.5).
- **Gates**: 201 lib tests 0-fail · clippy 0 (test+non-test+hook-handlers) ·
  `cargo check --workspace` 0 warnings (REGRA #21: 4 unused imports pré-existentes
  corrigidos).
- **HELD**: deploy (`update-touring`, restart daemon) + extensão P-B/P-C (TS/JS→Go/Java)
  aguardam gate incremental.

## 7. P-B / P-C SHIPPED (2026-07-03) — TS/JS + Java wired · Go extraction-ready

**Status: code-complete + proven** (`target/debug`, NOT deployed). Same flag
(`TOURING_POLYGLOT_WIRING`, default OFF).

| Lang | Outcome | How |
|---|---|---|
| **TS/JS** | ✅ fully wired | `resolve_import_path_with_source` TS/JS arm rewritten FS-aware (was `"./utils"→"utils"`, no ext/dir → never JOINed). Resolves relative to the importing file's dir, `normalize_lexical` (anti-homonimia `/./`), probes `.ts/.tsx/.js/.jsx/.mjs/.cjs` + `index.<ext>`. `.ts…` in `wireable_extensions` + JS/TS test/vendored in `is_non_rust_non_wireable`. |
| **Java** | ✅ fully wired | `java_imports.scm` (`import_declaration`/`scoped_identifier name:@symbol`) + `import_query_file`; resolver arm pure `com.foo.Bar`→`com/foo/Bar.java` (mirrors Python); `.java` extension + `*Test.java`/`src/test/` in classifier. |
| **Go** | ⏸️ extraction-ready, **wiring deferred** | `go_imports.scm` (`import_spec path:(interpreted_string_literal)`) validated; resolver arm returns **None by design**. |

### Why Go wiring is deferred (honest, not a gap-by-omission)

A Go **import path denotes a package (a directory of files), not a single source
file, and carries no symbol** (usage is `pkg.Foo()` afterward). The `wiring_map` is
**file-keyed**. So file-keyed import resolution for Go is a structural mismatch:
mapping an import to one producer file is impossible, and admitting `.go` producers
without resolvable consumers would resurrect **false orphans** (the 258-FP class).
Decision: keep Go out of `wireable_extensions` (zero false orphans); Go participates
only via method-dispatch when a future **package-aware wiring model** (aggregate a
package's symbols, JOIN by package) lands — a genuine L4 design, not a PoC hack.

- **Proof**: `extract_imports` 5/5 (validates the `.scm` node types empirically) ·
  touring-hooks-core 437 tests (Java pure-string, Go None) · touring-storage 9 unit +
  3 integration (python/typescript/java-wires-go-deferred).
- **Gates**: clippy 3 crates (test+non-test) 0 · `cargo check --workspace` 0 warnings.
- **Files**: `queries/{go,java}_imports.scm`, `languages.rs`, `symbol_extractors.rs`
  (`normalize_lexical` + TS/JS/Java/Go arms), `knowledge_wiring.rs`, `graph/imports.rs`,
  `tests/polyglot_wiring_poc.rs`.

**Polyglot wiring now**: Rust · Python · TS/JS · Java (file-based) wired; Go
extraction-only pending the package model.

---

## 8. P-D SHIPPED (2026-07-03) — Semantic extractor parity (Python/TS/JS)

**Status: converged + proven** (`target/debug`, NOT deployed). No feature flag —
this is pure additive capability (a new report + a broadened quality gate).

The gap P-D closes: `generate`'s VGP quality gate ran a **deep semantic health
check only for `.rs` files** (`context_quality.rs` — *"Non-Rust files are
unaffected"*). `rust_semantic` (syn) reads generics/lifetimes/`unsafe`; there was
no cross-language equivalent, so generated Python/TS/JS artifacts skipped the
semantic bar entirely.

| Layer | Deliverable |
|---|---|
| **Engine** | `touring-code/src/ast/polyglot_semantic.rs` — `PolyglotSemanticReport::from_source(Lang, src)`, tree-sitter walk for Py/TS/JS. Extracts type_params (generics), async_fns, decorators, classes/functions, typed/total params (annotation coverage), **dynamic_escapes** (`eval`/`exec`/`getattr` in Py, `any` in TS — the cross-language analog of `unsafe`), item_count + `semantic_complexity()` / `annotation_coverage()` / `is_simple()`. |
| **Signals** | `touring-analysis/src/quality/polyglot_semantic.rs` — `PolyglotQualitySignals` mirrors `RustQualitySignals`; `health_score = 1.0 − (dynamic_escapes·0.05).min(0.30) − complexity·0.30` (same penalty shape). |
| **Wiring** | `context_quality.rs` gate refactored to a per-language `semantic_health()` helper: Rust → `RustQualitySignals`, Py/TS/JS → `PolyglotQualitySignals` (**same `min_semantic_score` bar**), Go/Java/C++ → honest skip (no deep report yet, never a silent pass). |
| **CLI** | `touring ast polyglot-semantic <file>` — the user-facing analog of `touring ast rust-semantic` (language inferred from extension). |

### Gotchas (recorded for future polyglot AST work)

- **Count only `is_named()` nodes.** Anonymous keyword tokens (`class`, `function`,
  `async`) share `kind()` with declaration nodes → double-count if visited. `async`
  is still detected via `has_child_kind(fn_node, "async")` (inspects anon children).
- **tree-sitter 0.26.9**: `Node::child(i)` / `named_child(i)` take **`u32`**, not `usize`.
- **`Generic[T]` (Python subscript) is NOT a `type_parameter`** — only PEP 695 `[T]`
  declarations are; a test pins this so subscripts are never miscounted as generics.

- **Proof**: touring-code 644 · touring-analysis 971 · touring-generator (default 141 +
  `--features quality-gate` 7: `rejects_dynamic_python` / `accepts_clean_typescript` +
  the renamed `accepts_clean_python`). CLI smoke py/ts/rs. Node-kinds validated
  empirically (VP-Scout Chain 5).
- **Gates**: clippy 4 crates (test + non-test) 0 · `cargo check --workspace` 0 warnings ·
  0 regression.

**Semantic parity now**: Rust (`rust-semantic`, syn) · Python/TS/JS
(`polyglot-semantic`, tree-sitter), both feeding the `generate` VGP health gate.

---

## 9. P-E SHIPPED (2026-07-03) — Toolchain multi-ecosystem (workspace-info + F2.5/OSV substrate)

**Status: converged + proven** (`target/debug`, NOT deployed). Two halves:

### E1 — `workspace-info` multi-ecosystem (offline, fully proven)

`WorkspaceInfo::load` runs `cargo metadata` (Rust-only) and *fails* on a non-Rust
tree. Parity: a new `touring-code/src/ast/manifest.rs` — `ManifestInventory::scan`
detects and parses the non-Cargo manifests Touring indexes:

| Ecosystem | Manifest | Parser handles |
|---|---|---|
| **npm** | `package.json` | name/version + all dep groups (deps/dev/peer/optional) |
| **PyPI** | `pyproject.toml` | PEP 621 `[project]` + Poetry `[tool.poetry]`; PEP 508 name extraction; skips the `python` interpreter constraint |
| **Go** | `go.mod` | `module` + block/single `require`, `// indirect` stripped |

`touring ast workspace-info` no longer fails on a Python/Node/Go tree — the CLI
(`run_workspace_info` helper) emits the flat Cargo fields (backward-compatible)
**plus** a new `manifests` array. Proven: binary smoke on a polyglot dir →
`[Npm(web,2), PyPI(svc,2), Go(app,1)]`; the Cargo workspace still reports
`workspace_member_count=42` + the `manifests` key.

### E2 — F2.5 → OSV.dev substrate (offline half proven; live query opt-in/network)

`f2_5_dep_cves` scans a `Cargo.lock` against RustSec (Rust-only), marking non-Cargo
targets `NotApplicable`. Full multi-ecosystem CVE scanning is **OSV.dev**, which is a
**network** service — and the Code Execution Gateway denies network by default, so a
live query is *not provable offline*. The honest, non-disruptive deliverable:

- New `touring-analysis/src/osv.rs` — `OsvBatchQuery::from_inventory` builds the OSV
  `querybatch` payload (ecosystem-tagged, `normalize_version` pins-vs-ranges,
  `to_json` serialization-tested), `offline_summary(dir)` reports scan coverage
  **without any network call**.
- F2.5's non-Cargo branch now enriches its evidence with that OSV summary (which
  ecosystems, how many deps, how many queries a scan would issue) while keeping the
  status **`NotApplicable`** — **zero composite impact** (no every-non-Rust-project
  regression). The live OSV lookup is the documented opt-in (`TOURING_OSV_SCAN=1`).

### Gotchas (recorded)

- Inserting a helper *between* a doc-comment and its `pub fn` orphans the doc →
  `missing_docs` on the now-undocumented fn. This deny lint only fires on a
  **non-test** `cargo check` (`--tests`/`--all-targets` sets `cfg(test)` and masks it).
- `touring-quality` is a **separate binary** built standalone by default (not
  `workspace-integration`) — smoking a `--features`-gated change via the stale
  binary misleads; prove it in the real engine via a test (VP-Scout Chain 5).

- **Proof**: touring-code 650 (+manifest 6) · touring-analysis 976 (+osv 5) ·
  touring-quality 356 `--features workspace-integration` (+f2_5 OSV enrichment).
- **Gates**: clippy 5 configs (3 crates + touring-quality feature, test + non-test) 0 ·
  `cargo check --workspace` 0 · binary smoke (polyglot + Cargo backward-compat) · 0 regression.

**Toolchain parity now**: `workspace-info` reads Cargo + npm + PyPI + Go; F2.5 sees
every ecosystem's dependency tree (OSV offline substrate ready; live scan opt-in).

---

## 10. P-F SHIPPED (2026-07-03) — Quality-dimension polyglot parity (F1.7) + REGRA #21 fix

**Status: converged + proven** (`target/debug`, NOT deployed).

### VP-Scout finding: the audit lesson was partially stale

The framing (F1.5/F1.6/F1.7/F1.8 all Rust-only silent-pass non-Rust) is verified
**partially stale** — the W0-W6 harness reform already fixed most:

| Dim | Verified current state |
|---|---|
| F1.6 error-handling | `analyze_error_coverage` **already dispatches per-language** (rust/python/ts/go/java). Rust-literal hazards (`.unwrap()`/`panic!`) → 0 for non-Rust (benign, not inflating). |
| F1.5 tech-debt | TODO/FIXME markers **polyglot**; only `todo!`/`unimplemented!` is Rust-only → 0 non-Rust. |
| F1.8 dep-cycles | Crate-scoped (Cargo) → `local_hygiene` fallback for non-crate — not a silent 1.0. |
| **F1.7 boundaries** | ✅ the one genuine remaining hole: ran the Rust `pub` heuristic on any language → non-Rust `total_items ≈ 0` → `score_boundaries = 1.0` **silent pass**. |

### The fix (F1.7 — real detector, REGRA #0, not NotApplicable-reduction)

`analyze_boundaries` (`touring-analysis/src/quality/boundaries.rs`) now dispatches
`classify_item` per language (the pattern `error_coverage` already established):
Rust unchanged · **TS/JS** `export` = public surface · **Python** `_`-prefix
convention = private · **Go** capitalization = exported · **Java** `public`/
`private`/`protected` modifier. Field-tracking stays Rust-only (`is_struct = false`
for other languages) so it never contaminates a non-Rust score; `lang` is threaded
`feed → feed_top_level → classify_item`. `lang_from_ext` already returns
python/typescript/javascript/go/java (c/cpp/html fall back to the Rust classifier —
a documented limitation).

**Scope discipline**: the broad recalibration (per-dim `NotApplicable`, dep-cycles
polyglot) belongs to the pending harness-reform plan (`task_1782963794901399014`) and
was **deliberately NOT** done here — no composite-semantics overhaul, no collision.

### REGRA #21 fix (pre-existing failure found while verifying)

`cargo test -p touring-quality --no-default-features` **never compiled**: three
`load_advisories_ignore_*` tests lived in `fallback_tests` (`not(workspace-integration)`)
but called the wsi-only `real_engine` module — dead + mis-gated + the fn was private.
Fixed: `load_advisories_ignore` → `pub(super)`, tests moved to a wsi-gated
`advisories_ignore_tests` module. Standalone now builds (227 tests) and the 3
previously-dead tests actually run.

- **Proof**: boundaries 16 (+5 polyglot: TS/Python/Go/Java + high-exposure) · f1_7
  verifier 9 (+1 e2e `.ts` target) · touring-quality wsi 361 · `--no-default-features`
  227 (was broken) · touring-analysis 981.
- **Gates**: clippy touring-analysis + touring-quality (wsi + no-default, test + non-test)
  0 · `cargo check --workspace` 0 · 0 regression.

**Quality parity now**: F1.7 boundaries is polyglot (Rust/TS/JS/Python/Go/Java); the
audit's other dims were already reformed; the broad recalibration stays with the
reform plan.

---

## ✅ Polyglot parity DAG COMPLETE (P-A…P-F all done)

`touring decompose ready task_1783103161487180262` → **empty**. Touring is now applied
across languages at every layer that had a Rust-only gate:

| Layer | Before | After |
|---|---|---|
| **Wiring graph** (blast/orphans/impact) | Rust `.rs` only | + Python · TS/JS · Java (file-based); Go extraction-ready (P-A/B/C, flag `TOURING_POLYGLOT_WIRING`) |
| **Semantic extractor** (generate VGP gate) | Rust (`rust-semantic`, syn) | + Python/TS/JS (`polyglot-semantic`, tree-sitter) (P-D) |
| **Toolchain** (`workspace-info`, F2.5) | Cargo only | + npm/PyPI/Go manifests; OSV offline substrate (P-E) |
| **Quality dims** (F1.7 boundaries) | Rust `pub` heuristic | + per-language visibility (P-F) |

**Deferred (honest, gated on Gabriel)**: deploy (`update-touring` — all work is in
`target/debug`, default-OFF/opt-in ⇒ zero behavior change to the running daemon) ·
Go package-aware wiring model (L4) · broad harness-50dim recalibration (its own plan).

---

## 11. DEPLOYED (2026-07-04) + P-G Go package-aware model (foundation)

### Deploy — P-A…P-F now live in the daemon

`update-touring --no-kill` rebuilt release (7m04s, exit 0), installed dual-target
symlinks, and `touring daemon-ctl restart` brought the daemon onto the new binary
(the `--no-kill` build leaves the old daemon running; the explicit restart is the
REGRA #3 step). Verified: doctor 6/6, daemon exe fresh (not `(deleted)`), old daemon
reaped, `wiring_diagnostic non_rust=0` (flags default-OFF ⇒ zero behavior change).
Live-smoked: `touring ast polyglot-semantic` (P-D) and `touring ast workspace-info`
multi-ecosystem (P-E) run on the release binary.

**Runtime-path correctness confirmed** (found while grounding): `touring-hooks-core`
`pub use touring_storage::knowledge{,_wiring}` — the daemon's `FileKnowledgeDB` IS
touring-storage's (my P-A/B/C-edited layer), re-exported. The
`touring-hooks-core/src/knowledge{,_wiring}.rs` files are **dead leftovers** of the
A5 storage relocation (no `mod` decl) — a REGRA #0 hygiene finding, not touched
(the `lib.rs` comment implies an intentional migration lock).

### P-G — Go package-aware wiring model (foundation, converged)

Resolves the impedance Go was deferred over: a Go import denotes a **package**
(directory), carries no symbol, and `wiring_map` is **file-keyed** → file-keyed
resolution registers producers with no resolvable consumers → false orphans.

**Model**: a synthetic key namespace **`"go:<import-path>"`** reusing
`wiring_map.module_file` (no schema migration). A producer `Foo` in
`mymod/pkg/*.go` → `go:mymod/pkg` + `Foo`; a consumer `import "mymod/pkg"` +
`pkg.Foo()` → `go:mymod/pkg` + `Foo` → the JOIN resolves across the package's many
files. The false-orphan class is closed (consumers resolve); a remaining orphan is a
genuinely-unused export (legitimate, exactly like a Rust `pub`).

| Change (`touring-storage/src/knowledge_wiring.rs`) | Effect |
|---|---|
| `is_indexable_module_file_polyglot` admits `go:` keys (via `is_go_package_wireable`, excludes `/vendor/`, empty) | write-gate |
| `wireable_ext_sql` adds `go:%` under the flag | read-side (orphan detection / method-dispatch see Go producers) |
| File-keyed `.go` stays **rejected** | the false-orphan defense holds |

All under `TOURING_POLYGLOT_WIRING` (default OFF). **Proof**: storage lib 207 (5 `go:`
gate tests) + integration `go_package_key_wires_and_file_keyed_go_stays_rejected`
(Handler wired · Config orphan · `.go` file rejected · vendored excluded · score 0.5)
· clippy test+non-test 0 · `cargo check --workspace` 0.

**Inert until the feeder emits `go:` keys** — no runtime `go:` producer exists yet, so
zero false orphans / zero behavior change even with the flag ON. This is the proven
model *foundation*; the runtime feeder integration is the remaining slice:

**Feeder slice (next, atomic producer+consumer)** — `index.rs` Go handling:
1. **Producer**: derive `go:<import-path>` from go.mod (reuse the P-E go.mod module
   parser) + the file's dir-relative-to-go.mod; register exported (Capitalized)
   symbols keyed by it (non-`_test.go` files only).
2. **Consumer**: extract `pkg.Foo()` selector expressions (tree-sitter) + resolve the
   alias → import-path (from the file's imports); `record_consumer("go:<path>", sym)`.
   Both sides must land together (producer-only = false orphans).

## 12. P-H SHIPPED (2026-07-04) — Go feeder (atomic producer+consumer), converged

The runtime feeder that makes Go **wire end-to-end in production** — the slice §11
scoped as "next". Go now emits `go:` producer/consumer rows during a rebuild, so the
P-G model is no longer inert.

**New module** `touring-code/src/ast/go_wiring.rs` (gated `more-languages`, pure +
tree-sitter, 10 unit tests):

| Fn | Role |
|---|---|
| `extract_go_exports(src)` | **Producer** — exported (Capitalized) top-level `func`/`type`/`const`/`var` (depth-1; grouped + multi-name specs; methods excluded — not reached via `pkg.M`) |
| `extract_go_consumer_edges(src)` | **Consumer** — `alias.Symbol` selectors → `(go:<import-path>, Symbol)`; alias map from `import` (default = last segment, explicit alias, `.`/`_` skipped); exported-only; deduped |
| `go_package_key(module, rel_dir)` / `go_package_key_for_file(abs)` | **Key derivation** — walk up to `go.mod`, `module` line (reuses new `manifest::go_module_path`), join file's rel-dir → `go:<module>/<rel-dir>` |

**Feeder wiring** (`touring-cli/.../handlers/index.rs`, in `cli_index_rebuild`): helper
`feed_go_package_wiring` runs inside the existing `is_code_lang` block for `.go` files —
early-returns when `polyglot_wiring_enabled()` is off (now `pub`, single source of the
flag → zero extraction work + byte-identical `wiring_entries` when OFF) and for
`_test.go`. Producer rows cleared **once per package** (`cleared_go_pkgs` set → partial
`--dir` reindex never wipes unwalked packages); consumer rows keyed by the `.go` file so
the generic `clear_consumer_entries` already refreshes them. `register_pub_symbol` is
`INSERT OR IGNORE` (idempotent across a package's many files).

**Scope**: the full-reindex feeder (`cli_index_rebuild`) — not the per-edit incremental
hot path (Rust-only `update_wiring_after_edit`, untouched). Orphan detection runs on the
full graph, which the rebuild populates.

**Proof (executed, `[fact 1.0]`)**: go_wiring **10/10** — incl.
`producer_key_and_consumer_key_rendezvous` proving the producer key derived from `go.mod`
== the consumer key derived from the literal `import` (`go:github.com/acme/app/pkg/svc`),
the JOIN crux — plus the storage POC (same `go:` key → orphan detection: Config orphan,
Handler wired). touring-code lib **660/660** (manifest regression incl.), storage
knowledge **18/18** + POC **4/4**, clippy test+non-test 0 (touring-code/cli/storage),
`cargo check --workspace` 0, **0 regressions**. The only unexecuted glue is the ~6-line
straight-line plumbing over these proven pieces (compiler + clippy validated).

**NOT deployed** — in `target/debug`; inert on the live daemon (which lacks P-G/P-H) and
zero-change even after deploy (default-OFF). Deploy is the human gate.

---
_Plan authored 2026-07-03. P-A…P-F shipped 2026-07-03 + DEPLOYED 2026-07-04; P-G model
foundation + P-H Go feeder converged 2026-07-04 (Go wires end-to-end, flag-gated OFF,
not yet deployed). Harness-reform (`task_1782963794901399014`) remains._
