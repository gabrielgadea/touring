#!/usr/bin/env python3
"""Verdict of a paired timing run (tempos_pareados.py) by the rules PRE-REGISTERED on 17/10/2026
(PREREGISTRO-P95-PAREADO-2026-10-17.md). The rules live here as constants; changing them after a result is seen breaks
the pre-registration.

Per id: m_A, m_B = median of the 4 calls of each side. Scale-free statistics over ids:
  R_tipico = median_i (m_B / m_A)            (ids with m_A > 0)
  R_p95    = p95_i(m_B) / p95_i(m_A)          p95 = the judge's nearest rank, sorted[ceil(0.95·n) − 1]
CI95 by bootstrap over ids (2000 resamples, seed 20261017). Absolute p95_i(m_B) in ms and the machine factor
median_i(m_A) are REPORTED, never gate (the absolute 150 ms only gates once the motor reports CPU time — phase D).

  --modo aa : PROTOCOLO_OK (exit 0) when both CIs ⊂ [0.95, 1.05]; else PROTOCOLO_INADEQUADO (exit 2)
  --modo ab : SEM_REGRESSAO (exit 0) when both CI upper bounds ≤ 1.10; else REGRESSAO (exit 1)
  any mode  : INCONCLUSIVO (exit 3) when > 1% of ids lack a time on either side (absence never reads as zero)

Usage: analisa_pareado.py <pareado.json> --modo aa|ab --fase c [--json <saida>]

PHASE D (PREREGISTRO-FASE-D-2026-10-18.md, the default): the gated measures are the retired user-space INSTRUCTIONS and
the tree CPU time per call (conta_custo.py, recorded by tempos_pareados.py as instr_A/B and cpu_A/B); wall time is only
reported. Same statistics (R_tipico, R_p95, bootstrap over ids, 2000 resamples) with seed 20261018. Rules:
  absent measure : > 1% of the calls of instr or cpu, or of the ids' wall times, lack a value  -> INCONCLUSIVO (exit 3)
  --modo aa      : ruler = instr if both CIs ⊂ [0.99, 1.01]; else cpu if both ⊂ [0.95, 1.05]; else
                   PROTOCOLO_INADEQUADO (exit 2). The ruler is written to the verdict.
  --modo ab --aa <veredito_aa.json> : ruler = the one the A/A chose (NEVER re-chosen after seeing the A/B); both CI upper
                   bounds ≤ 1.10 on it -> SEM_REGRESSAO, else REGRESSAO (exit 1). An A/A that was not PROTOCOLO_OK
                   means the A/B is not read (exit 2 / 3). On the v2 judge (layer 1) the absolute p95 of the CPU
                   per call of B must also be ≤ 150 ms; on v3 (layer 2, own ceiling of 1 s) it is only reported.
Exit 4 = misuse (nothing was decided).

Usage (phase D): analisa_pareado.py <pareado.json> --modo aa [--json <saida>]
                 analisa_pareado.py <pareado.json> --modo ab --aa <veredito_aa.json> [--juiz v2|v3] [--json <saida>]
"""
import argparse
import json
import math
import random
import statistics
import sys
from pathlib import Path

AA_FAIXA = (0.95, 1.05)
AB_TETO = 1.10
FALTA_MAX = 0.01
BOOT_N = 2000
BOOT_SEMENTE = 20261017

# --- phase D (PREREGISTRO-FASE-D-2026-10-18.md): every number below is a pre-registered rule
AA_INSTR_FAIXA = (0.99, 1.01)
AA_CPU_FAIXA = (0.95, 1.05)
ABSOLUTO_CPU_MS = 150.0
BOOT_SEMENTE_D = 20261018
CHAMADAS_POR_LADO = 4  # ABBABAAB: 4 calls of each binary per id
MEDIDAS_D = {"instr": ("instr_A", "instr_B"), "cpu": ("cpu_A", "cpu_B"), "parede": ("A", "B")}


def p95(valores: list[float]) -> float:
    """The judge's definition (juiz_motor_vgp.py): nearest rank."""
    if not valores:
        return math.inf
    ordenados = sorted(valores)
    return ordenados[min(len(ordenados) - 1, math.ceil(0.95 * len(ordenados)) - 1)]


def medianas(por_id: dict) -> tuple[dict[str, tuple[float, float]], list[str]]:
    """(m_A, m_B) per id; ids with any missing time are returned apart, never imputed."""
    pares, faltando = {}, []
    for pid, reg in por_id.items():
        a, b = reg.get("A") or [], reg.get("B") or []
        if not a or not b or any(not isinstance(x, (int, float)) for x in (*a, *b)):
            faltando.append(pid)
            continue
        pares[pid] = (statistics.median(a), statistics.median(b))
    return pares, faltando


