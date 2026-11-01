"""Before/after family attribution over the real transcripts (same population as `touring kpi -j`).

`before` = text needles only (context_budget.rs today); `after` = emitter identity (the `command`
Claude Code records on each hook_success attachment) + text needles. Mirrors the Rust design:
  1. a DEDICATED emitter owns its output;   2. else the text needles;   3. else the emitter fallback.
Run:  python3 -I classify_after.py [fixture]   (fixture: write anonymized real records as JSONL)
"""
import collections
import glob
import json
import os
import re
import sys

ROOT = "/home/gabrielgadea/.claude/projects/-home-gabrielgadea-projects-touring"
TEXT = [
    ("code-mode-deny", ["CODE MODE", "G7 re-inspe", "G10 write", "G1 rajada", "G11 ", "G3 edit"]),
    ("touring-suggest", ["TOURING SUGGEST"]),
    ("loop-outer", ["LOOP RESUME", "work-outer", "explore-ledger", "OUTER ·"]),
    ("session-start", ["Touring Knowledge", "ATZILUTH", "SCOUT PERP", "TOURING (slim)", "Painel"]),
    ("prompt-enhance", ["PROMPT ENHANCEMENT"]),
    ("past-lessons", ["lições de erros passados", "Similar command failed"]),
    ("post-tool-echo", ["temporal: reliability", "last fail:", "post-tool-batch"]),
]
OWNS = [
    ("analyzing blast radius", "pre-edit"),
    ("speculative validation", "pre-write"),
    ("analyzing file context", "pre-read"),
    ("checking command history", "pre-bash"),
    ("syntax pre-check", "pre-edit-prevention"),
    ("verifying edit quality", "post-edit"),
    ("registering new file", "post-write"),
    ("evidence_guard.py", "evidence-guard"),
    ("CLAUDE_PLUGIN_ROOT", "plugin-hook"),
    ("prose_integrity_gate.py", "prose-integrity"),
    ("check_context.sh", "context-monitor"),
    ("delegation-nudge", "delegation-nudge"),
]
FALLBACK = [
    ("touring cli-suggest", "touring-suggest"),
    ("loop_resume.py", "loop-outer"),
    ("touring-hook post-bash", "post-tool-echo"),
]


def text_family(text):
    for name, needles in TEXT:
        if any(n in text for n in needles):
            return name
    return "other"


def claim(label):
    for needle, fam in OWNS:
        if needle in label:
            return fam, True
    for needle, fam in FALLBACK:
        if needle in label:
            return fam, False
    return None


def after_family(text, label):
    c = claim(label or "")
    if c and c[1]:
        return c[0]
    f = text_family(text)
    if f != "other":
        return f
    return c[0] if c else "other"


def contexts(att):
    out = []
    for key in ("stdout", "content"):
        raw = att.get(key)
        if not isinstance(raw, str) or not raw.strip():
            continue
        try:
            parsed = json.loads(raw)
        except Exception:
            out.append(raw)
            continue
        hs = parsed.get("hookSpecificOutput") if isinstance(parsed, dict) else None
        if isinstance(hs, dict):
            out += [hs[k] for k in ("additionalContext", "systemMessage") if isinstance(hs.get(k), str)]
        if isinstance(parsed, dict) and isinstance(parsed.get("systemMessage"), str):
            out.append(parsed["systemMessage"])
    return out


def records():
    files = sorted(glob.glob(ROOT + "/*.jsonl"), key=os.path.getmtime, reverse=True)[:3]
    for f in files:
        for line in open(f, errors="replace"):
            if '"hook_success"' in line:
                try:
                    rec = json.loads(line)
                except Exception:
                    continue
                if (rec.get("attachment") or {}).get("type") == "hook_success":
                    yield line, rec


def report(title, fn):
    fam = collections.defaultdict(lambda: [0, 0])
    total = 0
    for _, rec in records():
        att = rec["attachment"]
        for text in contexts(att):
            b = len(text.encode())
            total += b
            f = fn(text, att.get("command"))
            fam[f][0] += b
            fam[f][1] += 1
    print(f"{title}: total={total}")
    for name, (b, n) in sorted(fam.items(), key=lambda kv: -kv[1][0]):
        print(f"  {name:20s} {b:9d} {100 * b / total:5.1f}%  blocks={n}")
    return fam


def residue():
    """What is STILL 'other' after the fix, by command."""
    rest = collections.Counter()
    n = collections.Counter()
    for _, rec in records():
        att = rec["attachment"]
        for text in contexts(att):
            if after_family(text, att.get("command")) == "other":
                rest[att.get("command")] += len(text.encode())
                n[att.get("command")] += 1
    print("residual other by command:")
    for c, b in rest.most_common(10):
        print(f"  {b:7d} {n[c]:4d}  {str(c)[:80]!r}")


def anon(s):
    s = s.replace("/home/gabrielgadea", "/home/user")
    s = re.sub(r"gabrielgadea", "user", s)
    return s


def write_fixture(path, per_cmd=2, cap=2600):
    """Per distinct command: the `per_cmd` blocks nearest its median size, anonymized, capped."""
    by_cmd = collections.defaultdict(list)
    for _, rec in records():
        att = rec["attachment"]
        texts = contexts(att)
        if texts:
            by_cmd[att.get("command")].append((sum(len(t) for t in texts), att, rec))
    out = []
    for cmd, rows in sorted(by_cmd.items(), key=lambda kv: str(kv[0])):
        rows.sort(key=lambda r: r[0])
        mid = len(rows) // 2
        for _, att, rec in rows[max(0, mid - per_cmd // 2): max(0, mid - per_cmd // 2) + per_cmd]:
            texts = [anon(t)[:cap] for t in contexts(att)]
            old = [text_family(t) for t in contexts(att)]
            if [text_family(t) for t in texts] != old:  # truncation must not change the text family
                texts = [anon(t) for t in contexts(att)]
            body = json.dumps({"hookSpecificOutput": {"hookEventName": att.get("hookEvent"),
                                                      "additionalContext": texts[0]}}, ensure_ascii=False)
            out.append({"type": "attachment", "attachment": {
                "type": "hook_success", "hookName": att.get("hookName"), "hookEvent": att.get("hookEvent"),
                "content": "", "stdout": body, "stderr": "", "exitCode": 0,
                "command": anon(str(att.get("command"))), "durationMs": 1}})
    with open(path, "w") as fh:
        for r in out:
            fh.write(json.dumps(r, ensure_ascii=False) + "\n")
    print(f"fixture: {len(out)} records, {os.path.getsize(path)} bytes -> {path}")


if __name__ == "__main__":
    if len(sys.argv) > 1:
        write_fixture(sys.argv[1])
    else:
        report("BEFORE", lambda t, c: text_family(t))
        report("AFTER ", after_family)
        residue()
