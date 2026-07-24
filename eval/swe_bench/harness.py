#!/usr/bin/env python3
"""touring-eval - SWE-bench-lite harness for Rust (and polyglot) issue resolution.

Master Plan E.W2.P1.T5 (`touring-eval` SWE-bench-lite Rust). A deterministic,
credit-safe evaluation harness that measures whether a *solver* (TACO/Touring,
Aider, or any patch producer) actually resolves a curated set of real
software-engineering issues - tests go from red to green without regressions.

Design constraints honored:
  * REGRA #11 - never operates git on the Touring workspace. Inline mode writes a
    self-contained mini-repo to a system temp dir; git mode (for external
    SWE-bench instances) confines every git call to a throwaway temp clone.
  * Memory [Code Analyses, LLM Synthesises] - this harness NEVER invokes an LLM.
    Solvers are a pluggable interface; the built-ins (GoldSolver, FilePatchSolver)
    are 100% deterministic. A model-backed solver is a documented, manually
    credit-authorized extension - it is intentionally NOT built in, so running
    the harness can never burn API credits on its own.

Metrics (exactly those E.W2.P1.T5 enumerates):
  * resolved_pct            - fraction of instances actually resolved
  * vgp_false_positive_rate - solver CLAIMED resolved but the harness PROVED it
                              did not (the SWE-bench-for-Touring signal the plan
                              calls out: "medir false-positive rate alem de resolved%")
  * mean_tokens             - solver-reported token cost (0 for deterministic solvers)
  * aider comparison        - optional side-by-side when an Aider result is supplied

Subcommands:
  selftest      run the bundled self-test dataset with the gold solver; assert 100%
  validate      structural + correctness check (gold must turn red tests green,
                and the fail_to_pass tests must genuinely be red pre-patch)
  run           evaluate a dataset with a chosen solver; emit JSON report; CI --check
  emit-dataset  materialize the bundled self-test instances as a JSONL example

Usage:
  eval/swe_bench/harness.py selftest
  eval/swe_bench/harness.py validate --dataset eval/swe_bench/datasets/touring-rust-selftest.jsonl
  eval/swe_bench/harness.py run --dataset <f> --solver gold --out report.json
  eval/swe_bench/harness.py run --dataset <f> --solver file:/path/to/patches --check --threshold 0.30
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

DEFAULT_TIMEOUT = 600
HERE = Path(__file__).resolve().parent
DEFAULT_DATASET = HERE / "datasets" / "touring-rust-selftest.jsonl"


# --------------------------------------------------------------------------- #
# Data model (SWE-bench-lite compatible)
# --------------------------------------------------------------------------- #
@dataclass
class EvalInstance:
    """One benchmark issue.

    Two resolution-source flavors are supported so the harness is robust both for
    self-contained fixtures and for real external repos:
      * inline mode: `files` carries the initial mini-repo tree; resolution is a
        full-file replacement map `gold_files` (no diff-apply fragility).
      * git mode:    `repo` + `base_commit` are cloned/checked-out in a temp dir;
        resolution is a unified diff `gold_patch` applied with `git apply`.
    """

    instance_id: str
    problem_statement: str = ""
    repo: str = "inline"
    base_commit: str = "INLINE"
    mode: str = "inline"  # "inline" | "git"
    files: dict = field(default_factory=dict)
    gold_files: dict = field(default_factory=dict)
    gold_patch: str = ""
    test_patch: str = ""  # git mode: diff bringing the f2p/p2p tests, applied after checkout
    test_cmd: str = "cargo test --quiet"
    fail_to_pass: list = field(default_factory=list)
    pass_to_pass: list = field(default_factory=list)
    setup_cmds: list = field(default_factory=list)
    aider_resolved: Optional[bool] = None

    @classmethod
    def from_dict(cls, d: dict) -> "EvalInstance":
        known = {f.name for f in cls.__dataclass_fields__.values()}  # type: ignore[attr-defined]
        return cls(**{k: v for k, v in d.items() if k in known})

    def to_dict(self) -> dict:
        return {
            "instance_id": self.instance_id,
            "problem_statement": self.problem_statement,
            "repo": self.repo,
            "base_commit": self.base_commit,
            "mode": self.mode,
            "files": self.files,
            "gold_files": self.gold_files,
            "gold_patch": self.gold_patch,
            "test_patch": self.test_patch,
            "test_cmd": self.test_cmd,
            "fail_to_pass": self.fail_to_pass,
            "pass_to_pass": self.pass_to_pass,
            "setup_cmds": self.setup_cmds,
            "aider_resolved": self.aider_resolved,
        }

    def structural_problems(self) -> list:
        probs = []
        if not self.instance_id:
            probs.append("missing instance_id")
        if not self.fail_to_pass:
            probs.append("fail_to_pass is empty (an instance must have a test that starts red)")
        if self.mode == "inline" and not self.files:
            probs.append("inline mode requires a non-empty `files` tree")
        if self.mode == "inline" and not self.gold_files and not self.gold_patch:
            probs.append("inline mode requires `gold_files` or `gold_patch`")
        if self.mode == "git" and (self.repo == "inline" or self.base_commit in ("", "INLINE")):
            probs.append("git mode requires a real `repo` and `base_commit`")
        if self.mode not in ("inline", "git"):
            probs.append(f"unknown mode {self.mode!r}")
        return probs


@dataclass
class SolverReport:
    """A solver's answer for one instance."""

    solver_name: str
    patch_files: dict = field(default_factory=dict)
    patch_diff: str = ""
    tokens: int = 0
    claims_resolved: bool = True

    def is_empty(self) -> bool:
        return not self.patch_files and not self.patch_diff.strip()


