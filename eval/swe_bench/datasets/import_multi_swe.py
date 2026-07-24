#!/usr/bin/env python3
"""import_multi_swe.py - convert a Multi-SWE-bench instance to the touring-eval format.

Master Plan E.W2 (real path). Multi-SWE-bench (ByteDance-Seed/Multi-SWE-bench)
ships, per instance: org/repo/number, base.sha, fix_patch (gold solution diff),
test_patch (diff bringing the f2p/p2p tests), and f2p_tests/p2p_tests dicts.

This importer turns ONE such instance into a harness git-mode EvalInstance:
  * clones the repo at base.sha into a temp dir (NOT the workspace - REGRA #11),
  * reads the pre-fix content of every src file the fix_patch touches  -> `files`
    (oracle context for the solver, the standard SWE-bench "oracle retrieval"),
  * applies fix_patch and reads the post-fix content of those files    -> `gold_files`,
  * carries test_patch verbatim (the harness applies it after checkout),
  * sets fail_to_pass / pass_to_pass from f2p_tests / p2p_tests (p2p capped).

The result is scored exactly like any other harness instance. The clone is a
build-time step confined to a system temp dir.

Usage:
  import_multi_swe.py --dataset <multi-swe.jsonl> --instance-id tokio-rs__bytes-732 \
      --out eval/swe_bench/datasets/multi-swe-bytes-732.jsonl [--p2p-cap 12]
"""
from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Optional


class ImportError_(RuntimeError):
    """Raised when the source repo cannot be prepared into an instance."""


def _run(cmd: str, cwd: Path, timeout: int = 600) -> tuple:
    try:
        p = subprocess.run(cmd, cwd=str(cwd), shell=True, capture_output=True,
                           text=True, timeout=timeout)
        return p.returncode, (p.stdout or "") + (p.stderr or "")
    except subprocess.TimeoutExpired:
        return 124, f"timeout after {timeout}s"


def touched_files(diff: str) -> list:
    """Return the set of `b/` paths a unified diff modifies (excluding /dev/null)."""
    out = []
    for line in diff.splitlines():
        if line.startswith("+++ b/"):
            path = line[len("+++ b/"):].strip()
            if path and path != "/dev/null":
                out.append(path)
    return out


def build_problem(row: dict) -> str:
    """Compose the problem statement from the resolved issue(s) + PR title."""
    parts = [f"# {row.get('title', '').strip()}"]
    for issue in row.get("resolved_issues") or []:
        t = (issue.get("title") or "").strip()
        b = (issue.get("body") or "").strip()
        if t:
            parts.append(f"\nIssue: {t}")
        if b:
            parts.append(b[:4000])
    return "\n".join(p for p in parts if p).strip()


def import_instance(row: dict, p2p_cap: int) -> dict:
    """Clone + diff a Multi-SWE-bench row into a harness git-mode instance dict."""
    org, repo = row["org"], row["repo"]
    sha = row["base"]["sha"]
    url = f"https://github.com/{org}/{repo}.git"
    iid = row.get("instance_id") or f"{org}__{repo}-{row.get('number')}"

    fix_files = touched_files(row.get("fix_patch", ""))
    test_files = set(touched_files(row.get("test_patch", "")))
    # src files = fix targets that the test_patch does not also own
    src_files = [f for f in fix_files if f not in test_files]
    if not src_files:
        raise ImportError_(f"{iid}: fix_patch touches no non-test files")

    tmp = Path(tempfile.mkdtemp(prefix=f"import_{iid}_"))
    try:
        rc, out = _run(f"git clone --quiet --filter=blob:none {url} repo", tmp)
        if rc != 0:
            raise ImportError_(f"clone failed: {out[-300:]}")
        repo_dir = tmp / "repo"
        rc, out = _run(f"git checkout --quiet {sha}", repo_dir)
        if rc != 0:
            raise ImportError_(f"checkout {sha} failed: {out[-300:]}")

        buggy = {}
        for f in src_files:
            p = repo_dir / f
            if not p.exists():
                raise ImportError_(f"src file {f} missing at base")
            buggy[f] = p.read_text(errors="replace")

        (repo_dir / "__fix.patch").write_text(row["fix_patch"])
        rc, out = _run("git apply __fix.patch", repo_dir)
        if rc != 0:
            raise ImportError_(f"fix_patch apply failed: {out[-300:]}")
        gold = {f: (repo_dir / f).read_text(errors="replace") for f in src_files}

        f2p = list((row.get("f2p_tests") or {}).keys())
        if not f2p:
            # No fail_to_pass test => nothing to prove red->green. Some Multi-SWE-bench
            # rows record the change only under s2p/n2p; such instances are not usable
            # by this harness. Skip loudly rather than emit a malformed instance.
            raise ImportError_(f"{iid}: no fail_to_pass tests (empty f2p_tests) — skipped")
        p2p_all = list((row.get("p2p_tests") or {}).keys())
        p2p = p2p_all[:p2p_cap]
        if len(p2p_all) > p2p_cap:
            print(f"  note: p2p sampled {p2p_cap}/{len(p2p_all)} for runtime", file=sys.stderr)

        return {
            "instance_id": iid,
            "problem_statement": build_problem(row),
            "repo": url,
            "base_commit": sha,
            "mode": "git",
            "files": buggy,
            "gold_files": gold,
            "gold_patch": "",
            "test_patch": row.get("test_patch", ""),
            "test_cmd": "cargo test --quiet",
            "fail_to_pass": f2p,
            "pass_to_pass": p2p,
            "setup_cmds": [],
            "aider_resolved": None,
        }
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def main(argv: Optional[list] = None) -> int:
    ap = argparse.ArgumentParser(prog="import-multi-swe",
                                 description="Multi-SWE-bench -> touring-eval instance.")
    ap.add_argument("--dataset", type=Path, required=True)
    ap.add_argument("--instance-id", required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--p2p-cap", type=int, default=12)
    args = ap.parse_args(argv)

    rows = [json.loads(line) for line in args.dataset.read_text().splitlines() if line.strip()]
    match = [r for r in rows
             if (r.get("instance_id") == args.instance_id
                 or f"{r.get('org')}__{r.get('repo')}-{r.get('number')}" == args.instance_id)]
    if not match:
        raise SystemExit(f"instance_id {args.instance_id} not found in {args.dataset}")

    try:
        inst = import_instance(match[0], args.p2p_cap)
    except (ImportError_, KeyError) as e:
        raise SystemExit(f"import failed: {e}")

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(inst) + "\n")
    print(f"wrote {args.out}: {inst['instance_id']} "
          f"({len(inst['files'])} src file(s), {len(inst['fail_to_pass'])} f2p, "
          f"{len(inst['pass_to_pass'])} p2p)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
