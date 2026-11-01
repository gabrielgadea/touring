#!/usr/bin/env python3
"""Extrai o melhor exemplar (conteúdo mais longo) de cada basename campeão de recriação.

Entrada: os mesmos transcripts do transcript_script_miner. Saída: um JSON por basename
em champions_content/ com {basename, path, session, month, bytes, content} — insumo do P1
(generalizar as instâncias reais em scripts canônicos do portfólio).
"""
from __future__ import annotations

import json
from pathlib import Path

PROJECTS = Path.home() / ".claude" / "projects"
OUT_DIR = Path(__file__).resolve().parent / "champions_content"

CHAMPIONS = {
    "probe.py", "probe2.py", "dump.py", "fetch.py", "build.py", "show.py",
    "verifica.py", "recalc.py", "mutantes.py", "ver.py", "extrai.py",
    "audit-plan-completion.sh", "rg_g10.sh", "rodar.sh",
}


def main() -> None:
    """Uma passada: guarda o conteúdo mais longo já visto por basename."""
    best: dict[str, dict] = {}
    for fp in sorted(PROJECTS.rglob("*.jsonl")):
        try:
            with fp.open("rb") as fh:
                for raw in fh:
                    if b'"Write"' not in raw:
                        continue
                    try:
                        obj = json.loads(raw)
                    except (json.JSONDecodeError, UnicodeDecodeError):
                        continue
                    msg = obj.get("message")
                    if not isinstance(msg, dict):
                        continue
                    for c in msg.get("content") or []:
                        if not isinstance(c, dict) or c.get("type") != "tool_use" or c.get("name") != "Write":
                            continue
                        inp = c.get("input") or {}
                        path = str(inp.get("file_path", ""))
                        base = Path(path).name
                        if base not in CHAMPIONS:
                            continue
                        content = str(inp.get("content", ""))
                        if len(content) > best.get(base, {}).get("bytes", 0):
                            best[base] = {
                                "basename": base, "path": path, "session": fp.stem,
                                "month": str(obj.get("timestamp", ""))[:7],
                                "bytes": len(content), "content": content,
                            }
        except OSError:
            continue
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for base, rec in best.items():
        (OUT_DIR / f"{base.replace('/', '_')}.json").write_text(
            json.dumps(rec, ensure_ascii=False, indent=1)
        )
    print(json.dumps({b: {"bytes": r["bytes"], "month": r["month"], "path": r["path"][-60:]}
                      for b, r in sorted(best.items())}, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
