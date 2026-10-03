#!/usr/bin/env python3
"""Fechamento do S3 do r3 (Errata 13, E13.2): cobertura de eventos do shadow em ~24 h.

S3 = >= 95% dos eventos elegiveis deixam 1 linha em 24 h (E5.4/E6.2). O denominador e o contador duravel de
ofertas (`code_mode_arm.json`, `offered` somado nos bracos) menos a base tomada as 15:15:50; o numerador, as
linhas `a2_offer_closed` cuja oferta (`offered_secs`, epoca em segundos) cai depois da base. Uma oferta pendente
quando a sessao acaba nao emite linha por desenho, entao um deficit sozinho nunca vira REPROVADO: vira
NAO_DEMONSTRADO. REPROVADO exige evidencia direta de perda (descarte do emissor, linha alheia ou ilegivel no
`touring kpi -j` do daemon). A unit `r3-shadow-evaluator` so e parada quando nada indica perda.

Limites que o veredito carrega (lidos no codigo pelo touring-a4, M20; Errata 13, E13.4):
- `emitter_dropped_since_start` e contador de PROCESSO: um restart do daemon o zera, e 0 so quer dizer "sem perda desde
  o inicio do daemon". O script le o inicio do daemon (/proc) e so autoriza parar a unit se ele nasceu antes da base.
- O denominador (`bump_arm`) conta a oferta com ou sem shadow; a linha so existe com o shadow ligado (`on_offer` sai cedo).
  A flag e cacheada por processo, entao so muda num restart (sinalizado em `flags`): um deficit pode ser shadow desligado.
- O journal de eventos e GLOBAL e o denominador e POR PROJETO: linha de outro projeto com shadow inflaria o numerador
  (hoje so o `touring` liga o shadow); `distinct_sessions` fica no resultado para auditar.
- Contador ainda se movendo no teto da espera: o PASS segue valido (a cobertura e um limite inferior, porque oferta
  pendente so aumenta o denominador) e o fato vai em `flags`.

Uso: r3_s3_close.py [--stop-unit]   (exit 0 = PASS; qualquer outro veredito sai com 1, 2 = erro de leitura)
"""
import argparse
import json
import os
import pathlib
import subprocess
import sys
import time

HOME = pathlib.Path.home()
PID_FILE = pathlib.Path(f"/run/user/{os.getuid()}/touring-daemon.pid")
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


def window_closed(events, base_t_ms):
    """Linhas `a2_offer_closed` cuja OFERTA foi feita na janela (>= base); sem `offered_secs` valido nao conta."""
    lines = []
    for e in events:
        if e.get("kind") != "a2_offer_closed":
            continue
        s = e.get("offered_secs")
        if isinstance(s, (int, float)) and not isinstance(s, bool) and s * 1000 >= base_t_ms:
            lines.append(e)
    return lines


def closed_in_window(events, base_t_ms):
    return len(window_closed(events, base_t_ms))


def proc_start_epoch(stat_text, btime_s, hz):
    """Epoca (s) em que o processo nasceu: campo 22 de /proc/<pid>/stat, lido DEPOIS do ultimo ')' (o comm pode ter ')')."""
    tail = stat_text.rsplit(")", 1)[1].split()
    return btime_s + int(tail[19]) / hz


def read_daemon_start_ms(pid_file=PID_FILE, proc=pathlib.Path("/proc"), hz=None):
    """Inicio do touring-daemon global em ms (epoca); None se o PID file, o comm ou o /proc nao fecharem (REGRA #19)."""
    try:
        pid = pathlib.Path(pid_file).read_text().strip()
        if not pid.isdigit() or (proc / pid / "comm").read_text().strip() != "touring-daemon":
            return None
        btime = next(int(ln.split()[1]) for ln in (proc / "stat").read_text().splitlines() if ln.startswith("btime "))
        return int(proc_start_epoch((proc / pid / "stat").read_text(), btime, hz or os.sysconf("SC_CLK_TCK")) * 1000)
    except (OSError, ValueError, StopIteration, IndexError):
        return None


def findings(counters, offers, closed, window_h):
    """O que impede um veredito de cobertura, em ordem de precedencia: o 1o vira o status, os demais vao em `also`."""
    out = []
    if counters is None or any(not isinstance(counters.get(k), int) for k in COUNTER_KEYS):
        out.append((UNMEASURED, "contadores de perda do kpi indisponiveis"))
    elif any(counters[k] > 0 for k in COUNTER_KEYS):
        out.append((FAILED, "evidencia direta de perda no kpi do daemon"))
    if offers < 0:
        out.append((UNMEASURED, "o contador de ofertas regrediu abaixo da base"))
    if window_h < MIN_WINDOW_H:
        out.append((INTERIM, f"janela de {window_h:.2f} h < {MIN_WINDOW_H} h"))
    if closed > offers:
        out.append((INCONSISTENT, "mais linhas fechadas do que ofertas contadas"))
    return out


