#!/usr/bin/env python3
"""S3 RECALIBRATE — a inspeção que o modo `code` nega é isolada ou em rajada?

O deny por classe de hoje (`CODE_MODE_COLLAPSED_CLASSES`) dispara na PRIMEIRA
chamada de `grep`/`cat`/`find`. O S3 propõe deixar a isolada passar e negar só a
rajada. Trocar uma calibração medida por intuição seria o erro que a REGRA "meça
antes de otimizar" existe para impedir — então este script mede o que o novo
predicado teria de decidir:

* quantas chamadas dessas classes são ISOLADAS (nenhuma outra da mesma classe na
  janela, sem `touring run` no meio);
* que fração do VOLUME está em rajadas de tamanho >= 2, >= 3, >= 4;
* quanto o deny perderia de cobertura em cada limiar candidato.

A classificação vem de `n5_injection_kpi` (o extrator do S2), não de uma cópia:
um verificador que reimplementa o predicado do extrator mede outra coisa.

Uso:
    python3 scripts/s3_burst_distribution.py [--since YYYY-MM-DD] [--window-secs N]
"""
from __future__ import annotations

import argparse
import glob
import json
import os
import sys
from collections import defaultdict
from datetime import datetime

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from n5_injection_kpi import (  # noqa: E402
    effective_tokens,
    is_code_route,
    is_read_bash,
    is_search_bash,
)

TRANSCRIPT_GLOB = os.path.expanduser("~/.claude/projects/*/*.jsonl")

# As três classes que `CODE_MODE_COLLAPSED_CLASSES` nega hoje, e as que passam
# isoladas. Espelham o Rust; divergir aqui mediria um gate que não existe.
CLASSES_NEGADAS = {"grep", "cat", "find"}
CLASSES_QUE_PASSAM = {"ls", "wc", "sed"}


def scan_class(cmd: str) -> str | None:
    """Classe de inspeção do comando, ou None se não for inspeção.

    Espelha `scan_class_of` do Rust ao nível que importa para esta medição: o
    verbo efetivo (após env-prefix, wrappers e segmentos `cd`).
    """
    if is_code_route(cmd):
        return None
    toks = effective_tokens(cmd)
    if not toks:
        return None
    verb = os.path.basename(toks[0])
    if verb in ("rg", "egrep", "fgrep", "ag"):
        verb = "grep"
    elif verb in ("head", "tail", "less", "more"):
        verb = "cat"
    if verb in CLASSES_NEGADAS or verb in CLASSES_QUE_PASSAM:
        if verb in ("grep", "find") and not is_search_bash(cmd):
            return None
        if verb == "cat" and not is_read_bash(cmd):
            return None
        return verb
    return None


def parse_ts(raw: str):
    try:
        return datetime.fromisoformat(raw.replace("Z", "+00:00"))
    except Exception:
        return None


def iter_bash(path: str):
    """('ts', cmd) de cada Bash, em ordem cronológica."""
    with open(path, errors="replace") as f:
        for line in f:
            try:
                d = json.loads(line)
            except Exception:
                continue
            if d.get("isSidechain") or d.get("type") != "assistant":
                continue
            ts = parse_ts(d.get("timestamp") or "")
            for c in d.get("message", {}).get("content", []):
                if not isinstance(c, dict) or c.get("type") != "tool_use":
                    continue
                if (c.get("name") or "") != "Bash":
                    continue
                cmd = (c.get("input") or {}).get("command") or ""
                if cmd:
                    yield ts, cmd


