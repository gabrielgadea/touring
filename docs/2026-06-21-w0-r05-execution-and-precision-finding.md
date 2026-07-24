# W0 + R0.5 Execution — and the Precision Finding that reframes the whole program (2026-06-21)

> Executed under Gabriel's directive "executar W0+R0.5". Mid-execution Gabriel issued a
> critical epistemological correction that **reframes the harness program**:
>
> > "só porque a conexão diminuiu o score, isso não significa que o que estava produzindo
> > o score menor era o que estava descalibrado, pode ser justamente o contrário, o que
> > produzia um score maior é que pode estar inflando … toda análise, diagnóstico e
> > mensuração que a infraestrutura de harness realiza deve ser extremamente exigente e
> > fundamentada nas melhores práticas … Touring deve ter uma infraestrutura de harness
> > absolutamente precisa, confiável e que eleve todos os padrões."
>
> He was right, and the dogfood proved it three ways. All gates green; numbers are real ($PIPESTATUS).

## Part 1 — W0 (verifier correctness): 3 bugs fixed, fixture-proven

| Verifier | Before | After | Best-practice basis |
|---|---|---|---|
| **f4_3 deprecated** | 0.300 Fail — counted `#[deprecated]` *declarations* (inverted); the only file matching in `touring-quality/src` was its **own detector** | **1.000** — measures *consumption* (`allow(deprecated)` suppressions) | D42: declaring is good hygiene; the defect is consuming + silencing |
| **f1_8 dep-cycles** | 0.000 Fail — `1 − (mod+use crate::)*0.1`, **penalised healthy imports** | **1.000** — file-hygiene smells only; discloses Tarjan SCC is the workspace gate | D08: acyclicity is a graph property; `touring wiring cycles` = 0 cycles (authoritative) |
| **f3_1 coverage** | density `tests/(loc/50)`, penalised long files | **0.886** honest presence-ratio (tests / public surface) | D27: true line coverage = cargo-llvm-cov (workspace gate) |

6 new tp/fp fixture tests (DoD-8). 176 lib tests pass, clippy `-D warnings` clean, fmt clean. Self-match = 0 (needles split via `concat!` so a verifier never scores its own detector).

## Part 2 — R0.5 (13-gate de-theatralization): 0.9703 Diamond → 0.9452 Platinum

`elite_aggregate.py`: the four worst theater gates were re-pointed at the real 50-dim engine via a
`dims:<F-ids>` sentinel (`run_dim_gate`). The honest result:

| Gate | Was | Now | Note |
|---|---|---|---|
| 05_testing | 1.0 (file-size proxy, literal `# proxy: file size = testability`) | **0.7789** real | f3_1-backed |
| 09_modularization | 1.0 (same file-size proxy) | **0.9056** real | F1.7+F1.8 (dropped f1_2 — see below) |
| 15_dependencies | 1.0 (N/A constant) | **1.0** real | F2.5 dep-CVEs "no known-bad versions" |
| 03_security | 1.0 (N/A constant) | **held** | F2.x are imprecise stubs — see Part 3 |

## Part 3 — THE FINDING: direction ≠ correctness (Gabriel vindicated 3×)

The dogfood of linking gates to un-validated dims produced **noise**, exactly as the plan warned
— but the *reason* is deeper than "aggregation artifact". A single file made it concrete:

**`crates/inferlets/src/crates_size_via_cli_wrapper.js`** scored **F2.1 OWASP = 0.000**.

