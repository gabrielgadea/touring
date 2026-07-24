# Phase 3B: Documentation & API Docs Review

> Touring workspace · 2026-06-13 · agent: technical documentation architect (read-only, code-verified)
> North star: what blocks Touring's docs from **Premium, Elite-of-Market** (public repo + external contributors + SDK).
> Method: every doc claim verified against actual code. **Accuracy is the #1 concern.**

## Verdict (TL;DR)

The docs *look* elite (Diátaxis layout, badges, doc-as-code gates, generated reference catalogs) but **9 of the
8 user-facing docs contain at least one claim the code disproves**. The headline metrics in the two most-read
files (README, ARCHITECTURE.md body) describe a workspace that **no longer exists** (36/38 crates, ~428–476k LOC,
a 127k-LOC `touring-hooks` that was decomposed). The single most damaging item is a **3-way version
contradiction**: README says 30.0.0, ARCHITECTURE.md says v30.3.6, and the binary actually prints **0.1.0**
(`Cargo.toml:142` → `env!("CARGO_PKG_VERSION")`). The doc-as-code gates that should prevent this (`sync_metrics.py`,
`gen_reference.py`) are real and good but **under-scoped** — they gate the *header* line of ARCHITECTURE.md and a
*string-literal* tool count, not the body topology, the README, or the version.

