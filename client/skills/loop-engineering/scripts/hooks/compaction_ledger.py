"""compaction_ledger — record every compaction and judge what its summary dropped (08/10/2026).

Item 4 of the compaction plan (docs/plans/2026-10-08-compactacao-65/ in ~/projects/touring): the
instrument that MEASURES the other items. Until now a compaction left no record a script could read
back: whether the summary kept the files changed, the DAGs cited and the tests run was a matter of
faith, and so was the effect of rewriting the Compact Instructions or trimming hook advice.

- ``record`` (PostCompact): one line per compaction in ``<dir>/compactions.jsonl`` — tokens before
  and after (from the transcript's ``compact_boundary``), summary size, and the coverage of three
  deterministic fact classes; the gaps go to ``<dir>/compact_gaps/<session>.json``.
- ``gaps_context`` (SessionStart, source=compact): the injection is ONLY the gaps, measured, and
  silence when the summary kept everything. Never a re-summary.

``<dir>`` is ``$TOURING_COMPACTION_LEDGER_DIR`` or ``~/.claude/touring``. Every entry point is
fail-open: a recorder must never break a compaction or a session start.
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import time
from pathlib import Path

DAG_RE = re.compile(r"task_\d{12,}")
TEST_RE = re.compile(r"\b(pytest|cargo test|cargo nextest|npm test|test_[\w-]+\.py)\b")
TEST_TOKEN_RE = re.compile(r"[\w./-]*test_[\w.-]*\.py|cargo (?:nextest|test)(?: -p [\w-]+)?")
PATH_RE = re.compile(r"(?:~|/)[\w.-]+/[\w./-]+")
WRITERS = {"Edit", "Write", "MultiEdit", "NotebookEdit"}
GAP_TTL_SECS = 1800
MAX_LISTED = 12


def ledger_dir() -> Path:
    return Path(os.environ.get("TOURING_COMPACTION_LEDGER_DIR") or Path.home() / ".claude" / "touring")


def _load(path) -> list[dict]:
    out = []
    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            try:
                out.append(json.loads(line))
            except json.JSONDecodeError:
                continue
    return out


def _is_boundary(rec: dict) -> bool:
    return rec.get("type") == "system" and rec.get("subtype") == "compact_boundary"


def _segment(recs: list[dict]) -> tuple[list[dict], dict | None]:
    """The compacted segment and its fresh boundary (None when not yet written).

    PostCompact and the boundary write have no guaranteed order. A boundary with no assistant turn
    after it is the one just written: the segment ends there. Otherwise the open tail after the
    last boundary is what was compacted.
    """
    idx = [i for i, r in enumerate(recs) if _is_boundary(r)]
    if not idx:
        return recs, None
    last = idx[-1]
    if any(r.get("type") == "assistant" for r in recs[last + 1:]):
        return recs[last + 1:], None
    start = idx[-2] + 1 if len(idx) > 1 else 0
    return recs[start:last], recs[last]


def _block_text(block) -> str:
    if not isinstance(block, dict):
        return ""
    if block.get("type") == "text":
        return block.get("text") or ""
    if block.get("type") == "tool_use":
        return json.dumps(block.get("input") or {}, ensure_ascii=False)
    return ""


def _texts(rec: dict):
    """User/assistant prose and tool inputs — never tool results nor an earlier summary."""
    if rec.get("isCompactSummary"):
        return
    content = (rec.get("message") or {}).get("content")
    if isinstance(content, str):
        yield content
        return
    for block in content or []:
        yield _block_text(block)


def _tool_uses(rec: dict):
    if rec.get("type") != "assistant":
        return
    for c in (rec.get("message") or {}).get("content") or []:
        if isinstance(c, dict) and c.get("type") == "tool_use":
            yield c.get("name"), c.get("input") or {}


def _ordered_unique(items) -> list[str]:
    return list(dict.fromkeys(i for i in items if i))


def _collect(seg: list[dict]) -> tuple[list, list, list]:
    files, tests, dags = [], [], []
    for rec in seg:
        for name, inp in _tool_uses(rec):
            if name in WRITERS:
                files.append(inp.get("file_path") or inp.get("notebook_path"))
            elif name == "Bash" and TEST_RE.search(inp.get("command") or ""):
                tests.append(inp.get("command"))
        for text in _texts(rec):
            dags.extend(DAG_RE.findall(text))
    return files, tests, dags


def segment_facts(transcript_path) -> dict:
    """Files changed, DAG ids cited and test commands run in the compacted segment."""
    seg, boundary = _segment(_load(transcript_path))
    files, tests, dags = _collect(seg)
    meta = (boundary or {}).get("compactMetadata") or {}
    return {
        "files": _ordered_unique(files),
        "dags": _ordered_unique(dags),
        "tests": _ordered_unique(tests),
        "boundary": None if boundary is None else {"uuid": boundary.get("uuid"), **meta},
    }


def _mentions(summary: str, token: str) -> bool:
    if token in summary:
        return True
    tail = Path(token).name
    return bool(tail) and re.search(rf"(?<![\w.-]){re.escape(tail)}(?!\w)", summary) is not None


def _test_token(cmd: str) -> str:
    m = TEST_TOKEN_RE.search(cmd)
    return m.group(0) if m else cmd[:40]


def judge(summary: str, facts: dict) -> dict:
    """What the summary does not mention, per fact class (deterministic, no model)."""
    return {
        "files": [f for f in facts["files"] if not _mentions(summary, f)],
        "dags": [d for d in facts["dags"] if d not in summary],
        "tests": [t for t in facts["tests"] if not _mentions(summary, _test_token(t))],
    }


def _coverage(facts: dict, gaps: dict) -> dict:
    return {k: [len(facts[k]) - len(gaps[k]), len(facts[k])] for k in ("files", "dags", "tests")}


def _summary_from_transcript(recs: list[dict]) -> str | None:
    idx = [i for i, r in enumerate(recs) if _is_boundary(r)]
    for rec in recs[(idx[-1] + 1) if idx else 0:]:
        if rec.get("isCompactSummary"):
            content = (rec.get("message") or {}).get("content")
            return content if isinstance(content, str) else "".join(
                c.get("text", "") for c in content or [] if isinstance(c, dict))
    return None


def _session(payload: dict) -> str:
    return re.sub(r"[^A-Za-z0-9_-]", "", str(payload.get("session_id") or "unknown"))[:80] or "unknown"


def _gap_file(session: str) -> Path:
    return ledger_dir() / "compact_gaps" / f"{session}.json"


def _row(payload: dict, session: str, boundary: dict, summary: str, facts: dict, gaps: dict) -> dict:
    """One ledger line: what the compaction cost, what its summary looks like, what it kept."""
    return {
        "ts": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "session_id": session, "cwd": payload.get("cwd"),
        "trigger": payload.get("trigger") or boundary.get("trigger"), "boundary_uuid": boundary.get("uuid"),
        "pre_tokens": boundary.get("preTokens"), "post_tokens": boundary.get("postTokens"),
        "duration_ms": boundary.get("durationMs"), "summary_chars": len(summary),
        "paths_in_summary": len(PATH_RE.findall(summary)),
        "has_format_block": "TASK:" in summary and "NEXT:" in summary,
        "coverage": _coverage(facts, gaps), "gaps": {k: v[:20] for k, v in gaps.items()},
    }


def record(payload: dict) -> dict | None:
    """PostCompact: append one ledger line per boundary and refresh this session's gap file."""
    tp = payload.get("transcript_path")
    if not tp or not Path(tp).exists():
        return None
    facts = segment_facts(tp)
    summary = payload.get("compact_summary") or _summary_from_transcript(_load(tp)) or ""
    gaps = judge(summary, facts)
    session = _session(payload)
    boundary = facts["boundary"] or {}
    # sha1, never hash(): hash() is salted per process, so two PostCompact runs would disagree.
    key = boundary.get("uuid") or "sum:" + hashlib.sha1(summary.encode()).hexdigest()[:12]
    gap_path = _gap_file(session)
    previous = json.loads(gap_path.read_text()) if gap_path.exists() else {}
    row = _row(payload, session, boundary, summary, facts, gaps)
    gap_path.parent.mkdir(parents=True, exist_ok=True)
    if previous.get("key") != key:
        with open(ledger_dir() / "compactions.jsonl", "a", encoding="utf-8") as fh:
            fh.write(json.dumps(row, ensure_ascii=False) + "\n")
    gap_path.write_text(json.dumps({"key": key, "epoch": time.time(), **gaps,
                                    "coverage": row["coverage"]}, ensure_ascii=False))
    return row