# --------------------------------------------------------------------------- #
# Solvers (pluggable; NO LLM built in - see module docstring)
# --------------------------------------------------------------------------- #
class Solver(ABC):
    name = "abstract"

    @abstractmethod
    def solve(self, inst: EvalInstance) -> SolverReport:  # pragma: no cover - interface
        ...


class GoldSolver(Solver):
    """Returns the reference resolution. Used for selftest/validate and as the
    upper-bound CI baseline (proves the dataset + harness are well-formed)."""

    name = "gold"

    def solve(self, inst: EvalInstance) -> SolverReport:
        return SolverReport(
            solver_name=self.name,
            patch_files=dict(inst.gold_files),
            patch_diff=inst.gold_patch,
            tokens=0,
            claims_resolved=True,
        )


class FilePatchSolver(Solver):
    """Reads pre-computed solver output from a directory. This is the credit-safe
    bridge to *any* external solver (TACO, Aider, a model agent): run that solver
    OUT OF BAND, dump its patches here, then score them deterministically.

    Layout per instance <id>:
      <root>/<id>.files.json   -> {"path": "full new content", ...}   (inline mode)
      <root>/<id>.patch        -> unified diff                        (git mode)
      <root>/<id>.meta.json    -> {"tokens": int, "claims_resolved": bool}
    Missing output => empty patch, claims_resolved=False (an honest non-answer).
    """

    name = "file"

    def __init__(self, root: str):
        self.root = Path(root)

    def solve(self, inst: EvalInstance) -> SolverReport:
        files_p = self.root / f"{inst.instance_id}.files.json"
        diff_p = self.root / f"{inst.instance_id}.patch"
        meta_p = self.root / f"{inst.instance_id}.meta.json"
        patch_files: dict = {}
        patch_diff = ""
        tokens = 0
        claims = False
        if files_p.exists():
            patch_files = json.loads(files_p.read_text())
            claims = True
        if diff_p.exists():
            patch_diff = diff_p.read_text()
            claims = True
        if meta_p.exists():
            meta = json.loads(meta_p.read_text())
            tokens = int(meta.get("tokens", 0))
            claims = bool(meta.get("claims_resolved", claims))
        return SolverReport(
            solver_name=self.name,
            patch_files=patch_files,
            patch_diff=patch_diff,
            tokens=tokens,
            claims_resolved=claims,
        )


