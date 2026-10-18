"""Mutation harness: apply ONE textual mutant at a time to a COPY of client/hooks, run the pinned tests, record killed/survived.

Usage: mutate.py <spec.py> <out.json>
spec.py defines MUTANTS = [(id, relative_file, old, new, [test files])]; `old` must occur exactly once.
"""
import importlib.util
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

SRC = Path("/home/gabrielgadea/projects/touring-wt-ecc-py")
MUT = Path(os.environ.get(
    "MUT_DIR", "/tmp/claude-1000/-home-gabrielgadea-projects-touring/efe418c2-95f7-4a62-8bac-57080ed455b8/scratchpad/mut"))

spec = importlib.util.spec_from_file_location("mutant_spec", sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
mutants = module.MUTANTS
extra = getattr(module, "COPY", [])

shutil.copytree(SRC / "client" / "hooks", MUT / "client" / "hooks", dirs_exist_ok=True)
for rel in extra:
    dst = MUT / rel
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(SRC / rel, dst)

results = []
for mid, rel, old, new, tests in mutants:
    original = (SRC / rel).read_text(encoding="utf-8")
    count = original.count(old)
    if count != 1:
        results.append({"id": mid, "status": "INVALID", "detail": f"old occurs {count}x"})
        print(mid, "INVALID", count, flush=True)
        continue
    target = MUT / rel
    target.write_text(original.replace(old, new), encoding="utf-8")
    assert target.read_text(encoding="utf-8") != original, f"{mid}: the swap did not happen"
    try:
        proc = subprocess.run(
            [sys.executable, "-m", "pytest", *tests, "-x", "-q", "-p", "no:cacheprovider", "-W", "error::UserWarning"],
            cwd=MUT, capture_output=True, text=True, timeout=400, check=False)
        killed = proc.returncode != 0
        tail = [ln for ln in proc.stdout.splitlines() if ln.startswith("FAILED") or " passed" in ln or " failed" in ln][-2:]
    except subprocess.TimeoutExpired:
        killed, tail = True, ["TIMEOUT (counts as killed: the mutant hangs the suite)"]
    target.write_text(original, encoding="utf-8")
    results.append({"id": mid, "status": "killed" if killed else "SURVIVED", "detail": tail})
    print(mid, "killed" if killed else "SURVIVED", tail, flush=True)

Path(sys.argv[2]).write_text(json.dumps(results, ensure_ascii=False, indent=1), encoding="utf-8")
k = sum(r["status"] == "killed" for r in results)
print(f"RESULT {k}/{len(results)} killed; survivors:", [r["id"] for r in results if r["status"] != "killed"])
