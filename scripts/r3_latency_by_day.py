#!/usr/bin/env python3
"""p95 de latencia por hook, dia (BRT) e fase (antes/depois de o shadow ser ligado), a partir do `hook_trace.jsonl`.

Instrumento da Errata 14 (S1b): o `r3_prove_delivery.py s1b` devolve UM numero contra um envelope, e a pergunta que ele nao
responde e se o nivel ja existia ANTES do shadow. Aqui a mesma fonte e quebrada por dia e fase, com a mediana ao lado, porque
so a cauda varia com a carga da maquina. Nao e um criterio: nao decide nada, so mostra de onde o numero vem.

    r3_latency_by_day.py --trace ~/.claude/touring/hook_trace.jsonl --on-ts 1790964562000 [--hook cli-suggest] [--min-n 30]
"""
import argparse
import datetime as dt
import json
import pathlib
import sys

from r3_prove_delivery import _pctl  # UMA definicao de percentil: a do criterio que este instrumento explica

HOOKS = ("cli-suggest", "post-tool-rl")
# O envelope do S1b agrupa por dia UTC (`ts_ms // DAY_MS`); BRT fica como opcao, e o default segue o criterio.
TZS = {"utc": dt.timezone.utc, "brt": dt.timezone(dt.timedelta(hours=-3))}


def p95(values):
    """Percentil 95 por posto mais proximo com teto (`_pctl` do r3_prove_delivery); None se vazio."""
    return _pctl(sorted(values), 95) if values else None


def breakdown(rows, on_ms, hooks=HOOKS, min_n=30, tz=TZS["utc"]):
    """rows: dicts com ts_ms/hook/elapsed_ms. Devolve [{hook, day, phase, n, p50, p95}] ordenado, so celulas com n >= min_n."""
    cells = {}
    for r in rows:
        if r.get("hook") not in hooks:
            continue
        ms, ts = r.get("elapsed_ms"), r.get("ts_ms")
        if isinstance(ms, bool) or not isinstance(ms, (int, float)) or isinstance(ts, bool) or not isinstance(ts, (int, float)):
            continue
        day = dt.datetime.fromtimestamp(ts / 1000, tz).strftime("%d/%m")
        cells.setdefault((r["hook"], day, "ON" if ts >= on_ms else "off"), []).append(ms)
    out = []
    for (hook, day, phase), v in sorted(cells.items(), key=lambda kv: (kv[0][0], kv[0][1][3:] + kv[0][1][:2], kv[0][2])):
        if len(v) >= min_n:
            out.append({"hook": hook, "day": day, "phase": phase, "n": len(v), "p50": _pctl(sorted(v), 50), "p95": p95(v)})
    return out


def read_rows(path):
    rows = []
    for line in pathlib.Path(path).read_text(errors="replace").splitlines():
        try:
            obj = json.loads(line)
        except ValueError:
            continue
        if isinstance(obj, dict):
            rows.append(obj)
    return rows


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--trace", required=True)
    ap.add_argument("--on-ts", type=int, required=True, help="epoca em ms em que o shadow foi ligado")
    ap.add_argument("--hook", action="append", help="repetivel; default: cli-suggest e post-tool-rl")
    ap.add_argument("--min-n", type=int, default=30)
    ap.add_argument("--tz", choices=sorted(TZS), default="utc", help="fuso do corte de dia; utc e o do envelope do S1b")
    a = ap.parse_args(argv)
    cells = breakdown(read_rows(a.trace), a.on_ts, tuple(a.hook) if a.hook else HOOKS, a.min_n, TZS[a.tz])
    print(json.dumps(cells, ensure_ascii=False))
    return 0 if cells else 2  # sem celula com amostra suficiente: nao ha sinal, nunca um verde


if __name__ == "__main__":
    sys.exit(main())
