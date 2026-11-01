"""Reconhecimento: o que existe de fato no corpus (transcripts, mtimes, tipos de advice, prefixes).

Nao mede nada ainda. Imprime so agregados + exemplos curtos, para escrever o medidor sem adivinhacao.
Metodo de data: epoch->ISO UTC. mtime do transcript e o ultimo write.
"""

import json
import os
import sys
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

PROJ = Path.home() / ".claude/projects/-home-gabrielgadea-projects-analise"
ADVICE = ("hook_additional_context", "hook_success", "async_hook_response")
BASE_CUTOFF = "2026-10-08T17:00:00"  # a linha de base foi medida ~17h de 08/10


def iso(e):
    return datetime.fromtimestamp(e, timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def main():
    files = sorted(PROJ.glob("*.jsonl"), key=lambda f: f.stat().st_mtime, reverse=True)
    print(f"### {len(files)} transcripts no projeto")
    print("\n-- 20 mais recentes por mtime --")
    for f in files[:20]:
        print(f"  {iso(f.stat().st_mtime)}  {f.stat().st_size / 1e6:7.1f}MB  {f.name[:44]}")

    antes = [f for f in files if iso(f.stat().st_mtime) <= BASE_CUTOFF]
    print(f"\n-- com mtime <= {BASE_CUTOFF}: {len(antes)} transcripts --")
    for f in antes[:16]:
        print(f"  {iso(f.stat().st_mtime)}  {f.stat().st_size / 1e6:7.1f}MB  {f.name[:44]}")

    # Instrumentacao de uma amostra: quais tipos, quais eventos, quais chaves, quais prefixes.
    amostra = antes[:12] if antes else files[:12]
    kinds, events, hk, prefixes = Counter(), Counter(), Counter(), Counter()
    keys_seen = Counter()
    n_att = 0
    for f in amostra:
        for line in f.open(encoding="utf-8", errors="replace"):
            if '"attachment"' not in line:
                continue
            try:
                rec = json.loads(line)
            except json.JSONDecodeError:
                continue
            att = rec.get("attachment") or {}
            k = att.get("type")
            if k not in ADVICE:
                continue
            n_att += 1
            kinds[k] += 1
            events[att.get("hookEvent") or "?"] += 1
            raw = next((att[z] for z in ("hookName", "command", "hookCommand") if isinstance(att.get(z), str)), "?")
            parts = [p for p in raw.split() if not p.startswith("python")]
            base = os.path.basename(parts[0]) if parts else "?"
            hk[f"{base} {parts[1]}" if base == "touring-hook" and len(parts) > 1 else base[:40]] += 1
            for key in att:
                keys_seen[key] += 1
            body = att.get("content")
            s = body if isinstance(body, str) else json.dumps(body, ensure_ascii=False)
            for tok in (s or "")[:70].replace("\n", "\\n").split():
                pass
            head = " ".join((s or "")[:70].split())[:70]
            prefixes[head] += 1

    print(f"\n### {n_att} anexos de advice na amostra ({len(amostra)} transcripts)")
    print(f"\n-- tipos --\n  {dict(kinds)}")
    print(f"\n-- eventos --\n  {dict(events.most_common(15))}")
    print(f"\n-- chaves de attachment --\n  {dict(keys_seen.most_common(20))}")
    print(f"\n-- hookName (top 20) --")
    for k, v in hk.most_common(20):
        print(f"  {v:>6}  {k}")
    print(f"\n-- 30 primeiros tokens da injeção (prefixo literal) --")
    for k, v in prefixes.most_common(30):
        print(f"  {v:>5}  {k[:66]!r}")


if __name__ == "__main__":
    sys.exit(main())