def bursts_of_session(path: str, window_secs: int, since: str | None):
    """Rajadas por classe: lista de tamanhos, na ordem em que fecharam.

    Uma rajada acumula chamadas consecutivas da MESMA classe enquanto o gap for
    <= janela e nenhum `touring run` aparecer no meio — as duas condições que o
    ledger do G10 já usa em produção.
    """
    aberta: dict[str, list] = {}
    fechadas: dict[str, list[int]] = defaultdict(list)

    def fecha(cls: str):
        if cls in aberta:
            fechadas[cls].append(len(aberta.pop(cls)))

    for ts, cmd in iter_bash(path):
        if since and ts and ts.strftime("%Y-%m-%d") < since:
            continue
        if is_code_route(cmd):
            for cls in list(aberta):
                fecha(cls)
            continue
        cls = scan_class(cmd)
        if cls is None:
            continue
        anterior = aberta.get(cls)
        if anterior and ts and anterior[-1] and (ts - anterior[-1]).total_seconds() > window_secs:
            fecha(cls)
            anterior = None
        aberta.setdefault(cls, []).append(ts)
    for cls in list(aberta):
        fecha(cls)
    return fechadas


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--since", help="YYYY-MM-DD")
    ap.add_argument("--window-secs", type=int, default=600,
                    help="janela da rajada (default 600 = EXEC_BURST_WINDOW_SECS)")
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    paths = glob.glob(TRANSCRIPT_GLOB)
    if not paths:
        print("nenhum transcript encontrado", file=sys.stderr)
        return 2

    total: dict[str, list[int]] = defaultdict(list)
    for p in paths:
        for cls, tamanhos in bursts_of_session(p, args.window_secs, args.since).items():
            total[cls].extend(tamanhos)

    linhas = []
    for cls in sorted(total, key=lambda c: -sum(total[c])):
        tam = total[cls]
        chamadas = sum(tam)
        isoladas = sum(t for t in tam if t == 1)
        rajadas = {k: sum(t for t in tam if t >= k) for k in (2, 3, 4)}
        linhas.append({
            "classe": cls,
            "negada_hoje": cls in CLASSES_NEGADAS,
            "chamadas": chamadas,
            "rajadas": len(tam),
            "isoladas": isoladas,
            "pct_isoladas": round(100 * isoladas / chamadas, 1) if chamadas else 0.0,
            "vol_em_rajada>=2": rajadas[2],
            "pct_vol>=2": round(100 * rajadas[2] / chamadas, 1) if chamadas else 0.0,
            "pct_vol>=3": round(100 * rajadas[3] / chamadas, 1) if chamadas else 0.0,
            "pct_vol>=4": round(100 * rajadas[4] / chamadas, 1) if chamadas else 0.0,
            "maior_rajada": max(tam) if tam else 0,
        })

    if args.json:
        print(json.dumps({"window_secs": args.window_secs, "classes": linhas}, indent=2))
        return 0

    print(f"janela={args.window_secs}s  transcripts={len(paths)}"
          + (f"  since={args.since}" if args.since else ""))
    print(f"{'classe':<8}{'neg':<5}{'chamadas':>9}{'rajadas':>9}{'isoladas':>10}"
          f"{'%isol':>8}{'%vol>=2':>9}{'%vol>=3':>9}{'%vol>=4':>9}{'maior':>7}")
    for r in linhas:
        print(f"{r['classe']:<8}{'SIM' if r['negada_hoje'] else '-':<5}"
              f"{r['chamadas']:>9}{r['rajadas']:>9}{r['isoladas']:>10}"
              f"{r['pct_isoladas']:>7.1f}%{r['pct_vol>=2']:>8.1f}%"
              f"{r['pct_vol>=3']:>8.1f}%{r['pct_vol>=4']:>8.1f}%{r['maior_rajada']:>7}")

    neg = [r for r in linhas if r["negada_hoje"]]
    if neg:
        ch = sum(r["chamadas"] for r in neg)
        iso = sum(r["isoladas"] for r in neg)
        v2 = sum(r["vol_em_rajada>=2"] for r in neg)
        print(f"\nclasses NEGADAS hoje: {ch} chamadas | {iso} isoladas ({100*iso/ch:.1f}%)"
              f" | {v2} em rajada>=2 ({100*v2/ch:.1f}%)")
        print(f"deixar a isolada passar renuncia a {100*iso/ch:.1f}% dos denies"
              f" e preserva {100*v2/ch:.1f}% do volume que a rajada explica.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