def _fresh_gaps(session: str) -> dict | None:
    path = _gap_file(session)
    try:
        data = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError):
        return None
    return data if time.time() - float(data.get("epoch", 0)) < GAP_TTL_SECS else None


def _render(gaps: dict, coverage: dict) -> str | None:
    if not any(gaps.get(k) for k in ("files", "dags", "tests")):
        return None
    lines = ["[COMPACTAÇÃO · lacunas do resumo, medidas no transcript] O resumo não cita o que segue; "
             "confira no disco antes de agir sobre isto."]
    labels = (("files", "arquivos alterados"), ("dags", "DAG"), ("tests", "testes rodados"))
    for key, label in labels:
        if gaps.get(key):
            shown = gaps[key][:MAX_LISTED]
            more = f" (+{len(gaps[key]) - len(shown)})" if len(gaps[key]) > len(shown) else ""
            lines.append(f"- {label}: {', '.join(f'`{x}`' if key == 'tests' else x for x in shown)}{more}")
    cov = " · ".join(f"{label.split()[0]} {coverage[k][0]}/{coverage[k][1]}"
                     for k, label in labels if coverage[k][1])
    lines.append(f"Cobertura: {cov}.")
    return "\n".join(lines)


def gaps_context(payload: dict) -> str | None:
    """SessionStart(source=compact): the measured gaps, or None (silence) when there are none."""
    if payload.get("source") != "compact":
        return None
    data = _fresh_gaps(_session(payload))
    if data:
        return _render(data, data.get("coverage") or {})
    tp = payload.get("transcript_path")
    if not tp or not Path(tp).exists():
        return None
    recs = _load(tp)
    summary = _summary_from_transcript(recs)
    if summary is None:
        return None
    facts = segment_facts(tp)
    gaps = judge(summary, facts)
    return _render(gaps, _coverage(facts, gaps))