def estatisticas(pares: dict[str, tuple[float, float]]) -> tuple[float, float]:
    ids = list(pares)
    r_tip = statistics.median(pares[i][1] / pares[i][0] for i in ids if pares[i][0] > 0)
    pa = p95([pares[i][0] for i in ids])
    r_p95 = p95([pares[i][1] for i in ids]) / pa if pa > 0 else math.inf
    return r_tip, r_p95


def bootstrap(pares: dict[str, tuple[float, float]]) -> dict[str, tuple[float, float]]:
    rng = random.Random(BOOT_SEMENTE)
    ids = list(pares)
    tips, p95s = [], []
    for _ in range(BOOT_N):
        amostra = {f"{k}:{i}": pares[i] for k, i in enumerate(rng.choice(ids) for _ in ids)}
        t, p = estatisticas(amostra)
        tips.append(t)
        p95s.append(p)
    tips.sort()
    p95s.sort()
    lo, hi = int(0.025 * BOOT_N), int(0.975 * BOOT_N) - 1
    return {"R_tipico": (tips[lo], tips[hi]), "R_p95": (p95s[lo], p95s[hi])}


def veredito(por_id: dict, modo: str) -> tuple[str, int, dict]:
    pares, faltando = medianas(por_id)
    total = len(pares) + len(faltando)
    info = {"ids": total, "faltando": len(faltando), "faltando_ids": sorted(faltando)[:20]}
    if total == 0 or len(faltando) > FALTA_MAX * total:
        return "INCONCLUSIVO", 3, info
    r_tip, r_p95 = estatisticas(pares)
    ic = bootstrap(pares)
    info |= {"R_tipico": round(r_tip, 4), "R_p95": round(r_p95, 4),
             "IC95": {k: [round(v[0], 4), round(v[1], 4)] for k, v in ic.items()},
             "p95_B_ms": round(p95([b for _, b in pares.values()]), 1),
             "p95_A_ms": round(p95([a for a, _ in pares.values()]), 1),
             "fator_maquina_mediana_A_ms": round(statistics.median(a for a, _ in pares.values()), 2)}
    if modo == "aa":
        ok = all(AA_FAIXA[0] <= v[0] and v[1] <= AA_FAIXA[1] for v in ic.values())
        return ("PROTOCOLO_OK", 0, info) if ok else ("PROTOCOLO_INADEQUADO", 2, info)
    ok = all(v[1] <= AB_TETO for v in ic.values())
    return ("SEM_REGRESSAO", 0, info) if ok else ("REGRESSAO", 1, info)


def valido(x) -> bool:
    """A measurement is a finite number. None, text, NaN and inf are ABSENT — never read as zero."""
    return isinstance(x, (int, float)) and not isinstance(x, bool) and math.isfinite(x)


def extrai(por_id: dict, campos: tuple[str, str]) -> tuple[dict[str, tuple[float, float]], int, int, list[str]]:
    """(m_A, m_B) per COMPLETE id, calls expected, calls missing, ids with any hole. A hole is never imputed."""
    pares, ids_faltando, total, faltando = {}, [], 0, 0
    for pid, reg in por_id.items():
        listas = [reg.get(c) if isinstance(reg.get(c), list) else [] for c in campos]
        bons = [[x for x in lst if valido(x)] for lst in listas]
        esperado = sum(max(len(lst), CHAMADAS_POR_LADO) for lst in listas)
        falta = esperado - sum(len(b) for b in bons)
        total += esperado
        faltando += falta
        if falta:
            ids_faltando.append(pid)
        else:
            pares[pid] = (statistics.median(bons[0]), statistics.median(bons[1]))
    return pares, total, faltando, sorted(ids_faltando)


def bootstrap_d(pares: dict[str, tuple[float, float]]) -> dict[str, tuple[float, float]]:
    """Same estimators and 2000 resamples over ids as `bootstrap`, seed 20261018, on index arrays (fast)."""
    rng = random.Random(BOOT_SEMENTE_D)
    a = [m[0] for m in pares.values()]
    b = [m[1] for m in pares.values()]
    razao = [y / x if x > 0 else None for x, y in zip(a, b, strict=True)]
    n = len(a)
    tips, p95s = [], []
    for _ in range(BOOT_N):
        idx = rng.choices(range(n), k=n)
        rs = [razao[i] for i in idx if razao[i] is not None]
        pa = p95([a[i] for i in idx])
        tips.append(statistics.median(rs) if rs else math.inf)
        p95s.append(p95([b[i] for i in idx]) / pa if pa > 0 else math.inf)
    tips.sort()
    p95s.sort()
    lo, hi = int(0.025 * BOOT_N), int(0.975 * BOOT_N) - 1
    return {"R_tipico": (tips[lo], tips[hi]), "R_p95": (p95s[lo], p95s[hi])}


