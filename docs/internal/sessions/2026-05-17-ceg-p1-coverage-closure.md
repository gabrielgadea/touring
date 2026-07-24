# CEG — P1 Coverage Closure (wave report)

> Pln2 plan: `docs/2026-05-17-ceg-pln2-plan.md`, phase **P1**.
> Executed 2026-05-17. FASE 0 gate PASS (daemon 5/5; `cargo check --workspace` exit 0).
> Wave scope: **P1.1** (verification + plan correction) + **P1.2** (implemented + tested).
> P1.3–P1.6 deferred to the next wave (see §4).

## 1. P1.1 — pre-bash exec-surface coverage: ALREADY SATISFIED (plan correction)

VP-Scout Chain 5 — verify against reality, not plan docs. The Pln1 critique
recorded gap *"G1: pre-bash only fires for cargo/rustc/touring; generic
python3/bash do NOT trigger pre-bash"*. Verified against the **live**
`~/.claude/settings.json` (`PreToolUse` hook array):

| # | Assertion | Evidence | Verdict |
|---|-----------|----------|---------|
| 1 | A pre-bash `PreToolUse` entry exists | `PreToolUse[3]` | PASS |
| 2 | Its matcher is `Bash` | `matcher="Bash"` | PASS |
| 3 | Its `if` condition is unconditional | `if=""` (empty string) | PASS |
| 4 | It invokes `touring-hook pre-bash` | command chain: `touring-hook pre-bash ; block_git.sh ; taco-forge-guard.sh Bash ; touring-hook cli-suggest` | PASS |

**Conclusion**: the pre-bash hook **already fires on every Bash tool call** —
there is no `cargo|rustc|touring` `if` filter to extend. `python3 x.py`,
`bash x.sh`, `node x.js`, `ruby`, `sh` invocations (all Bash commands)
**already** trigger `touring-hook pre-bash`, which runs `bash_ast_validator` +
the structural validator. P1.1's acceptance criterion is met by the current
configuration.

**Plan correction**: P1.1's "if extension" deliverable is moot — the Pln1 G1
premise was stale. No code/config change was required. (The deeper need —
extracting and statically analysing the *code body* inside `python3 x.py`
rather than the bash invocation string — is the X1/X2 gateway work in **P3**,
not P1.1.)

## 2. P1.2 — `.sh` gated in taco-forge-guard: IMPLEMENTED + TESTED

### 2.1 Gap confirmed
`is_code_file()` in `~/.claude/hooks/taco-forge-guard.sh:63` listed
`.rs .py .ts .tsx .js .jsx .go .c .cpp .cc .cxx .h .hpp .java .kt .swift` —
**`.sh` absent** (gap G4). Shell scripts could be created via raw Write/Bash
with no taco-forge gate.

### 2.2 Implementation — phased WARN→ENFORCE (the chosen rollout mode)
5 surgical edits to `taco-forge-guard.sh`:

| # | Edit | Effect |
|---|------|--------|
| 1 | `is_code_file()` += `*.sh\|*.bash` | `.sh`/`.bash` now recognised as code files |
| 2 | new `is_shell_file()` helper | identifies the phased-enforcement subset |
| 3 | `emit_block()` phased downgrade | shell-file blocks → WARN unless `TACO_FORGE_GUARD_SH_ENFORCE=1` |
| 4 | 3 `check_bash` regexes += `sh\|bash` | `cat>foo.sh`, `sed -i foo.sh`, rm+recreate caught |
| 5 | `check_write` kind_hint += `*.sh` case | suggests `--content-from -` for shell scripts |

Phase control: **`TACO_FORGE_GUARD_SH_ENFORCE`** — unset/`0` → `.sh` WARN
(visible nudge to taco-forge, exit 0); `1` → `.sh` BLOCK (exit 2), identical
to `.rs`. `.rs/.py/.ts/...` keep hard-blocking regardless — the phasing governs
only the new `.sh` surface.

### 2.3 Verification
`bash -n` SYNTAX OK · `shellcheck -S error` clean. New test harness
`~/.claude/hooks/tests/test-taco-forge-guard.sh` (created via
`taco-forge perfect-create --content-from -`) — **6/6 PASS**:

| Test | Asserts |
|------|---------|
| P1.2-1 | Write new `.sh`, default → WARN, exit 0 |
| P1.2-2 | Write new `.sh`, `SH_ENFORCE=1` → BLOCK, exit 2 |
| P1.2-3 | Bash `cat>foo.sh`, default → WARN, exit 0 |
| P1.2-4 | Bash `cat>foo.sh`, `SH_ENFORCE=1` → BLOCK, exit 2 |
| P1.2-5 | Edit `.sh` → nudge (NOTE), exit 0 |
| P1.2-6 | Write new `.rs` → still BLOCK, exit 2 (regression: phasing did not weaken `.rs`) |

### 2.4 Plan correction — Write/Edit matchers
The plan listed `settings.json` Write/Edit matcher changes for P1.2. Verified:
the Write matcher (`PreToolUse[2]`) and Edit matcher (`PreToolUse[1]`) **already
have `if=""`** (universal — they fire for every Write/Edit). The guard's
`is_code_file()` is the sole filter; adding `.sh` there is the complete change.
**No `settings.json` edit was needed.**

## 3. P1 wave outcome

| Deliverable | Status |
|-------------|--------|
| P1.1 | Verified already-satisfied (plan correction — no change) |
| P1.2 | Implemented + 6/6 tests pass |
| P1.3–P1.6 | Deferred — next wave (§4) |

The acute gap P1 targets — *"generic script execution running unvalidated"* — is
**closed at the configuration layer**: every Bash command already hits pre-bash
(P1.1), and shell-script creation now routes through the taco-forge gate (P1.2,
phased). The X1/X2 *code-body* analysis (statically analysing the Python/JS
inside a script, not just the bash invocation) is the gateway-core work in P3.

## 4. P1.3–P1.6 — next wave, with a scouting prerequisite

| Deliverable | Note |
|-------------|------|
| P1.3 — `detect_forbidden_calls` → 11 langs, AST-based | **Needs a scout pass first.** "11 languages" requires tree-sitter grammars for **ruby/perl/php/elixir** — *not* in the workspace `Cargo.toml` (present grammars: python, rust, ts, js, go, java, bash, html, css, json, toml, yaml, md). Grammar acquisition + ABI-15 compatibility (deps-audit §3) must be scoped before implementation. The ast-grep 0.42 atomic bump remains a separate gated sub-task — the feature ships on the working ast-grep 0.36 for the languages whose grammars already exist. |
| P1.4 — `ctx_execute` forbidden_calls ENFORCED | deps P1.3 |
| P1.5 — inferlets + jobs exec-surface coverage | deps P1.1 (satisfied) |
| P1.6 — heredoc temporal-split detection | deps P1.1 (satisfied) |

---
_P1.1+P1.2 complete. Files touched: `~/.claude/hooks/taco-forge-guard.sh` (5 edits),
`~/.claude/hooks/tests/test-taco-forge-guard.sh` (new, 6 tests). Zero settings.json
change required (plan corrections §1, §2.4)._
