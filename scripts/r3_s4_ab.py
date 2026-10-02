#!/usr/bin/env python3
"""S4 forte (CRITERIOS Errata 9): ON x OFF x controle por daemons por projeto, com as mesmas entradas em lockstep.

Three scratch projects (`/tmp/r3s4/{on,off,ctl}`) with a dedicated per-project daemon each, identical but for `[shadow] r3`.
Every input goes to the three in the order ON, OFF, ctl; the four hooks a Bash call touches (pre-bash, cli-suggest, post-bash,
post-tool-rl) are compared byte for byte after the project root is normalised. The control (OFF x ctl) must be identical or
the run is VOID; the verdict is never read off by hand.
"""
import argparse
import hashlib
import json
import os
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r3_event_schema as schema  # noqa: E402
import r3_prove_delivery as pd  # noqa: E402

BASE = Path("/tmp/r3s4")
NAMES = ("on", "off", "ctl")
HOOKS = ("pre-bash", "cli-suggest", "post-bash", "post-tool-rl")
SHIM = Path.home() / ".claude/hooks/touring-hook"
JOURNAL = Path.home() / ".claude/touring/logs/r3_events.jsonl"
STRONG_N = 50                 # E9.4
PREFIX = "s4ab-"              # E9.2: unique session ids, so the real journal can be searched for contamination
KEEP_ENV = {"TOURING_PILLAR_INDUCTION_ARMED"}


def rewrite(payload: dict, root: str, prefix: str = PREFIX) -> dict:
    """A pinned per-project daemon refuses paths outside its tree: real paths move inside the project (same rule for all arms)."""
    text = json.dumps(payload).replace("/home/gabrielgadea/projects/touring", root).replace("/home/gabrielgadea", root + "/home")
    out = json.loads(text)
    out["cwd"] = root
    out["transcript_path"] = root + "/t.jsonl"
    out["session_id"] = prefix + str(payload.get("session_id", ""))
    return out


def exit_code_for(cmd: str) -> int:
    """Deterministic 'outcome' of a command: about a third fail (so the Pensieve learns and pre-bash injects context)."""
    return 1 if int(hashlib.sha256(cmd.encode()).hexdigest()[:2], 16) % 3 == 0 else 0


def post_payload(pre: dict, hook_event: str = "PostToolUse") -> dict:
    cmd = pre["tool_input"]["command"]
    code = exit_code_for(cmd)
    return {**pre, "hook_event_name": hook_event,
            "tool_response": {"stdout": "" if code else "ok", "stderr": "s4 failure" if code else "", "exit_code": code, "interrupted": False}}


def normalise(out: bytes, root: str) -> bytes:
    return out.replace(root.encode(), b"@ROOT@")


def hook_env(root: str, extra: dict | None = None) -> dict:
    env = {k: v for k, v in os.environ.items() if not k.startswith("TOURING_") or k in KEEP_ENV}
    env["CLAUDE_PROJECT_DIR"] = root
    env.update(extra or {})
    return env


def call(root: str, hook: str, payload: dict, extra: dict | None = None) -> bytes:
    r = subprocess.run([str(SHIM), hook], input=json.dumps(payload).encode(), cwd=root, env=hook_env(root, extra),
                       capture_output=True, timeout=90)
    return normalise(r.stdout, root)


def classify(n_nonempty: int) -> str:
    return "forte" if n_nonempty >= STRONG_N else "fraco" if n_nonempty >= 1 else "anulado"


def compare_hook(rows: list[dict]) -> dict:
    """`rows[i]` = {name: bytes} for one hook. ON vs OFF diffs, the control (OFF vs ctl) and the strength."""
    diffs = [i for i, o in enumerate(rows) if o["on"] != o["off"]]
    n_ctl = sum(o["off"] != o["ctl"] for o in rows)
    n_ne = sum(pd.has_content(o["on"]) or pd.has_content(o["off"]) for o in rows)
    return {"n": len(rows), "n_diff": len(diffs), "n_control_diff": n_ctl, "n_nonempty": n_ne, "strength": classify(n_ne),
            "passes": None if n_ctl else not diffs, "first_diff_input": diffs[0] if diffs else None,
            "reason": "control (OFF x ctl) differs: the run is VOID, a diff is never blamed on the flag" if n_ctl else None}


