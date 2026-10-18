"""Context size per call (cache_read + cache_creation + input), by model family and main×subagent, last 14 days."""
import collections
import json
import re
import statistics
import time
from pathlib import Path

PROJ = Path.home() / ".claude/projects"
corte = time.time() - 14 * 86400


def familia(m):
    m = m or "?"
    return "claude" if m.startswith("claude") else ("kimi" if ("k3" in m or "kimi" in m.lower()) else ("minimax" if "minimax" in m.lower() else "outro"))


for proj in ("-home-gabrielgadea-projects-touring", "-home-gabrielgadea-projects-analise"):
    uso = {}
    for p in (PROJ / proj).rglob("*.jsonl"):
        if p.stat().st_mtime < corte:
            continue
        sub = "subagents" in p.parts or p.name.startswith("agent-")
        with p.open(encoding="utf-8", errors="replace") as fh:
            for line in fh:
                if '"usage"' not in line or '"assistant"' not in line:
                    continue
                try:
                    rec = json.loads(line)
                except ValueError:
                    continue
                msg = rec.get("message") or {}
                u = msg.get("usage")
                if rec.get("type") == "assistant" and isinstance(u, dict) and msg.get("id"):
                    uso[msg["id"]] = (familia(msg.get("model")), "sub" if sub else "principal",
                                      u.get("input_tokens", 0), u.get("cache_creation_input_tokens", 0), u.get("cache_read_input_tokens", 0))
    grupos = collections.defaultdict(list)
    for fam, tipo, i, c, r in uso.values():
        grupos[(fam, tipo)].append((i, c, r))
    print(f"\n[{proj.split('-')[-1]}]  contexto por chamada = entrada + escrita + leitura de cache (mil tokens)")
    for (fam, tipo), xs in sorted(grupos.items(), key=lambda kv: -len(kv[1])):
        if len(xs) < 30:
            continue
        ctx = sorted(i + c + r for i, c, r in xs)
        tot = sum(ctx)
        hit = 100 * sum(r for _, _, r in xs) / max(1, tot)
        q = lambda f: ctx[min(len(ctx) - 1, int(f * (len(ctx) - 1)))] / 1000
        print(f"  {fam:8s} {tipo:9s} n={len(xs):6d} · mediana {q(0.5):6.0f} · p90 {q(0.9):6.0f} · máx {ctx[-1] / 1000:6.0f}"
              f" · média {statistics.mean(ctx) / 1000:6.0f} · cache {hit:5.1f}% · volume {tot / 1e9:5.2f} B")