def resumo_d(pares: dict[str, tuple[float, float]]) -> dict:
    r_tip, r_p95 = estatisticas(pares)
    ic = bootstrap_d(pares)
    return {"n_ids": len(pares), "R_tipico": round(r_tip, 4), "R_p95": round(r_p95, 4),
            "IC95": {k: [round(v[0], 4), round(v[1], 4)] for k, v in ic.items()}}


def dentro(ic: dict, faixa: tuple[float, float]) -> bool:
    return all(faixa[0] <= v[0] and v[1] <= faixa[1] for v in ic.values())


def ausencias_d(por_id: dict) -> tuple[dict[str, dict[str, tuple[float, float]]], dict, bool]:
    """(pairs per measure on the ids complete in ALL of them, report so far, whether the data is too holey to decide)."""
    ext = {m: extrai(por_id, campos) for m, campos in MEDIDAS_D.items()}
    excluidos = sorted({i for _, _, _, ids in ext.values() for i in ids})
    ausencia = {m: {"chamadas": tot, "faltando": falt, "ids_excluidos": ids} for m, (_, tot, falt, ids) in ext.items()}
    info = {"fase": "D", "ids": len(por_id), "ausencia": ausencia, "ids_excluidos": excluidos[:20],
            "n_ids_excluidos": len(excluidos)}
    pares = {m: {i: pr[i] for i in pr if i not in set(excluidos)} for m, (pr, _, _, _) in ext.items()}
    sem_dado = not pares["instr"] or any(
        tot == 0 or falt > FALTA_MAX * tot for m in ("instr", "cpu") for _, tot, falt, _ in [ext[m]]
    )
    sem_parede = len(ext["parede"][3]) > FALTA_MAX * len(por_id)  # the C' rule: ids without a wall time
    return pares, info, sem_dado or sem_parede


def veredito_aa_d(ic: dict, info: dict) -> tuple[str, int, dict]:
    """The A/A picks the ruler: instructions if both CIs are inside [0.99, 1.01], else the CPU inside [0.95, 1.05]."""
    ok = {"instr": dentro(ic["instr"], AA_INSTR_FAIXA), "cpu": dentro(ic["cpu"], AA_CPU_FAIXA)}
    info["aa"] = {m: {"ok": v} for m, v in ok.items()}
    info["regua"] = "instr" if ok["instr"] else ("cpu" if ok["cpu"] else None)
    return ("PROTOCOLO_OK", 0, info) if info["regua"] else ("PROTOCOLO_INADEQUADO", 2, info)


def veredito_ab_d(por_id: dict, pares: dict, ic: dict, info: dict, regua: str, juiz: str) -> tuple[str, int, dict]:
    """The A/B on the ruler the A/A chose; on the v2 judge the CPU p95 per call of B must also be ≤ 150 ms."""
    alvo = regua  # fixed by the A/A; reading the A/B must never change it
    relativo = all(v[1] <= AB_TETO for v in ic[alvo].values())
    chamadas_b = [x for i in pares["cpu"] for x in por_id[i]["cpu_B"]]
    p95_calls = p95(chamadas_b)
    abs_ok = p95_calls <= ABSOLUTO_CPU_MS
    portao_abs = juiz == "v2"
    info |= {"regua": alvo, "juiz": juiz,
             "absoluto": {"cpu_p95_B_ms": round(p95_calls, 2), "limite_ms": ABSOLUTO_CPU_MS, "ok": abs_ok,
                          "portao": portao_abs,
                          "cpu_p95_B_por_id_ms": round(p95([b for _, b in pares["cpu"].values()]), 2)},
             "portoes": {"relativo": relativo, "absoluto": abs_ok if portao_abs else None}}
    passou = relativo and (abs_ok or not portao_abs)
    return ("SEM_REGRESSAO", 0, info) if passou else ("REGRESSAO", 1, info)