def evaluate(base, arm_now, events, counters, end_t_ms, daemon_start_ms=None, settled=True):
    base_t_ms = int(base["t_ms"])
    offers = offered_total(arm_now) - offered_total(base)
    lines = window_closed(events, base_t_ms)
    closed = len(lines)
    # Os hashes (nao so a contagem) ficam no resultado: contaminacao se prova por INTERSECAO com os arquivos do
    # experimento, e uma contagem cresce sozinha quando entra sessao viva (touring-a4, M22).
    hashes = sorted({e["session_hash"] for e in lines if isinstance(e.get("session_hash"), str)})
    window_h = (end_t_ms - base_t_ms) / 3_600_000
    covers = daemon_start_ms is not None and daemon_start_ms <= base_t_ms
    flags = []
    if daemon_start_ms is None:
        flags.append("INICIO_DO_DAEMON_DESCONHECIDO")
    elif not covers:
        flags.append("DAEMON_REINICIADO_NA_JANELA")
    if not settled:
        flags.append("CONTADOR_AINDA_SE_MOVIA")
    found = findings(counters, offers, closed, window_h)
    if found:
        status, reason = found[0]
    elif offers > 0 and closed / offers >= COVERAGE_MIN:
        status, reason = PASS, f"cobertura >= {COVERAGE_MIN}"
    else:
        status, reason = NOT_DEMONSTRATED, "deficit pode ser oferta pendente no fim da sessao (nao emite linha por desenho)"
    return {
        "status": status,
        "reason": reason,
        "also": list(dict.fromkeys(s for s, _ in found[1:] if s != status)),
        "window_h": round(window_h, 3),
        "offers_in_window": offers,
        "closed_in_window": closed,
        "unexplained": offers - closed,
        "coverage": round(closed / offers, 4) if offers > 0 else None,
        "session_hashes": hashes,
        "distinct_sessions": len(hashes),
        "counters": counters,
        "daemon_start_ms": daemon_start_ms,
        "loss_counter_covers_window": covers,
        "flags": flags,
        "may_stop_unit": status in STOPPABLE and covers,
    }


def settle(read, key=offered_total, sleep=time.sleep, clock=time.monotonic, quiet_s=QUIET_S, max_wait_s=MAX_WAIT_S, poll_s=POLL_S):
    """Espera `key(read())` (o total de ofertas) ficar parado por `quiet_s` (uma oferta aberta fecha por TTL).

    Devolve (ULTIMO documento lido, estabilizou): quem decide usa esse mesmo snapshot, sem reler o arquivo.
    """
    start = clock()
    last = read()
    since = clock()
    while True:
        if clock() - since >= quiet_s:
            return last, True
        if clock() - start >= max_wait_s:
            return last, False
        sleep(poll_s)
        cur = read()
        if key(cur) != key(last):
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
    ap.add_argument("--pid-file", default=str(PID_FILE))
    ap.add_argument("--stop-unit", action="store_true", help="para a unit so se o veredito nao indicar perda")
    ap.add_argument("--quiet-s", type=int, default=QUIET_S)
    ap.add_argument("--max-wait-s", type=int, default=MAX_WAIT_S)
    a = ap.parse_args(argv)
    try:
        base = json.loads(pathlib.Path(a.base).read_text())
        arm_path = pathlib.Path(a.arm)
        def read_arm():
            return json.loads(arm_path.read_text())

        arm_first, settled = settle(read_arm, quiet_s=a.quiet_s, max_wait_s=a.max_wait_s)
        events, bad = read_events(a.events)
        # O braco e lido DEPOIS do journal: `bump_arm` precede `on_offer`, entao toda linha lida acima ja tem a sua
        # oferta neste snapshot (closed <= offers). Ler o braco antes deixava uma oferta que abre e fecha no intervalo
        # entrar so no numerador, e a cobertura inflava (touring-a4, M21).
        arm_now = read_arm()
        counters = kpi_counters()
        res = evaluate(base, arm_now, events, counters, int(time.time() * 1000), read_daemon_start_ms(a.pid_file), settled)
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