**Severity counts:** 2 Critical · 6 High · 7 Medium · 4 Low.
**Doc-accuracy verdict:** of 8 reviewed user-facing docs, **6 contradict the code** (README, ARCHITECTURE.md,
SECURITY.md, plus the generated mcp-tools.md, the version metadata, and README's internal links). CONTRIBUTING.md
and SUPPORT.md are honest. CHANGELOG.md is accurate-but-low-signal.

---

## Doc ↔ Code Accuracy Table (claim → reality → evidence)

| # | Doc claim | Reality (code) | Evidence (`file:line`) |
|---|---|---|---|
| D1 | README: "**36 crates** totaling **~428k LOC**" (`README.md:12`) | **46 crates**, ~499k src / ~567k workspace LOC | `README.md:12` vs `cargo metadata` (46) + `docs/sync_metrics.py` (loc_src 499,421) |
| D2 | README badge "version-30.0.0" (`README.md:5`); ARCHITECTURE "v30.3.6" (`:3`) | Binary version = **0.1.0** (`touring --version` prints `env!("CARGO_PKG_VERSION")`) | `README.md:5`, `ARCHITECTURE.md:3` vs `Cargo.toml:142` + `crates/touring-cli/src/cli/health.rs:236` |
| D3 | ARCHITECTURE.md body: crate tree lists `touring-core`, `touring-index`, `touring-vfs`, `touring-semantics` | **None exist** — `ls crates/` has no such dirs (they were fused into foundation/code/storage) | `ARCHITECTURE.md:161,170,184,186,797,809,821,827` vs `crates/` listing |
| D4 | ARCHITECTURE.md body: "`touring-hooks … LOC: 127,575`" | touring-hooks is now a **1.1k façade** (decomposed 2026-06-11); no crate >64.5k | `ARCHITECTURE.md:167,806` vs header `:3` (says hooks=1.1k) — **self-contradicting within one file** |
| D5 | ARCHITECTURE.md body total: "**38 crates** / **476,728 LOC (src)**" (`:835`) | 46 crates / 499,421 src | `ARCHITECTURE.md:835` vs `sync_metrics.py --json` |
| D6 | SECURITY.md: "credential env vars (`AWS_*`, `GITHUB_TOKEN`, …) are never in `ENV_ALLOWLIST`" (`:31-32`) | A **separate** `CREDENTIAL_ENV_WHITELIST` explicitly passes `GITHUB_TOKEN, AWS_*, ANTHROPIC_API_KEY, OPENAI_API_KEY, NPM_TOKEN, KUBECONFIG…` into the sandbox child | `SECURITY.md:31-32` vs `crates/touring-ceg/src/gateway/sandbox_executor.rs:542-569` (= SEC-04) |
| D7 | README: "**88 MCP tools**" (`:14,42,53`) | **164** distinct tool names / **184** `#[tool]` macros in touring-server (217 raw) | `README.md:14` vs `grep '#\[tool' crates/touring-server/src` = 213; distinct names 164 |
| D8 | Generated `docs/reference/mcp-tools.md`: "**Count: 164**" | Real macro count **184** (touring-server) +3 reasoning +1 session = ~188 tools; the generator regex misses macro-defined tools whose names aren't string literals | `docs/reference/mcp-tools.md` vs `gen_reference.py:71-81` (string-literal extraction) |
| D9 | README: "**198 lifecycle hooks**" (`:14,42`); footer "hooks: 218" (`:128`) | `ALL_DAEMON_HOOK_NAMES` → **218** (per generated `hooks.md`) | `README.md:14` (198) vs `:128` (218) vs `docs/reference/hooks.md` "Count: 218" — README internally inconsistent |
| D10 | README link `[CONTRIBUTING.md](docs/CONTRIBUTING.md)` (`:115`) | **MISSING** — real file is root `CONTRIBUTING.md` | `README.md:115` vs `ls docs/CONTRIBUTING.md` = absent |
| D11 | README link `plans/touring-47-to-13-residual/plan.md` (`:89`) | **MISSING** path | `README.md:89` vs filesystem |
| D12 | README badge "license: tiered"; "LICENSE-{MIT,APACHE}" present | No **root `LICENSE`** file; `Cargo.toml:143` declares `MIT OR Apache-2.0` (standard, NOT "tiered") | `README.md:6,8` vs `ls LICENSE*` (only LICENSE-MIT/APACHE) + `Cargo.toml:143` |
| D13 | SECURITY/CEG: landlock "target state once the `landlock` crate is wired (P2.4-B deferral)" | Landlock **IS** wired — `handle_access(AccessNet::BindTcp | ConnectTcp)` is live | `enforce_linux.rs:45,206` (deferred framing) vs `:382` (live impl) = SEC-10 |
| D14 | README: "36 kinds via touring-generator" (`:43`) | Generated `generators.md` says **Count: 36** ✅ accurate | `docs/reference/generators.md` — one of the few README numbers that holds |
| D15 | README: `touring doctor → 6/6 OK` (`:29`) | ARCHITECTURE header confirms "doctor 6/6 green" — ✅ consistent | `README.md:29` ↔ `ARCHITECTURE.md:3` |

---

## Findings (Severity · what's wrong · concrete fix)

### 🔴 [CRITICAL] DOC-01 — Three contradicting version numbers; the binary is 0.1.0
README badge `version-30.0.0` (`README.md:5`), ARCHITECTURE.md `v30.3.6` (`:3`), CHANGELOG entries up to "v3-0-0",
yet `touring --version` emits **`touring 0.1.0`** because `health.rs:236` formats `env!("CARGO_PKG_VERSION")` and
`Cargo.toml:142` pins `version = "0.1.0"` workspace-wide. For a would-be public/SDK repo this is the single most
credibility-destroying inconsistency: a visitor runs `touring --version`, sees `0.1.0`, and stops trusting every
other number on the page.
**Fix:** pick ONE source of truth. Either bump `[workspace.package] version` to the real release (e.g. `30.3.6`)
so `--version`, README badge, and ARCHITECTURE agree, **or** drop the "30.x" framing everywhere and own `0.1.0`.
Then add a `sync_metrics.py`-style gate that asserts `README badge == Cargo.toml version == ARCHITECTURE header`.

### 🔴 [CRITICAL] DOC-02 — ARCHITECTURE.md body still describes the pre-decomposition architecture (A3, fully scoped)
The **header** (line 3) was synced by session G-1 to "46 crates / touring-hooks 1.1k". But everything below it is
the OLD map: the crate tree (`:155-200`), the dependency diagram (`:336,342`), and the final inventory table
(`:785-835`) list **4 phantom crates** (`touring-core :161/797`, `touring-index :170/809`, `touring-semantics
:184/821`, `touring-vfs :186/827`), claim **`touring-hooks LOC: 127,575`** (`:167,806`) — which the header on the
same file contradicts — and total **"38 crates / 476,728 LOC"** (`:835`). Each phantom row also links to a
non-existent `crates/touring-core/ARCHITECTURE.md` etc. `sync_metrics.py` cannot catch this: `declared_crates()`
returns the **first** `\d+ crates` regex match (the header "46"), so the body's "38" never trips the gate.
**Fix:** (a) regenerate the crate tree + inventory table from `cargo metadata` (the data is already in
`gen_reference.py:crate_modules`); (b) delete the 4 phantom rows and the 127,575 line; (c) extend `sync_metrics.py
--check` to scan **all** `\d+ crates` occurrences and assert the per-crate inventory table matches the live crate
list (not just the header). The body is ~680 lines of mostly-historical content — fold the wave-history into
`docs/` session reports and make ARCHITECTURE.md a thin, generated map.

### [HIGH] DOC-03 — SECURITY.md actively misleads on credential handling (SEC-04)
`SECURITY.md:31-32` states credential vars are "never in `ENV_ALLOWLIST`". That is **technically true of the
named constant** (`builtins.rs:17` ENV_ALLOWLIST = PATH/HOME/USER/LANG/LC_ALL/TERM/TZ) but **materially false**:
the sandbox subprocess gets its env from a *different* whitelist, `CREDENTIAL_ENV_WHITELIST`
(`sandbox_executor.rs:542`), which deliberately forwards GITHUB_TOKEN, all AWS_*, GCP, KUBECONFIG, NPM_TOKEN,
OPENAI_API_KEY, ANTHROPIC_API_KEY. A reader concludes the sandbox is credential-free; it is not. A SECURITY.md
that the code disproves is worse than none — it's the document an external auditor reads first.
**Fix:** rewrite the Hardening Note to describe the *actual* model: "the CEG `Sandboxed` capability profile grants
no credentials; however the legacy sandbox executor forwards a fixed `CREDENTIAL_ENV_WHITELIST` of cloud/LLM
tokens so first-party CLI tools (`gh`, `aws`, `kubectl`) can authenticate — see `sandbox_executor.rs:542`."
Better: make it opt-in/profile-scoped and document that. Reconcile with SEC-04's required regression test.

### [HIGH] DOC-04 — README headline numbers wrong (crates, LOC, MCP tools, hooks)
`README.md:12` "36 crates / ~428k LOC" (reality 46 / ~499k), `:14,42,53` "88 MCP tools" (reality ~164–184),
`:14,42` "198 hooks" while the same file's footer (`:128`) says "218". The README is the front door of an elite
repo; every number a first-time visitor can check is wrong or self-inconsistent.
**Fix:** template the README's count-bearing lines and have `sync_metrics.py`/`gen_reference.py` inject them
(the data already exists: crates=46, loc_src=499421, mcp=184, hooks=218, generators=36). Add a `--check` README
gate alongside the ARCHITECTURE one.

### [HIGH] DOC-05 — The MCP tool catalog (Touring's #1 public API) is undercounted and uncurated in docs
`gen_reference.py:71-81` extracts MCP tool names by regex over `"touring_…"`/`"ctx_…"` **string literals**, yielding
164. The real surface is **184 `#[tool]` macros** in touring-server (217 raw incl. multi-line). Tools whose names
come from the macro attribute rather than a bare string literal are silently dropped from the published catalog.
Worse, the catalog intro (`gen_reference.py:160-162`) and `Cargo.toml:80-94` describe a "curated 22-tool surface
behind `--features mcp-curated`" — but that flag gates only **5 cfg blocks** and (per Phase 1 A6 / Phase 2)
*adds* 3 tools rather than restricting to 22. The documented curation contract does not exist.
**Fix:** extract tool names from the `#[tool]` macro (parse the attribute), not string literals, so the count is
exact; emit per-tool metadata (description, capability scope, since-version) for a real SDK catalog; and either
implement the 22-tool curation the docs promise or delete the claim from `Cargo.toml`, `gen_reference.py:160`,
README, and SUPPORT.

### [HIGH] DOC-06 — Rustdoc/API docs unenforced on the biggest public-API crates → no usable SDK reference
`#![deny(missing_docs)]` now covers **8 crates** (contracts, generator, foundation, dispatch, hooks, identity,
license, lsp — all genuinely clean, 0 `allow(missing_docs)` overrides — real progress over the prior 2). But the
**largest public surfaces are unenforced**: touring-server (67.9k, 184 MCP tools), touring-intelligence (64.3k,
**1,756 pub items**), touring-cli (the daemon-side query handler API consumed by touring-server's 22 imports), and
touring-ceg (the security crown jewel). An SDK consumer running `cargo doc` gets a reference where the most
important crates have undocumented public items. The crate-level `//!` docs exist (good) but item-level coverage
is unverified.
**Fix:** ratchet `missing_docs` from `warn`→`deny` outward, crate by crate, starting with touring-ceg (security
API must be documented) and touring-contracts-adjacent boundary crates; for the 1,756-item intelligence crate,
gate `warn(missing_docs)` first and burn down. Add `cargo doc --no-deps -D warnings` (rustdoc lints incl. broken
intra-doc links — touring-cli already sets `warn(rustdoc::broken_intra_doc_links)` at `lib.rs:15`) to CI as the
"API reference builds clean" gate. This is the **#1 SDK-readiness lever**.

### [HIGH] DOC-07 — README links are broken (contributor's first clicks 404)
`README.md:115` points to `docs/CONTRIBUTING.md` (the file is at root `CONTRIBUTING.md`); `:89` points to
`plans/touring-47-to-13-residual/plan.md` (missing); `:80` references "`plan.md` Section I"; `:88` and `:90`
reference plan/cookbook paths. A contributor following the README hits dead links immediately.
**Fix:** fix the CONTRIBUTING link to `CONTRIBUTING.md`; remove/repoint the residual-plan link; add a markdown
link-check (e.g. `lychee`) to CI so README/ARCHITECTURE/CONTRIBUTING links are gated.

### [HIGH] DOC-08 — CONTRIBUTING.md omits the one thing an external contributor must know: there is no git
This repo is managed **without git** (REGRA #11 — git is prohibited, Touring is source of truth). CONTRIBUTING.md
(`:9-15`) lists `cargo check/clippy/test`, `sync_metrics.py --check`, `touring doctor` — all correct — and
mentions "PRs" and "RFCs under `docs/rfcs/`" (`:42-44`), but **never explains how an external contributor
actually proposes a change** when there's no public git remote, no PR workflow, and the canonical tooling is
`taco-forge perfect-*` (which an outsider doesn't have). For a *public* repo this is a contradiction: you cannot
both prohibit git internally and invite GitHub PRs. The "no-git" reality is invisible in the contributor doc.
**Fix:** decide and document the external-contribution model explicitly. If going public: mirror to a git remote
and let external contributors use normal git/PR (the no-git rule is an *internal* TACO constraint, not a
contributor-facing one) — say so. Document the build-from-source path (SUPPORT.md notes "installation requires
compiling the workspace"), the test exclusions (`--exclude touring-python`, never `--test graph_service_e2e`),
and which gates are advisory vs blocking. Add `docs/rfcs/` (referenced but the dir's existence/contents are
unverified) and the RFC-006 extension contract.

### [MEDIUM] DOC-09 — README claims unverifiable / aspirational features as present
`README.md:45` "Z3 SMT solver for proofs" (Phase 2 / scope describe **cvc5** in touring-offensive, not Z3 —
likely inaccurate); `:46` "8 arms + 25 dims"; `:128` "e2e: 0.83 → target 0.90"; the entire License-tier pricing
table (`:103-110`, $99/$499/Enterprise with SLAs) presents a commercial product that — per SUPPORT.md's honest
"single-user, Claude-oriented infrastructure" (`:17`) — does not operate. README and SUPPORT.md disagree on
maturity.
**Fix:** verify Z3-vs-cvc5 against `touring-offensive`; mark the pricing/SLA table as "planned" or move it to a
separate `docs/commercial.md`; align README's product framing with SUPPORT.md's honest maturity statement.

### [MEDIUM] DOC-10 — ARCHITECTURE.md is a 835-line history dump, not a reference
Beyond the phantom map (DOC-02), the file mixes a WASM-component section (`:10-95`), a `touring serve` runtime
model (`:97-130`), "Vector B — Zero-Copy rkyv IPC" (`:549`), wave changelogs, and cross-audit fix tables — content
that belongs in `docs/` session reports or the narrative `docs/explanation/architecture.md`. The header itself
admits it's "intentionally not a thin pointer" because "the metrics-as-code gate lives here" — but that's an
argument for a *small* generated metrics file, not a 52KB grab-bag.
**Fix:** split into (a) a generated, gated crate-map/metrics file (~80 lines), (b) `docs/explanation/architecture.md`
for the narrative, (c) `docs/` dated reports for wave history. The relocate_session_docs.py script (G-8) already
exists for moving session docs.

### [MEDIUM] DOC-11 — CHANGELOG synth is accurate but near-zero-signal
`changelog_synth.py` deterministically renders 102 TOON checkpoints (good — no drift), but the output is
`- **topic** (unknown): (checkpoint)` and `(+2 more)` (`CHANGELOG.md:11-25`). It follows Keep-a-Changelog markers
(`### Added/Changed/Fixed` present, 28 matches) in the hand-curated tail, but the auto-synth top section reads as
machine noise: many entries have role "unknown" (`changelog_synth.py:145-146` defaults), bodies "(checkpoint)"
(`:152`), and no Added/Changed/Fixed categorization. There are **no migration guides** for breaking changes
(schema v8, the 30.x→ rearch, the crate fusions that broke `touring-{ast,learning,cognitive}` import paths).
**Fix:** (a) categorize synth entries into Added/Changed/Fixed/Removed by parsing the checkpoint role/intent;
(b) suppress empty "(checkpoint)" bodies; (c) add a hand-written **Migration Guide** section for schema v8 and
the shim-crate renames (old `touring-learning::X` → new `touring-intelligence::X`) — external consumers pinned to
old crate names need this.

### [MEDIUM] DOC-12 — Generated reference exists but is shallow for SDK use (G-3 modules.md present)
`docs/reference/{generators,mcp-tools,hooks,modules,quality-gates}.md` exist and are gated by `gen_reference.py
--validate` (CI drift gate — good dogfooding). modules.md (Count: 313) was added by G-3. But the catalogs are
**name-only lists** — no signatures, no descriptions, no capability/stability/since metadata. An SDK consumer
needs "what does `touring_file_ops` take and return, what capability does it require" — not just the name.
**Fix:** enrich mcp-tools.md from the `#[tool]` macro's doc + params; enrich generators.md from `GeneratorKind`
docs; cross-link modules.md to rustdoc. Pair with DOC-06's `cargo doc` gate.

### [MEDIUM] DOC-13 — `sync_metrics.py --check` gates only the header crate count (drift escaped to body)
The gate is good (cargo-metadata authoritative crate count + LOC + test_fn drift within 5%) but `declared_crates()`
(`sync_metrics.py:94-103`) returns the **first** `\d+ crates` match. ARCHITECTURE.md's body "38 crates / 476,728
LOC" (`:835`) and the phantom inventory slid past it. The gate proves the *header* honest while the *body* lies —
exactly the false sense of security DOC-02 exploits.
**Fix:** make the gate parse and assert the per-crate inventory **table** (not just a header integer) against
`cargo metadata`; flag any crate name in the doc that isn't in the workspace (would have caught the 4 phantoms),
and any header/body count mismatch within the same file.

### [LOW] DOC-14 — No screenshots / asciinema / quickstart proof
README has no terminal recording, screenshot, or copy-pasteable "first 60 seconds" proof. `touring.dev/install.sh`
(`:25`) is referenced but unverifiable (no domain proof in-repo; SUPPORT.md `:21` admits "prebuilt binary is on the
roadmap" — so the curl-install almost certainly 404s). Elite repos lead with a demo.
**Fix:** add an asciinema of `index rebuild → ast overview → status`; reconcile the install.sh claim with SUPPORT's
"compile from source" reality (the README quickstart is currently aspirational).

### [LOW] DOC-15 — `cargo-deny` / advisory policy undocumented (ties SEC-03)
CONTRIBUTING lists 5 gates but **not `cargo deny check`** despite Phase 2 SEC-03 finding the advisory check RED
(6 vulns incl. postgres-protocol CVSS 8.7). There's a `deny.toml` (referenced by SEC-03) but no doc tells a
contributor it exists, what the ignore-list policy is, or that advisories are (or should be) a gate.
**Fix:** document the supply-chain policy (`deny.toml`, advisory cadence, ignore-list review) in CONTRIBUTING +
SECURITY; add `cargo deny check` to the CONTRIBUTING gate list once SEC-03's RED is fixed.

### [LOW] DOC-16 — Stale/duplicate top-level planning docs pollute the repo root
Root carries `ARCHITECTURE.v29.5.0.md` (142KB), `ARCHITECTURE_PLAN.md`, `PLAN-file-metadata-expansion-v{1,2}.md`,
`PLANO-CORRECAO.md`, `TOURING_CRATES_*.md`, `STATUS.md` (Apr 25), `TOURING_PERF_*.md` — historical artifacts that
make the root look like a scratchpad, not an elite repo.
**Fix:** move to `docs/archive/` (relocate_session_docs.py exists); keep root to README/ARCHITECTURE/SECURITY/
CONTRIBUTING/SUPPORT/CHANGELOG/LICENSE only.

---

## What's already elite (don't regress)

- **Doc-as-code gates are real and dogfooded** — `sync_metrics.py --check`, `gen_reference.py --validate`,
  `wiring_integrity_gate.py`, `file_size_gate.py`, `changelog_synth.py` run in CI; the *concept* is elite, only the
  *scope* (DOC-02/04/13) is short.
- **`missing_docs` ratcheted 2→8 clean crates** (0 `allow` overrides) — the right mechanism, expanding correctly.
- **Diátaxis structure exists** — `docs/{tutorial,how-to,explanation,reference}/`, generated `reference/`.
- **CONTRIBUTING + SUPPORT are honest** — SUPPORT.md's "single-user, Claude-oriented, compile-from-source" maturity
  statement (`:17-23`) is the truthful framing the README should adopt.
- **CHANGELOG follows Keep-a-Changelog** in its hand-curated tail and is drift-free (synth-generated).

---

## The #1 documentation lever toward elite

**Make the doc-as-code gates cover the claims that actually break trust — version, README counts, and the
ARCHITECTURE body topology — then ratchet `cargo doc -D warnings` (missing_docs) onto the public-API crates.**

Concretely, one wave:
1. Unify the version (DOC-01): `Cargo.toml` version == README badge == ARCHITECTURE header, gated.
2. Regenerate ARCHITECTURE.md crate map from `cargo metadata`; delete 4 phantoms + the 127,575 line; extend
   `sync_metrics.py --check` to assert the inventory table + flag any non-existent crate name (DOC-02/13).
3. Template README counts from the same metrics source (DOC-04); fix the broken links (DOC-07).
4. Fix SECURITY.md to match `sandbox_executor.rs:542` (DOC-03) — accuracy is non-negotiable for a security doc.
5. Add `cargo doc --no-deps -D warnings` to CI and ratchet `missing_docs` onto touring-ceg → touring-server →
   touring-intelligence (DOC-06) so `cargo doc` produces a usable SDK reference.

This converts "the docs *claim* to be auto-synced and accurate" into "the docs *cannot* drift from the code" —
which is the difference between a polished-looking repo and an Elite-of-Market one external contributors and SDK
consumers can trust.