def build_solver(spec: str) -> Solver:
    if spec == "gold":
        return GoldSolver()
    if spec.startswith("file:"):
        return FilePatchSolver(spec[len("file:"):])
    raise SystemExit(f"unknown solver spec {spec!r} (use 'gold' or 'file:<dir>')")


# --------------------------------------------------------------------------- #
# Results
# --------------------------------------------------------------------------- #
@dataclass
class InstanceResult:
    instance_id: str
    resolved: bool
    applied: bool
    claims_resolved: bool
    vgp_false_positive: bool
    tokens: int
    fail_to_pass: dict = field(default_factory=dict)
    pass_to_pass: dict = field(default_factory=dict)
    regressions: list = field(default_factory=list)
    malformed: list = field(default_factory=list)
    aider_resolved: Optional[bool] = None
    error: Optional[str] = None

    def to_dict(self) -> dict:
        return self.__dict__.copy()


@dataclass
class Report:
    solver_name: str
    results: list = field(default_factory=list)

    @property
    def total(self) -> int:
        return len(self.results)

    @property
    def resolved(self) -> int:
        return sum(1 for r in self.results if r.resolved)

    @property
    def resolved_pct(self) -> float:
        return (self.resolved / self.total) if self.total else 0.0

    @property
    def claimed(self) -> int:
        return sum(1 for r in self.results if r.claims_resolved)

    @property
    def vgp_false_positives(self) -> int:
        return sum(1 for r in self.results if r.vgp_false_positive)

    @property
    def vgp_false_positive_rate(self) -> float:
        return (self.vgp_false_positives / self.claimed) if self.claimed else 0.0

    @property
    def mean_tokens(self) -> float:
        return (sum(r.tokens for r in self.results) / self.total) if self.total else 0.0

    @property
    def aider_resolved(self) -> int:
        return sum(1 for r in self.results if r.aider_resolved)

    @property
    def aider_known(self) -> int:
        return sum(1 for r in self.results if r.aider_resolved is not None)

    def to_dict(self) -> dict:
        out = {
            "solver": self.solver_name,
            "total": self.total,
            "resolved": self.resolved,
            "resolved_pct": round(self.resolved_pct, 4),
            "claimed_resolved": self.claimed,
            "vgp_false_positives": self.vgp_false_positives,
            "vgp_false_positive_rate": round(self.vgp_false_positive_rate, 4),
            "mean_tokens": round(self.mean_tokens, 2),
            "results": [r.to_dict() for r in self.results],
        }
        if self.aider_known:
            out["comparison"] = {
                "aider_resolved": self.aider_resolved,
                "aider_known": self.aider_known,
                "aider_resolved_pct": round(self.aider_resolved / self.aider_known, 4),
                "delta_vs_aider": round(self.resolved_pct - (self.aider_resolved / self.aider_known), 4),
            }
        return out

    def summary(self) -> str:
        lines = [
            f"touring-eval report  (solver={self.solver_name})",
            f"  instances        : {self.total}",
            f"  resolved         : {self.resolved}/{self.total}  ({self.resolved_pct*100:.1f}%)",
            f"  VGP false-pos    : {self.vgp_false_positives}/{self.claimed}  "
            f"({self.vgp_false_positive_rate*100:.1f}% of claimed)",
            f"  mean tokens      : {self.mean_tokens:.0f}",
        ]
        if self.aider_known:
            ap = self.aider_resolved / self.aider_known
            lines.append(
                f"  vs Aider         : {self.resolved_pct*100:.1f}% vs {ap*100:.1f}%  "
                f"(delta {(self.resolved_pct-ap)*100:+.1f} pp over {self.aider_known} shared)"
            )
        return "\n".join(lines)


