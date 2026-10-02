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
import re
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r3_event_schema as schema  # noqa: E402
import r3_prove_delivery as pd  # noqa: E402

BASE = Path("/tmp/r3s4b")     # pristine projects: the first set (/tmp/r3s4) took the exploratory probes (E9.8-g) and was discarded
NAMES = ("on", "off", "ctl")
HOOKS = ("pre-bash", "cli-suggest", "post-bash", "post-tool-rl")
SHIM = Path.home() / ".claude/hooks/touring-hook"
JOURNAL = Path.home() / ".claude/touring/logs/r3_events.jsonl"
STRONG_N = 50                 # E9.4
NOISE_SLACK, NOISE_FRAC = 2, 0.25   # E9.8-m: the flag "exceeds the noise" iff max(ON-OFF, ON-ctl) - (OFF-ctl) > 2 + 25% of (OFF-ctl)
PREFIX = "s4ab-"              # E9.2: unique session ids, so the real journal can be searched for contamination
KEEP_ENV = {"TOURING_PILLAR_INDUCTION_ARMED"}


def rewrite(payload: dict, root: str, prefix: str = PREFIX) -> dict:
    """The Pensieve hashes the WHOLE command (post_bash FIX6): the root of an arm inside the command text made two identical daemons
    learn different things (E9.8-n). Real /home/gabrielgadea paths therefore become RELATIVE (cwd = root), so the command text is
    byte-identical in every arm and no hook reads the live repository. The daemon pin only checks `project_root` (cwd), never the
    command text (touring-dispatch daemon.rs `pin_admits`); other absolute paths (/dev/null, /tmp/...) stay and are equal in all arms."""
    text = (json.dumps(payload).replace("/home/gabrielgadea/projects/touring/", "").replace("/home/gabrielgadea/projects/touring", ".")
            .replace("/home/gabrielgadea/", "home/").replace("/home/gabrielgadea", "home"))
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


VOLATILE = ((rb"reliability \d+%", b"reliability N%"), (rb"velocity [\d.]+ edits/day", b"velocity V edits/day"),
            (rb"h\xc3\xa1 \d+s", b"h\xc3\xa1 Ns"))   # E9.8-j: ONLY fields seen varying between OFF and the control (identical arms)


def normalise(out: bytes, root: str) -> bytes:
    out = out.replace(root.encode(), b"@ROOT@")
    for pattern, repl in VOLATILE:
        out = re.sub(pattern, repl, out)
    return out


def order_for(i: int) -> tuple:
    """E9.8-k: rotate ON/OFF/ctl per input, so a fixed call order never lines up with an arm."""
    k = i % len(NAMES)
    return NAMES[k:] + NAMES[:k]


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
    n_on_ctl = sum(o["on"] != o["ctl"] for o in rows)
    odd = [i for i, o in enumerate(rows) if o["on"] != o["off"] or o["off"] != o["ctl"]][:4]
    cut = lambda b: b[:400].decode(errors="replace")  # noqa: E731
    excess = max(len(diffs), n_on_ctl) - n_ctl
    return {"n": len(rows), "n_diff": len(diffs), "n_on_ctl_diff": n_on_ctl, "n_control_diff": n_ctl, "n_nonempty": n_ne,
            "strength": classify(n_ne), "passes": None if n_ctl else not diffs, "first_diff_input": diffs[0] if diffs else None,
            "reason": "control (OFF x ctl) differs: the run is VOID, a diff is never blamed on the flag" if n_ctl else None,
            "noise_excess": excess, "noise_read": "indistinguishable" if excess <= NOISE_SLACK + NOISE_FRAC * n_ctl else "exceeds_noise",
            "samples": [{"input": i, **{k: cut(rows[i][k]) for k in ("on", "off", "ctl")}} for i in odd]}


def compare(outs: list[dict]) -> dict:
    return {hook: compare_hook([row[hook] for row in outs]) for hook in HOOKS}


def overall(per_hook: dict, on_events: int) -> dict:
    void = any(h["passes"] is None for h in per_hook.values())
    diff = any(h["passes"] is False for h in per_hook.values())
    idle = on_events < 1
    # A difference fails (unless the control voided the run); an idle emitter makes ON == OFF prove nothing (E9.5): no signal.
    passes = None if void else False if diff else None if idle else True
    noise = "no_signal_idle_emitter" if idle else ("indistinguishable" if all(v["noise_read"] == "indistinguishable" for v in per_hook.values()) else "exceeds_noise")
    return {"passes": passes, "void": void, "any_diff": diff, "on_events": on_events, "idle_emitter": idle, "noise_read": noise,
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


def drive(rows: list[dict], roots: dict, on_env: dict, pause: float = 0.0) -> list[dict]:
    """Per input and hook, the three arms in a ROTATING order (E9.8-k); `pause` after each call lets a daemon's async work
    settle (E9.8-l: the Pensieve skips a failure it cannot borrow, a race the control exposes)."""
    outs = []
    for idx, p in enumerate(rows):
        pres = {n: rewrite(p, roots[n]) for n in NAMES}
        posts = {n: post_payload(pres[n]) for n in NAMES}
        row = {h: {} for h in HOOKS}
        for h in HOOKS:
            for n in order_for(idx):
                payload = (pres if h in ("pre-bash", "cli-suggest") else posts)[n]
                row[h][n] = call(roots[n], h, payload, on_env if n == "on" else None)
                time.sleep(pause)
        outs.append(row)
    return outs


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--inputs", default=str(Path.home() / ".claude/touring/r3-data/s4_inputs.jsonl"))
    ap.add_argument("--out", required=True)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--base", default=str(BASE), help="directory holding the on/off/ctl projects")
    ap.add_argument("--events", default="")
    ap.add_argument("--pause", type=float, default=0.2, help="seconds after every hook call (E9.8-l)")
    a = ap.parse_args(argv)
    a.events = a.events or str(Path(a.base) / "on-events.jsonl")
    rows = [json.loads(line) for line in Path(a.inputs).read_text().splitlines() if line.strip()]
    rows = rows[: a.limit] if a.limit else rows
    roots = {n: str(Path(a.base) / n) for n in NAMES}
    on_env = {"TOURING_R3_EVENTS_FILE": a.events}
    lines_before = JOURNAL.read_text().count("\n") if JOURNAL.exists() else 0
    spawn_daemons(roots, on_env)
    start_events = sum(1 for _ in open(a.events)) if Path(a.events).exists() else 0
    per_hook = compare(drive(rows, roots, on_env, a.pause))
    events = (sum(1 for _ in open(a.events)) if Path(a.events).exists() else 0) - start_events
    leaked = real_journal_contaminated({PREFIX + str(p.get("session_id", "")) for p in rows}, JOURNAL)
    res = {"n_inputs": len(rows), "per_hook": per_hook, "overall": overall(per_hook, events), "real_journal_contaminated_hashes": leaked,
           "real_journal_lines_added": (JOURNAL.read_text().count("\n") - lines_before) if JOURNAL.exists() else 0}
    Path(a.out).write_text(json.dumps(res, ensure_ascii=False, indent=1))
    print(json.dumps({"overall": res["overall"], "contaminated": leaked}, ensure_ascii=False))
    return 3 if leaked else 0 if res["overall"]["passes"] else 1 if res["overall"]["passes"] is False else 2


if __name__ == "__main__":
    sys.exit(main())