1. **The low score pointed at a REAL bug.** The file had `execSync(\`find "${cratePath}" …\`)`
   where `cratePath` derives from external input (`inputObj.workspace`) → genuine **OWASP A03 /
   CWE-78 command injection**. I almost dismissed the 0.000 as "WorstOf-over-1839-files noise."
   That dismissal was the **inflation-bias Gabriel named.** → **Fixed** (REGRA #21): `execFileSync`
   with an argv array, no shell.
2. **But the f2_1 *signal* was a FALSE POSITIVE.** f2_1 matches the substring `exec(`; the file's
   line 27 is `memberRegex2.exec(membersStr)` — a benign **regex** call. After removing the real
   injection, F2.1 is **still 0.000** (it was never measuring the injection).
3. **And f2_1 MISSED the real injection** — `execSync(` does not contain the substring `exec(`, so
   the one true vulnerability was a **false negative**.

So the low score was "right file, wrong reason"; the high scores (Diamond gates) were inflation
(assumed-PASS). **Neither high nor low could be trusted without best-practice validation.**

### Root cause (systemic): **41 of 50 verifiers are self-declared `W3/W4 stub` substring matchers.**

Substring matching *cannot* be precise — it cannot separate benign `regex.exec()` from dangerous
`os.system()`. That is precisely why each dimension's D-rule names a real engine: **Semgrep/CodeQL
(D13 OWASP), gitleaks (D17 secrets), cargo-llvm-cov (D27 coverage), Tarjan/cargo (D08 cycles),
cargo-deny (D14/D44 deps).** The verifier headers literally say "Production (W6+): replace with
proper external tool integration."

## Corrected philosophy (the standard, going forward)

1. **Every measurement — high OR low — is a hypothesis to validate against the dimension's
   best practice, never trusted by direction.** A dropped composite does not anoint the lower
   number; an inflated one does not anoint the higher.
2. **A harness "low" is triaged FP-vs-real before action**: real → fix the code (REGRA #21, never
   hide); false-positive → fix the verifier's precision (never dismiss the dimension).
3. **The harness must be precise (low FP) AND demanding (low FN), grounded in best practice and
   tp/fp-validated** (gitleaks-style: every rule checked against true- and false-positive fixtures).
4. **Convergence is an *output* of rigor, not a target to engineer.** Making three composites meet
   at a number is meaningless if the number itself isn't a best-practice measurement. The real goal
   is that each of the 50 dims measures its dimension correctly; whatever they then converge to is
   the truth.

## The reframed program (replaces "just link the gates")

**R0 / W0′ — Verifier precision upgrade (the real work):** upgrade the 41 substring stubs to the
best-practice engine each D-rule names, every rule tp/fp-validated. Order by enforcement weight:
the **6 BLOCK P0 dims first** (F2.1 OWASP→Semgrep, F2.4 secrets→gitleaks, F2.5 CVEs→cargo-deny/OSV,
F2.6 config→Trivy, F4.3 deprecated→rustc lint, F4.5 pkg→cargo-deny). Only a precise dim may back a
gate (R0.5). Until then, a stub-backed gate is held, not linked (no theater, no noise).

**W1 — Scope-faithful aggregation:** WorstOf is correct for security (one injection = vulnerable)
but only once the verifier is precise; for proxy dims it needs per-file→percentile, not raw worst.
F4.5's 50-dep cap is a per-crate threshold mis-applied to a 263-dep workspace.

## Status (this slice)

- ✅ W0: f4_3, f1_8, f3_1 corrected + fixture-proven; touring-quality green (176 tests, clippy -D, fmt).
- ✅ R0.5: 13-gate de-theatralized for *validated* dims (testing, modularization, dep-CVEs);
  0.9703 Diamond → **0.9452 Platinum** (honest). `--check` exits 0.
- ✅ REGRA #21: real command-injection in the inferlets `.js` found + fixed.
- 🔎 FINDING: 41/50 verifiers are substring stubs → the precision program above is now the #1
  priority, ahead of further gate-linking.
- 🔓 Composite divergence did NOT reach ≤0.05 (0.3865 → 0.4135) — and that is the *honest* result:
  it cannot, while both sides are un-validated stubs. Per-axis (testing) divergence collapsed to ~0,
  proving the linking mechanism; the composite closes only after the precision program.

## Part 4 — the precision program's first probe: the lesson applies RECURSIVELY

Gabriel chose the **hybrid** architecture (in-house engines + external tools only where unique).
Exhaustive exploration found Touring **already has** the engines the 41 stubs should delegate to:
`touring-analysis::quality` (3732 LOC, 0 stubs — `complexity`/`security`/`error_coverage`/
`antipatterns`/`tdg`/`test_proxy`) composing `touring-offensive::vuln::PatternRegistry` (10 curated
CWE/OWASP detectors). `touring-quality` already declares `touring-analysis` as an optional
`workspace-integration` dep — but **no verifier ever used it** (the wiring was never done).

So I wired **F2.1 OWASP → `SecurityAnalyzer`** (feature-gated; substring fallback kept), built with
the feature (4m46s, pulls touring-analysis+offensive), and **empirically validated** — and the
probe failed in a way that proves Gabriel's lesson is recursive:

| F2.1 input (feature build, real engine) | score |
|---|---|
| `fn noop() {}` (nothing at all) | **0.220** |
| benign `regex.exec()` | 0.220 |
| `a + b` arithmetic | 0.220 |
| real SQLi `format!("…{}", uid)` + query | 0.220 |

**The "mature" in-house engine returns a constant 0.220 for every input** — equally untrustworthy as
the substring stub. **I had assumed it was precise because it exists, is mature-looking, and is
labelled `0 stubs` — exactly the assumption Gabriel warned against, now made about the in-house
engine.** Validate, never assume — not the stub, not the curated-looking engine.

**Consequence for the architecture:** "wire the existing engines" is necessary but **not sufficient**.
The `touring-offensive` CWE patterns are themselves broad/regex (a `0.220` machine), not AST/taint.
Reaching precision needs either (a) fixing `touring-offensive::vuln` detection to be context-aware
(AST/taint over `touring-ast-polyglot`), or (b) the external context-aware tool (Semgrep/CodeQL) —
i.e. the hybrid's "external where unique" likely extends to security too. Each engine must be
tp/fp-validated **before** it backs a gate — empirically, not by reputation.

**State (non-regressed):** the deployed (default, no-feature) binary keeps the substring fallback,
now slightly improved (dropped `exec(`/`deserialize(` → kills the `regex.exec()` FP): clean→1.0,
`regex.exec`→1.0, `eval(`→0.0, 176 tests green. The F2.1→SecurityAnalyzer wiring stays in code
(feature-gated infrastructure) but is **NOT deployed** until the engine is proven precise.

**Next (debug-first, not wire-first):** before wiring the remaining 5 BLOCK P0, debug the `0.220`
constant (my wiring vs the engine vs the single-file score path) and tp/fp-validate
`touring-offensive::vuln` itself. Precision is earned per-engine by measurement, never assumed.

## Part 5 — the 0.220 root cause: a one-character over-broad regex (FIXED + proven)

Debug discipline (read the code, never guess). `0.220 = 1 − 7.8/10` → exactly **one vuln match of
severity 7.8** for every input. Only one of the 10 CWE patterns has severity 7.8: **`LdapInjectionPattern`
(CWE-90)**, regex `(\*\)|\)|cn=)`. The middle alternative **`\)` is a bare literal closing paren** —
it matches **every `)` in any source file**. `fn noop()` has `)` → LDAPi fires → 0.220. That, not my
wiring (formula correct) nor `read_target_source`, is the whole constant.

**Fix (minimal, root-cause, test-preserving):** `(\*\)|\)|cn=)` → `(\*\)|cn=)` (drop the bare `\)`).
The 3 existing TPs still match (`*)`, `cn=Admin`, `(name=*)` via `\*\)`/`cn=`); added 4 FP regression
asserts (`fn noop()`, `(*f)(x)`, `compute(a,b)`, `if(x){y()}` → no match).

**End-to-end proof (feature binary, fixed SecurityAnalyzer):**

| F2.1 input | before | after |
|---|---|---|
| `fn noop() {}` | 0.220 | **1.000** |
| benign `regex.exec()` | 0.220 | **1.000** |
| `a + b` | 0.220 | **1.000** |
| real SQLi `' OR '1'='1` | 0.220 | **0.020** (flagged) |
| real cmd-inj `&& curl` | 0.220 | **0.070** (flagged) |

The engine now **discriminates** benign from dangerous at file scope. Validation green, zero
regressions: touring-offensive (309 tests), touring-analysis (500 tests across modules), clippy
`-D warnings` both, touring-quality default 176. The LDAP fix lives in `touring-offensive` source so
it benefits **every** consumer of `PatternRegistry`/`SecurityAnalyzer` (CEG static stage, the daemon
when rebuilt), not just F2.1.

**Residual tp/fp work identified (by reading the 10 regexes) — non-fatal but real (next chunk):**
- `PathTraversalPattern` `(\.\./|%2e%2e%2f)` — single `../` FPs on relative imports/comments;
  attack signature is `(\.\./){2,}`.
- `XssPattern` `on\w+=` — FPs on `monitor=`/`online=`; needs a curated event-handler list or `\b`.
- `LdapInjectionPattern` residuals — `cn=` FPs on `cn=` assignments; `*)` on regex strings.
- `CmdInjectionPattern` — false **negative**: catches `; rm`/`| ncat`/`&& curl` but MISSED the real
  `execSync(\`…${x}…\`)` injection (it has no shell-exec-with-interpolation rule).

Each remaining pattern earns precision the same way: realistic-benign-corpus tp/fp test, then tighten.
**Deployed binary kept standalone** (substring fallback) — the feature engine is now *usable* but the
residual broad patterns would still FP at workspace scope via WorstOf, so deploy waits for the full
P0-pattern precision pass.

## Part 6 — the 3 residual patterns tightened, tp/fp-validated, end-to-end proven

The residuals identified in Part 5 are now fixed in `touring-offensive/src/vuln/cwe_patterns.rs`, each
grounded in an **empirical FP corpus over the real `crates/` tree** (measure, never assume) + OWASP
CheatSheetSeries (context7) + an executed tp/fp regression corpus.

| Pattern (CWE) | old → new | FP over `crates/` (before → after) | basis |
|---|---|---|---|
| **PathTraversal** (22) | `(\.\./\|%2e%2e%2f)` → `((\.\./){2,}\|(\.\.\\){2,}\|%2e%2e%2f\|%2e%2e/\|\.\.%2f\|\.\.%5c)` | **260 → 61** (−77%) | multi-level climb is the CWE-22 signature; single `../` is a normal sibling import. Residual 61 = compile-time `include_str!("../../x")` macro literals (AST-layer job; regex has no lookbehind) |
| **XSS** (79) | `on\w+=` → curated **lowercase** DOM-event list + `<script[\s>/]` | **131 → 30** (the 30 are the detector's own source/tests + legit XSS test fixtures; **~0 real production FP**) | OWASP XSS Filter Evasion. Lowercase is the precision key: real HTML attrs are lowercase (`onerror=`), React/JSX props are camelCase (`onClick=`), minified JS emits `oneMapping=`/`onUpdate=` — all empirically the 131 old FP, 100% vendor minified JS |
| **CmdInjection** (78) | added shell-exec-interp alts | — (catalogs `execSync($$$)`/`os.system($$$)` **excluded**; sandbox `sh -c` **not flagged** by design) | keyed on INTERPOLATION/opt-in: `execSync`/`exec` of a `` `…${…}` `` template, `os.system(f"…")`/concat, `subprocess(…, shell=True)`, `{shell:true}` |

**FP/FN cases the new regexes were proven against** (the empirically-discovered traps, all now correct):
React `onClick={}` (camelCase, Pass) · vendor `{onUpdate:cb}` (Pass) · `memberRegex.exec(str)` (the prior
FP, Pass) · single `import "../utils"` (Pass) · the safe `execFileSync([...])` (my own Part-2 remediation,
Pass) · the security tooling's `execSync($$$)`/`os.system($$$)` pattern catalogs (Pass) · the CEG sandbox
`Command::new("sh").arg("-c")` (Pass, intentional). And the FN that started this: `execSync(\`ping ${host}\`)`
now **Fails** (flagged).

**End-to-end proof (feature binary, F2.1 via SecurityAnalyzer composing these patterns) — 11/11 correct:**
TP all `Fail` (SQLi 0.02 · XSS 0.19 · `execSync(\`${}\`)` 0.07 · `os.system(f"…")` 0.07 · `../../etc/passwd` 0.20);
FP all `1.000 Pass` (React · `regex.exec` · single relative import · `execFileSync` · plain code · vendor camelCase).

**Gate (real `$PIPESTATUS`):** `cargo test -p touring-offensive` (1 downstream test corrected — the concolic
`test_executor_detect_path_traversal` fixture asserted the old single-`../` FP behavior → updated to a real
`../../etc/passwd` climb; REGRA #21) · `clippy -p touring-offensive --all-targets -D` = 0 · `touring-analysis`
full (incl. `security_analyzer_test.rs`) = 0 · `touring-ceg` = 0 · `touring-quality` lib 176 = 0 · fmt clean.

**Meta-lesson reinforced:** every regex was tightened against the *real tree* as its FP corpus (260, 131
benign lines measured, not guessed) and against OWASP for the demanding side — precision came from
**measurement + best-practice**, the standard Part 1-5 established. The fix is in `touring-offensive`
source, so it lifts the feature path, the CEG static stage, and the daemon — not just F2.1. Deployed binary
remains standalone (stable); full feature-engine deploy still gated on the remaining 6 patterns
(SQLi/IntOvf/BufOvf/Deser/SSRF/XMLi — all reasonably specific, not catastrophically broad) being
tp/fp-validated + the WorstOf aggregation reconsidered (PathTraversal's `include_str!` residual would tank
workspace-scope WorstOf).

## Part 7 — the remaining 6 patterns validated, WorstOf reconsidered, feature engine DEPLOYED

**My "all reasonably specific" claim from Part 6 was an INFERENCE — and the measurement falsified it**
(Gabriel's standing lesson: validate, never assume). Empirical FP over `crates/` (prod = excluding the
detector source / catalogs / vendor):

| Pattern (CWE) | total → prod | verdict + fix |
|---|---|---|
| SQLi (89) | 25 → 16 | test fixtures (`' OR '1'='1`); kept regex, +FP corpus (`a \|\| b`, `// -- comment`) |
| IntOverflow (190) | 4 → **0** | 0 prod FP; weak presence-heuristic — `\bINT_MAX\b` anchor + honest doc |
| BufferOvf (121) | 28 → 7 | tightened to CALLS `\b(strcpy\|strcat\|sprintf\|vsprintf\|gets)\s*\(` (prose mention + `snprintf` no longer FP) |
| Deser (502) | 9 → 2 | catalog-only; `yaml.load` ≠ `yaml.safe_load` confirmed; +FP corpus |
| **SSRF (918)** | 41 → **37** 🔴 | **the surprise**: `http://127.0.0.1` was the local **Touring daemon's own address** (37 FP). Rewrote to OWASP deny-list high-signal: `169.254.169.254`/`metadata.{amazonaws,google}`/`gopher\|dict\|phar://`/interpolated `file://${`. **41 → 10** over the tree; `failover/impl_daemon.rs` 0→**1.000 Pass** |
| **XMLInj (91)** | 7 → 2 🔴 | `<!DOCTYPE html>` (benign HTML5) was FP. Rewrote to XXE signal `<!ENTITY\|<!DOCTYPE[^>]*\[`; `<!DOCTYPE html>`→Pass |

**WorstOf reconsidered (not softened).** `WorstOf = min` is *correct* for a BLOCK/security dim (one real
injection must fail the scope — confirmed in `aggregate.rs`: it "closes the 2 MiB-truncation hole"). So the
fix is **precision + scoping + allowlisting the detectors' own source**, never weakening the aggregation:
- **PathTraversal `include_str!` residual (61 files)** → fixed at the source: `in_compile_time_include`
  suppresses `../` literals inside `include_str!`/`include_bytes!`/`concat!`/`env!`/`option_env!` (bounded
  backward scan; `find_iter` still reports a genuine traversal after the macro). Benefits every consumer.
- **Vendor minified JS (the 131 XSS FP)** → already scoped out: `enumerate_source_files` SKIP_DIRS already
  contains `vendor`+`dist`; `.html` is not in SOURCE_EXTS. No workspace-scan contribution.
- **Detector own source** → `is_detector_own_source` extended from `touring-quality/*` to the security
  *engines*: `touring-offensive/src` (CWE registry + concolic corpus), `touring-analysis/src/quality` +
  `tests` (the SecurityAnalyzer + its corpora), `touring-hooks-shared` forbidden/risk/antipattern catalogs.
  These embed attack literals as detection logic / test inputs — FP-by-context, exactly what gitleaks/Semgrep
  allowlist.

**Deploy.** `touring-quality` Cargo.toml `default = ["workspace-integration"]` (the previous unused
`touring-cli` dep dropped — F2.1 only needs `touring-analysis`). The deployed binary now uses the real
**SecurityAnalyzer CWE/OWASP engine** (verified: `<img onerror=>` → 0.19 Fail with evidence "SecurityAnalyzer";
the substring fallback returned 1.0). `--no-default-features` still builds the standalone fallback.

**End-to-end proof (deployed engine, F2.1) — 8/8 correct:** TP all Fail (SSRF-metadata 0.14, SSRF-gopher 0.14,
XXE 0.28, sprintf-call 0.09); FP all 1.000 Pass (local daemon `127.0.0.1`, `<!DOCTYPE html>`, `snprintf`,
`yaml.safe_load`). Plus the offensive **277-test corpus** (full tp/fp for all 10 patterns + the `include_str!`
suppression) passes.

**Gate (real `$PIPESTATUS`):** offensive 277 + clippy --all-targets -D=0 · touring-quality lib 176 (feature
default) + standalone `--no-default-features` check=0 + clippy -D=0 · touring-analysis=0 · touring-ceg=0 · fmt=0.

**Honest WorstOf status (no number-gaming).** Workspace-scope F2.1 `WorstOf` is still 0 — held by *security
test corpora* (e.g. the generator's `security_gate_sqli_pattern_detected` test holds `"UNION SELECT … --"`)
and pattern-in-prose (a comment listing SQLi triggers). This is the **irreducible limit of a TEXT-regex
engine over a security-tooling monorepo**, not vulnerable code; allowlist-spamming every test to fake 1.0
would be exactly the convergence-engineering Gabriel warned against. The actionable, now-precise metric is
**per-file F2.1** (the 6 BLOCK PreToolUse hooks + pre-edit) — a dev writing `execSync(\`${x}\`)` is blocked
while `regex.exec(str)` / React `onClick=` / `import "../utils"` / the local daemon URL are not. Closing the
workspace-WorstOf to 1.0 needs **AST / test-region awareness** (ignore comments + `#[cfg(test)]` corpora) —
the documented next frontier, not a regex job. `03_security` keeps using cargo-deny; workspace-F2.1 is
informational until the AST pass lands.

---

## Part 8 — AST-aware region pass + measurement-driven CWE pattern precision (2026-06-21)

The "next frontier" of Part 7 — *ignore comments + `#[cfg(test)]` corpora* — is now **built, deployed, and
empirically validated**, together with the pattern-precision fixes the measurement surfaced. Discipline
throughout: **measure each WorstOf holder, classify it (comment / test / benign-code-construct / real sink),
fix only the genuine FPs** — never allowlist-spam a number to 1.0.

### What shipped

1. **AST-aware region pass** — new `touring-analysis/src/quality/code_regions.rs`
   (`non_executable_regions(src, lang)` + `offset_suppressed`), wired into `SecurityAnalyzer::analyze`
   (drops any `VulnMatch` whose span starts in a comment or a Rust `#[cfg(test)]`/`#[test]` region).
   A single-pass, **string-literal-aware** lexer (so `//` in `"http://…"` and a Rust `'"'` char literal
   never open phantom comments/strings; handles raw strings, Python triple-quotes; `#[cfg(not(test))]` is
   **production** and is NOT suppressed). 15 unit tests. Grounded in the SAST gold standard (Semgrep
   `generic_comment_style` + `.semgrepignore` test exclusion). Production string literals are deliberately
   **not** suppressed — injection lives in strings; the pattern regex is the lever there.

2. **SQLi precision** (`cwe_patterns.rs`): injection arms use **same-line** `[ \t]` (not `\s`, which the
   engine matches across NEWLINES) and require a **string-break quote** before `; --`:
   `('[ \t]*OR[ \t]*'|'[^'\n]{0,80};[ \t]*--|UNION\s+SELECT)`. Killed three FP shapes — CLI `--help` text
   (`"…state; --persist…"`), the embedded SQL **DDL schema** in `touring-foundation`
   (`'id');\n-- comment`), and `'A'\nOR\n'B'` prose. TPs preserved: `'; --`, `' OR 1=1; --`, `' OR '`,
   `UNION SELECT`.

3. **XSS precision**: `javascript:` → `javascript:(?:[^:]|$)` so the Rust **path separator** `::` is not
   flagged (`tree_sitter_javascript::LANGUAGE` is not the `javascript:` URI vector). `regex` has no
   lookahead → `(?:[^:]|$)` is the linear-time `(?!:)` while still matching a bare `javascript:`.

4. **Non-production harness exclusion** (F2.1 verifier `is_detector_own_source`): `tests/`/`benches/` are not
   a deployed attack surface (the CEG's `benches/ceg_baseline.rs` feeds `shell=True` to *measure* detection).
   SAST-standard (Semgrep `.semgrepignore`). **F2.1-only** — `f2_4_secrets` must still scan tests (a fixture
   credential is genuinely leakable; a secret in a comment is still committed).

### Empirical trajectory (deployed engine, real exit codes)

WorstOf-F2.1 climbed as each highest-severity FP class was cleared — a healthy convergence-by-rigor:
`0.02 (SQLi 9.8)` → `0.07 (CMDi 9.3, bench corpus)` → `0.19 (XSS 8.1, javascript::)` → `0.20 (PathTraversal 8.0)`.
All 7 measured holders (typestate, command_table, gate_metrics, migrate_from_global, foundation/knowledge,
ceg_baseline, languages) now score **1.0 Pass**. TP controls still fire (`execSync(\`${x}\`)` 0.07,
`' OR '` in prod 0.02) — **zero over-suppression**. Gate: offensive 277+14+18, analysis 408+6+48+3+13+13,
quality 176+24+1, `code_regions` 15/15, clippy `--all-targets -D`=0, fmt=0, REGRA #0 (both new pub fns wired
in `security.rs`).

### The honest residual — PathTraversal is the taint/AST frontier, NOT a regex job

Final workspace-WorstOf = **0.20**, held by `glob_diag.rs` — a CLI **help string**
`Prefer crates/**/*.rs over ../../../crates/**/*.rs` that `(\.\./){2,}` matches. A workspace sweep confirmed
the residual class: **textual `../../`** in doc strings, glob patterns, and relative-path code (~7 prod
files). Deser/BufOvf hits are detector-source-only (allowlisted); SSRF/IntOvf/XMLInj have **zero** prod
holders. `(\.\./){2,}` is **regex-irreducible**: textual `../../` is indistinguishable from a real CWE-22
sink without **data-flow / taint** (untrusted input → path → file-open) — and a *hardcoded* `../../` is not a
vuln at all (CWE-22 needs untrusted input). Forcing it to 1.0 would either blind real detection or be
arbitrary path exclusions = the convergence-engineering Gabriel forbade. **Stopped here by principle.**
Per-file F2.1 (the 6 BLOCK hooks + pre-edit) is now precise on every real case; closing workspace-WorstOf to
1.0 is the taint-analysis upgrade (the "41 stubs → real engines" program), a separate and larger body of work.

**Files**: `touring-analysis/src/quality/{code_regions.rs (new), mod.rs, security.rs}`,
`touring-offensive/src/vuln/cwe_patterns.rs` (SQLi+XSS), `touring-quality/src/verifications/f2_1_owasp.rs`
(harness exclusion). Engine redeployed (`target/release/touring-quality`, default `workspace-integration`).

## Part 9 — F2.5 dep-CVEs: stub → real RustSec engine, made *precise* (CVE ≠ unmaintained)

Gabriel endorsed the next slice verbatim: *"migrar o primeiro P0 BLOCK verifier de stub-substring
para engine real tp/fp-validado."* Chose **F2.5** (the clearest stub → real engine).

### What changed

1. **Engine swap (stub → RustSec).** `f2_5_dep_cves.rs` under `workspace-integration` now delegates to
   `touring_analysis::security::SecurityDb` — the in-process RustSec advisory DB (`~/.cargo/advisory-db`).
   New `SecurityDb::scan_lockfile` resolves the manifest's **`Cargo.lock`** (the *resolved transitive* tree —
   `Cargo.toml` carries `"1.0"` requirements, not the versions an advisory matches) and reuses the
   already-tested `scan_package`. The prior W1 MVP was a **4-entry hardcoded substring list** that caught
   **0** of the real advisories in the tree. **Manifest-scoped** (`AggKind::ScopeNative`): a `.rs` file has no
   dep tree of its own → 1.0 pass (no per-source-edit blocking on a project-level CVE); per-lockfile cache;
   graceful-offline (absent DB / unreadable lock → 1.0, never blocks a workflow on tooling absence);
   standalone substring **fallback** preserved under `--no-default-features`.

2. **rustsec 0.30 → 0.33** (`Cargo.toml`). 0.30.4's `Database::open` **fails** parsing CVSS-4.0 advisories
   (`unsupported CVSS version: 4.0` on `RUSTSEC-2026-0073.md`) → the DB loaded *offline* → F2.5 silently
   caught nothing. 0.33 (matching cargo-audit 0.22) supports CVSS 4.0. Clean bump: cargo-lock 10→11, +toml
   0.9, **zero API breakage**; `cargo check --workspace` = 0 (`WS_CHECK_EXIT=0`).

3. **Precision refinement — CVE ≠ unmaintained (the tp/fp finding).** A naive "count every advisory match"
   would BLOCK a `Cargo.toml` edit because `paste` is *unmaintained* — a **false positive** for a dimension
   named **"Dependency CVEs" (D14)**. RustSec's own `informational` field (`None` = genuine vulnerability;
   `Some(Unmaintained|Unsound|Notice)` = advisory but not a CVE) is the canonical signal, and cargo-audit
   makes the same vuln-vs-warning split. So: `SecurityAdvisory` gained an `informational: Option<String>`
   field (populated from `adv.metadata.informational.as_str()`); `score()` partitions matches and **BLOCKs
   (0.0) only on real vulnerabilities** (informational `None`), surfacing unmaintained/unsound as a
   **non-blocking note → F4.5 (D44, package-management)**. TP preserved ("a real CVE detector *should* fail on
   real CVEs"), FP removed (unmaintained no longer fails the CVE gate). New unit test asserts the partition:
   vuln→0.0, unmaintained-only→1.0, mixed→0.0, empty→1.0.

### Empirical result (deployed binary, real `$?`)

`touring-quality check --gate F2.5 --target Cargo.toml` → **0.0 Fail**:
`6 dependency CVE(s) in the resolved tree: gix-date@0.9.4 (RUSTSEC-2025-0140), postgres-protocol@0.6.11
(RUSTSEC-2026-0179), postgres-protocol@0.6.11 (RUSTSEC-2026-0180), pyo3@0.24.2 (RUSTSEC-2026-0176),
pyo3@0.24.2 (RUSTSEC-2026-0177) (+1 more) | 12 non-blocking informational (unmaintained/unsound — see F4.5)`.
A `.rs` target → **1.0** (manifest-scoped). The 4-entry stub caught 0 of these 6.

Gate: `TESTS_EXIT=0` (analysis 409 + quality suites incl. new tp/fp test), `CLIPPY_EXIT=0` (`--all-targets -D`),
`WS_CHECK_EXIT=0`, `BUILD_EXIT=0`.

### Real security finding (Gabriel's decision — lockfile/deny.toml NOT touched)

The resolved tree carries **6 genuine vulnerabilities** (the gate now correctly fails on them): **pyo3@0.24.2**
×2 (RUSTSEC-2026-0176 OOB read in `PyList`/`PyTuple` iterators; -0177 missing `Sync` bound on
`PyCFunction::new_closure`), **postgres-protocol@0.6.11** ×2 (-0179 unbounded SCRAM iteration → CPU-exhaustion
DoS, CVSS-4.0 `VA:H`; -0180 panic on malformed `hstore` → DoS), **tokio-postgres@0.7.17** (RUSTSEC-2026-0178
panic on short `DataRow` → DoS), **gix-date@0.9.4** (RUSTSEC-2025-0140 non-utf8 via `TimeBuf::as_str`). Plus
**12 informational** (unmaintained: bincode, dirs, instant, paste, rustls-pemfile, proc-macro-error2,
humantime, …; unsound: lru, rand). Remediation is a security/dep decision for Gabriel:
`cargo update -p <crate>` the patchable ones; `[advisories.ignore]` in deny.toml (justified) for
unavoidable/unmaintained. **Operational consequence**: with the real engine + the registered master BLOCK hook
`touring-quality-block-all.sh`, editing the workspace `Cargo.toml` is now BLOCKED **by the 6 real CVEs only**
(correct) until remediated/ignored.

### Record correction + hook hygiene (REGRA #21)

- **Self-correction (honesty).** A prior compaction-summary claim that the BLOCK hook had F2.5 disabled
  (`n`-typo) was **FALSE** — `touring-quality-block-all.sh:112` enforces all 6 P0 dims correctly. Falsified by
  direct read; not propagated.
- **5 bugs fixed** in the *unregistered, superseded* `touring-quality-f2-5-block.sh` (a latent fail-open trap):
  invalid `score --gate` → `check --gate --target`; `.composite` → `.dimensions.F2_5.value`; **hallucinated**
  `taco-forge perfect-quality-f2-5-deps` (PLANNED W7) → real fix string (`cargo update` / deny.toml ignore /
  `perfect-edit`); false "registered in settings.json" header → "OPTIONAL granular alternative, NOT
  registered"; **missing `export LC_ALL=C`** (pt_BR locale emitted `0,00` + a `printf` error — disk-hygiene
  REGRA #7). Validated end-to-end: clean `0.00`, exit 2 BLOCK on a simulated `Cargo.toml`, exit 0 on `.rs`.

**Files**: `touring-analysis/src/security.rs` (`SecurityAdvisory.informational` field + `scan_lockfile`),
`touring-quality/src/verifications/f2_5_dep_cves.rs` (real engine + vuln/informational partition + tp/fp test),
`Cargo.toml` (rustsec 0.33), `~/.claude/hooks/touring-quality-f2-5-block.sh` (5 fixes). Engine redeployed.

## Part 10 — F2.6 config-security: stub → real `ConfigSecurityAnalyzer` (2nd P0 BLOCK migrated)

The harness-infrastructure program (stub → D-rule-named engine, 6 P0 BLOCK first). After F2.5
(Part 9), the **crudest remaining P0 BLOCK stub** was F2.6 (72 LOC, 2 tests, 0 delegation): it scored a
file by the ratio of `debug!`/`println!`/`env::` occurrences — a **false-positive machine** (any logging
lowered the score) that was also a **total false negative** for actual misconfiguration (it never inspected
TLS/CORS/debug settings). Migrated it to a real engine.

### New engine — `touring_analysis::quality::ConfigSecurityAnalyzer` (sibling of `SecurityAnalyzer`)

`crates/touring-analysis/src/quality/config_security.rs` (new). A curated **OWASP A05:2021 Security
Misconfiguration** catalog — 6 CWE-mapped rules: TLS/cert verification disabled (CWE-295, sev 6.0 → a single
hit BLOCKs: `1.0 - 6.0/10 = 0.4 < 0.5`), permissive CORS wildcard (CWE-942), active debug in production
(CWE-489, Flask-scoped to avoid the Cargo `[profile] debug = true` debuginfo false hit), insecure cookie
flags (CWE-614), unsafe CSP directives (CWE-693), world-writable modes (CWE-732). Emits
`touring_offensive::vuln::VulnMatch` (shared shape) so the **same AST-aware `code_regions` pass (Part 8)**
suppresses misconfig literals living in comments / `#[cfg(test)]` corpora; pure-comment lines are skipped
pre-match (format-agnostic `//`/`#`/`/*`/`*`); score `1.0 - sum/10` clamped (identical to `SecurityReport`'s
vuln-score half, so F2.6 composes consistently with F2.1). Zero-dependency line scanner (no `regex` dep,
matching `code_regions`). 13 analyzer tp/fp unit tests.

### F2.6 verifier — delegate (mirrors F2.1/F2.5)

`crates/touring-quality/src/verifications/f2_6_config.rs` (rewritten): under `workspace-integration`
delegates to `ConfigSecurityAnalyzer`; labelled substring fallback for standalone builds;
`is_detector_own_source` allowlist (test/bench dirs + `touring-analysis/src/quality` + `touring-quality/src`,
since the rule catalog + fallback embed misconfig literals as detection logic). 6 verifier tests, incl.
`logging_is_not_a_misconfig` (the old-stub FP, now Pass) + `tls_disabled_blocks` (BLOCK).

### Gate (real `$?`) + empirical

`cargo test -p touring-analysis -p touring-quality` = 0 (analysis 422 incl. 13 new; quality 182 incl. 6 new) ·
`cargo clippy --workspace --all-targets -- -D warnings` = 0 (touring-analysis is foundational → ripples to
every crate, clean) · `cargo check --workspace` = 0 · `touring-elite` composite = **0.9452 Platinum**
(unchanged). Deployed `touring-quality` empirical: a `.rs` with `danger_accept_invalid_certs(true)` →
**0.40 Fail/BLOCK** (CWE-295); a clean handler with `println!` → **1.0 Pass** (the stub would have failed it).

TP preserved (real misconfig BLOCKs), FP removed (logging / `cfg!(debug_assertions)` / Cargo `[profile]
debug = true` / commented misconfig / `#[cfg(test)]` fixture all Pass — proven by unit tests). The dim is now
precise to its D19 name ("Configuration Security"), not a logging-density proxy.

**Files**: `touring-analysis/src/quality/{config_security.rs (new), mod.rs}`,
`touring-quality/src/verifications/f2_6_config.rs`. Engine redeployed (`target/release/touring-quality`).

### P0 BLOCK program status

| Dim | Engine | State |
|-----|--------|-------|
| F2.1 OWASP | `SecurityAnalyzer` + CWE `PatternRegistry` + region pass | real (per-file precise; workspace taint frontier remains) |
| F2.4 secrets | in-crate gitleaks-style (regex + Shannon entropy + provider prefixes, 36 tests) | real |
| F2.5 dep-CVEs | `SecurityDb` (RustSec advisory DB) | real (Part 9) |
| **F2.6 config** | **`ConfigSecurityAnalyzer` (OWASP A05, region-aware)** | **real (this part)** |
| F4.3 deprecated | `allow(deprecated)` consumption (W0 inversion fix) | semi-real (small) |
| **F4.5 pkg-mgmt** | **`DepHealthAnalyzer` (cargo-deny `[bans]` + RustSec informational + cargo machete)** | **real (Part 11)** |

**4 of 6 P0 BLOCK dims now have real engines** (F2.1, F2.4, F2.5, F2.6, F4.5 real; F4.3 semi-real).

---

## Part 11 — F4.5 pkg-mgmt: dependency-count heuristic → real cargo-deny-bans + machete engine

**Date**: 2026-06-22. **Request**: "prossiga com F4.5 pkg-mgmt (o último P0 ainda heurístico) → delegar a
cargo-deny bans (multiple-versions/wildcards/unmaintained) + cargo machete."

### The stub

`f4_5_pkg_mgmt.rs` scored a manifest by *counting* deps (`= "\""` matches) and warning past a soft cap
(50 cargo / 100 npm / 30 pip). Pure quantity heuristic: it flagged a lean manifest with one bad dep as
*better* than a fat-but-clean one, and never looked at what cargo-deny / cargo-machete actually check
(wildcards, duplicate versions, unmaintained, unused). The docstring even promised "defers to
`touring-analysis::dep_audit`" — a phantom module that never existed (same aspirational-docstring pattern
F2.6's stub had).

### The real engine — `touring_analysis::quality::dep_health::DepHealthAnalyzer`

Hermetic (no external binary), faithful to the cargo-deny `[bans]` + cargo-machete semantics D44 grounds:

- **wildcards** (`bans.wildcards = "deny"`): a `= "*"` registry spec in `[dependencies]`/`[build-dependencies]`,
  parsed from `Cargo.toml` TOML. Path/git/`workspace = true` deps are never wildcards (mirrors
  `allow-wildcard-paths`). **This is the sole BLOCK driver** (weight 6.0 → one prod wildcard = `0.40 < 0.5`).
- **multiple-versions** (`bans.multiple-versions`, default `"warn"`): a crate at 2+ versions in `Cargo.lock`.
- **unmaintained / unsound** (RustSec `advisories` informational): **reuses F2.5's `SecurityDb`** over the
  resolved lockfile, filtering `informational.is_some()` — exactly the advisories F2.5's CVE-vs-informational
  partition (Part 9) *defers here*. F4.5 is the designated home for the unmaintained signal.
- **unused** (cargo-machete): a declared registry dep whose normalized name (`-`→`_`) never appears in the
  crate's sources (`src`/`tests`/`benches`/`examples` + `build.rs`, bounded walk). Conservative — substring
  presence errs toward "used" (a `serde`/`serde_json` shared prefix is never falsely flagged).

### The anti-theater scoring invariant (the crux)

Score mirrors `config_security` (`1.0 - sum/10`, clamped) **but the hygiene penalty — everything except
prod/build wildcards (dev wildcards + unmaintained + duplicate-versions + unused) — is capped at
`HYGIENE_CAP = 4.5`**, so it can drive the score no lower than `0.55` (Silver/WARN, above the 0.5 BLOCK line).
**Only an author-controlled registry wildcard fails the gate.** This is deliberate: F4.5 must NOT
re-introduce the transitive-debt false positive that F2.5's partition was built to avoid. A workspace with a
few accepted unmaintained transitive deps (`paste`, `instant`, `bincode`…) is *imperfect hygiene to surface*,
not a fail-closed break. Calibrated as a pure unit-testable `compute_score` (proven: hygiene-only with
5 dev-wildcards + 20 unmaintained + 5 unsound + 40 dups + 30 unused still scores ≥ 0.55).

### Dogfood found real debt (acted on, REGRA #0/#21)

Running the new F4.5 on `touring-quality`'s own `Cargo.toml` flagged **4 genuinely-unused deps** (`thiserror`,
`tabs` in `[dependencies]`; `pretty_assertions`, `proptest` in `[dev-dependencies]` — all 0 src refs; the
crate uses `anyhow::Result`, not `thiserror`). Verified TP by grep (0 refs) **and proven** by removing all 4
and re-running `cargo test -p touring-quality` = **184 passed, 0 failed** (truly unused — the crate compiles
and every test passes without them). Removing them is the gate driving its own remediation.

### Gate (real `$?`) + empirical

`cargo test -p touring-analysis --lib dep_health` = 0 (14 new) · `cargo test -p touring-quality f4_5` = 0
(4 new) · full `cargo test -p touring-quality` = **184 passed, 0 failed** · `cargo clippy --workspace
--all-targets -- -D warnings` = 0 · `cargo check --workspace` = 0 · `touring-elite` composite = **0.9452
Platinum** (unchanged — no regression). Deployed `touring-quality check --gate F4.5` empirical:
`foo = "*"` manifest → **0.400 Fail/BLOCK** (names `foo (dependencies)` + `[BLOCK]`); clean manifest →
**1.000 Diamond/Pass**; dogfood (`touring-quality/Cargo.toml`) → **0.550 WARN, 0 blockers** (8 real
unmaintained/unsound + 100 duplicate-version crates, never fail-closing); workspace root → **0.550 WARN,
0 blockers** (10 unmaintained + 181 dups, capped). The anti-theater invariant is proven *empirically*: real
transitive debt → WARN, only an author wildcard → BLOCK.

**Files**: `touring-analysis/src/quality/{dep_health.rs (new, 590 L), mod.rs}`,
`touring-quality/src/verifications/f4_5_pkg_mgmt.rs` (rewritten), `touring-quality/Cargo.toml`
(4 unused deps removed). Engine redeployed (`target/release/touring-quality`).

### CC refactor (F1.1 self-discipline)

The verifier's evidence assembly initially hit CC=15 (five `if !empty` blocks). Collapsed to a single
table-driven `[(label, findings, show_list); 5]` filter/map loop → CC well under 10. Dogfooding F1.1 on the
file that implements the harness.

## Part 12 — F1.1 complexity / F3.1 coverage / F1.8 dep-cycles: 3 WARN-tier stubs → real engines + a real cycle remediation (Gabriel "prossiga … F1.1 (já parte real via touring ast), F3.1 (→ cargo-llvm-cov), F1.8 (→ Tarjan, já tem wiring cycles)")

First WARN-tier slice of the stub→real-engine program (the 6 P0 BLOCK dims are done; this attacks the
highest-leverage advisory dims). Same delegation shape as the P0 work: `#[cfg(feature="workspace-integration")]`
(default) delegates to a `touring-analysis` engine; `#[cfg(not(...))]` keeps a labelled fallback so the crate
stays standalone-buildable. Each engine reuses or extends a real `touring-analysis` capability that already
existed but sat **unused** by the substring-stub verifier.

### F1.1 — Complexity (D01, AggKind::WeightedLoc)

**The stub** counted control-flow keywords (`if `/`match `/`&&`/`?`…) over the flattened buffer; its own
doc-comment **falsely claimed** delegation to `touring-analysis::cognitive_complexity` that was never wired
(drift). **Now delegates** to the real `touring_analysis::quality::estimate_complexity(source, lang)` —
language-aware (rust/py/ts/js/go/java/c/cpp via a `lang_from_ext` map), computing cyclomatic
(`max_complexity`), REAL cognitive complexity (a nesting-penalised single pass), and the SEI/Mozilla
maintainability index. Score = the D01 piecewise cyclomatic band (CC≤5→1.0, ≤10→0.8, ≤20→0.5, >20→fail)
dampened by a **per-function** cognitive penalty (`cognitive.checked_div(function_count)`, >15 severe / >8
warn) — normalised by fn count so a long file of simple functions is NOT punished for length (the
faithfulness fixture proves it). 6 verifier tests.

### F3.1 — Coverage (D27, AggKind::CoverageRatio)

True line coverage cannot be computed from a source buffer — it needs the suite run under instrumentation.
**New engine** `touring_analysis::quality::coverage_artifact::CoverageArtifact`
(`crates/touring-analysis/src/quality/coverage_artifact.rs`): **consumes the LCOV artifact `cargo llvm-cov`
emits** — walks up from the target to the canonical spots (`lcov.info`, `target/llvm-cov/lcov.info`,
`coverage/lcov.info`, `target/nextest/lcov.info`), parses `SF:`/`LF:`/`LH:` records (falls back to counting
`DA:` hits), and matches the target file by canonical path then last-two-components (`parent/file`) suffix
(disambiguates `mod.rs`). Returns the file's **REAL** `hit/found` ratio when a record matches; otherwise the
verifier falls back to the honest presence proxy (test fns / public surface), clearly labelled — never a
guessed coverage number. Composes with the CoverageRatio roll-up (Σcovered/Σtotal). Proof fixture: 2 untested
pub fns (presence proxy → 0.0) + an LCOV record `LF:10 LH:9` → **0.900** (real coverage wins). 8 engine + 5
verifier tests.

### F1.8 — Dependency cycles (D08, AggKind::ScopeNative)

**The stub** (post-W0) only counted local hygiene smells (`extern crate`, deep `super::super::super::`) — it
could never see a cycle. Gabriel pointed at the existing `detect_import_cycles` (`wiring cycles`), but that
engine queries the daemon's `wiring_map` DB — **scope-incorrect for an arbitrary target and staleness-prone**
(a gate that reports "0 cycles" by querying a DB about a *different* project is a lying gate). **New hermetic
engine** `touring_analysis::wiring::module_cycles::ModuleCycleAnalyzer`: builds the module→module import graph
**from the target crate's own source tree** (`use crate::<top-level>` edges, collapsed to top-level module so
an SCC is a real coupling cycle, self-references dropped) and runs the **same petgraph Kosaraju SCC** as
`detect_import_cycles` — but sourced hermetically, so it's correct for ANY target with no staleness. Cargo
forbids cross-crate cycles, so intra-crate module cycles are the only meaningful F1.8 signal. Non-crate target
→ local-hygiene fallback (never claims acyclicity it can't observe). Score: 0 → 1.0; `(0.8 - cycles*0.15)`
otherwise (D08 bands; ADVISORY). 8 engine + 6 verifier tests.

### DOGFOOD found AND fixed a real production cycle (REGRA #0/#21)

F1.8 on `touring-analysis` itself reported **0.65 Bronze: 1 cycle `e2e↔pipeline↔temporal↔wiring`**. Verified
it was NOT an engine FP (C08 cross-caller compare on the real `use crate::` lines): the cycle is genuine in
the `crate::` graph — `e2e/mod.rs` uses `crate::pipeline::AnalysisPipelineBuilder`, `pipeline.rs` uses
`crate::{temporal,wiring}`, and `wiring`/`temporal` use `crate::e2e::schema_guard`. **Root cause**:
`schema_guard` was relocated to `touring_foundation` in the A5 migration (2026-06-15) but `e2e/mod.rs` left a
backward-compat re-export, and 6 consumers still imported it via the legacy `crate::e2e::schema_guard` alias —
closing a spurious cycle on a path whose real dependency is on `foundation` (acyclic). **Completed the
migration** (move-utils-down): redirected all 6 (`wiring/{mod,orphan,functional_chains}`, `temporal/trends`,
`knowledge/mod`, `learning/mod`) `use crate::e2e::schema_guard` → `use touring_foundation::schema_guard`
(identity-preserving, foundation already a dep, compiler-verified). Result: touring-analysis F1.8 **0.65
Bronze → 1.0 Diamond** ("0 module-import cycles across 16 modules — acyclic"). The new dim drove a real
architectural cleanup the old stub could never surface — the same TP-with-remediation pattern as F4.5's
dogfood (Part 11). (The orphaned `e2e/schema_guard.rs` on disk awaits Gabriel's `git rm`, REGRA #11.)

### Gate (real $?)

F1.1 verifier 6/6; F3.1 engine 8/8 + verifier 5/5; F1.8 engine 8/8 + verifier 6/6. Full suites:
touring-quality 190+, touring-analysis 452+ (0 failed). Standalone fallback (`--no-default-features`) builds.
**clippy --workspace --all-targets -D warnings = 0** (one fix mid-way: `manual_checked_ops` on the F1.1
cognitive division → `checked_div`). check --workspace = 0. **touring-elite composite 0.9453 Platinum (no
regression** from 0.9452; F1.8 does not back the 13-gate composite — `02_architecture` uses
`wiring_integrity_gate`).

### Lessons

1. **A faithful dim surfaces real defects the stub couldn't** — F1.8's first dogfood run found a genuine
   production module cycle in touring-analysis (and drove its fix). Convergence is an OUTPUT of this rigor, not
   a target engineered into the scores.
2. **Hermetic-from-source beats DB-delegation for a portable gate** — re-deriving the import graph from the
   target's files (vs querying the daemon's `wiring_map`) makes F1.8 correct for any target and immune to
   cross-project staleness; the cost (a small `use crate::` parser) buys honesty.
3. **Coverage: consume the artifact, never guess** — F3.1 reports real `cargo-llvm-cov` line coverage when the
   artifact exists and an explicitly-labelled presence proxy otherwise; it never reports a proxy as coverage.
4. **Normalise per-unit, not per-file** — F1.1's cognitive penalty is per-function (`cognitive/function_count`)
   so file length alone never lowers the score (the same trap the W0 f3_1 density measure fell into).
5. **A surfaced cycle can be a stale-migration artifact** — completing the A5 `schema_guard`→foundation move (6
   legacy-alias imports) broke the cycle with a zero-blast import redirect; the engine pointed exactly at the
   unfinished migration.

**Files**: `touring-analysis/src/quality/{complexity.rs (reused), coverage_artifact.rs NEW, mod.rs}`,
`touring-analysis/src/wiring/{module_cycles.rs NEW, mod.rs}`, `touring-analysis/src/lib.rs`, the 6
schema_guard redirects (`wiring/{mod,orphan,functional_chains}.rs`, `temporal/trends.rs`, `knowledge/mod.rs`,
`learning/mod.rs`), `touring-quality/src/verifications/{f1_1_complexity,f3_1_coverage,f1_8_dep_cycles}.rs`
(all rewritten). Doc Part 12.

## Part 13 — F1.2 maintainability / F1.4 SOLID / F1.6 error-handling → real engines, *with* engine-effectiveness verification (Gabriel: "sempre verifique a qualidade, excelência e efetividade das engines que serão utilizadas para as validações do harness")

Three more WARN-tier stubs migrated to real `touring-analysis` engines — but the headline is a **process directive Gabriel injected mid-slice** that proved its worth immediately: *before the harness depends on an engine, verify the engine's own quality/effectiveness*. This is the program's thesis stated as a rule, and it caught a major defect that a naive delegation would have shipped.

**Engine-effectiveness verification (the new mandatory step).** I rated all four candidate engines by mechanism, ran the 248-test quality baseline (correctness), then ran each NEW dim on **real workspace files** before trusting it:

| Engine | Mechanism | Rating | Verdict |
|---|---|---|---|
| `RustQualitySignals` (rust_semantic) | **syn AST** | ★★★★★ | use directly (F1.4 core, F1.2 blend) |
| `estimate_maintainability_index` (MI) | SEI/Mozilla formula | ★★★★ formula / **miscalibrated bands** | **defect found — fixed** |
| `analyze_error_coverage` | memmem substring | ★★★ | propagation bonus only |
| `count_unwraps`/`count_expects` | memmem substring | ★★ (comment+test FP) | **upgraded → region-aware** |

**Defect #1 (major, F1.2) — MI bands miscalibrated per-file.** Empirically, clean well-factored files scored **Fail**: `complexity.rs` MI=2, `unwrap_audit.rs` MI=20, `f1_2_maintainability.rs` MI=22 — all "unmaintainable" under the engine doc-comment's `≥85` bands. Root cause: the doc-comment bands describe the *pre-normalization* 0–171 SEI scale, but the formula returns the `×100/171`-normalized value, where the `16.2·ln(LLOC)` term crushes MI for any non-trivial file. Had I wired `MI≥85→good` blindly, F1.2 would have failed **every** real file — a worse gate than the stub (which at least gave clean code ~1.0). Fix: recalibrate to **radon's empirically-validated ranks for this identical formula** — A (very high) MI≥20, B 10–20, C<10 — blended 0.6/0.4 with the size-robust syn-AST `health_score` so a large-but-clean file is a "consider splitting" Warn, not a Fail. After: `unwrap_audit` **0.437→0.940 Pass**, `f1_2_maintainability` **0.453→0.907 Pass**, `complexity.rs` (819 lloc) 0.268→0.495 (correctly flagged as a large file). A deterministic `test_radon_calibration_not_miscalibrated` locks it. (F1.2 does **not** feed the 13-gate composite — R0.5 had already dropped the broken `f1_2=0.007` from 09_modularization — so this is a re-enablement, no composite risk.)

**Defect #2 (narrow, F1.6) — verified, documented.** Empirically `unwrap_audit.rs` (≈26 `.unwrap()`, almost all in its `#[cfg(test)]` module) scored F1.6=0.697 with prod `unwrap=2 expect=2 panic=1`. Grep-verified those 2/2/1 are the detector's **own byte-string needles** (`b".unwrap()"` line 24, `b".expect("` line 48, plus `count_prod_hazards`'s body) in production string literals — `code_regions` deliberately doesn't suppress prod strings (the detector-self-match class, cf. Part 9 f2_1). Confirmed *not* a masking bug (doc-comment `///` mentions correctly suppressed; the ~24 test-module unwraps correctly excluded). Documented as a known narrow limitation (rare outside detector/lint code, never a comment/test FP).

**Engine upgrade (REGRA #0).** F1.6's faithful D06 signal required a region-aware scanner: added `touring_analysis::quality::count_prod_hazards(source, lang) -> ProdHazards` — counts `.unwrap()`/`.expect(`/`panic!` **excluding comments and `#[cfg(test)]`/`#[test]`** via the Part 8 `code_regions` (reusable by all consumers, not just F1.6). Effectiveness proven by `test_prod_hazards_divergence_from_raw_scanner` (raw `count_unwraps`=3 vs prod=1 on prod+comment+test) and real dogfood (26 raw → 5 prod). The stub counted test unwraps as production hazards — exactly the FP D06 exempts.

**The three dims.** F1.2 = radon-calibrated MI (estimate_complexity) blended 0.6/0.4 with syn-AST health. F1.4 = SOLID *heuristic* from syn-AST `RustQualitySignals` (`semantic_complexity` as SRP proxy, `unsafe` as encapsulation smell) — honestly labelled as signal-grounded, not mechanically-decidable; non-Rust falls back to avg-complexity-per-fn. F1.6 = region-aware prod hazards + error-propagation bonus, with a labelled raw-substring standalone fallback.

**Gate (real exit codes).** unwrap_audit engine 15/15 · touring-quality lib **200 passed / 0 failed** (incl. `test_test_module_unwraps_do_not_penalise_production`, `test_radon_calibration_not_miscalibrated`, `test_unsafe_lowers_solid`) · touring-analysis lib **459 passed / 0 failed** · standalone `--no-default-features` builds · `clippy --workspace --all-targets -D warnings = 0` · `elite_aggregate composite=0.9453 Platinum` (**no regression**).

**Lessons.** (1) *A real-looking engine can still be ineffective* — the SEI MI is a genuine industry formula yet its per-file output was miscalibrated against its own doc bands; only an empirical run on real code exposed it. (2) *Always verify the engine before depending on it* (Gabriel's directive, now a standing reflex): rate the mechanism, run the baseline, **score real files**, and only then wire it — convergence/effectiveness is an output of that rigor, never assumed. (3) *Calibrate to the tool that uses the same formula* (radon for MI) rather than to a doc-comment's bands. (4) *Region-awareness is the faithful form of any prod-vs-test count* (D06 exempts test code; the raw scanner's comment/test FP is systemic). (5) *A failing test can be testing the wrong thing* — `test_low_mi_scores_below_clean` conflated F1.1 control-flow with F1.2 size; fixed to vary size, the dimension's actual signal.

**Files**: `touring-analysis/src/quality/{unwrap_audit.rs (ProdHazards + count_prod_hazards, region-aware, +7 tp/fp tests), mod.rs (re-export)}`, `touring-quality/src/verifications/{f1_2_maintainability,f1_4_solid,f1_6_error_handling}.rs` (all rewritten). **Stub→real: 6 P0 BLOCK + 6 WARN-tier (F1.1/F3.1/F1.8 + F1.2/F1.4/F1.6) real.** Doc Part 13.

## Part 14 — F1.5 tech-debt: stub → new dedicated `tech_debt` engine (Gabriel: "F1.5 tech-debt")

The next WARN-tier dim, and the engine-effectiveness reflex ([[feedback-verify-engine-effectiveness]], now standing) earned its keep three times in one slice. No existing engine covered D05: `antipatterns::detect_antipatterns` has the *code-debt* half (`todo!()`/`unimplemented!()`/`#[allow(dead_code)]`) but not region-aware and without the *comment markers* (TODO/FIXME/HACK/XXX) — the classic SQALE signal; `TdgReport` is a composite (complexity/coverage/…) that would conflate dimensions. So a **new dedicated `touring_analysis::quality::tech_debt`** engine (`analyze_tech_debt`), faithful to the D05 rule, with three precision levers the substring stub lacked:

1. **Word-boundary markers** — the stub's `raw.matches("BUG")` counted **`DEBUG`**; `\bBUG\b`/`\bTODO\b` whole-word matching makes `DEBUG`/`debugger`/`TODO_LIMIT`/`AUTODOC` no longer debt.
2. **Comment-scoped markers** — a marker is debt only inside a comment/test region (via `code_regions`, the *inverse* of `count_prod_hazards`): `// TODO` counts, `let todo = …` and `"TODO in a string"` do not.
3. **Code debt vs managed debt** — `todo!()`/`unimplemented!()` + `#[allow(dead_code/unused)]` counted in production only (not comment mentions); a `TODO(#123)` carrying an issue reference is *tracked* debt, weighted far lighter than a bare invisible `TODO` (SQALE: managed debt < hidden debt).

**Verification caught two defects (both mine) before they shipped.** (a) *My score multiplier was miscalibrated* — `×35` saturated small-file debt to 0.0, so `test_tracked_debt_lighter_than_untracked` failed (both 5-line fixtures clamped to 0 → `0 > 0` false); recalibrated to `×25` and rewrote the test to a **deterministic `score_tech_debt` comparison** (tiny file fixtures saturate density and hide the weight difference — the same trap as Part 13's `test_low_mi_scores_below_clean`). (b) *Detector self-match* — the empirical run scored `tech_debt.rs` itself **0.0** (markers=35: its doc-comments prose-mention "TODO/FIXME/HACK/XXX" and the test fixtures use them; code_debt=2/suppressions=2 are the `b"todo!("`/`b"allow(dead_code"` const needles). Same class as Part 9 f2_1 / Part 13 f1_6 unwrap_audit. Fixed with the established `is_detector_own_source` allowlist (mirrors `f2_6_config`: test/bench dirs + `touring-{analysis/src/quality,quality/src}`); documented trade-off: a genuine TODO inside those heavily-reviewed detector dirs is not flagged by F1.5.

**Empirical (real-code, the directive's confirmation step).** `clean_demo` (`const DEBUG_MODE` + `fn run_debugger`) → **1.0 Pass** (the stub would have failed it on `DEBUG`≈`BUG` — the end-to-end FP fix); `debt_demo` (2 `todo!()`/`unimplemented!()` + `allow(dead_code)` + `FIXME`) → **0.0 Fail**; allowlisted `tech_debt.rs` → **1.0 Pass** (evidence "detector-own-source … allowlisted").

**Gate (real exit codes).** tech_debt engine 10/10 · touring-quality lib **205 passed / 0 failed** (incl. `debug_identifiers_not_penalised`, `tracked_debt_lighter_than_untracked`, `detector_own_source_allowlisted`) · touring-analysis lib **469 passed / 0 failed** · standalone `--no-default-features` builds · `clippy --workspace --all-targets -D warnings = 0` · `elite_aggregate composite=0.9453 Platinum` (**no regression**).

**Lessons.** (1) When no single engine covers a dim, *build the dedicated one* (consistent with Parts 9-13: dep_health/config_security/coverage_artifact/module_cycles/count_prod_hazards) rather than borrowing a composite that conflates dimensions. (2) *Word-boundary + comment-scoping is the faithful form of marker detection* — the stub's bare substring counted `DEBUG` as a `BUG` marker. (3) *Markers vs hazards invert the region rule* — debt markers live in comments (count there), production hazards live in code (count there); the same `code_regions` serves both with opposite predicates. (4) *The detector-self-match allowlist is now a program-wide pattern* (`is_detector_own_source` in F2.1/F2.5/F2.6 and now F1.5). (5) *Recalibrate against the test, deterministically* — a saturating density on tiny fixtures hides real differences; test the score function on realistic inputs.

**Files**: `touring-analysis/src/quality/{tech_debt.rs NEW (10 tp/fp tests), mod.rs (mod + re-export)}`, `touring-quality/src/verifications/f1_5_tech_debt.rs` (rewritten + `is_detector_own_source`). **Stub→real: 6 P0 BLOCK + 7 WARN-tier (F1.1/F3.1/F1.8 + F1.2/F1.4/F1.6 + F1.5) real.** Doc Part 14.

## Part 15 — F1.3 duplication: stub → new dedicated `duplication` engine, + a dogfood TP the engine correctly flags but maturity declines (Gabriel "F1.3 duplication")

No existing engine measured code duplication (`dep_health` does duplicate *versions*; `TdgReport.duplication` is a `caller-supplied; MVP 1.0` placeholder) → built **new `touring_analysis::quality::duplication::analyze_duplication`**, **Type-1 (exact, modulo whitespace) block clone detection** (jscpd/SonarQube-CPD style). The substring stub counted every *isolated* line that recurred — so `Ok(())` / `let x = Vec::new();` idioms were scored as "duplication" while real copy-paste **blocks** were missed. The engine instead finds runs of **6+ consecutive meaningful production lines** recurring at a non-overlapping position: content-keyed windows (no hash-collision false clone), comments + blank/pure-structural (`}`, `);`) lines + `#[cfg(test)]` regions excluded (via `code_regions`, jscpd's test-exclusion convention), greedy non-overlapping occurrence count (a single long identical run is not a clone). Score on jscpd/D03 bands: <3% healthy→1.0, 3–8% warn→0.8–0.5, >8% fail→0.5–0.1.

**Engine-effectiveness verification ([[feedback-verify-engine-effectiveness]]) — clean this time, and it surfaced a real finding.** 8 engine tp/fp tests prove the levers (`isolated_repeated_idiom_is_not_duplication`, `copy_pasted_block_is_detected`, `comments_excluded`, `cfg_test_duplication_excluded`, `structural_and_blank_lines_ignored`). Empirical on real files: `duplication.rs`/`antipatterns.rs`/`f1_3_duplication.rs` → **1.0** (no detector-self-match — a structural detector embeds no clone literals, unlike the marker/secret detectors). But **`complexity.rs` → 0.1 Fail, ratio 24.2% (107 dup / 443 lines, 16 clone blocks)** — a dogfood finding the directive *required* me to classify TP-vs-FP before trusting. **CLI-verified TP** (not an engine bug): `b"*="` at lines 310/367/416/464 (4 language arms) + `+=`/`-=`/`==`/`<=` recurring 4–5× each — the `operator_inventory` polyglot tables share large verbatim operator blocks across the rust/python/ts/go arms. jscpd would flag this identically.

**Maturity: flag, don't blindly "fix".** Unlike the F4.5 (unused deps) / F1.8 (cycle) dogfoods where the fix was clear-cut and beneficial, here remediation is **declined with reasoning**: (1) it is a deliberate **zero-alloc** design (`&'static [&'static [u8]]` per language) — deduping would force runtime concatenation (losing zero-alloc) or a macro (added complexity) for a metric; (2) it is exactly the case the D03 rule itself warns against — *"DRY de conhecimento, não de código acidental — trechos iguais por coincidência (não compartilham razão de mudar) NÃO devem ser fundidos"*: the operators recur because each language *has* them, with no shared reason-to-change. The dimension did its job (flag, correctly); the human judgment is "acceptable". This is the dim demonstrating value *and* the discipline not to refactor clarity into a metric.

**Gate (real exit codes).** duplication engine 8/8 · touring-quality lib **209 passed / 0 failed** (incl. `repeated_idiom_not_penalised`, `copy_paste_block_lowers_score`, `score_duplication_bands`) · touring-analysis lib **477 passed / 0 failed** · standalone `--no-default-features` builds · `clippy --workspace --all-targets -D warnings = 0` · `elite_aggregate composite=0.9453 Platinum` (**no regression** — F1.3 doesn't back the 13-gate).

**Lessons.** (1) *Block, not line, is the faithful unit of duplication* — the stub's isolated-line count was an idiom FP-machine that missed real clones. (2) *A structural detector has no self-match* (no embedded literals), so no allowlist is needed — verify the class before assuming the F2.x/tech_debt pattern applies. (3) *Verify a dogfood Fail is TP before acting* — CLI evidence (`grep` the recurring block) distinguished a real clone from a hypothetical engine FP. (4) *Not every flagged duplication is debt to pay* — D03's own coincidental-duplication caveat + a zero-alloc trade-off make "decline with documented reasoning" the mature call; the metric flags, the human decides.

**Files**: `touring-analysis/src/quality/{duplication.rs NEW (8 tp/fp tests), mod.rs (mod + re-export)}`, `touring-quality/src/verifications/f1_3_duplication.rs` (rewritten). **Stub→real: 6 P0 BLOCK + 8 WARN-tier (F1.1/F3.1/F1.8 + F1.2/F1.4/F1.6 + F1.5 + F1.3) real.** Doc Part 15.

## Part 16 — F1.7 boundaries: stub → new visibility-aware `boundaries` engine, recalibrated from real-file testing (Gabriel "F1.7 boundaries", + context7)

D07/F1.7 measures component **boundaries / encapsulation**. The stub counted lines beginning `pub fn`/`pub struct`/… via substring — so it was **blind to `pub(crate)`/`pub(super)`** (restricted visibility = *good* encapsulation, counted as neither leak nor credit), blind to **`pub` struct fields** entirely (the C-STRUCT-PRIVATE signal), and used an arbitrary `pub_count / 50` threshold that punished any large legitimate public API.

**context7 (Gabriel asked).** Queried `/websites/rust-lang_github_io_api-guidelines` — the Future-proofing checklist ranks boundary signals exactly as D07 does: **(1) private struct fields (C-STRUCT-PRIVATE)** — `pub` fields leak the representation, the strongest/least-ambiguous signal; (2) sealed traits / minimal public surface; (3) newtype encapsulation.

**Engine.** `touring-analysis` is **zero-syn** (its engines are line/byte scanners + `code_regions`, not `syn`), so — consistent with `duplication`/`tech_debt` and D44 dep-minimization — built a **new zero-dep `touring_analysis::quality::boundaries::analyze_boundaries`**: a brace-aware scanner classifying every column-0 top-level item's visibility (`Vis::{Public, Restricted, Private}` via `vis_prefix` — handles `pub(crate)`/`pub(super)`/`pub(in)`, `async`/`unsafe`/`const`/`extern` qualifiers, `macro_rules!`), tracking struct bodies to count `pub` fields, reusing `code_regions` to drop comments + `#[cfg(test)]`. `score_boundaries` = field-leak (C-STRUCT-PRIVATE) ⊕ exposure-ratio; `pub(crate)` is excluded from the exposure numerator (the credit the stub never gave). Scope honestly documented: intra-file surface only — cross-module "pub with 0 consumers" is wiring (F1.8 / `touring wiring impact`); inline submodules + tuple-struct fields not classified.

**Engine-effectiveness verification ([[feedback-verify-engine-effectiveness]]) — caught a systematic FP, recalibrated before depending.** 10 engine + 6 verifier tp/fp tests pass (restricted-not-a-leak, pub-field-bag-penalised, public-fn-API-not-over-punished, comments/test excluded). Then I **scored 8 real files** (the directive's "score REAL files" step): logic-pure files (`code_regions.rs`/`complexity.rs`/`aggregate.rs` — `aggregate` showing the `pub(crate)` credit working) → **1.000 Pass**; but 4/8 (`lib.rs` 0.482 **Fail**, `boundaries.rs`/`tech_debt.rs`/`duplication.rs` 0.50–0.67 Warn) were penalised **only** for carrying `*Report`/`DimScore` **result DTOs** — public read-only data types where `pub` fields are idiomatic Rust. Unlike the F1.3 dogfood (one file, deliberate design), this was a *systematic* pattern (every DTO file), i.e. over-penalization, not isolated TP. Fix: **field weight 0.5 → 0.4** so a *pure* public data bag (all-`pub` fields **and** exposure 1.0, e.g. a lone `pub struct Config{pub,pub,pub}`) still Fails (~0.40 via the exposure penalty), while a DTO mixed among private logic lands in advisory Silver. Re-verified: `lib.rs` 0.482 Fail → **0.582 Warn**, `boundaries.rs` → 0.733, `tech_debt.rs` → 0.600, logic-pure stays 1.000. The engine discriminates without being noisy.

**Self-dogfood (REGRA #21).** The post-edit hook flagged my own `analyze_boundaries` at **CC=18 > 15** — I can't ship a quality engine that fails the complexity dim. Extracted a private `Scanner` struct (`feed`/`feed_struct_body`/`feed_top_level`), each method CC < 10; the hook re-reported "simple".

**Gate (real exit codes).** boundaries engine **10/10** · verifier F1.7 **6/6** · touring-quality lib **213/0** · touring-analysis lib **487/0** · standalone `--no-default-features` builds · `clippy --workspace --all-targets -D warnings = 0` · `elite_aggregate composite=0.9419 tier=Platinum`. The composite moved **0.9453 → 0.9419** — *not* a regression of tier (still Platinum, gate exit 0) but the honest **de-inflation** the R0.5 thesis predicts: a faithful boundary dim surfaces the pub-field DTO surface the stub mistakenly scored as clean. REGRA #0: the 3 new `pub` symbols (`BoundaryReport`/`analyze_boundaries`/`score_boundaries`) are consumed by the F1.7 verifier (not orphans).

**Lessons.** (1) *Visibility is a three-way classification, not a substring* — the stub conflated `pub fn` detection with boundary measurement and was blind to the two signals that matter most (`pub(crate)` credit, `pub` fields). (2) *A faithful dim de-inflates the composite, and that is correct* — 0.9453→0.9419 is the stub's masked boundary debt becoming visible, exactly the W0/R0.5 finding. (3) *Real-file testing is the recalibration oracle* — fixtures passed; only scoring real DTO files exposed the over-penalization, and the directive forced that step before I trusted the weight. (4) *Dogfood the engine on its own complexity dim* — D01 caught CC=18 in the very engine that scores boundaries; fix before ship.

**Environment note (REGRA #21, surfaced not silenced).** A holon PreToolUse hook (`generator_health_client.py`) fails `ModuleNotFoundError: No module named 'capnp'` — a missing `pycapnp` dep, orthogonal to this slice, fail-open (did not affect any gate). Installing it needs system `libcapnp-dev`; deferred to Gabriel rather than mutating the Python env unprompted.

**Files**: `touring-analysis/src/quality/{boundaries.rs NEW (10 tp/fp tests, `Scanner`/`analyze_boundaries`/`score_boundaries`), mod.rs (mod + re-export)}`, `touring-quality/src/verifications/f1_7_boundaries.rs` (rewritten to delegate). **Stub→real: 6 P0 BLOCK + 9 WARN-tier (F1.1/F3.1/F1.8 + F1.2/F1.4/F1.6 + F1.5 + F1.3 + F1.7) real.** Doc Part 16.

## Part 17 — F4.1 idioms: stub → new **polyglot** `idioms` engine (7 languages), "o mais completo possível" (Gabriel "F4.1 idioms… não só clippy para rust, mas os equivalentes para as outras linguagens… o mais completo possível" + context7)

D40/F4.1 measures **idiomaticity** — does code use each language's idiomatic construct vs a legacy/transplanted form? The stub counted `let ` + `match ` occurrences and returned `1.0` above five — a metric with no relationship to idiomaticity. Gabriel scoped this explicitly: **polyglot** (clippy *and* its equivalents) and **as complete as possible**.

**context7 (Gabriel asked).** Validated the canonical idiom lints: clippy (`/websites/rust-lang_github_io_rust-clippy_stable`) — `len_zero` (`.len()==0`→`is_empty`), `bool_comparison` (`==true`), `comparison_to_empty` (`==""`), `ptr_arg` (`&Vec`/`&String`); ruff (`/websites/astral_sh_ruff`) — E711 (`==None`), E712 (`==True/False`), E721 (`type()==`), E731 (`=lambda`), E722 (bare `except:`), mutable-default-arg.

**Engine** — NEW zero-dep `touring_analysis::quality::idioms::analyze_idioms`: a `~50`-rule catalogue across **7 languages** (each a high-confidence subset of its lint oracle): **Rust/clippy** (len_zero family, bool_comparison family, ptr_arg, `#[allow(clippy::)]` suppression, get_first, iter_nth_zero, redundant_pattern_matching), **Python/ruff** (E711/E712/E721/E731/E722, `range(len())`, `.has_key`, `import *`), **TypeScript+JavaScript/ESLint** (no-var, no-array-constructor, no-eval, no-explicit-any, ban-ts-comment), **Go/go-vet** (`interface{}`→`any`, `errors.New(fmt.Sprintf())`), **C++/clang-tidy** (`using namespace std`, `NULL`→`nullptr`, C-cast), **Java** (legacy boxing ctors, `Vector`/`Hashtable`, `printStackTrace`). Reuses `code_regions` (comments + `#[cfg(test)]` excluded; `// use == None` documentation never flagged). **Key design finding**: a bare `== null` needle would match *inside* the correct strict `=== null` → FP, so JS/TS loose-equality (`eqeqeq`) needs a **char-aware** scanner (`count_loose_equality`: a `==` whose neighbour is `=` is part of `===` and never counted); `== None` (Python) / `== true` (Rust) stay substring-safe because those languages have no `===`.

**Engine-effectiveness verification ([[feedback-verify-engine-effectiveness]]) — caught two things.** 11 engine + 13 verifier tp/fp tests pass (rust/python/ts/go, eqeqeq-loose-not-strict). Scoring real files: (1) **detector self-match** — `idioms.rs` → 0.402 Fail (30 "violations") because the engine's own byte-string needles (`b".len() == 0"`, `b"== None"`, …) are data the scanner matches in its own source; fixed with `is_detector_own_source` (the program-wide allowlist mirroring F1.5/F2.x; `idioms.rs` lives under `touring-analysis/src/quality`) → 1.000. (2) **FN check** — the 4 real workspace `.py` scripts all scored 1.000; verified via `grep` that they contain **0** `==None`/`==True`/`range(len(`/bare-`except:` → the 1.000 is a true positive (genuinely idiomatic), not a false negative. Real `.rs` (clippy-clean by RBP-01) and `.py` → 1.0 (no FP); the relative behaviour (non-idiomatic < idiomatic, loose `==` < strict `===`) is test-proven. SCALE=8 kept (test-validated; no real idiom-debt file in the clippy-clean workspace to recalibrate against).

**Self-fix (REGRA #21).** clippy `unnecessary_sort_by` on my `findings.sort_by(|a,b| b.1.cmp(&a.1))` → `sort_by_key(|f| Reverse(f.1))`.

**Environment task (Gabriel "instale libcapnp-dev de sistema").** `libcapnp-dev` was already present (`capnp 1.0.1`); the holon hook's `ModuleNotFoundError: capnp` was the missing **Python binding** → installed `pycapnp 2.2.3` into `~/.local` (`--user --break-system-packages`, not touching the OS site-packages; PEP 668). `import capnp` now resolves and `generator_health_client.py` reaches its argparse (was the import that failed).

**Gate (real exit codes).** idioms engine **11/11** · verifier F4.1 **13/13** · touring-quality lib **218/0** · touring-analysis lib **498/0** · standalone `--no-default-features` builds · `clippy --workspace --all-targets -D warnings = 0` · `elite_aggregate composite=0.9419 tier=Platinum` (no regression — F4.1 doesn't back the 13-gate). REGRA #0: the 3 new `pub` symbols (`IdiomReport`/`analyze_idioms`/`score_idioms`) are consumed by the F4.1 verifier (not orphans).

**Lessons.** (1) *A needle can match the correct form* — `== null` ⊂ `=== null`, so loose-equality must be char-aware; substring works only where the language has no stricter form. (2) *A catalogue detector self-matches its own needles* — the same `is_detector_own_source` allowlist as the marker/secret detectors applies. (3) *1.0 on real files is not self-evidently a TP* — `grep` the patterns to rule out a false negative (the directive's "score real files" includes proving the clean score is earned). (4) *Polyglot via a per-language rule table + `code_regions`* scales to 7 languages cheaply and stays honest about being a lint-subset, not a replacement.

**Files**: `touring-analysis/src/quality/{idioms.rs NEW (~50 rules, 11 tp/fp tests, char-aware `count_loose_equality`), mod.rs (mod + re-export)}`, `touring-quality/src/verifications/f4_1_idioms.rs` (rewritten to delegate + `is_detector_own_source`). **Stub→real: 6 P0 BLOCK + 10 WARN-tier (F1.1/F3.1/F1.8 + F1.2/F1.4/F1.6 + F1.5 + F1.3 + F1.7 + F4.1) real.** Doc Part 17.

## Part 18 — F1.9 api-design: stub → NEW **polyglot** `api_design` engine (7 langs) + a C/C++ `code_regions` root-cause fix (Gabriel "F1.9 api-design… melhores práticas no context7… Premium de Elite de Mercado em todos os sentidos… não pode ser somente para rust, precisa ser para todas as demais principais linguagens")

**D09/F1.9 = public-API *contract* design.** The stub counted `pub fn`/`pub struct`/`pub trait` and **penalised a wide public surface** — an *anti-metric*: a large well-designed API scored *worse* than a tiny badly-designed one (the inverse of "easy to use right"). Gabriel scoped it like F4.1: **polyglot, all major languages**, context7-grounded.

**context7** `/rust-lang/api-guidelines`: C-GETTER (a getter for `first` is `first()`, never `get_first()`), C-CONV (`as_` borrows cheaply / `to_` is an expensive borrow / `into_` consumes `self`), C-DEBUG ("all public types should implement Debug"), C-BUILDER (builder for many params), and **C-GOOD-ERR** ("an error type is any E in a `Result<T,E>` of a public fn… should implement `std::error::Error`" — `String` does **not**, so `Result<_, String>` is the smell). Sealed traits / `#[non_exhaustive]` noted for future-proofing.

**Engine** NEW zero-dep `touring_analysis::quality::api_design::analyze_api_design` — a **7-language** contract-smell catalogue, **disjoint from F4.1 idioms by construction** (idioms = local *style*; api-design = public *contract*): **Rust** (`Result<_, String>` via a top-level-generic-arg parser so `Result<String, MyErr>` is NOT flagged; `pub fn get_*` with a `mut`/`unchecked`/`or`/`many`/… allowlist; `into_*(&self)` / `as_*(self)`→owned C-CONV; `pub struct/enum` without `Debug` via attribute-carry lookback + manual-`impl Debug` check; `new()` >5 params), **Python** (mutable default `=[]`/`={}`, `raise Exception(`, `def` >5 non-`self` params), **TS/JS** (`throw "string"`, function/constructor >4 params), **Go/Effective Go** (`GetX()` getter prefix, library `panic(`), **Java** (`public` instance field, `throws Exception/Throwable`), **C++/Effective C++ Item 2** (function-like `#define X(...)`). Reuses `code_regions` (comments + `#[cfg(test)]` excluded). Shared helpers: `count_params` (balances `()[]{}<>`), `last_generic_arg`, `wide_params_on_keyword` (unifies Rust `new` / Python `def` / JS `function`+`constructor`).

**Root-cause fix surfaced by the C++ test (REGRA #0 potentialize).** `cpp_function_macros` returned `[]` for `#define MAX(a,b)`: `code_regions` had no `cpp` arm → fell to `GENERIC` whose `line: &["//", "#"]` treats **`#` as a comment** — wrong for C/C++ (and Java), where `#` is a *preprocessor directive*, so it was wrongly suppressing every `#define`/`#include` line for **all** dims analysing C/C++/Java (idioms, security, …). Added a `CPP` `LangSyntax` (`//` + `/* */`, no `#`) and mapped `cpp|c++|cc|cxx|c|h|hpp|java → &CPP`. Global correctness fix, not a local patch.

**Engine-effectiveness [[feedback-verify-engine-effectiveness]] — scored REAL files, caught a precise FP.** Fixtures (20 engine + 7 verifier tp/fp) pass, but the directive's "score real files" step ran the CLI on 6 real files: `inferlets/*.py`, `touring-{ast,server,hooks}/lib.rs` → **1.000** (no FP on large real public APIs; `grep` proved the Python files have 0 mutable-defaults/broad-raise → 1.0 is *earned*, TP-clean), `pipeline.rs` → **0.993** (2 `missing_debug` = **TP**: `AnalysisPipeline`/`…Builder` genuinely lack `Debug`, no manual impl), `engine.rs` → **0.979** with one finding `as_str(self) -> &'static str`. That last is a **precise FP mode** `[FACT 1.0]`: a `Copy` fieldless enum's `as_str(self) -> &'static str` *borrows static data* and is idiomatic — `as_*(self)` is only a real C-CONV misuse when it returns an **owned** value (that's an `into_*`). Refined `rust_conv_violations` with `returns_owned_on_line` (flag `as_*(self)` only when the `-> T` return is not a `&`-reference) → engine.rs **0.979→1.000**, FP gone, the genuine `as_thing(self) -> V` still flagged. Not an over-penalization pattern like F1.7's DTOs (SCALE=6.0 kept).

**Self-fix (REGRA #21).** `clippy --workspace --all-targets` (stricter than `cargo test`'s expected-type coercion) flagged `Option<&[u8]> == Some(b"String")` (`b"String"` is `&[u8; 6]`) → made it explicit `Some(b"String".as_slice())`.

**Gate (real exit codes).** api_design engine **20/20** · verifier F1.9 **7/7** · touring-quality lib **223/0** · touring-analysis lib **518/0** (incl. `code_regions` no-regress) · standalone `--no-default-features` builds · `clippy --workspace --all-targets -D warnings = 0` · `elite_aggregate composite=0.9419 tier=Platinum` (no regression — F1.9 is ADVISORY, not in the 13-gate; the `code_regions` change did not move any gate). REGRA #0: the 3 new `pub` symbols (`ApiDesignReport`/`analyze_api_design`/`score_api_design`) are consumed by the F1.9 verifier (not orphans).

**Lessons.** (1) *An anti-metric is worse than a stub* — the old F1.9 rewarded a smaller public surface regardless of design; a faithful contract dim had to invert that. (2) *A new language detector can surface a shared-infra bug* — the C++ macro test exposed `code_regions` mis-treating `#` as a comment for the whole GENERIC fallback (C/C++/Java); fixing the root (REGRA #0) helps every dim, not just F1.9. (3) *Real-file scoring is where precision is won* — `as_str(self) -> &'static str` passed every fixture yet was an FP on real code; the fix (`returns_owned`) made the signal *more* meaningful (only the genuine "consuming + owned-return = should-be-`into_`" case). (4) *`cargo test` coercion ≠ clippy strictness* — `Some(b"lit")` array/slice mismatch compiled under test inference but failed `--all-targets`; always run the workspace clippy gate. (5) *Disjoint dims by construction* — api-design scores the contract, idioms the style; choosing contract-only signals (error types, getter naming, field exposure, builder) avoids double-counting with F4.1.

**Files**: `touring-analysis/src/quality/{api_design.rs NEW (7-lang, 20 tp/fp tests, `returns_owned_on_line` precision refinement), code_regions.rs (new `CPP` arm — C/C++/Java `#`-not-a-comment fix), mod.rs (mod + re-export)}`, `touring-quality/src/verifications/f1_9_api_design.rs` (rewritten to delegate + `is_detector_own_source`). **Stub→real: 6 P0 BLOCK + 11 WARN/ADVISORY-tier (… + F1.9) real.** Doc Part 18.

## Part 19 — F4.4 modernization: stub → NEW **polyglot** `modernization` engine (7 langs) + an honest elite-composite finding (Gabriel "F4.4 modernization", standing polyglot mandate)

**D43/F4.4 = adoption of newer language/edition features replacing superseded ones.** The stub counted `try!` + `extern crate` only (flat 0.5/1.0). Replaced by `touring_analysis::quality::modernization::analyze_modernization`, **disjoint from F4.1 idioms by construction**: idioms scores per-version *style* (`.is_empty()`, `===`, `var`, `interface{}`, `typedef`, `NULL`), modernization scores *version adoption* — so those idiom-owned smells are **not** repeated here.

**context7** `/rust-lang/rust`: `try!(e) => match e { Ok(e) => e, Err(e) => return Err(e) }` (legacy; rustfmt `use_try_shorthand` rewrites to `?`), and `extern crate whiskers; // needed as ui test defaults to edition 2015` (confirming `extern crate` is an edition-2015 construct, unnecessary in 2018+). Edition migration is lint-driven (`FutureIncompatibleInfo`, `EditionError 2018`).

**Engine** NEW zero-dep, **7-language**, version-anchored: **Rust** (edition 2018 / 1.80: `try!(`→`?`, `extern crate X`→paths with a sysroot allowlist `alloc`/`core`/`test`/`proc_macro`/`std`, `#[macro_use]`→`use`, `lazy_static!`→`LazyLock`/`OnceLock`), **Python** (Py2→Py3: `super(Cls, self)`→`super()`, `(object):` redundant base), **TS/JS** (ESM/ES6+: `require(`→`import`, `module.exports`→`export`, `Object.assign({`→spread, `indexOf(..) !== -1`→`includes`), **Go** (1.16/1.20: `ioutil.`→`io`/`os`, `rand.Seed(`), **Java** (8/16: anonymous functional-interface class→lambda via `new Runnable()`/`new Comparator<`/…, `Collectors.toList()`→`.toList()`), **C++** (`std::bind(`→lambda, C headers `<stdio.h>`→`<cstdio>`). Mostly pure-needle + 3 custom detectors (`rust_extern_crate` allowlist, `py_super_with_args`, `jsts_indexof_includes`). Reuses `code_regions` (the Part-18 `CPP` fix means `#include` lines are seen, not suppressed — a regression test guards this).

**Engine-effectiveness [[feedback-verify-engine-effectiveness]] — honest "all-modern" outcome.** 14 engine + 7 verifier tp/fp tests pass (each language: legacy→flagged, modern→clean, relative). Scoring real files: `inferlets/*.py` + `touring-{ast,server,hooks}/lib.rs` + `engine.rs` → **1.000**. A workspace-wide grep for every needle (`try!`, `lazy_static!`, `extern crate` non-sysroot, `ioutil`, `super(args)`) found **zero** production hits (the only `#[macro_use]` is in the F4.4 verifier's own doc/fixtures, allowlisted) — the workspace is genuinely edition-2024-modern, so 1.0 everywhere is **TP-earned**, and the engine *firing* on legacy is proven by the 14 unit tests (same honesty as F4.1's clippy-clean-workspace case). SCALE=6.0 kept (no real legacy file to recalibrate against).

**Honest elite finding (REGRA #21) — the composite moved, and why it is NOT an F4.4 regression.** `elite_aggregate` went **0.9419→0.9284** (still Platinum). Arithmetic pins it to gate **14_craftsmanship** dropping from ~0.73 to its **0.5 WARN** value (weight 0.7 × ~0.23 ≈ the 0.16 weighted-sum delta; composite = 10.9546/11.8 = 0.9284, exact). Running `craftsmanship_tdg_gate.py` in isolation: **167/308 files** fail `cognitive_score > 0.7`, spread across **25 crates** (touring-intelligence 28 — the `rl/**` ACO/bandit/qtable files, touring-hooks-core 17, touring-server 14, …). This is **pre-existing workspace cognitive-complexity debt**; the gate reads live `cognitive_score` from the index, so the earlier 0.9419 readings (F1.7/F4.1/F1.9) were taken with a **partially-warm index** that under-counted — 0.9284 is the fully-measured truth. Verified **none of the F4.4 slice's files** are in the failure set: `modernization.rs` grade **B** (cognitive < 0.7), `f4_4_modernization.rs` and `touring-analysis/quality/mod.rs` not flagged. (The grep did surface `touring-quality/src/{lib.rs, f2_4_secrets.rs, verifications/mod.rs}` — but those are pre-existing files this slice never touched; F4.4 edits `touring-analysis/quality/mod.rs`, not `touring-quality/verifications/mod.rs`.) So the F4.4 change introduced **zero** regression; the composite simply reflects the harness now honestly measuring a real, pre-existing, 25-crate complexity debt — a candidate for a *separate* dedicated cognitive-refactor effort, not part of the stub→real program.

**Gate (real exit codes).** modernization engine **14/14** · verifier F4.4 **7/7** · touring-quality lib **228/0** · touring-analysis lib **532/0** · standalone `--no-default-features` builds · `clippy --workspace --all-targets -D warnings = 0` · `elite_aggregate composite=0.9284 tier=Platinum` (above the Gold floor; the delta from 0.9419 is the index-dependent craftsmanship gate measuring pre-existing debt, not F4.4). REGRA #0: the 3 new `pub` symbols (`ModernizationReport`/`analyze_modernization`/`score_modernization`) are consumed by the F4.4 verifier (not orphans).

**Lessons.** (1) *Modernization ≠ idioms* — keep them disjoint by anchoring modernization to a language *version/edition* boundary (`try!`(2015), `lazy_static!`→`LazyLock`(1.80), `super`(Py3), `ioutil`(Go 1.16)) and leaving per-version style to F4.1; choosing distinct needles avoids double-counting. (2) *A modern codebase scores a modernization dim at 1.0, and that is correct* — prove it with a workspace-wide needle grep (zero hits) + unit tests that the engine fires on legacy, rather than assuming. (3) *The elite composite has a daemon-index-dependent component* — `craftsmanship_tdg_gate` reads live `cognitive_score`, so "no-regress X" claims are only as stable as the index warmth; the honest current measure is 0.9284, and the move is pre-existing 25-crate complexity debt, not this slice. (4) *Run the swing gate in isolation* — `elite_aggregate --json` + the single gate script localised a 0.0135 composite move to one gate and 167 named files in minutes, turning "did I regress?" into a data-backed "no, here is the pre-existing cause".

**Files**: `touring-analysis/src/quality/{modernization.rs NEW (7-lang, 14 tp/fp tests + a `code_regions` `#include` regression guard), mod.rs (mod + re-export)}`, `touring-quality/src/verifications/f4_4_modernization.rs` (rewritten to delegate + `is_detector_own_source`). **Stub→real: 6 P0 BLOCK + 12 WARN/ADVISORY-tier (… + F4.4) real.** Doc Part 19.

## Part 20 — F1.11 design-patterns: stub → NEW **polyglot** `design_patterns` engine (7 langs) detecting GoF transplants / ownership smells (Gabriel "F1.11 design-patterns", standing polyglot mandate)

**D11/F1.11 = the idiomatic *structural pattern* choice.** The stub scored the `impl`/`trait` ratio (unrelated to pattern quality). Replaced by `touring_analysis::quality::design_patterns::analyze_design_patterns`, **disjoint from F4.1 idioms (style) / F1.9 api-design (contract) / F4.4 modernization (version)** — it scores the *pattern* choice.

**context7** `/rust-unofficial/patterns`: the explicit anti-patterns are **Deref polymorphism** (`impl Deref for Bar` to emulate inheritance) and **clone-to-satisfy-the-borrow-checker** (though `Rc::clone` is cheap/acceptable), against the idiomatic **newtype**, **RAII guard** (Drop+Deref on a wrapper), **typestate-via-generics**, and **sealed trait**. Two precision calls `[FACT 1.0]`: **`transmute` is dropped** (already owned by `antipatterns.rs` — verified by grep, VGP V2), and **`impl Deref for` is dropped** (a scanner cannot tell the inheritance-emulation smell from a legitimate smart-pointer/newtype `Deref`, so flagging it would be the systematic FP the F1.7-DTO lesson warns against).

**Engine** NEW zero-dep, **7-language** anti-pattern catalogue: **Rust** (`static mut` Singleton-via-unsafe-global→`OnceLock`, `Rc<RefCell<` shared-mutable overuse, `unsafe impl Send/Sync` manual thread-safety, `.downcast`/`dyn Any` type-erasure→enum/generic), **Python** (`global` statement, `__new__` override), **TS/JS** (`getInstance(` Singleton, `as unknown as` escape hatch), **Go** (`func init()` hidden global init, `reflect.`), **Java** (`getInstance(`, `FactoryFactory`, `Cloneable`, `extends Thread` — Java is genuinely GoF-heavy), **C++** (`getInstance(`, `dynamic_cast`, `friend class`). Pure-needle + 2 custom: `rust_unsafe_marker_impls` (line-scoped `unsafe impl` + `Send for`/`Sync for`, catching the generic `unsafe impl<T> Send for X<T>` form) and `python_global_statement` (line-start `global ` so `myglobal = …` is not a false match).

**Engine-effectiveness [[feedback-verify-engine-effectiveness]] — fires on real smells, excludes comments, no over-penalization.** 12 engine + 6 verifier tp/fp tests pass. Real-file scoring (workspace has 0 `static mut` — edition 2024 hard-errors it, 2 `Rc<RefCell<`, 9 `unsafe impl Send/Sync`): `generator_health.rs` → **0.992** (1 `Rc<RefCell<` finding — TP, negligible impact), `touring-code/src/lib.rs` → **1.000** (its `unsafe impl Send/Sync` lives in a `// NOTE:` comment → correctly excluded by `code_regions`, *not* a false negative), `incremental_pipeline.rs` (the sanity case) → **0.993 with 4 findings** (the detector *does* fire on real `unsafe impl Send/Sync` code), clean files (`touring-ast/lib.rs`, Python) → **1.000**. Every score is Pass 0.99+ → ADVISORY working as intended, no F1.7-style over-penalization. The comment-vs-code distinction (1.000 on the commented mention, 0.993 on the real impl) is the precise proof the engine is correct. SCALE=6.0 kept.

**Gate (real exit codes).** design_patterns engine **12/12** · verifier F1.11 **6/6** · touring-quality lib **232/0** · touring-analysis lib **544/0** · standalone `--no-default-features` builds · `clippy --workspace --all-targets -D warnings = 0` · `elite_aggregate composite=0.9284 tier=Platinum` — **identical to the post-F4.4 reading**, which both confirms F1.11 introduces zero regression *and* corroborates Part 19's finding (the 0.9419→0.9284 move was the index-warming craftsmanship gate, now stable). REGRA #0: the 3 new `pub` symbols (`DesignPatternReport`/`analyze_design_patterns`/`score_design_patterns`) are consumed by the F1.11 verifier (not orphans).

**Lessons.** (1) *Drop a signal another detector owns* — `transmute` is `antipatterns.rs`'s; a grep before writing (VGP V2) avoided double-counting. (2) *Drop a signal too FP-prone to be precise* — `impl Deref for` flags every smart-pointer/newtype guard, the same systematic-FP shape as F1.7's DTOs; omitting it (with an honest doc note) beats a noisy dim. (3) *The comment-vs-code test is the cleanest effectiveness proof* — the same `unsafe impl Send` string scored 1.000 in a comment and 0.993 in real code, proving both that the engine fires and that `code_regions` excludes documentation. (4) *A stable composite across two slices corroborates a prior root-cause* — 0.9284 unchanged from F4.4→F1.11 confirms the craftsmanship gate (not my engines) drove the earlier move.

**Files**: `touring-analysis/src/quality/{design_patterns.rs NEW (7-lang, 12 tp/fp tests incl. generic-`unsafe impl` + `myglobal` FP guards), mod.rs (mod + re-export)}`, `touring-quality/src/verifications/f1_11_patterns.rs` (rewritten to delegate + `is_detector_own_source`). **Stub→real: 6 P0 BLOCK + 13 WARN/ADVISORY-tier (… + F1.11) real.** Doc Part 20.

## Part 21 — F2.2 input-validation: stub → NEW **polyglot** `input_validation` engine (7 langs), the first security WARN dim + a `WorstOf`-aware precision discipline (Gabriel "F2.2 input-validation (allowlist no boundary, path-traversal — security polyglot)")

**D15/F2.2 = boundary input validation** — the first **security WARN-tier** dim migrated (the 6 P0 BLOCK security dims F2.1/F2.4/F2.5/F2.6/F4.3/F4.5 were done in earlier parts). The stub scored `validate`/`sanitize`/`.parse()` keyword density. **Critical roll-up difference**: F2.2 is `AggKind::WorstOf` (workspace score = the *worst* file — one unvalidated boundary is a vulnerability), which **amplifies any false positive to the whole scope**, so the catalogue is kept high-precision and the Rust signals (the workspace language) conservative.

**context7** `/owasp/cheatsheetseries`: allowlist regex `^[a-z0-9]{3,10}$` (the good form), path traversal via `realpath` + `str_starts_with($realBase)` (so the *blocklist* `.replace("../")` is the anti-pattern), and `pickle.loads(data)` / `ObjectInputStream`+`resolveClass` explicitly flagged as insecure deserialization (CWE-502). **Disjoint from F2.1 OWASP** (injection sinks via the `SecurityAnalyzer` ast-grep catalogs — verified by grep, VGP V2): F2.2 scores boundary validation, not injection.

**Engine** NEW zero-dep, **7-language**, all-needle (+1 structural): **Rust** (`.replace("../"` blocklist CWE-22, `from_utf8_unchecked`), **Python** (`pickle.load(s)`/`yaml.load(`/`marshal.loads` CWE-502, `.replace("../"`), **TS/JS** (`dangerouslySetInnerHTML`/`document.write(` DOM-XSS, `.replace("../"`), **Go** (`template.HTML(`/`.JS(`/`.URL(` escaping bypass), **Java** (`ObjectInputStream`/`.readObject(` CWE-502), **C/C++** (`gets(` CWE-242, `strcpy`/`strcat`/`sprintf`, `scanf("%s"` CWE-120). SCALE=8.0 (steeper than the style dims — a single deser/unbounded smell is a real exposure, and `WorstOf` surfaces it).

**FP found by a test, fixed by a word boundary `[FACT 1.0]`.** The C/C++ test failed: the `gets(` needle also matches the **safe `fgets(`** (suffix collision `gets(` ⊂ `fgets(`), so `fgets(buf, …)` was wrongly flagged — and under `WorstOf` that single FP would tank the whole C/C++ scope. Fixed with a structural `cpp_bare_gets` (skip when the preceding byte is an identifier char, so `fgets`/`Wgets`/any `…gets(` is excluded; only the bare `gets(` counts). The safe-form guards generalise: `yaml.load(` does not match `yaml.safe_load(`, `scanf("%s` does not match the bounded `scanf("%31s"`, and `pickle.load(` does not match `pickle.loads(` (distinct needles).

**Engine-effectiveness [[feedback-verify-engine-effectiveness]] — clean workspace, fires test-proven.** 10 engine + 7 verifier tp/fp tests pass. Ground-truth grep before writing: the workspace has **0** `.replace("../"`, `from_utf8_unchecked`, `pickle`/`yaml.load`, etc. (a Rust edition-2024 systems codebase with safe Python inferlets) → every real file scores **1.000**, and `score crates/touring-analysis/src --dims F2.2` (the `WorstOf` crate roll-up) = **1.0** — no FP-tank. The engine *firing* on real vulns is proven by the 10 unit tests, each language's anti-pattern→flagged and its safe form→clean (the same honesty as F4.4/F1.11's clean-workspace case, but here the `WorstOf` roll-up makes the zero-FP property load-bearing).

**Gate (real exit codes).** input_validation engine **10/10** · verifier F2.2 **7/7** · touring-quality lib **237/0** · touring-analysis lib **554/0** · standalone `--no-default-features` builds · `clippy --workspace --all-targets -D warnings = 0` · `elite_aggregate composite=0.9284 tier=Platinum` (no regression — F2.2 is not in the 13-gate; stable across F4.4→F1.11→F2.2, re-confirming the craftsmanship-gate root cause). REGRA #0: the 3 new `pub` symbols are consumed by the F2.2 verifier (not orphans). Note: the `security-guidance` plugin warned on the engine source's `yaml.load`/`pickle` *needle literals* — those are detection data, handled by `is_detector_own_source` (self-match allowlist), not real calls.

**Lessons.** (1) *A `WorstOf` security dim makes zero-FP load-bearing* — one false positive drags the whole scope, so the catalogue must be conservative (esp. in the workspace language) and every needle's safe-form superset must be excluded. (2) *Suffix collisions need a word boundary* — `gets(` ⊂ `fgets(` is the security-relevant case (the substring IS the safe function); a structural preceding-char check is the fix, the same shape as the F4.1 char-aware `eqeqeq`. (3) *Choose needles whose safe form is not a superset* — `yaml.load(`∤`yaml.safe_load(`, `scanf("%s`∤`scanf("%31s"`, `pickle.load(`∤`pickle.loads(` are all distinct by construction, so the safe form is never a false positive. (4) *Detector-own-source covers third-party security scanners too* — the plugin's `yaml.load` warning on the engine's needle table is the same self-match the allowlist already handles.

**Files**: `touring-analysis/src/quality/{input_validation.rs NEW (7-lang, 10 tp/fp tests incl. `fgets`/`yaml.safe_load`/`scanf("%31s"` FP guards, structural `cpp_bare_gets`), mod.rs (mod + re-export)}`, `touring-quality/src/verifications/f2_2_input_validation.rs` (rewritten to delegate + `is_detector_own_source`). **Stub→real: 6 P0 BLOCK + 14 WARN/ADVISORY-tier (… + F2.2) real.** Doc Part 21.

## Part 22 — F1.10 data-model: stub → NEW **polyglot** `data_model` engine (7 langs), "make illegal states unrepresentable" + primitive obsession, with a brace-matched struct block-scanner (Gabriel "F1.10 data-model (estados ilegais representáveis / primitive obsession)")

**D10/F1.10 = is the data modelled in *types*** (enums for states, newtypes for domain values, `Option` for absence) rather than in raw primitives. The stub scored `derive(` per `struct ` ratio — meaningless (a struct with one `#[derive]` and three `status: String` fields scored 1.0).

**context7** `/websites/rust-lang_github_io_api-guidelines` (score 83) gave the canonical elite anchor: the type-safety checklist says "use **types instead of `bool` or `Option`** for arguments to convey meaning" + "**newtypes for static distinctions**" (`Miles`/`Kilometers`, an `Ascii` validated wrapper pushing validation to the conversion boundary) + "**`bitflags` for sets of flags** instead of [multiple bools]". These map 1:1 to the three detectors.

**Engine** NEW zero-dep, **7-language**, three *structural* detectors (not pure-needle — F1.10 is about type shape, so it needs adjacency + block analysis): (1) **stringly-typed domain field** — a curated closed-set `DOMAIN_WORDS` table (status/state/kind/mode/phase/role/level/priority/severity/category/direction/color/colour) × the language's string-type token (Rust `String`, TS `string`, Py `str`, Go `string`, Java `String`, C++ `string`) with a **whole-word boundary** on the token + an **exact-match adjacent identifier** (name-before for Rust/TS/Py/Go via `ident_before` skipping `:`/ws; name-after for Java/C++ via `ident_after`) → excludes `name: String` (not a domain word), `getStatus()` (≠ exact `status`), `StringBuilder`/`to_string`/`String::new` (whole-word); (2) **type-erasure escape** — `: any`/`as any`/`<any>` (TS), `interface{}` (Go), `<Object>`/`Object[]` (Java), `void*` (C/C++), `: Any`/`-> Any` (Py); (3) **boolean-flag explosion** — a **brace-matched struct block-scanner** (`find_struct_blocks`: Rust `struct…{…}` via forward-scan-for-`{`-bail-on-`;`/`(`, Go `…struct{…}`; restricted to these two because their bodies are method-free so byte-level bool-counting is exact — Java/C++/TS class bodies interleave methods and are deferred) that flags a struct with **≥3 `bool` fields** (weight 2.0). **Disjoint from F1.9** (api-design flags `String` in the *error position* of `Result<_,String>`; F1.10 flags it in the *domain-field* position — different by construction), **F1.11**, **F4.4**, **F2.2**.

**Engine-effectiveness [[feedback-verify-engine-effectiveness]] — live signal, every score a real TP.** Baseline grep BEFORE writing found **75 stringly-typed hits in 58 files** (`pub status: String`, `pub kind: String`, `pub color: String`, `severity: String`…) — so unlike F4.4/F2.2's clean-workspace case, F1.10 produces a *live* WARN gradient across the workspace, exactly what the directive wants. Scored 5 real files: `encoding.rs` (`color: String`) → **0.968** (1 stringly TP), `synergy_health_check.rs` (`severity`+`status`) → **0.967** (2 stringly TP), `plan/schema.rs` (12 `: bool`) → **0.938** (**bool-explosion ×2 structs** — the block-scanner found 2 real structs with ≥3 bool fields), `data_model.rs` (own source) → **1.000** (allowlist guard — the file embeds `String`/`bool`/`interface{}`/`DOMAIN_WORDS` as data), and the intended "control" `engine.rs` → **0.931** which turned out to be a **6th TP**: `AnalysisConfig { cross_crate, temporal, knowledge, learning: bool }` is a 4-bool config = exactly the `bitflags` candidate context7 names. **6 real TPs, 0 FP.** Calibration: SCALE=6.0 (style-tier) clusters real files at 0.93–0.97 Pass — the correct ADVISORY gradient (minor data debt, not Fail; a Fail needs ~0.083 density = a genuinely smelly data file). No recalibration.

**Gate (real exit codes).** data_model engine **13/13** · verifier F1.10 **6/6** · touring-quality lib **241/0** · touring-analysis lib **567/0** · standalone `--no-default-features` builds · `clippy --workspace --all-targets -D warnings = 0` · `elite_aggregate composite=0.9284 tier=Platinum` (no regression — F1.10 is `WeightedLoc`, not in the 13-gate; daemon confirmed alive PID 2812372 → craftsmanship gate read a warm index → stable, re-confirming the F4.4 finding that 0.9419→0.9284 was index-warming not regression). REGRA #0: the 3 new `pub` symbols are consumed by the F1.10 verifier (not orphans). The SessionStart "daemon degraded 0.5" was the spurious self-healing race (REGRA #19) — `touring daemon-ctl status` (non-destructive) confirmed the socket alive.

**Lessons.** (1) *A curated closed-set `DOMAIN_WORDS` list is the precision lever* — `name`/`title`/`message`/`path` are legitimately strings and excluded; only enumerable concepts (`status`/`kind`/`color`/`severity`…) are flagged, so the FP rate stays near zero even though `String` is ubiquitous. (2) *The brace-matched block-scanner is what makes bool-explosion precise* — a line-level `: bool` count would FP across unrelated structs/functions; restricting to Rust/Go method-free struct bodies + a real brace-match keeps it exact (proven by the 2 TP structs in `plan/schema.rs`). (3) *Baseline grep before writing proved the engine yields live signal* (75 real hits) — the inverse of F4.4/F2.2's clean-workspace outcome, both honest: a stub→real migration's value is the *real-file* behaviour, not the fixture count. (4) *Adjacent-identifier exact-match + whole-word boundary is the FP guard* — `name: String` / `getStatus()` / `StringBuilder` are all the obvious near-misses, and all are excluded by construction. (5) *The "control" being a TP is the effectiveness proof* — a config struct with 4 bools genuinely IS the `bitflags` smell context7 names; an ADVISORY 0.931 surfaces it gently without blocking. **Stub→real: 6 P0 BLOCK + 15 WARN/ADVISORY-tier (+F1.10) real.** Files: `touring-analysis/src/quality/{data_model.rs NEW (7-lang, 13 tests incl. StringBuilder/free-text-field/2-bool-clean FP guards + brace-matched block-scanner), mod.rs (mod + re-export)}`, `touring-quality/src/verifications/f1_10_data_model.rs` rewritten to delegate + `is_detector_own_source`. Doc Part 22.

## Part 23 — F2.7 db-perf: stub → NEW **polyglot** `db_perf` engine (7 langs), N+1 query-in-loop (brace-matched + Python-indent loop-body scanner) + `SELECT *` over-fetch (Gabriel "F2.7 db-perf (N+1")

**D20/F2.7 = is the database accessed efficiently.** The stub scored the ratio of `.query_async(`/`.fetch(` to `.query(`/`.execute(` — an anti-metric (it rewarded an async ratio, not performance). **context7** `/prisma/web` (score 84.91): the named anti-pattern is **"Avoiding n+1 in loops"** — the fix is a batched `in` filter (`where: { authorId: { in: users.map(u => u.id) } }`), an `include`, or `relationLoadStrategy: "join"` (1-2 queries instead of N); and **"select only needed fields"** (not `SELECT *`). `findMany`/`findUnique` validate the token choices. **VGP V2 load-bearing check**: `code_regions.rs:23-27` confirms "production string literals are deliberately **not** suppressed" → a `"SELECT * FROM …"` in a string IS scannable while comments/`#[cfg(test)]` are excluded (verified before designing the `SELECT *` detector).

**Engine** NEW zero-dep, **7-language**, two *structural* detectors: (1) **N+1** — a curated `DB_TOKENS` set (`.execute(`/`.query(`/`.query_row(`/`.query_map(`/`.fetch_one|all|optional(`/`.findOne|Many|Unique|First(`/`.Query(`/`.QueryRow(`/`.Exec(`; collection methods `.get(`/`.find(`/`.iter(` are *deliberately excluded* to stay DB-unambiguous) occurring inside a *loop body*, via `find_loop_blocks`: **brace-matched and paren-aware** for brace langs (handles C/JS `for (i=0; i<n; i++) {` where `;` is at paren depth 1, handles Go 3-clause `for i := 0; i < n; i++ {` — no parens, `;` at depth 0 — by **dropping the `;`-bail entirely** and accepting the first `{` at paren depth ≤ 0, and skips a closure brace in the iterator expr `for x in f(|y| { … }) {` because that `{` is at paren depth > 0), and **indent-scoped** for Python (the `for`/`while` must be the first token of its line, so a list comprehension `[f(x) for x in xs]` — `for` mid-line — is excluded); (2) **`SELECT *`** over-fetch (case-insensitive needle, region-excluded). Weights: N+1 1.0, SELECT* 0.6. **Disjoint from F2.1 OWASP** (which scores SQL *injection* — a quote-break in a string sink — via the `SecurityAnalyzer`): F2.7 reads the same SQL string for *performance*, never for a quote-break.

**Engine-effectiveness [[feedback-verify-engine-effectiveness]] — live signal, every finding a verified TP.** Baseline grep BEFORE writing found a real N+1 (`sqlite_vec.rs:136` `for id in ids { conn.execute("DELETE FROM t WHERE id = ?", params![id]) }`). Scored 5 real files = **6 TPs (5 N+1 + 1 SELECT*), 0 FP**: `sqlite_vec.rs` (2 N+1: per-id DELETE + per-point insert) → **0.969**, `consolidation.rs` (1 N+1 + 1 real `SELECT *` in a production migration string) → **0.991**, `migrate.rs` (1 N+1 in a per-table migration loop; the comment `// … SELECT * mismatch` was correctly **EXCLUDED** by region masking, and the prose `println!("To execute: …")` was correctly **NOT** matched because `.execute(` requires the literal dot+paren) → **0.996**, `gotchas.rs` (1 N+1 — confirmed verbatim: `for id in to_delete { self.conn.query_row(&select_sql, params![id], …); self.conn.execute(&delete_sql, params![id]) }` = textbook per-id SELECT-count + DELETE) → **0.983**, `db_perf.rs` (own source) → **1.000** guard. **Rigor (VP-Scout / REGRA #21)**: I had labelled `gotchas.rs`/`migrate.rs` as "clean controls", but they fired — so I read each flagged loop body verbatim (`sed`/`rg`) and confirmed a genuine per-id DB call before accepting it as a TP, rather than assuming. Both are real N+1. Calibration: SCALE=6.0 clusters real files at 0.97–0.99 Pass — the correct ADVISORY gradient (a few N+1 sites over large files).

**Gate (real exit codes).** db_perf engine **12/12** (incl. Go 3-clause `for`, C-style `for`, closure-not-misscoped, Python comprehension-excluded, collection-methods-not-DB) · verifier F2.7 **6/6** · touring-quality lib **245/0** · touring-analysis lib **579/0** · standalone `--no-default-features` builds · `clippy --workspace --all-targets -D warnings = 0` · `elite_aggregate composite=0.9284 tier=Platinum` (no regression — F2.7 is `WeightedLoc`, not in the 13-gate; stable across F4.4→F1.11→F2.2→F1.10→F2.7). REGRA #0: the 3 new `pub` symbols are consumed by the F2.7 verifier (not orphans).

**Lessons.** (1) *A curated DB-token set is the N+1 precision lever* — including `.find(`/`.get(`/`.iter(` would false-positive on every `HashMap`/`Vec`; restricting to unambiguous DB calls (`.execute(`/`.query(`/`.fetch_*`/`.findMany(`/Go `.Query(`) keeps a loop-body scan clean. (2) *The no-`;`-bail, paren-aware brace finder is what makes the loop scan polyglot* — a Go 3-clause `for` puts `;` at depth 0 while a C `for(;;)` puts it at depth 1, so a naïve `;`-bail breaks Go; dropping the bail and accepting the first `{` at paren depth ≤ 0 satisfies both (and skips closure braces in the iterator expression). (3) *A literal curated token does not false-match prose* — `.execute(` (dot+paren) never matches `"To execute:"`, so the prose string in `migrate.rs` was correctly ignored; the boundary is built into the needle. (4) *Real-file scoring caught that my "clean controls" were TPs* — `gotchas.rs`/`migrate.rs` have genuine N+1 loops; verifying the loop body verbatim (VP-Scout) before asserting turned "is this an FP?" into "no, here is the per-id DELETE". (5) *Region-exclusion + curated-token compose on one file* — `migrate.rs` proved both layers at once: the comment `SELECT *` was dropped by region masking, and the prose "execute" was not matched by the curated token. **Stub→real: 6 P0 BLOCK + 16 WARN/ADVISORY-tier (+F2.7) real.** Files: `touring-analysis/src/quality/{db_perf.rs NEW (7-lang, 12 tests incl. Go-3-clause / C-style-for / closure-not-misscoped / comprehension-excluded / collection-methods-not-DB guards + paren-aware brace + Python-indent loop scanners), mod.rs (mod + re-export)}`, `touring-quality/src/verifications/f2_7_db_perf.rs` rewritten to delegate + `is_detector_own_source`. Doc Part 23.

## Part 24 — F2.8 memory-mgmt: stub → NEW `memory` engine (unbounded / leak / Rc-cycle / hot-path clone) + a shared `loop_blocks` extraction that surfaced & fixed TWO latent bugs in F2.7's loop scanner (Gabriel "F2.8 memory-mgmt (unbounded growth / Rc cycles / clone no hot-path)")

**D21/F2.8 = is memory bounded, leak-free, and not needlessly copied.** The stub scored `Box::`/`Vec::` density (an anti-metric — every allocation lowered the score). **context7** `/websites/rs_tokio` (score 86.11): `unbounded_channel` is "without backpressure… messages will be arbitrarily buffered… **using an unbounded channel has the ability of causing the process to run out of memory, in which case the process will be aborted**" → the fix is a bounded `channel(N)`. The real workspace TPs `LruCache::unbounded()` (an "LRU" with no bound) are the same OOM class.

**DRY architecture (REGRA #0).** F2.8's hot-path detector ("a `.to_vec()`/`.to_owned()` inside a loop") needs the same loop-body finder as F2.7's N+1. Rather than duplicate ~80 LOC (which the F1.3 Type-1 clone detector would itself flag), I **extracted** `quality/loop_blocks.rs` (`pub(crate) fn loop_bodies(bytes, regions, lang)`), refactored `db_perf` to call it (its three private loop fns removed), and `memory` uses it too. db-perf stayed **12/12** through the refactor.

**Engine** NEW zero-dep, four detectors: (1) **unbounded** — `unbounded_channel(`/`unbounded(` (Rust, word-boundary so `is_unbounded(` is excluded) + `maxsize=None` (Python `lru_cache`); (2) **leak** — `Box::leak(`/`mem::forget(`/`.leak(` (Rust); (3) **refcount cycle** — a `parent`/`prev`/`owner`/`root`/`back` field typed as a *strong* `Rc<`/`Arc<` (Rust, via `ident_before` adjacency — `Weak<` doesn't match the strong token so the fix isn't flagged) or `shared_ptr<` (C++, line-level + no `weak_ptr`); (4) **hot-path alloc** — `.to_vec()`/`.to_owned()` in a loop body (Rust; bare `.clone()` is *excluded* — at 2699 occurrences in an `Arc`-heavy codebase it is dominated by cheap refcount bumps, so the unambiguous deep-copy conversions are the precise signal). **Disjoint from F1.11 design-patterns** (verified by grep — F1.11 owns `Rc<RefCell<`/`Cloneable`; F2.8 keys on the back-reference *name*, never the `RefCell` wrapper). `mem::forget` lives in the legacy `antipatterns` pipeline which is **not wired to any 50-dim verifier** (grep-confirmed), so F2.8 is its dim-level owner (covering the gap, not ceding it). F2.8 is heaviest on Rust/C++ (manual refcounting + explicit leaks); GC languages have a small detectable surface, so the engine covers the language-specific signals it can (Python `maxsize=None`).

**TWO latent bugs in the shared loop scanner, surfaced by real-file scoring of one complex file (esaa.rs) and fixed — benefiting F2.7 too [[feedback-verify-engine-effectiveness]] (REGRA #21).** esaa.rs scored a hot-path finding I could not locate in any real loop. Investigation found two distinct bugs the unit fixtures had missed:
1. **String-brace miscount** `[FACT 1.0]`. `code_regions` deliberately does *not* suppress string literals (injection lives in strings — the F2.1/F2.7 detectors need them). But the brace-matcher counted `{`/`}` *inside* strings: a `for x in xs { log("open brace {"); } let y = data.to_vec();` had the string's `{` inflate the depth so the loop's real `}` did not close it — the body **engulfed the trailing `to_vec()` that is outside the loop** (a false positive, score 0.60). Fixed with `skip_literal` (the brace matcher now skips `"`/`` ` `` strings — with `\`-escapes — and `'{'`/`'}'` char literals). A regression test proves the body stops at the real `}`.
2. **`impl Trait for Type {` false loop** `[FACT 1.0]`. esaa.rs's `impl EsaaSubsystem for Analyzer { … .to_vec() … }` has a `for ` keyword that matched the loop needle → the **impl body** (with a `.to_vec()`) was scanned as a loop body. This is a Rust-only ambiguity (Go/C/JS have no `impl … for`). Fixed lang-aware: a Rust `for ` is a loop only if it has the ` in ` keyword before the body brace (a trait impl never does). The *naïve* first fix ("require ` in ` or `(`") broke the **Go 3-clause `for i := 0; i < n; i++ {`** (no `in`, no immediate paren) — corrected by scoping the impl-for exclusion to Rust only (Go/C have no `impl for`, so a bare `for` is always a loop), threading `lang` through `loop_bodies`.

**F2.7 re-validated after the fix**: `sqlite_vec.rs`=2, `gotchas.rs`=1 N+1 — **unchanged** → those were genuine `for … in` loops (the bug only fired where an impl body held the token without a real loop, as in esaa). So F2.7's earlier results were correct, and are now also robust to string-braces and impl-for.

**Engine-effectiveness (post-fix).** unbounded: `cache.rs` (2× `LruCache::unbounded()`) → **0.984**, `esaa.rs` (1×, the phantom hot-path now gone) → **0.996**; leak: `plugin.rs` + `generator_spec.rs` (production `Box::leak`) → **0.907** / **0.977**, while `analyzer.rs` (a `std::mem::forget(dir)` in **test** code) → **1.000** (correct TN — region-excluded); refcount cycle: **0** in the workspace (honest — touring uses `Weak`/arena patterns; the detector fires via fixtures and would fire on a strong back-reference); guard `memory.rs` → **1.000**. Real TPs across unbounded + leak, 0 FP after the two fixes. SCALE=6.0 clusters real files at 0.90–0.99 Pass (correct ADVISORY).

**Gate (real exit codes).** loop_blocks **9/9** (incl. Go-3-clause + impl-for-exclusion + string-brace + closure-not-misscoped) · db_perf **no-regress 12/12** · memory engine **10/10** · verifier F2.8 **6/6** · touring-analysis lib **598/0** · touring-quality lib **249/0** · standalone builds · `clippy --workspace --all-targets -D warnings = 0` (one `manual_contains` self-fixed) · `elite 0.9284 Platinum, 0 gates failing` (no regression). REGRA #0: 3 new `pub` consumed by the verifier; `loop_blocks` is `pub(crate)`, consumed by db_perf + memory.

**Lessons.** (1) *Extracting shared scan infra (loop_blocks) is the elite move* — it avoids F1.3 self-flagging AND means a single bug-fix repairs every consumer at once (F2.7 + F2.8). (2) *Real-file scoring of one complex file surfaced TWO latent bugs that fixtures missed* — the exact payoff of the verify-engine-effectiveness directive; a stub→real migration's value is the real-file behaviour. (3) *Strings are not region-suppressed (a deliberate `code_regions` choice for SQL/injection), so any brace/paren counter must be independently string-aware* — `skip_literal` is now the canonical guard. (4) *`impl Trait for Type` shares the `for ` keyword with loops* — the distinction is lang-specific (Rust needs ` in `), and a lang-agnostic fix breaks Go's 3-clause `for`; the exclusion must be Rust-scoped. (5) *When a shared-infra bug is fixed, re-validate every consumer* — F2.7 was re-scored and confirmed unchanged (its results were genuine), not assumed. **Stub→real: 6 P0 BLOCK + 17 WARN/ADVISORY-tier (+F2.8) real.** Files: `touring-analysis/src/quality/{loop_blocks.rs NEW (shared, string-aware, lang-aware, 9 tests), memory.rs NEW (4 detectors, 10 tests), db_perf.rs (refactored to use loop_blocks), mod.rs}`, `touring-quality/src/verifications/f2_8_memory.rs` rewritten + `is_detector_own_source`. Doc Part 24.