# --------------------------------------------------------------------------- #
# Harness
# --------------------------------------------------------------------------- #
class Harness:
    def __init__(self, workdir_root: Optional[str] = None, keep: bool = False,
                 timeout: int = DEFAULT_TIMEOUT, strict_pretest: bool = True):
        self.workdir_root = workdir_root
        self.keep = keep
        self.timeout = timeout
        self.strict_pretest = strict_pretest

    @staticmethod
    def _run(cmd: str, cwd: Path, env: Optional[dict], timeout: int) -> tuple:
        try:
            p = subprocess.run(
                cmd, cwd=str(cwd), env=env, shell=True,
                capture_output=True, text=True, timeout=timeout,
            )
            return p.returncode, (p.stdout or "") + (p.stderr or "")
        except subprocess.TimeoutExpired:
            return 124, f"TIMEOUT after {timeout}s"
        except Exception as e:  # noqa: BLE001 - fail closed, never crash the run
            return 1, f"EXEC_ERROR: {e}"

    def _write_tree(self, dest: Path, tree: dict) -> None:
        for rel, content in tree.items():
            target = dest / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(content)

    def _materialize(self, inst: EvalInstance, dest: Path, env: dict) -> Optional[str]:
        if inst.mode == "inline":
            self._write_tree(dest, inst.files)
        elif inst.mode == "git":
            rc, out = self._run(f"git clone --quiet {inst.repo} .", dest, env, self.timeout)
            if rc != 0:
                return f"git clone failed: {out[-400:]}"
            rc, out = self._run(f"git checkout --quiet {inst.base_commit}", dest, env, self.timeout)
            if rc != 0:
                return f"git checkout failed: {out[-400:]}"
            if inst.test_patch.strip():
                # Bring the f2p/p2p tests onto the base checkout (SWE-bench semantics).
                tp = dest / "__test.patch"
                tp.write_text(inst.test_patch)
                rc, out = self._run("git apply __test.patch", dest, env, self.timeout)
                tp.unlink(missing_ok=True)
                if rc != 0:
                    return f"test_patch apply failed: {out[-400:]}"
        else:
            return f"unknown mode {inst.mode!r}"
        for cmd in inst.setup_cmds:
            rc, out = self._run(cmd, dest, env, self.timeout)
            if rc != 0:
                return f"setup_cmd failed ({cmd}): {out[-400:]}"
        return None

    def _apply(self, report: SolverReport, inst: EvalInstance, dest: Path, env: dict) -> Optional[str]:
        if report.patch_files:
            self._write_tree(dest, report.patch_files)
        if report.patch_diff.strip():
            # Prefer git apply when the instance is a git checkout; else fall back to `patch`.
            applier = "git apply" if inst.mode == "git" else "patch -p1 --forward --no-backup-if-mismatch -r -"
            diff_path = dest / "__candidate.patch"
            diff_path.write_text(report.patch_diff)
            cmd = ("git apply __candidate.patch" if inst.mode == "git"
                   else "patch -p1 --forward --no-backup-if-mismatch < __candidate.patch")
            rc, out = self._run(cmd, dest, env, self.timeout)
            diff_path.unlink(missing_ok=True)
            if rc != 0:
                return f"patch apply failed via {applier!r}: {out[-400:]}"
        if report.is_empty():
            return "solver produced no patch"
        return None

    def _test(self, dest: Path, test_cmd: str, name: str, env: dict) -> bool:
        rc, _ = self._run(f"{test_cmd} {name}".strip(), dest, env, self.timeout)
        return rc == 0

    def run_one(self, inst: EvalInstance, solver: Solver) -> InstanceResult:
        res = InstanceResult(
            instance_id=inst.instance_id, resolved=False, applied=False,
            claims_resolved=False, vgp_false_positive=False, tokens=0,
            aider_resolved=inst.aider_resolved,
        )
        res.malformed = inst.structural_problems()
        if res.malformed:
            res.error = "malformed instance"
            return res

        root = Path(self.workdir_root) if self.workdir_root else None
        dest = Path(tempfile.mkdtemp(prefix=f"sweb_{inst.instance_id}_", dir=root))
        # Isolate cargo so we never touch the workspace target dir.
        env = dict(os.environ)
        env["CARGO_TARGET_DIR"] = str(dest / "target")
        try:
            err = self._materialize(inst, dest, env)
            if err:
                res.error = err
                return res

            # Pre-patch correctness gate: fail_to_pass MUST start red, else the
            # instance is malformed (a "fix" for an already-passing test is bogus).
            if self.strict_pretest:
                for t in inst.fail_to_pass:
                    if self._test(dest, inst.test_cmd, t, env):
                        res.malformed.append(f"fail_to_pass test '{t}' already passes pre-patch")
                if res.malformed:
                    res.error = "malformed instance (pre-test gate)"
                    return res

            report = solver.solve(inst)
            res.claims_resolved = report.claims_resolved
            res.tokens = report.tokens
            apply_err = self._apply(report, inst, dest, env)
            res.applied = apply_err is None
            if apply_err:
                res.error = apply_err
                # claimed but couldn't even apply => VGP false positive
                res.vgp_false_positive = report.claims_resolved
                return res

            f2p_ok = True
            for t in inst.fail_to_pass:
                ok = self._test(dest, inst.test_cmd, t, env)
                res.fail_to_pass[t] = ok
                f2p_ok = f2p_ok and ok
            p2p_ok = True
            for t in inst.pass_to_pass:
                ok = self._test(dest, inst.test_cmd, t, env)
                res.pass_to_pass[t] = ok
                if not ok:
                    res.regressions.append(t)
                p2p_ok = p2p_ok and ok

            res.resolved = f2p_ok and p2p_ok
            res.vgp_false_positive = report.claims_resolved and not res.resolved
            return res
        finally:
            if not self.keep:
                shutil.rmtree(dest, ignore_errors=True)

    def run_all(self, instances: list, solver: Solver) -> Report:
        rep = Report(solver_name=solver.name)
        for inst in instances:
            rep.results.append(self.run_one(inst, solver))
        return rep


