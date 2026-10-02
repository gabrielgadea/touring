#!/usr/bin/env python3
"""S-12 — build the inputs of the S4 proof (byte-identical `additionalContext`, shadow ON vs OFF): REAL Bash commands
from real Claude Code transcripts, shaped as `PreToolUse`/`Bash` hook inputs.

    r3_make_s4_inputs.py --projeto-dir ~/.claude/projects/<dir> --out ~/.claude/touring/r3-data/s4_inputs.jsonl [--n 200]

Deterministic (seeded), deduplicated, drawn ACROSS sessions (round-robin, not from one), commands over MAX_CHARS refused.
The session id in each payload is a hash — the raw transcript name never travels. The output holds real command text,
so it is written OUTSIDE the repo (the repo is public); this script writes nothing else.
"""
import argparse
import hashlib
import json
import random
from pathlib import Path

SEED = 20261002
MAX_CHARS = 4000


def _commands(path: Path):
    for line in path.read_text(errors="replace").splitlines():
        try:
            content = (json.loads(line).get("message") or {}).get("content")
        except ValueError:
            continue
        for block in content if isinstance(content, list) else []:
            cmd = (block.get("input") or {}).get("command") if block.get("type") == "tool_use" and block.get("name") == "Bash" else None
            if isinstance(cmd, str) and cmd.strip() and len(cmd) <= MAX_CHARS:
                yield cmd


def _payload(session_hash: str, cmd: str) -> dict:
    return {"session_id": session_hash, "transcript_path": f"/nonexistent/{session_hash}.jsonl", "cwd": "/nonexistent",
            "hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {"command": cmd}}


def build_inputs(projeto_dir, n=200, seed=SEED) -> list:
    rng = random.Random(seed)
    per_session, seen = {}, set()
    for p in sorted(Path(projeto_dir).glob("*.jsonl")):
        sid = hashlib.sha256(p.stem.encode()).hexdigest()[:12]
        for cmd in _commands(p):
            if cmd not in seen:
                seen.add(cmd)
                per_session.setdefault(sid, []).append(cmd)
    for cmds in per_session.values():
        rng.shuffle(cmds)
    out, sessions = [], sorted(per_session)
    rng.shuffle(sessions)
    while len(out) < n and any(per_session.values()):   # round-robin across sessions
        for sid in sessions:
            if per_session[sid] and len(out) < n:
                out.append(_payload(sid, per_session[sid].pop()))
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(prog="r3_make_s4_inputs")
    ap.add_argument("--projeto-dir", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--n", type=int, default=200)
    a = ap.parse_args(argv)
    rows = build_inputs(a.projeto_dir, a.n)
    Path(a.out).write_text("".join(json.dumps(r, ensure_ascii=False) + "\n" for r in rows))
    print(json.dumps({"n": len(rows), "sessions": len({r["session_id"] for r in rows}), "out": a.out}))
    return 0 if rows else 2


if __name__ == "__main__":
    raise SystemExit(main())
