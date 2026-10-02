#!/usr/bin/env python3
"""Fechamento do S3 do r3 (Errata 13, E13.2): cobertura de eventos do shadow em ~24 h.

S3 = >= 95% dos eventos elegiveis deixam 1 linha em 24 h (E5.4/E6.2). O denominador e o contador duravel de
ofertas (`code_mode_arm.json`, `offered` somado nos bracos) menos a base tomada as 15:15:50; o numerador, as
linhas `a2_offer_closed` cuja oferta (`offered_secs`, epoca em segundos) cai depois da base. Uma oferta pendente
quando a sessao acaba nao emite linha por desenho, entao um deficit sozinho nunca vira REPROVADO: vira
NAO_DEMONSTRADO. REPROVADO exige evidencia direta de perda (descarte do emissor, linha alheia ou ilegivel no
`touring kpi -j` do daemon). A unit `r3-shadow-evaluator` so e parada quando nada indica perda.

Uso: r3_s3_close.py [--stop-unit]   (exit 0 = PASS; qualquer outro veredito sai com 1, 2 = erro de leitura)
"""
import argparse
import json
import pathlib
import subprocess
import sys
import time

HOME = pathlib.Path.home()
BASE = HOME / ".claude/touring/r3-data/s3_baseline.json"
ARM = HOME / "projects/touring/.claude/touring/code_mode_arm.json"
EVENTS = HOME / ".claude/touring/logs/r3_events.jsonl"
OUT = HOME / ".claude/touring/r3-data/s3_close.json"
UNIT = "r3-shadow-evaluator"

MIN_WINDOW_H = 23.5
COVERAGE_MIN = 0.95
QUIET_S = 120
MAX_WAIT_S = 600
POLL_S = 15
COUNTER_KEYS = ("emitter_dropped_since_start", "foreign_lines", "unreadable_lines")

PASS = "PASS"
NOT_DEMONSTRATED = "NAO_DEMONSTRADO"
FAILED = "REPROVADO"
INTERIM = "INTERINO"
UNMEASURED = "NAO_MEDIDO"
INCONSISTENT = "INCONSISTENTE"
STOPPABLE = (PASS, NOT_DEMONSTRATED)


def offered_total(doc):
    """Soma `offered` de todos os bracos; aceita o envelope da base (`{"t_ms", "arm"}`) ou o arquivo cru."""
    arms = doc.get("arm", doc)
    return sum(
        v["offered"]
        for v in arms.values()
        if isinstance(v, dict) and isinstance(v.get("offered"), int) and not isinstance(v.get("offered"), bool)
    )


def closed_in_window(events, base_t_ms):
    """Linhas `a2_offer_closed` cuja OFERTA foi feita na janela (>= base); sem `offered_secs` valido nao conta."""
    n = 0
    for e in events:
        if e.get("kind") != "a2_offer_closed":
            continue
        s = e.get("offered_secs")
        if isinstance(s, (int, float)) and not isinstance(s, bool) and s * 1000 >= base_t_ms:
            n += 1
    return n


def evaluate(base, arm_now, events, counters, end_t_ms):
    base_t_ms = int(base["t_ms"])
    offers = offered_total(arm_now) - offered_total(base)
    closed = closed_in_window(events, base_t_ms)
    window_h = (end_t_ms - base_t_ms) / 3_600_000
    res = {
        "window_h": round(window_h, 3),
        "offers_in_window": offers,
        "closed_in_window": closed,
        "unexplained": offers - closed,
        "coverage": round(closed / offers, 4) if offers > 0 else None,
        "counters": counters,
    }
    if counters is None or any(not isinstance(counters.get(k), int) for k in COUNTER_KEYS):
        res["status"] = UNMEASURED
        res["reason"] = "contadores de perda do kpi indisponiveis"
    elif any(counters[k] > 0 for k in COUNTER_KEYS):
        res["status"] = FAILED
        res["reason"] = "evidencia direta de perda no kpi do daemon"
    elif offers < 0:
        res["status"] = UNMEASURED
        res["reason"] = "o contador de ofertas regrediu abaixo da base"
    elif window_h < MIN_WINDOW_H:
        res["status"] = INTERIM
        res["reason"] = f"janela de {window_h:.2f} h < {MIN_WINDOW_H} h"
    elif closed > offers:
        res["status"] = INCONSISTENT
        res["reason"] = "mais linhas fechadas do que ofertas contadas"
    elif offers > 0 and closed / offers >= COVERAGE_MIN:
        res["status"] = PASS
        res["reason"] = f"cobertura >= {COVERAGE_MIN}"
    else:
        res["status"] = NOT_DEMONSTRATED
        res["reason"] = "deficit pode ser oferta pendente no fim da sessao (nao emite linha por desenho)"
    res["may_stop_unit"] = res["status"] in STOPPABLE
    return res