# --------------------------------------------------------------------------- #
# Bundled self-test dataset (real, runnable, credit-free)
# --------------------------------------------------------------------------- #
def builtin_selftest_instances() -> list:
    """Two self-contained instances proving the harness end-to-end.

    1. A real Rust crate (cargo test): `safe_add` overflows; gold uses checked_add.
    2. A Python instance (no toolchain build): proves harness logic instantly and
       exercises the non-Rust path of "SWE-bench-lite Rust/Python".
    """
    rust_buggy = (
        "pub fn safe_add(a: i32, b: i32) -> Option<i32> {\n"
        "    // BUG: silent overflow; i32::MAX + 1 panics in debug / wraps in release.\n"
        "    Some(a + b)\n"
        "}\n\n"
        "#[cfg(test)]\n"
        "mod tests {\n"
        "    use super::*;\n"
        "    #[test]\n"
        "    fn test_basic_add() {\n"
        "        assert_eq!(safe_add(2, 3), Some(5));\n"
        "    }\n"
        "    #[test]\n"
        "    fn test_no_overflow() {\n"
        "        assert_eq!(safe_add(i32::MAX, 1), None);\n"
        "    }\n"
        "}\n"
    )
    rust_fixed = rust_buggy.replace("    Some(a + b)\n", "    a.checked_add(b)\n").replace(
        "    // BUG: silent overflow; i32::MAX + 1 panics in debug / wraps in release.\n", ""
    )
    rust_cargo = (
        '[package]\nname = "arith"\nversion = "0.0.0"\nedition = "2021"\n\n'
        "[lib]\npath = \"src/lib.rs\"\n"
    )

    py_buggy = (
        "def safe_div(a, b):\n"
        "    # BUG: no zero guard; raises ZeroDivisionError instead of returning None.\n"
        "    return a / b\n"
    )
    py_fixed = (
        "def safe_div(a, b):\n"
        "    if b == 0:\n"
        "        return None\n"
        "    return a / b\n"
    )
    py_runner = (
        "import sys\n"
        "from solution import safe_div\n"
        "def test_basic_div():\n"
        "    assert safe_div(6, 3) == 2\n"
        "def test_zero_guard():\n"
        "    assert safe_div(1, 0) is None\n"
        "if __name__ == '__main__':\n"
        "    name = sys.argv[1] if len(sys.argv) > 1 else ''\n"
        "    fn = {'test_basic_div': test_basic_div, 'test_zero_guard': test_zero_guard}.get(name)\n"
        "    if fn is None:\n"
        "        print(f'no such test: {name}'); sys.exit(2)\n"
        "    fn(); print('ok')\n"
    )

    return [
        EvalInstance(
            instance_id="touring-selftest__rust-checked-add",
            problem_statement="safe_add overflows on i32::MAX; it should return None instead of panicking.",
            mode="inline",
            files={"Cargo.toml": rust_cargo, "src/lib.rs": rust_buggy},
            gold_files={"src/lib.rs": rust_fixed},
            test_cmd="cargo test --quiet",
            fail_to_pass=["test_no_overflow"],
            pass_to_pass=["test_basic_add"],
            aider_resolved=True,
        ),
        EvalInstance(
            instance_id="touring-selftest__py-zero-guard",
            problem_statement="safe_div raises on division by zero; it should return None.",
            mode="inline",
            files={"solution.py": py_buggy, "run_tests.py": py_runner},
            gold_files={"solution.py": py_fixed},
            test_cmd="python3 run_tests.py",
            fail_to_pass=["test_zero_guard"],
            pass_to_pass=["test_basic_div"],
            aider_resolved=False,
        ),
    ]


