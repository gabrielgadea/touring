#!/usr/bin/env python3
"""Summarize timestamp provenance for one corpus source path, never print code."""

import collections
import json
import sys
from pathlib import Path

ALVO = sys.argv[1]
root = Path.home() / ".claude/projects"
rows = []
for arquivo in root.glob("*/*.jsonl"):
    for raw in arquivo.open(encoding="utf-8", errors="replace"):
        if ALVO not in raw:
            continue
        try:
            record = json.loads(raw)
        except json.JSONDecodeError:
            continue
        for content in (record.get("message") or {}).get("content") or []:
            if not isinstance(content, dict):
                continue
            tool_input = content.get("input") or {}
            if tool_input.get("file_path") == ALVO:
                rows.append(
                    {
                        "arquivo": str(arquivo),
                        "record_type": record.get("type"),
                        "timestamp": record.get("timestamp"),
                        "uuid": record.get("uuid"),
                        "parentUuid": record.get("parentUuid"),
                        "content_type": content.get("type"),
                        "tool_id": content.get("id") or content.get("tool_use_id"),
                    }
                )
out = {
    "total": len(rows),
    "sem_timestamp": sum(not row.get("timestamp") for row in rows),
    "por_tipo": dict(
        collections.Counter(
            str((row["record_type"], bool(row.get("timestamp")))) for row in rows
        )
    ),
    "amostra_sem_ts": [row for row in rows if not row.get("timestamp")][:4],
    "amostra_com_ts": [row for row in rows if row.get("timestamp")][:4],
}
print(json.dumps(out, ensure_ascii=False, indent=1))