def compare(outs: list[dict]) -> dict:
    return {hook: compare_hook([row[hook] for row in outs]) for hook in HOOKS}


def overall(per_hook: dict, on_events: int) -> dict:
    void = any(h["passes"] is None for h in per_hook.values())
    diff = any(h["passes"] is False for h in per_hook.values())
    idle = on_events < 1
    # A difference fails (unless the control voided the run); an idle emitter makes ON == OFF prove nothing (E9.5): no signal.
    passes = None if void else False if diff else None if idle else True
    return {"passes": passes, "void": void, "any_diff": diff, "on_events": on_events, "idle_emitter": idle,
            "strength": {h: v["strength"] for h, v in per_hook.items()}}


def real_journal_contaminated(sessions: set[str], journal: Path) -> list[str]:
    """E9.6: events of the experiment must NEVER reach the live shadow journal."""
    hashes = {schema.hash_session(s) for s in sessions}
    if not journal.exists():
        return []
    text = journal.read_text()
    return sorted(h for h in hashes if h in text)


def spawn_daemons(roots: dict, on_env: dict) -> None:
    """The first calls of a project fail open while its daemon starts: warm all three the SAME way."""
    for n in NAMES:
        for _ in range(3):
            warm = rewrite({"hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {"command": "echo warm"}, "session_id": "warm"}, roots[n])
            call(roots[n], "pre-bash", warm, on_env if n == "on" else None)
            time.sleep(2)


def drive(rows: list[dict], roots: dict, on_env: dict) -> list[dict]:
    outs = []
    for p in rows:
        pres = {n: rewrite(p, roots[n]) for n in NAMES}
        posts = {n: post_payload(pres[n]) for n in NAMES}
        outs.append({h: {n: call(roots[n], h, (pres if h in ("pre-bash", "cli-suggest") else posts)[n], on_env if n == "on" else None)
                         for n in NAMES} for h in HOOKS})
    return outs


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--inputs", default=str(Path.home() / ".claude/touring/r3-data/s4_inputs.jsonl"))
    ap.add_argument("--out", required=True)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--events", default=str(BASE / "on-events.jsonl"))
    a = ap.parse_args(argv)
    rows = [json.loads(line) for line in Path(a.inputs).read_text().splitlines() if line.strip()]
    rows = rows[: a.limit] if a.limit else rows
    roots = {n: str(BASE / n) for n in NAMES}
    on_env = {"TOURING_R3_EVENTS_FILE": a.events}
    lines_before = JOURNAL.read_text().count("\n") if JOURNAL.exists() else 0
    spawn_daemons(roots, on_env)
    start_events = sum(1 for _ in open(a.events)) if Path(a.events).exists() else 0
    per_hook = compare(drive(rows, roots, on_env))
    events = (sum(1 for _ in open(a.events)) if Path(a.events).exists() else 0) - start_events
    leaked = real_journal_contaminated({PREFIX + str(p.get("session_id", "")) for p in rows}, JOURNAL)
    res = {"n_inputs": len(rows), "per_hook": per_hook, "overall": overall(per_hook, events), "real_journal_contaminated_hashes": leaked,
           "real_journal_lines_added": (JOURNAL.read_text().count("\n") - lines_before) if JOURNAL.exists() else 0}
    Path(a.out).write_text(json.dumps(res, ensure_ascii=False, indent=1))
    print(json.dumps({"overall": res["overall"], "contaminated": leaked}, ensure_ascii=False))
    return 3 if leaked else 0 if res["overall"]["passes"] else 1 if res["overall"]["passes"] is False else 2


if __name__ == "__main__":
    sys.exit(main())