# --------------------------------------------------------------------------- #
# Dataset IO
# --------------------------------------------------------------------------- #
def load_dataset(path: Path) -> list:
    insts = []
    for i, line in enumerate(path.read_text().splitlines(), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        try:
            insts.append(EvalInstance.from_dict(json.loads(line)))
        except Exception as e:  # noqa: BLE001
            raise SystemExit(f"{path}:{i}: invalid JSONL instance: {e}")
    return insts


def emit_dataset(path: Path) -> int:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w") as fh:
        for inst in builtin_selftest_instances():
            fh.write(json.dumps(inst.to_dict()) + "\n")
    print(f"wrote {path} ({len(builtin_selftest_instances())} instances)")
    return 0


# --------------------------------------------------------------------------- #
# CLI
# --------------------------------------------------------------------------- #
def _add_run_args(p: argparse.ArgumentParser) -> None:
    p.add_argument("--dataset", type=Path, default=DEFAULT_DATASET)
    p.add_argument("--solver", default="gold", help="'gold' or 'file:<dir>'")
    p.add_argument("--workdir", default=None, help="temp root (default: system tmp)")
    p.add_argument("--out", type=Path, default=None, help="write JSON report here")
    p.add_argument("--keep", action="store_true", help="keep temp workdirs for inspection")
    p.add_argument("--timeout", type=int, default=DEFAULT_TIMEOUT)
    p.add_argument("--no-strict-pretest", action="store_true",
                   help="skip the 'fail_to_pass must be red pre-patch' correctness gate")
    p.add_argument("--check", action="store_true", help="exit 1 if resolved_pct < threshold")
    p.add_argument("--threshold", type=float, default=1.0,
                   help="min resolved_pct for --check (default 1.0; CI baseline)")
    p.add_argument("--json", action="store_true", help="print JSON report to stdout")


def _do_run(args, instances: list) -> tuple:
    harness = Harness(
        workdir_root=args.workdir, keep=args.keep, timeout=args.timeout,
        strict_pretest=not args.no_strict_pretest,
    )
    solver = build_solver(args.solver)
    rep = harness.run_all(instances, solver)
    if args.json:
        print(json.dumps(rep.to_dict(), indent=2))
    else:
        print(rep.summary())
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(rep.to_dict(), indent=2))
        print(f"  report -> {args.out}", file=sys.stderr)
    return rep, 0