def settle(read_offered, sleep=time.sleep, clock=time.monotonic, quiet_s=QUIET_S, max_wait_s=MAX_WAIT_S, poll_s=POLL_S):
    """Espera o contador de ofertas ficar parado por `quiet_s` (uma oferta aberta fecha por TTL); (valor, estabilizou)."""
    start = clock()
    last = read_offered()
    since = clock()
    while True:
        if clock() - since >= quiet_s:
            return last, True
        if clock() - start >= max_wait_s:
            return last, False
        sleep(poll_s)
        cur = read_offered()
        if cur != last:
            last, since = cur, clock()


def find_block(obj):
    if isinstance(obj, dict):
        if "emitter_dropped_since_start" in obj:
            return obj
        for v in obj.values():
            hit = find_block(v)
            if hit is not None:
                return hit
    elif isinstance(obj, list):
        for v in obj:
            hit = find_block(v)
            if hit is not None:
                return hit
    return None


def kpi_counters(run=subprocess.run, exe="touring"):
    """Os 3 contadores de perda do `touring kpi -j` (processo vivo do daemon); None se algo falhar."""
    try:
        p = run([exe, "kpi", "-j"], capture_output=True, text=True, timeout=90)
        if p.returncode != 0:
            return None
        block = find_block(json.loads(p.stdout))
    except Exception:
        return None
    if block is None:
        return None
    vals = {k: block.get(k) for k in COUNTER_KEYS}
    if any(not isinstance(v, int) or isinstance(v, bool) for v in vals.values()):
        return None
    return vals


def read_events(path):
    events, bad = [], 0
    for line in pathlib.Path(path).read_text().splitlines():
        if not line.strip():
            continue
        try:
            obj = json.loads(line)
        except ValueError:
            bad += 1
            continue
        if isinstance(obj, dict):
            events.append(obj)
        else:
            bad += 1
    return events, bad


def stop_unit(unit=UNIT, run=subprocess.run):
    p = run(["systemctl", "--user", "stop", unit], capture_output=True, text=True, timeout=60)
    a = run(["systemctl", "--user", "is-active", unit], capture_output=True, text=True, timeout=30)
    return {"stop_rc": p.returncode, "active_after": a.stdout.strip()}


def write_atomic(path, doc):
    path = pathlib.Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(doc, ensure_ascii=False, indent=2) + "\n")
    tmp.replace(path)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--base", default=str(BASE))
    ap.add_argument("--arm", default=str(ARM))
    ap.add_argument("--events", default=str(EVENTS))
    ap.add_argument("--out", default=str(OUT))
    ap.add_argument("--unit", default=UNIT)
    ap.add_argument("--stop-unit", action="store_true", help="para a unit so se o veredito nao indicar perda")
    ap.add_argument("--quiet-s", type=int, default=QUIET_S)
    ap.add_argument("--max-wait-s", type=int, default=MAX_WAIT_S)
    a = ap.parse_args(argv)
    try:
        base = json.loads(pathlib.Path(a.base).read_text())
        arm_path = pathlib.Path(a.arm)
        _, settled = settle(lambda: offered_total(json.loads(arm_path.read_text())), quiet_s=a.quiet_s, max_wait_s=a.max_wait_s)
        arm_now = json.loads(arm_path.read_text())
        events, bad = read_events(a.events)
        counters = kpi_counters()
        res = evaluate(base, arm_now, events, counters, int(time.time() * 1000))
    except Exception as exc:  # leitura falhou: nunca para a unit, nunca aprova
        res = {"status": UNMEASURED, "reason": f"erro de leitura: {exc!r}", "may_stop_unit": False}
        write_atomic(a.out, res)
        print(json.dumps(res, ensure_ascii=False))
        return 2
    res["settled"] = settled
    res["unparsable_event_lines"] = bad
    res["generated_at"] = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    res["stop"] = "nao_pedido"
    if a.stop_unit:
        res["stop"] = stop_unit(a.unit) if res["may_stop_unit"] else "recusado_pelo_veredito"
    write_atomic(a.out, res)
    print(json.dumps(res, ensure_ascii=False))
    return 0 if res["status"] == PASS else 1


if __name__ == "__main__":
    sys.exit(main())
