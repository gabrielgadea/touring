#!/usr/bin/env python3
"""loop_converged.py — the Loop Engineering convergence gate.

Measures whether a loop run is "complete AND perfect" instead of asserting it.
Evaluates the convergence clauses from the ``loop-engineering`` skill and exits
0 (converged) or 1 (continue). Composes Touring CLI (decompose, touring-quality,
wiring, cargo) as a Layer-3 deterministic pipeline; fail-open on daemon errors.

Clauses (a clause is PASS / FAIL / N/A; converged ⟺ no clause is FAIL):
  1. judge_intact  — the graders match the judge of record (judge_attest).  (always)
  2. dag_done      — every subtask of --task is done/finalized.            (always)
  3. quality_gold  — touring-quality score --fail-below 0.80 passes.       (any code)
  4. no_p0_fail    — no P0 BLOCK dim (F2.1/2.4/2.5/2.6/F4.3/4.5) in Fail.  (any code)
  5. measured_whole_scope — the score covers the scope, not a prefix.      (any code)
  6. orphans_base  — wiring orphans <= baseline.                           (any code)
  7. cargo_green   — cargo check (+ test+clippy with --rust-full).         (Rust scope)
  8. cross_audit   — the bundle's audit-plan-completion.sh exits 0.        (if present)

This list is enforced, not merely declared: `judge_attest.py` reads the clause
names out of `_gather_clauses` by AST and compares them with the attestation, so
a clause that silently vanishes fails clause 1 instead of going unnoticed. It
went unnoticed before — this docstring claimed six while seven ran.

Usage:
    loop_converged.py --task <task_id> [--scope <path>] [--bundle <dir>]
                      [--rust-full] [--json] [--quiet]

Exit codes: 0 converged · 1 not converged · 2 usage error.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

try:
    from judge_attest import verdict as judge_verdict
except Exception:  # noqa: BLE001 — fail-open: a missing ledger never stalls a loop
    judge_verdict = None

P0_DIMS = {"F2_1", "F2_4", "F2_5", "F2_6", "F4_3", "F4_5"}
GOLD_OR_BETTER = {"Gold", "Platinum", "Diamond"}

# Terminal subtask vocabulary — shared with the three hooks so the gate and the
# hooks can never disagree about what "done" means (they did, until 08/08/2026:
# only this file listed "completed"). Guarded so a convergence run never dies on
# an import; the fallback is this file's own historical (correct) tuple.
sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parent / "hooks"))
try:
    from loop_marker import TERMINAL_SUBTASK_STATUSES, pending_subtask_ids
except Exception:  # noqa: BLE001 — fail-open
    TERMINAL_SUBTASK_STATUSES = ("done", "completed", "finalized")

    def pending_subtask_ids(subtasks):
        """Fallback mirror of `loop_marker.pending_subtask_ids`."""
        return [
            str(s.get("subtask_id", "")).split("::")[-1]
            for s in (subtasks or [])
            if str(s.get("status")) not in TERMINAL_SUBTASK_STATUSES
        ]


def run(cmd, timeout=900, cwd=None, env=None):
    """Run a command; return (rc, stdout, stderr). Never raises (fail-open)."""
    try:
        p = subprocess.run(
            cmd, capture_output=True, text=True, timeout=timeout, cwd=cwd, env=env
        )
        return p.returncode, p.stdout, p.stderr
    except Exception as exc:  # noqa: BLE001 — fail-open by design
        return 127, "", str(exc)


def parse_json(text):
    """Best-effort: parse the first JSON object/array in ``text``."""
    text = (text or "").strip()
    if not text:
        return None
    try:
        return json.loads(text)
    except Exception:  # noqa: BLE001
        start = text.find("{")
        end = text.rfind("}")
        if 0 <= start < end:
            try:
                return json.loads(text[start : end + 1])
            except Exception:  # noqa: BLE001
                return None
    return None


def is_rust_scope(scope: Path) -> bool:
    """A scope is Rust when its own root carries a Cargo.toml.

    Ancestors are deliberately NOT consulted (fixed 2026-08-02): a Python
    package under ``packages/`` inherited a sibling Rust workspace's manifest
    via the ancestor walk, which armed ``cargo_green`` for a scope with no
    Rust — and ``clause_cargo`` then ran in the invoker's CWD and failed with
    "could not find Cargo.toml", blocking convergence on a category error.
    A sub-path of a real crate should pass the crate root as --scope.
    """
    return (scope.resolve() / "Cargo.toml").exists()


# ── Clauses ──────────────────────────────────────────────────────────────────
def clause_dag_done(task):
    # `touring decompose get <task>` emits JSON by default (no -j flag). Fail
    # CLOSED: a convergence gate must NEVER declare "done" without positive
    # evidence that every subtask is done — missing evidence is "continue".
    _, out, _err = run(["touring", "decompose", "get", task], timeout=60)
    data = parse_json(out)
    if data is None or not isinstance(data.get("subtasks"), list):
        return False, "decompose get failed — DAG unverifiable (fail-closed)", "ensure the daemon is up and the task exists"
    subs = data["subtasks"]
    if not subs:
        return False, f"task {task} has no subtasks — cannot confirm completion", None
    # o DAG mistura vocabulários: "completed" (closes legados) e "done"
    # (loop_phase_close) — ambos são terminais; tratar um como pendente
    # tornaria dag_done inalcançável (observado em task_1784839254210619613)
    pending = pending_subtask_ids(subs)
    ok = not pending
    ev = f"{len(subs) - len(pending)}/{len(subs)} subtasks done"
    nxt = None if ok else f"execute pending subtask(s): {','.join(pending[:6])}"
    return ok, ev, nxt


def clause_quality(scope):
    """Returns ``(gold_ok, p0_fail_list, tier, composite, truncated_dims, blockers)``.

    ``gold_ok`` ⟺ **tier ≥ Gold** — the honest verdict. The quality-gate caps
    the tier to Silver when a WARN/BLOCK dim fails, so ``composite ≥ 0.80`` alone
    can still be Silver (a lenient false-pass). ``blockers`` names the dims doing
    that capping — the remedy must name THEM, never "raise the composite": measured
    2026-08-29 (peer analise-e0), composite=0.932 with 0/50 dims below 0.80 still
    yielded tier=Silver via ``blockers=["F1_3"]``, and the old fixed message told
    the operator to raise a number that already passed. Scores the SCOPE directly: NO
    ``--workspace`` (which would resolve to the ambient workspace and score the
    wrong tree — audit finding 2026-07-02). Fail-CLOSED on tool error.

    ``truncated_dims`` lists dims whose ``truncated`` flag is set — a score computed
    over a *prefix* of the corpus (the scope-native dims stop at the byte cap). Until
    2026-08-07 that fact travelled only as prose inside ``evidence``, so this gate
    consumed a prefix score as if it were a scope score. See ``clause_not_truncated``.
    """
    _, out, _err = run(
        ["touring-quality", "score", str(scope), "--format", "json"], timeout=1800)
    data = parse_json(out)
    if not data:
        # applicable but unverifiable → fail-closed. `None` for truncated_dims
        # distinguishes "could not measure" from "measured, nothing truncated".
        return False, [], None, None, None, []
    tier = data.get("tier")
    composite = data.get("composite")
    blockers = [str(b) for b in (data.get("blockers") or []) if b]
    p0_fail = []
    truncated = []
    dims = data.get("dimensions")
    if isinstance(dims, dict):
        for d in P0_DIMS:
            dim = dims.get(d)
            if isinstance(dim, dict) and dim.get("status") == "Fail":
                p0_fail.append(d)
        for name, dim in sorted(dims.items()):
            if isinstance(dim, dict) and dim.get("truncated") is True:
                truncated.append(name)
    return (tier in GOLD_OR_BETTER), p0_fail, tier, composite, truncated, blockers


def clause_not_truncated(truncated_dims):
    """Returns ``(ok, evidence)`` — refuse to converge on a partial measurement.

    **Every bounded computation announces its own bound.** A dim that scored only a
    prefix of its corpus is insensitive to remediation performed past the cut, so
    treating it as a scope verdict lets the gate pass on work it never measured —
    the same failure shape that had F2.6 scoring "insecure configuration" without
    having read one configuration file.

    ``None`` (quality unmeasurable) yields ``None`` here too: the truncation verdict
    is not applicable when there is no measurement to qualify, and ``quality_gold``
    already fails closed on that path. Reporting it as a *second* failure would
    double-count one root cause.
    """
    if truncated_dims is None:
        return None, "quality unmeasurable — truncation verdict N/A"
    if not truncated_dims:
        return True, "no dim reported a truncated corpus"
    return False, f"partial measurement in {len(truncated_dims)} dim(s): {','.join(truncated_dims)}"


def _index_root(scope: Path) -> Path:
    """The nearest ancestor (or the scope) marked as an index root by `.touring/` or `.git`."""
    return next((b for b in (scope, *scope.parents) if (b / ".touring").is_dir() or (b / ".git").exists()), scope)


def _resolve_record(raw: str, scope: Path, index_root: Path) -> Path:
    """A wiring `module_file` on disk: absolute as is; relative against the nearest base where it EXISTS
    (scope first — a crate records `src/…`); a file gone from disk falls back to the index root.

    `Path.resolve()` does not require existence, so joining a root-relative path to the scope made
    `relative_to(scope)` accept everything (6488 `.claude/` orphans reported in `scripts/eleitoral`).
    """
    p = Path(raw)
    if p.is_absolute():
        return p
    return next((b / p for b in (scope, *scope.parents) if (b / p).exists()), index_root / p)


def _is_under(path: Path, base: Path) -> bool:
    try:
        path.resolve().relative_to(base.resolve())
        return True
    except ValueError:
        return False


def _top_tree(scope: Path, index_root: Path) -> Path:
    """The first-level directory of the index root that contains the scope (the scope itself at the root)."""
    try:
        partes = scope.resolve().relative_to(index_root.resolve()).parts
    except ValueError:
        return scope
    return index_root / partes[0] if partes else scope


def _baseline_names_the_scope(baseline, scope: Path, index_root: Path) -> bool:
    """Whether at least one baseline entry (``path::symbol``) resolves under the scope."""
    return any(
        _is_under(_resolve_record(entrada.split("::", 1)[0], scope, index_root), scope) for entrada in baseline if entrada
    )


def clause_orphans(scope, bundle: Path):
    """SCOPED, NAMED orphan gate (2026-08-02 rewrite).

    The old clause compared the WHOLE-workspace orphan counter to a stored
    integer. That counter includes every indexed tree — even vendored cargo
    checkouts under ``~/.cargo/git`` — so it drifted a few units with routine
    reindexing and once held a converged loop on +14 global noise while the
    actual scope had ZERO orphans (verified via ``--full``). Now: fetch the
    full list (``wiring`` is brief-by-default; ``--full`` opts back in),
    filter to the scope, and baseline the NAMED set — so a failure always
    tells you exactly which new symbols to wire (REGRA #0), and out-of-scope
    churn can never block the loop.
    """
    scope = Path(scope).resolve()
    # cwd=scope (2026-08-07): in a per-project Touring setup the walk-up decides
    # WHICH daemon answers, so the invoker's CWD decided which index was asked.
    rc, out, _ = run(
        ["touring", "wiring", "orphans", "-j", "--full"], timeout=300, cwd=scope
    )
    data = parse_json(out)
    if rc != 0 or data is None:
        return False, "wiring orphans unavailable (fail-closed)"
    orphans = data.get("orphans")
    if not isinstance(orphans, list):
        return False, "orphan list unavailable (fail-closed; need --full support)"

    # This clause was VACUOUS until 2026-08-07, on two independent key mismatches
    # (measured on kazuba-geo-engine: 1257 real orphans, clause reported 0):
    #
    #  1. `module_file` arrives RELATIVE ("src/budget/bdi.rs") and the filter was
    #     `str(scope.resolve()) in module_file` — an absolute path can never be a
    #     substring of a relative one, so the filter matched NOTHING, for every
    #     scope, always. The clause could not fail.
    #  2. The label read `o["symbol"] / o["name"]`; the record's key is
    #     `symbol_name`, so every entry would have been "path::?" anyway.
    #
    # Both are the "gate that measures the wrong alias" family: a clause that
    # certifies REGRA #0 while measuring nothing is worse than an absent clause,
    # because the checklist reads as covered.
    def _record_path(o):
        return str(o.get("module_file") or o.get("file") or "")

    # 2026-09-14 (test_orphans_scope.py): `module_file` is relative to the INDEX root — see
    # `_resolve_record`. The old join-to-scope accepted every relative path as in scope.
    index_root = _index_root(scope)

    def _in_scope(raw: str) -> bool:
        return bool(raw) and _is_under(_resolve_record(raw, scope, index_root), scope)

    in_scope = sorted(
        {
            "{}::{}".format(
                _record_path(o),
                o.get("symbol_name") or o.get("symbol") or o.get("name") or "?",
            )
            for o in orphans
            if isinstance(o, dict) and _in_scope(_record_path(o))
        }
    )
    if not in_scope:
        # REGRA ZERO: zero orphans in scope is only a measurement if the corpus reaches the scope's
        # top-level tree at all. Measured 2026-09-14: 20830 orphan records, NONE under scripts/ —
        # a PASS there certified a tree the wiring corpus never saw.
        tree = _top_tree(scope, index_root)
        if not any(isinstance(o, dict) and _is_under(_resolve_record(_record_path(o), scope, index_root), tree)
                   for o in orphans if _record_path(o)):
            return None, (f"unmeasured: {len(orphans)} orphan records in the wiring corpus, none under "
                          f"{tree} — absence of records is not absence of orphans")
    base_file = bundle / ".baseline" / "orphans-scoped.txt"
    if base_file.exists():
        baseline = set(base_file.read_text().splitlines())
        # 2026-09-14 (test_orphans_scope.py): a baseline that names NOTHING in the scope was recorded blind —
        # by a corpus that never reached it or by the old filter — and comparing against it reads every in-scope
        # orphan as NEW (analise/scripts/eleitoral: 24860 names, none in scope; 1329 "NEW", 1200 pre-existing).
        # It is not a baseline for this scope: re-record it, declared, exactly like the first run.
        #
        # 2026-09-16, by Gabriel's order ("quero honestidade do registro"): this returns N/A, not PASS.
        # It used to answer True, and True says "the orphan gate approved this round" when the truth is
        # that there was nothing to compare against. Neither verdict blocks — N/A and PASS are the same
        # to the runner — so the whole difference is what the record says afterwards, and the record is
        # what someone reads six months from now. Measured: the branch fired once, 14/09 16:34, on the
        # analise round, and that round's ledger says PASS on a clause that measured nothing.
        if in_scope and not _baseline_names_the_scope(baseline, scope, index_root):
            base_file.write_text("\n".join(in_scope))
            return None, (
                f"unmeasured: {len(in_scope)} scoped orphans, and the previous baseline named "
                f"{len(baseline)} symbols with none of them in scope — it was recorded blind, so there "
                "was nothing to compare against. Re-recorded now; the NEXT run measures against it."
            )
        new = [s for s in in_scope if s not in baseline]
        ok = not new
        detail = f"scoped orphans={len(in_scope)} baseline={len(baseline)}"
        if new:
            detail += f"; NEW: {', '.join(n.rsplit('/', 1)[-1] for n in new[:5])}"
        return ok, detail
    base_file.parent.mkdir(parents=True, exist_ok=True)
    base_file.write_text("\n".join(in_scope))
    return True, f"scoped orphans={len(in_scope)} (named baseline recorded, first run)"


def _isolated_daemon_env(scope: Path):
    """Env for cargo runs: a PRIVATE Touring daemon socket per scope.

    2026-08-21: the e2e suites spawn `touring` CLI children that inherit the
    session's TOURING_DAEMON_SOCKET and so round-trip through the LIVE global
    daemon. Any heavy op already grinding on that shared single-threaded actor
    (an `index rebuild` of a 4.6k-file tree runs 10-40 min) starves every
    15s-budget probe, and `graph_service_e2e::test_graph_svg_output` fails on
    machine weather, not on code — measured live: standalone-after-restart
    green twice, in-suite red five times in one night. Same lesson
    `e2e_diagnostic_rfc100.rs` codified on 03/08/2026: "com socket próprio a
    contenção some por construção". The first CLI client auto-spawns the
    private daemon; it serves the same on-disk project DBs read-only-enough
    for the suite. The socket name is scope-stable so repeated gates reuse
    one daemon instead of leaking one per run.
    """
    import hashlib
    import os
    import time

    leaf = hashlib.sha256(str(scope).encode()).hexdigest()[:12]
    sock = f"/tmp/touring-gate-{leaf}.sock"
    env = dict(os.environ)
    env["TOURING_DAEMON_SOCKET"] = sock
    env["TOURING_DAEMON_SOCK"] = sock
    env["TOURING_PROJECT_ROOT"] = str(scope)

    # Bootstrap: only `touring-hook` autostarts a daemon — the CLI fails fast
    # with ENOENT (measured 21/08: `touring index status` under a fresh socket
    # spawns nothing), so doctor-asserting tests (e.g. touring-generator's
    # `generator_binary_wired_and_healthy`) die on "daemon_socket missing"
    # before any client would have spawned it. Spawn the private daemon
    # explicitly — the per-project pattern — and wait for the socket.
    #
    # ALWAYS fresh (21/08, gate #10): a reused private daemon carries whatever
    # heavy op the PREVIOUS suite queued on its single-threaded actor — an
    # `index rebuild` grinds 10-40 min and every 15s-budget e2e probe in the
    # NEXT run starves behind it. A leftover daemon at this socket is stopped
    # (touring daemon-ctl, REGRA #19 — never pkill) before the new spawn.
    # Fail-open: if the socket never appears, cargo's own failures name the gap.
    try:
        if os.path.exists(sock):
            subprocess.run(
                ["touring", "daemon-ctl", "stop", "--socket", sock],
                capture_output=True,
                timeout=30,
                env=env,
            )
            for _ in range(20):
                if not os.path.exists(sock):
                    break
                time.sleep(0.5)
            if os.path.exists(sock):
                os.remove(sock)  # stale file left by a dead daemon
        # stderr goes to a per-scope log, NOT /dev/null: the daemon's
        # `heavy op:` receipt lines are the only attribution instrument for
        # "which op is grinding the actor" — a silent daemon cost one whole
        # night of blind bisection (21/08/2026).
        dlog = open(f"/tmp/touring-gate-{leaf}.daemon.log", "w")  # noqa: SIM115
        subprocess.Popen(
            ["touring-daemon"],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=dlog,
            start_new_session=True,
        )
        for _ in range(30):
            if os.path.exists(sock):
                break
            time.sleep(0.5)
    except Exception:  # noqa: BLE001 — fail-open by design
        pass
    return env


#: How many concrete diagnoses a verdict line carries. The verdict is read in a
#: terminal and in a phase report, so it stays one line — but whatever is left
#: out is COUNTED in the text. A gate that truncates silently reads as "that was
#: everything", which is the failure this file exists to prevent.
CARGO_DETAIL_MAX = 5

#: A libtest summary entry: four spaces, then the test path, nothing else.
_TEST_FAILURE_LINE = re.compile(r"^ {4}(\S.*?)\s*$")
#: libtest's own tally, per test binary. Authoritative for HOW MANY.
_TEST_FAILED_COUNT = re.compile(r"^test result: FAILED\..*?(\d+) failed", re.M)
#: A rustc/clippy diagnostic header.
_RUSTC_ERROR = re.compile(r"^error(?:\[(?P<code>E\d+)\])?: (?P<msg>.+?)\s*$")
_RUSTC_LOCATION = re.compile(r"^\s*--> (?P<at>\S+)\s*$")
#: rustc's/cargo's closing tallies. They say how many, never which — they ARE
#: the line this gate used to report, and the reason a failure arrived nameless.
_RUSTC_TALLY = re.compile(r"^error: (?:could not compile|aborting due to|test failed)\b")


def _failing_tests(stdout: str) -> list[str]:
    """Test paths from libtest's ``failures:`` summary, in report order.

    Measured 23/08/2026: libtest writes the failing-test list to **stdout**
    while cargo writes ``error: test failed, to rerun pass `-p X --lib`'' to
    stderr. Keeping only the last stderr line therefore names the TARGET and
    never the TEST — which is exactly how a real regression in `kazuba-rlm
    --lib` reached this gate anonymous, twice.

    Over-reporting is possible in principle (a test that prints a line of
    four-space-indented text right after one reading ``failures:``) and is
    accepted: this string is a diagnosis, never a verdict. Under-reporting
    would be the harmful direction, and the tally below cross-checks it.
    """
    names: list[str] = []
    seen: set[str] = set()
    inside = False
    for line in (stdout or "").splitlines():
        if line.strip() == "failures:":
            inside = True
            continue
        if not inside:
            continue
        match = _TEST_FAILURE_LINE.match(line)
        if match:
            name = match.group(1)
            if name not in seen:
                seen.add(name)
                names.append(name)
        elif line.strip():
            inside = False  # the block ended (`---- x stdout ----` / `test result:`)
    return names


def _rustc_errors(stderr: str) -> list[str]:
    """Concrete rustc/clippy diagnostics as ``[code] location message``.

    The closing tallies are dropped on purpose: they are what the gate already
    said, and "2 previous errors" sends nobody to a line of code.
    """
    found: list[str] = []
    lines = (stderr or "").splitlines()
    for i, line in enumerate(lines):
        if _RUSTC_TALLY.match(line):
            continue
        match = _RUSTC_ERROR.match(line)
        if not match:
            continue
        at = ""
        for nxt in lines[i + 1 : i + 4]:
            loc = _RUSTC_LOCATION.match(nxt)
            if loc:
                at = loc.group("at")
                break
        head = " ".join(part for part in (match.group("code"), at) if part)
        found.append(f"{head} {match.group('msg')}".strip())
    return found


def _cargo_diagnosis(step: str, stdout: str, stderr: str) -> str:
    """One short line naming WHAT failed, not merely THAT something did.

    Falls back to the previous behaviour — the last line of stderr — whenever
    nothing parses, so a format change upstream degrades this gate's message
    instead of breaking its verdict. The verdict itself is untouched: this
    function only builds the ``detail`` string of an already-decided FAIL.
    """
    try:
        if step == "test":
            found = _failing_tests(stdout)
            counted = sum(int(n) for n in _TEST_FAILED_COUNT.findall(stdout or ""))
            total = counted or len(found)
            label = f"{total} failing test{'' if total == 1 else 's'}"
        else:
            found = _rustc_errors(stderr)
            total = len(found)
            label = f"{total} error{'' if total == 1 else 's'}"
        tail_lines = (stderr or "").strip().splitlines()
        tail = tail_lines[-1].strip() if tail_lines else ""
        if not found:
            return tail
        shown = found[:CARGO_DETAIL_MAX]
        elided = len(found) - len(shown)
        body = "; ".join(shown) + (f"; (+{elided} not listed)" if elided else "")
        return f"{label} — {body}" + (f" [{tail}]" if tail else "")
    except Exception:  # noqa: BLE001 — fail-open: a diagnosis never breaks a verdict
        tail_lines = (stderr or "").strip().splitlines()
        return tail_lines[-1].strip() if tail_lines else ""


def _stop_private_daemon(env) -> None:
    """Stop the judge's private gate daemon once the cargo clause is done.

    Cross-audit 14/09/2026 (C9): the daemon was spawned per run and never
    stopped, so it outlived the judge — a PID still alive after the verdict,
    holding an actor and the project DBs open. Its lifetime is the clause's:
    `touring daemon-ctl stop` (REGRA #19, never a signal by name), fail-open.
    """
    sock = env.get("TOURING_DAEMON_SOCKET")
    if not sock:
        return
    try:
        subprocess.run(
            ["touring", "daemon-ctl", "stop", "--socket", sock],
            capture_output=True,
            timeout=30,
            env=env,
        )
    except Exception:  # noqa: BLE001 — cleanup never changes a verdict
        pass


def clause_cargo(scope: Path, rust_full):
    # cwd=scope (2026-08-02): cargo used to run in the *invoker's* CWD, so the
    # clause measured whatever workspace the shell happened to sit in.
    cargo_env = _isolated_daemon_env(scope)
    try:
        return _clause_cargo_with(scope, rust_full, cargo_env)
    finally:
        _stop_private_daemon(cargo_env)


def _clause_cargo_with(scope: Path, rust_full, cargo_env):
    rc, out, err = run(
        ["cargo", "check", "--workspace"], timeout=1800, cwd=scope, env=cargo_env
    )
    if rc != 0:
        return False, f"cargo check FAILED (rc={rc}): {_cargo_diagnosis('check', out, err)}"
    if not rust_full:
        return True, "cargo check green (test+clippy deferred; pass --rust-full for the final gate)"
    # cwd=scope here too (2026-08-07). The 2026-08-02 fix above was applied to
    # `cargo check` and to NEITHER of these two, so the final gate — the one
    # that decides "done" — still ran in the invoker's CWD. Measured live in
    # `projects/analise`, which has no root Cargo.toml: `cargo test --workspace`
    # exited 101 with "could not find `Cargo.toml`", and the clause reported
    # "cargo test FAILED (rc=101)" for a crate whose suite was in fact green
    # (2798 passed, 0 failed). A judge that fails on a phantom directory is
    # worse than no judge: it sends the caller hunting a defect that is not there.
    # Same family as the `juiz-que-mede-o-diretorio-fantasma` lesson — and the
    # same trap as fixing one instance of a class and leaving its siblings.
    for name, cmd in (("test", ["cargo", "test", "--workspace"]),
                      ("clippy", ["cargo", "clippy", "--workspace", "--", "-D", "warnings"])):
        r, o, e = run(cmd, timeout=2400, cwd=scope, env=cargo_env)
        if r != 0:
            # Carry the concrete diagnosis, not just the last stderr line. That
            # line distinguishes "a test failed" from "cargo never found a
            # manifest" — which is why it was kept — but it never says WHICH
            # test or WHICH lint: for `cargo test` the names live on the OTHER
            # stream, and for check/clippy the last line is a tally. Measured
            # 23/08/2026 after a `kazuba-rlm --lib` regression reached this gate
            # twice with no name, sending the reader to reproduce blind. The
            # stderr tail is still carried, in brackets, so nothing that used to
            # be readable stopped being readable.
            return False, f"cargo {name} FAILED (rc={r}) in {scope}: {_cargo_diagnosis(name, o, e)}"
    return True, "cargo check + test + clippy green"


def clause_cross_audit(bundle: Path):
    script = bundle / "audit-plan-completion.sh"
    if not script.exists():
        return None, "no audit-plan-completion.sh — skipped"
    rc, out, err = run(["bash", str(script)], timeout=1200)
    return rc == 0, f"cross-audit rc={rc}"


# ── Orchestration ────────────────────────────────────────────────────────────
def clause_judge_intact():
    """Did the verdict come from the judge of record?

    First, because every other clause is only worth what the grader that scored
    it is worth. Origin: arXiv:2505.22954 Appendix H — an agent reached a perfect
    score by deleting the markers its detector counted, under an explicit
    instruction not to. The remedy there is the remedy here: make the grader's
    state part of the verdict instead of assuming it.
    """
    if judge_verdict is None:
        return None, "judge_attest unavailable — grader integrity not checked", None
    try:
        rep = judge_verdict()
    except Exception as exc:  # noqa: BLE001 — fail-open
        return None, f"judge_attest failed ({exc.__class__.__name__}) — not checked", None
    if rep["blocking"]:
        why = "; ".join(d["detail"] for d in rep["drift"] if d["blocking"])
        return (False, f"grader drift: {why}",
                "restore it, or declare the change: judge_attest.py --attest --why '<reason>'")
    if not rep["attested"]:
        return None, "no judge of record yet — run judge_attest.py --attest", None
    advisory = sorted({d["kind"] for d in rep["drift"] if not d["blocking"]})
    n = len(rep["clauses_enforced"] or [])
    if advisory:
        # 2026-09-16, by Gabriel's order. This used to pass with the drift noted
        # in parentheses — and that is exactly how it went unseen: four verdicts
        # (cross-audit R2, D3, B4) carried `(advisory: file_changed)` inside a
        # green line, and nobody acted, the author of this note included, having
        # read two of those logs. An advisory tucked into a PASS is read as a
        # PASS. A grader that changed since it was attested cannot certify that
        # anything converged: the clause fails until a human attests the change.
        return (
            False,
            f"grader changed since it was attested ({', '.join(advisory)}) — "
            f"{n} clauses, but this verdict is not the verdict of record",
            "inspect the diff, then attest it BY HAND in a terminal: "
            "judge_attest.py --attest --why '<reason>' (needs a TTY on purpose)",
        )
    return True, f"judge of record intact, {n} clauses", None


def _gather_clauses(task, scope: Path, bundle: Path, rust_full, rust):
    """Yield ``(name, ok, evidence, action)`` for every convergence clause.

    ``ok`` is True (pass) / False (fail — blocks) / None (N/A — does not block).
    """
    yield ("judge_intact", *clause_judge_intact())
    yield ("dag_done", *clause_dag_done(task))

    # quality/P0/orphans apply to ANY code scope (the 50-dim harness and the
    # wiring index are polyglot); only cargo is Rust-specific. Until 2026-08-02
    # all four were gated on the Rust flag, so a Python scope was never held to
    # the quality bar at all — the opposite of the harness's purpose.
    gold, p0, tier, comp, truncated, blockers = clause_quality(scope)
    # The remedy must name the field that actually fails the clause (A5/D8):
    # with blockers, the tier is capped below Gold regardless of composite —
    # "raise the composite" would point the operator at a number that already
    # passes (measured: composite=0.932, tier=Silver via blockers=[F1_3]).
    qual_ev = f"tier={tier} composite={comp}" + (
        f" blockers={','.join(blockers)}" if blockers else "")
    qual_fix = (
        f"fix blocker dim(s) {','.join(blockers)} — the tier is capped below "
        f"Gold by blockers, not by composite ({comp} already measured)"
        if blockers else "raise touring-quality to >= Gold (0.80)")
    yield ("quality_gold", gold, qual_ev, qual_fix)
    yield ("no_p0_fail", len(p0) == 0, f"P0 fails: {p0 or 'none'}",
           f"fix P0 BLOCK dims: {','.join(p0)}" if p0 else None)
    yield ("measured_whole_scope", *clause_not_truncated(truncated),
           "re-scope per crate: a prefix score is not a scope score")
    yield ("orphans_base", *clause_orphans(scope, bundle),
           "wire new orphan pub symbols (REGRA #0)")
    if rust:
        yield ("cargo_green", *clause_cargo(scope, rust_full),
               "fix the failing cargo check/test/clippy")
    else:
        yield ("cargo_green", None, "no Cargo.toml at scope root — not a Rust crate", None)

    yield ("cross_audit", *clause_cross_audit(bundle), "resolve cross-audit findings")


def evaluate(task, scope: Path, bundle: Path, rust_full) -> dict[str, Any]:
    rust = is_rust_scope(scope)
    clauses, unmet, next_action = {}, [], None
    for name, ok, evidence, action in _gather_clauses(task, scope, bundle, rust_full, rust):
        clauses[name] = {"result": _label(ok), "evidence": evidence}
        if ok is False:
            unmet.append(name)
            next_action = next_action or action
    return {
        "converged": not unmet,
        "judge": judge_verdict() if judge_verdict else {"attested": None},
        "task": task,
        "scope": str(scope),
        "rust_scope": rust,
        "clauses": clauses,
        "unmet": unmet,
        "next_action": None if not unmet else (next_action or "address unmet clauses"),
    }


def _label(ok):
    return "N/A" if ok is None else ("PASS" if ok else "FAIL")


def main(argv=None):
    ap = argparse.ArgumentParser(description="Loop Engineering convergence gate.")
    ap.add_argument("--task", required=True, help="Touring decompose task_id")
    ap.add_argument("--scope", default=".", help="path scored for quality/cargo (default: cwd)")
    ap.add_argument("--bundle", default=None, help="OKF bundle dir (baseline + cross-audit); default: --scope")
    ap.add_argument("--rust-full", action="store_true", help="also run cargo test + clippy (the final gate)")
    ap.add_argument("--json", action="store_true", help="emit JSON only")
    ap.add_argument("--quiet", action="store_true", help="no human output, just the exit code")
    args = ap.parse_args(argv)

    scope = Path(args.scope)
    # 22/08/2026: with no --bundle the judge used the SCOPE as bundle, so its
    # `.baseline/orphans-scoped.txt` landed in the repo root — where the elite
    # root-hygiene gate (08_ci_cd_devops, BLOCK tier) flagged it as a stray
    # hidden dir. The judge must never leave artifacts in the tree it judges:
    # bundle-less runs keep their baseline under ~/.claude, keyed by scope.
    if args.bundle:
        bundle = Path(args.bundle)
    else:
        import hashlib
        leaf = hashlib.sha1(str(scope).encode()).hexdigest()[:12]
        bundle = Path.home() / ".claude" / "loop-engineering" / "baselines" / leaf
        bundle.mkdir(parents=True, exist_ok=True)
    report = evaluate(args.task, scope, bundle, args.rust_full)

    if args.json:
        print(json.dumps(report, indent=2))
    elif not args.quiet:
        state = "✅ CONVERGED" if report["converged"] else "🔄 CONTINUE"
        print(f"{state}  task={report['task']}  scope={report['scope']}")
        for name, c in report["clauses"].items():
            glyph = {"PASS": "✅", "FAIL": "❌", "N/A": "➖"}.get(c["result"], "?")
            print(f"  {glyph} {name:<14} {c['evidence']}")
        if not report["converged"]:
            print(f"  → next: {report['next_action']}")

    return 0 if report["converged"] else 1


if __name__ == "__main__":
    sys.exit(main())