def veredito_d(por_id: dict, modo: str, regua: str | None = None, juiz: str = "v2") -> tuple[str, int, dict]:
    """Phase D verdict. `regua` (instr|cpu) is REQUIRED for the A/B: it is the one the A/A chose."""
    if modo == "ab" and regua not in ("instr", "cpu"):
        raise ValueError("o A/B exige a regua que o A/A escolheu (instr|cpu): ela nao se escolhe depois do A/B")
    pares, info, inconclusivo = ausencias_d(por_id)
    if inconclusivo:
        return "INCONCLUSIVO", 3, info
    for m, pr in pares.items():
        info[m] = resumo_d(pr)
    info["parede"] |= {"p95_B_ms": round(p95([b for _, b in pares["parede"].values()]), 1),
                       "p95_A_ms": round(p95([a for a, _ in pares["parede"].values()]), 1),
                       "fator_maquina_mediana_A_ms": round(statistics.median(a for a, _ in pares["parede"].values()), 2)}
    ic = {m: {k: tuple(v) for k, v in info[m]["IC95"].items()} for m in ("instr", "cpu")}
    if modo == "aa":
        return veredito_aa_d(ic, info)
    return veredito_ab_d(por_id, pares, ic, info, regua, juiz)


def juiz_dos_args(args: list[str]) -> str:
    """v3 (layer 2) when the judge ran with `--camadas` naming layer 2; anything else is v2 (layer 1)."""
    toks = [t.removeprefix("--arg-motor=") for t in args if t != "--arg-motor"]
    camadas = ""
    for i, t in enumerate(toks):
        if t == "--camadas" and i + 1 < len(toks):
            camadas = toks[i + 1]
        elif t.startswith("--camadas="):
            camadas = t.split("=", 1)[1]
    return "v3" if "2" in camadas.split(",") else "v2"


class Uso(Exception):
    """Misuse of the command line; exit 4 (2 is reserved for PROTOCOLO_INADEQUADO)."""


def regua_do_aa(caminho: str, d: dict) -> tuple[str | None, tuple[str, int, dict] | None]:
    """(ruler, None) from an A/A verdict that validated the protocol; (None, verdict) when the A/B must not be read."""
    aa = json.loads(Path(caminho).read_text(encoding="utf-8"))
    if aa.get("fase") != "D" or aa.get("modo") != "aa":
        raise Uso(f"{caminho} nao e o veredito de um A/A da fase D")
    if aa.get("sha_A") and d.get("sha_A") and aa["sha_A"] != d["sha_A"]:
        raise Uso(f"a referencia do A/B ({d['sha_A']}) nao e a do A/A ({aa['sha_A']}): a referencia e FIXA")
    if aa.get("veredito") == "PROTOCOLO_OK" and aa.get("regua") in ("instr", "cpu"):
        return aa["regua"], None
    nome, rc = ("INCONCLUSIVO", 3) if aa.get("veredito") == "INCONCLUSIVO" else ("PROTOCOLO_INADEQUADO", 2)
    return None, (nome, rc, {"fase": "D", "motivo": f"o A/A deu {aa.get('veredito')}: o A/B nao e lido"})


class Parser(argparse.ArgumentParser):
    def error(self, message):
        raise Uso(message)


def main() -> int:
    try:
        return _main()
    except Uso as e:
        print(f"analisa_pareado: {e}", file=sys.stderr)
        return 4


def _main() -> int:
    ap = Parser()
    ap.add_argument("pareado")
    ap.add_argument("--modo", choices=("aa", "ab"), required=True)
    ap.add_argument("--fase", choices=("c", "d"), default="d")
    ap.add_argument("--aa", help="phase D A/B: the A/A verdict (carries the ruler the A/A chose)")
    ap.add_argument("--juiz", choices=("v2", "v3"), help="default: deduced from the judge args in the pareado")
    ap.add_argument("--json")
    a = ap.parse_args()
    d = json.loads(Path(a.pareado).read_text(encoding="utf-8"))
    if a.fase == "c":
        nome, rc, info = veredito(d["por_id"], a.modo)
        out = {"veredito": nome, "modo": a.modo, "sha_A": d.get("sha_A"), "sha_B": d.get("sha_B"), **info}
    else:
        regua, recusa = None, None
        if a.modo == "ab":
            if not a.aa:
                raise Uso("a fase D exige --aa <veredito do A/A>: a regua nao se escolhe depois do A/B")
            regua, recusa = regua_do_aa(a.aa, d)
        juiz = a.juiz or juiz_dos_args(d.get("args") or [])
        nome, rc, info = recusa or veredito_d(d["por_id"], a.modo, regua, juiz)
        out = {"veredito": nome, "modo": a.modo, "sha_A": d.get("sha_A"), "sha_B": d.get("sha_B"), **info}
    texto = json.dumps(out, ensure_ascii=False, sort_keys=True)
    if a.json:
        Path(a.json).write_text(texto, encoding="utf-8")
    print(texto)
    return rc


if __name__ == "__main__":
    sys.exit(main())
