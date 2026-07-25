# Lessons L1-L10 — Hard-earned

> Distilled from Touring Premium Refactor (2026-05-11), 8 waves of W8 v1→v5
> shared-bucket iteration, and the sister-project mining session (2026-05-23).

---

## L1 — Iterate v1→v5 with forensic measurement, single hypothesis per version

**Symptom**: First fix doesn't move the metric; second fix moves it 50%; third
overshoots. Without forensic measurement between attempts, you can't tell
*which* change was the lever.

**Rule**: Each version makes **one** measurable change. Re-run the forensic
sub-script. If v(N+1) moves the metric < 10%, stop and pivot — the model
of the problem is wrong.

**Origin**: W8 iterated v1→v5 to find the shared-bucket leaf invariant.
Each version was one rule change to the classifier.

---

## L2 — `shared types` bucket must be leaf (no outgoing `crate::` deps)

**Symptom**: After a crate split, you discover cycles where none existed
before. The shared bucket consumes from runtime/knowledge/branch_fs.

**Rule**: For every file proposed in SHARED, grep `^use crate::`. If it
imports from a non-shared, non-facade bucket, **it is a consumer**, not a
shared leaf. Relocate to the bucket that uses it (tools / lifecycle).

**Origin**: W8 — first 4 iterations placed bucket-consumers in SHARED,
creating cycles. v5 enforced the leaf invariant: shared → consumed-by-many,
consumes-from-zero.

---

## L3 — `textwrap.dedent` gotcha in template generation

**Symptom**: Generated sub-scripts have stray 4-space indents on some lines.

**Root cause**: `textwrap.dedent` removes the **common minimum** leading
whitespace. If one interpolated line has 4-space indent while the surrounding
template uses 8-space, dedent only removes 4 — leaving 4 of residue everywhere.

**Rule**: Either keep all interpolations at the SAME leading-whitespace level
as the template, or use Jinja2 (which does not have this problem because
indentation is controlled by the template).

**Origin**: scaffold templates of Touring Premium Refactor session.

---

## L4 — Cross-audit `--baseline` mode distinguishes PENDING vs FAIL

**Symptom**: A plan with 15 PENDING waves and 0 FAILED returns
composite_score = 0.0 — indistinguishable from a plan where all 15 waves
ran and failed.

**Fix**: `cross_audit.py --baseline` excludes `status=PENDING` from the
average. Status becomes `BASELINE` (exit 0) when ALL waves are PENDING,
`FAIL` only when at least one ran and failed.

**Origin**: distinct estado inicial vs. estado falho is the difference
between "we haven't started" and "everything is broken".

---

## L5 — Forensic discovery first, refactor second

**Symptom**: 2 hours of refactor produce a 30-line patch that fails because
the codebase is no longer what the plan thought it was.

**Rule**: For every wave, create a `wN_discover_*.py` forensic sub-script
**before** any `wN_apply_*.py` script. Discover scripts:
- count the actual occurrences
- list the actual file paths
- measure the actual baseline metric

Each hour of forensic discovery saves ~10 hours of doomed refactor.

**Origin**: ubiquitous lesson — W2 (220 rewrites), W4 (77 consumers),
W6 (236% pub-ratio), W3.2 (anemic crates overlap with W1 KNOWN_DEAD).

---

## L6 — Re-measure premises before each wave

**Symptom**: Plan says "touring-intelligence cov = 15%". Wave kicks off.
First forensic script reports 83.14%. The plan was stale.

**Rule**: Every wave's first sub-script is `wN_measure.py` that re-checks
the plan's assumptions against current state. If the gap is > 30%, **re-scope
the wave**.

**Origin**: W11 was re-scoped 10-15d → 5-8d after discovering the coverage
premise was stale.

---

## L7 — Daemon `(deleted)` after binary rebuild → restart required

**Symptom**: After `cargo build --release`, the running daemon still
serves the old binary. `readlink -f /proc/<pid>/exe` shows `... (deleted)`.

**Rule**: Use `update-touring` (REGRA #6 of touring-rebuild). For TACO-wt
sub-scripts that depend on `touring` CLI, the first sub-script of each
session calls `touring doctor -j` — if `daemon_socket` reports error,
the session aborts before any wave runs.

**Origin**: `~/.claude/rules/touring-rebuild.md` REGRA #3.

---

## L8 — A validation script is itself a sub-script (same anatomy)

**Symptom**: Validator written as a one-off, doesn't accept `-j`, doesn't
write to `data/`, doesn't return the contract dict.

**Rule**: `validate_W<N>.py` follows the exact 4-phase anatomy of any
sub-script. Its `scan_X()` reads `data/W<N>-*.json`, its mutation phase is a
no-op (`--apply` ignored), its report has `{status, score, evidence_files}`
on top of the usual envelope.

**Origin**: this lets `cross_audit.py` treat validators and regular
sub-scripts identically.

---

## L9 — `--apply` ALWAYS opt-in; default is dry-run

**Symptom**: User runs `python3 W12_apply.py`, expects a preview, gets a
77-file rewrite. Roll-forward via git is the only recovery — but git is
forbidden (REGRA #11). The lesson costs a wave of rework.

**Rule**: The default of every sub-script is **dry-run** (read-only, prints
findings, exits 0). Mutations require explicit `--apply`. No exceptions.

The runner reinforces this: `forensic_runner.py` does NOT pass `--apply`
unless explicitly told via `--apply-all` (which itself prompts a `y/n`).

**Origin**: cardinal rule. Non-negotiable.

---

## L10 — JSON is the contract; markdown is a courtesy

**Symptom**: Two sub-scripts emit "report.md" + a JSON. Aggregator parses
the markdown for status. Markdown formatting drifts (table layouts change).
Aggregator silently breaks.

**Rule**: The contract between sub-scripts and the toolkit is the JSON
artifact in `data/`. Markdown in `staging/` is human-facing courtesy, never
parsed by automation. The cross-audit composer reads `data/`; the markdown
is optional.

**Corollary**: New sub-script fields go in JSON first, then optionally in
markdown. Never markdown-only.

**Origin**: Wave 2026-05-23 — designing the cross-audit aggregator and
discovering 3 sub-scripts had drifted into markdown-parsing territory.

---

## How lessons enter the corpus

A wave that fails reveals a new lesson. Steps:

1. Add a new entry here (`L11`, `L12`, ...) with one-line digest + full narrative.
2. `touring memory store "lesson:taco-wt:L<N>" "<digest>" --tier semantic`
3. `touring learning reward orchestrate +1.0 "lesson_L<N>_persisted"`
4. Update `SKILL.md` Lessons table with the one-liner.
5. If the lesson maps to a code change, add a test to `scripts/<plan>/tests/` that locks it in.