def _report_struct(inst: EvalInstance) -> bool:
    probs = inst.structural_problems()
    if probs:
        print(f"  STRUCT FAIL {inst.instance_id}: {probs}", file=sys.stderr)
    return bool(probs)


def _cmd_selftest(args: argparse.Namespace) -> int:
    harness = Harness(keep=args.keep, timeout=args.timeout, strict_pretest=True)
    rep = harness.run_all(builtin_selftest_instances(), GoldSolver())
    print(json.dumps(rep.to_dict(), indent=2) if args.json else rep.summary())
    ok = rep.resolved == rep.total and rep.total > 0
    for r in rep.results:
        if not r.resolved:
            print(f"  FAIL {r.instance_id}: {r.error or r.regressions or 'unresolved'}",
                  file=sys.stderr)
    print(("SELFTEST PASS" if ok else "SELFTEST FAIL"), file=sys.stderr)
    return 0 if ok else 1


def _cmd_validate(args: argparse.Namespace) -> int:
    instances = (builtin_selftest_instances()
                 if not args.dataset.exists() else load_dataset(args.dataset))
    bad = sum(1 for inst in instances if _report_struct(inst))
    harness = Harness(timeout=args.timeout, strict_pretest=True)
    rep = harness.run_all(instances, GoldSolver())
    for r in rep.results:
        if r.malformed:
            bad += 1
            print(f"  CORRECTNESS FAIL {r.instance_id}: {r.malformed}", file=sys.stderr)
        elif not r.resolved:
            bad += 1
            print(f"  GOLD-UNRESOLVED {r.instance_id}: {r.error or r.regressions}", file=sys.stderr)
    if args.json:
        print(json.dumps(rep.to_dict(), indent=2))
    print(f"validate: {rep.resolved}/{rep.total} gold-resolved, {bad} problem(s)", file=sys.stderr)
    return 0 if bad == 0 else 1


def _cmd_run(args: argparse.Namespace) -> int:
    instances = load_dataset(args.dataset)
    rep, _ = _do_run(args, instances)
    if args.check:
        ok = rep.resolved_pct >= args.threshold
        print(f"  --check: resolved_pct {rep.resolved_pct:.3f} "
              f"{'>=' if ok else '<'} threshold {args.threshold:.3f}", file=sys.stderr)
        return 0 if ok else 1
    return 0


def main(argv: Optional[list] = None) -> int:
    ap = argparse.ArgumentParser(
        prog="touring-eval", description="SWE-bench-lite harness for Rust/Python issue resolution.")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p_self = sub.add_parser("selftest", help="run bundled self-test with gold solver; assert all resolved")
    p_self.add_argument("--json", action="store_true")
    p_self.add_argument("--keep", action="store_true")
    p_self.add_argument("--timeout", type=int, default=DEFAULT_TIMEOUT)

    p_val = sub.add_parser("validate", help="structural + gold-resolves correctness check")
    p_val.add_argument("--dataset", type=Path, default=DEFAULT_DATASET)
    p_val.add_argument("--json", action="store_true")
    p_val.add_argument("--timeout", type=int, default=DEFAULT_TIMEOUT)

    p_run = sub.add_parser("run", help="evaluate a dataset with a chosen solver")
    _add_run_args(p_run)

    p_emit = sub.add_parser("emit-dataset", help="materialize bundled self-test as JSONL")
    p_emit.add_argument("--out", type=Path, default=DEFAULT_DATASET)

    args = ap.parse_args(argv)
    handlers = {
        "emit-dataset": lambda: emit_dataset(args.out),
        "selftest": lambda: _cmd_selftest(args),
        "validate": lambda: _cmd_validate(args),
        "run": lambda: _cmd_run(args),
    }
    return handlers[args.cmd]()


if __name__ == "__main__":
    raise SystemExit(main())
