#!/usr/bin/env python3
"""Mutation check of the phase D rules (PREREGISTRO-FASE-D-2026-10-18.md).

For every pre-registered rule there is a cheapest wrong implementation (a limit swapped, the ruler re-chosen after the
A/B, an absent reading read as zero, a PMU lost from the sum, ...). Each one is applied to a COPY of the scripts and the
tests must go red. The CONTROL (no mutation) runs first and must be green: a mutant verdict means nothing otherwise.

  killed   = at least one test FAILED on the mutated copy
  survived = every test passed -> the rule has no test that depends on it (a hole in the suite)
  invalid  = the mutated copy does not compile, or the anchor is not found exactly once (a hole in THIS file)

Usage: mutantes_fase_d.py [--json <saida>] [--jobs N] [--so <id>,...]       exit 0 iff control green and all killed
"""

import argparse
import concurrent.futures
import json
import py_compile
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

AQUI = Path(__file__).resolve().parent
ARQUIVOS = [
    "conta_custo.py",
    "analisa_pareado.py",
    "tempos_pareados.py",
    "test_conta_custo.py",
    "test_analisa_pareado.py",
    "test_analisa_pareado_d.py",
    "test_tempos_pareados.py",
]
T_ANALISE = ["test_analisa_pareado_d.py", "test_analisa_pareado.py"]
T_LANCADOR = ["test_conta_custo.py"]
T_PAREADO = ["test_tempos_pareados.py"]

# (id, alvo, regra, ancora, substituicao, testes)
MUTANTES = [
    # ---- analisa_pareado.py: the limits
    ("A01", "analisa_pareado.py", "A/A instr: faixa frouxa [0,90; 1,10]",
     "AA_INSTR_FAIXA = (0.99, 1.01)", "AA_INSTR_FAIXA = (0.90, 1.10)", T_ANALISE),
    ("A02", "analisa_pareado.py", "A/A instr: faixa apertada [0,999; 1,001]",
     "AA_INSTR_FAIXA = (0.99, 1.01)", "AA_INSTR_FAIXA = (0.999, 1.001)", T_ANALISE),
    ("A03", "analisa_pareado.py", "A/A cpu: faixa frouxa [0,80; 1,20]",
     "AA_CPU_FAIXA = (0.95, 1.05)", "AA_CPU_FAIXA = (0.80, 1.20)", T_ANALISE),
    ("A04", "analisa_pareado.py", "A/A cpu: faixa apertada [0,99; 1,01]",
     "AA_CPU_FAIXA = (0.95, 1.05)", "AA_CPU_FAIXA = (0.99, 1.01)", T_ANALISE),
    ("A05", "analisa_pareado.py", "A/B: teto 1,30 em vez de 1,10", "AB_TETO = 1.10", "AB_TETO = 1.30", T_ANALISE),
    ("A06", "analisa_pareado.py", "A/B: teto 1,02 em vez de 1,10", "AB_TETO = 1.10", "AB_TETO = 1.02", T_ANALISE),
    ("A07", "analisa_pareado.py", "absoluto: 1500 ms em vez de 150",
     "ABSOLUTO_CPU_MS = 150.0", "ABSOLUTO_CPU_MS = 1500.0", T_ANALISE),
    ("A08", "analisa_pareado.py", "absoluto: 15 ms em vez de 150",
     "ABSOLUTO_CPU_MS = 150.0", "ABSOLUTO_CPU_MS = 15.0", T_ANALISE),
    ("A09", "analisa_pareado.py", "semente 20261017 (a do C') em vez de 20261018",
     "BOOT_SEMENTE_D = 20261018", "BOOT_SEMENTE_D = 20261017", T_ANALISE),
    ("A10", "analisa_pareado.py", "bootstrap com 200 reamostras em vez de 2000", "BOOT_N = 2000", "BOOT_N = 200", T_ANALISE),
    ("A11", "analisa_pareado.py", "ausencia tolerada ate 50% em vez de 1%", "FALTA_MAX = 0.01", "FALTA_MAX = 0.5", T_ANALISE),
    # ---- analisa_pareado.py: the ruler is the A/A's, never re-chosen
    ("A12", "analisa_pareado.py", "regua re-escolhida depois do A/B (a que passa melhor)",
     "alvo = regua  # fixed by the A/A; reading the A/B must never change it",
     'alvo = min(("instr", "cpu"), key=lambda r: max(v[1] for v in ic[r].values()))', T_ANALISE),
    ("A13", "analisa_pareado.py", "regua invertida no A/B",
     "alvo = regua  # fixed by the A/A; reading the A/B must never change it",
     'alvo = "cpu" if regua == "instr" else "instr"', T_ANALISE),
    ("A14", "analisa_pareado.py", "A/B sempre nas instrucoes",
     "alvo = regua  # fixed by the A/A; reading the A/B must never change it", 'alvo = "instr"', T_ANALISE),
    ("A15", "analisa_pareado.py", "A/A prefere a CPU quando as duas valem",
     'info["regua"] = "instr" if ok["instr"] else ("cpu" if ok["cpu"] else None)',
     'info["regua"] = "cpu" if ok["cpu"] else ("instr" if ok["instr"] else None)', T_ANALISE),
    ("A16", "analisa_pareado.py", "A/A confere so o limite superior da faixa",
     "return all(faixa[0] <= v[0] and v[1] <= faixa[1] for v in ic.values())",
     "return all(v[1] <= faixa[1] for v in ic.values())", T_ANALISE),
    ("A17", "analisa_pareado.py", "A/B nao olha o R_p95",
     "relativo = all(v[1] <= AB_TETO for v in ic[alvo].values())",
     'relativo = ic[alvo]["R_tipico"][1] <= AB_TETO', T_ANALISE),
    ("A18", "analisa_pareado.py", "A/B nao olha o R_tipico",
     "relativo = all(v[1] <= AB_TETO for v in ic[alvo].values())",
     'relativo = ic[alvo]["R_p95"][1] <= AB_TETO', T_ANALISE),
    # ---- analisa_pareado.py: absence is never zero
    ("A19", "analisa_pareado.py", "ausencia vira zero",
     "bons = [[x for x in lst if valido(x)] for lst in listas]",
     "bons = [[x if valido(x) else 0.0 for x in lst] for lst in listas]", T_ANALISE),
    ("A20", "analisa_pareado.py", "campo ausente nao conta como chamadas esperadas",
     "esperado = sum(max(len(lst), CHAMADAS_POR_LADO) for lst in listas)",
     "esperado = sum(len(lst) for lst in listas)", T_ANALISE),
    ("A21", "analisa_pareado.py", "limite de ausencia em chamadas removido",
     'tot == 0 or falt > FALTA_MAX * tot for m in ("instr", "cpu")',
     'tot == 0 for m in ("instr", "cpu")', T_ANALISE),
    ("A22", "analisa_pareado.py", "regra do C' (parede por id) removida",
     'sem_parede = len(ext["parede"][3]) > FALTA_MAX * len(por_id)', "sem_parede = False", T_ANALISE),
    ("A23", "analisa_pareado.py", "id sem outra medida continua nas reguas",
     "pares = {m: {i: pr[i] for i in pr if i not in set(excluidos)} for m, (pr, _, _, _) in ext.items()}",
     "pares = {m: dict(pr) for m, (pr, _, _, _) in ext.items()}", T_ANALISE),
    # ---- analisa_pareado.py: the absolute
    ("A24", "analisa_pareado.py", "absoluto vira portao tambem no V3",
     'portao_abs = juiz == "v2"', "portao_abs = True", T_ANALISE),
    ("A25", "analisa_pareado.py", "absoluto nunca e portao",
     'portao_abs = juiz == "v2"', "portao_abs = False", T_ANALISE),
    ("A26", "analisa_pareado.py", "absoluto pelo p95 das medianas por id, nao das chamadas",
     'chamadas_b = [x for i in pares["cpu"] for x in por_id[i]["cpu_B"]]',
     'chamadas_b = [b for _, b in pares["cpu"].values()]', T_ANALISE),
    # ---- analisa_pareado.py: the CLI
    ("A27", "analisa_pareado.py", "juiz sempre v2",
     'return "v3" if "2" in camadas.split(",") else "v2"', 'return "v2"', T_ANALISE),
    ("A28", "analisa_pareado.py", "A/B le o A/B mesmo com A/A inadequado",
     'if aa.get("veredito") == "PROTOCOLO_OK" and aa.get("regua") in ("instr", "cpu"):',
     'if aa.get("veredito") != "INCONCLUSIVO":', T_ANALISE),
    ("A29", "analisa_pareado.py", "referencia do A/B pode diferir da do A/A",
     'if aa.get("sha_A") and d.get("sha_A") and aa["sha_A"] != d["sha_A"]:', "if False:", T_ANALISE),
    ("A30", "analisa_pareado.py", "A/B sem veredito do A/A e aceito", "if not a.aa:", "if False:", T_ANALISE),
    # ---- conta_custo.py
    ("L01", "conta_custo.py", "soma de PMUs perde uma (so a primeira)",
     "total = sum(v for v, _, _ in lidos.values())", "total = next(iter(lidos.values()))[0]", T_LANCADOR),
    ("L02", "conta_custo.py", "instr_atom some do JSON",
     'r["instr_atom"] = lidos.get("cpu_atom", (None,))[0]', 'r["instr_atom"] = None', T_LANCADOR),
    ("L03", "conta_custo.py", "contador indisponivel vira 0 (e nao null)",
     'r = {"instr": None, "instr_core": None, "instr_atom": None,',
     'r = {"instr": 0, "instr_core": 0, "instr_atom": 0,', T_LANCADOR),
    ("L04", "conta_custo.py", "descendentes nao contados (sem inherit)",
     "FLAGS = F_DISABLED | F_INHERIT | F_EXCLUDE_KERNEL | F_EXCLUDE_HV | F_ENABLE_ON_EXEC",
     "FLAGS = F_DISABLED | F_EXCLUDE_KERNEL | F_EXCLUDE_HV | F_ENABLE_ON_EXEC", T_LANCADOR),
    ("L05", "conta_custo.py", "conta tambem o python entre o fork e o exec (sem enable_on_exec)",
     "FLAGS = F_DISABLED | F_INHERIT | F_EXCLUDE_KERNEL | F_EXCLUDE_HV | F_ENABLE_ON_EXEC",
     "FLAGS = F_INHERIT | F_EXCLUDE_KERNEL | F_EXCLUDE_HV", T_LANCADOR),
    ("L06", "conta_custo.py", "conta o kernel (sem exclude_kernel)",
     "FLAGS = F_DISABLED | F_INHERIT | F_EXCLUDE_KERNEL | F_EXCLUDE_HV | F_ENABLE_ON_EXEC",
     "FLAGS = F_DISABLED | F_INHERIT | F_EXCLUDE_HV | F_ENABLE_ON_EXEC", T_LANCADOR),
    ("L07", "conta_custo.py", "rc do JSON sempre 0",
     '"wall_ms": round(wall, 3), "rc": codigo_de_saida(status), "erro": None,',
     '"wall_ms": round(wall, 3), "rc": 0, "erro": None,', T_LANCADOR),
    ("L08", "conta_custo.py", "codigo de saida do lancador sempre 0", 'return r["rc"]', "return 0", T_LANCADOR),
    ("L09", "conta_custo.py", "cpu_ms sem o stime",
     "return (uso.ru_utime + uso.ru_stime) * 1000.0", "return uso.ru_utime * 1000.0", T_LANCADOR),
    ("L10", "conta_custo.py", "falha de exec ignorada (le o contador que nunca ligou)",
     "    if falha_exec:", "    if False:", T_LANCADOR),
    ("L11", "conta_custo.py", "o comando sobrevive ao lancador (sem PDEATHSIG)",
     "ctypes.CDLL(None, use_errno=True).prctl(PR_SET_PDEATHSIG, signal.SIGKILL, 0, 0, 0)", "pass", T_LANCADOR),
    ("L12", "conta_custo.py", "PMU que nao abre e pulada (a soma segue sem ela)",
     'raise ContadorIndisponivel(f"{nome}: perf_event_open falhou: {e.strerror or e}") from e',
     "continue", T_LANCADOR),
    ("L13", "conta_custo.py", "leitura 0 aceita como 0", "    if total <= 0:", "    if total < 0:", T_LANCADOR),
    # ---- tempos_pareados.py
    ("P01", "tempos_pareados.py", "leitura ausente do lancador vira 0",
     'return {"instr": None, "cpu_ms": None, "erro": f"sem leitura do lancador: {type(e).__name__}"}',
     'return {"instr": 0, "cpu_ms": 0, "erro": f"sem leitura do lancador: {type(e).__name__}"}', T_PAREADO),
    ("P02", "tempos_pareados.py", "instr_A/instr_B trocados",
     'reg[f"instr_{lado}"].append(custo.get("instr"))',
     'reg[f"instr_{\'B\' if lado == \'A\' else \'A\'}"].append(custo.get("instr"))', T_PAREADO),
    ("P03", "tempos_pareados.py", "a chamada nao passa pelo lancador",
     'prefixo = [sys.executable, "-I", str(LANCADOR), "--out", str(arq), "--", binario]',
     "prefixo = [binario]", T_PAREADO),
    ("P04", "tempos_pareados.py", "ordem AABBAABB em vez de ABBABAAB",
     'ORDEM = "ABBABAAB"', 'ORDEM = "AABBAABB"', T_PAREADO),
]


EXTRA_PYTEST: list[str] = []  # `--sem-constantes` fills it: the behaviour tests alone, without the constants pin


def prepara(destino: Path) -> None:
    for nome in ARQUIVOS:
        shutil.copy2(AQUI / nome, destino / nome)


def pytest(pasta: Path, testes: list[str], parar_no_primeiro: bool) -> tuple[int, str]:
    cmd = [sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", *(["-x"] if parar_no_primeiro else []),
           *EXTRA_PYTEST, *testes]
    env = {"PATH": "/usr/bin:/bin:/usr/local/bin", "PYTHONDONTWRITEBYTECODE": "1", "HOME": str(pasta)}
    r = subprocess.run(cmd, cwd=pasta, capture_output=True, text=True, check=False, env=env)
    return r.returncode, r.stdout + r.stderr


def um_mutante(m: tuple) -> dict:
    mid, alvo, regra, ancora, subst, testes = m
    with tempfile.TemporaryDirectory(prefix=f"mut-{mid}-") as t:
        pasta = Path(t)
        prepara(pasta)
        arq = pasta / alvo
        fonte = arq.read_text(encoding="utf-8")
        if fonte.count(ancora) != 1:
            return {"id": mid, "regra": regra, "estado": "invalid", "motivo": f"ancora encontrada {fonte.count(ancora)}x"}
        arq.write_text(fonte.replace(ancora, subst), encoding="utf-8")
        try:
            py_compile.compile(str(arq), doraise=True)
        except py_compile.PyCompileError as e:
            return {"id": mid, "regra": regra, "estado": "invalid", "motivo": f"nao compila: {e.msg[:80]}"}
        rc, saida = pytest(pasta, testes, parar_no_primeiro=True)
        falhou = re.findall(r"^FAILED (\S+)", saida, flags=re.MULTILINE)
        if rc == 0:
            return {"id": mid, "regra": regra, "estado": "survived", "testes": testes}
        if falhou:
            return {"id": mid, "regra": regra, "estado": "killed", "por": falhou[0]}
        return {"id": mid, "regra": regra, "estado": "invalid", "motivo": f"pytest rc={rc} sem FAILED: {saida[-200:]}"}


def controle() -> tuple[int, str, str]:
    """The suite on an UNMUTATED copy: (rc, last line, full output). A mutant verdict means nothing unless it is green."""
    with tempfile.TemporaryDirectory(prefix="mut-controle-") as t:
        prepara(Path(t))
        rc, saida = pytest(Path(t), sorted({x for m in MUTANTES for x in m[5]}), parar_no_primeiro=False)
    return rc, (saida.strip().splitlines()[-1] if saida.strip() else "(sem saida)"), saida


def relata(res: list[dict], resumo_controle: str, destino: str | None) -> int:
    for r in res:
        print(f"  {r['id']} {r['estado']:8} {r['regra']}  {r.get('por') or r.get('motivo') or ''}")
    mortos = sum(r["estado"] == "killed" for r in res)
    print(f"MUTANTES: {mortos}/{len(res)} mortos")
    if destino:
        Path(destino).write_text(json.dumps({"controle": resumo_controle, "mortos": mortos, "total": len(res),
                                             "mutantes": res}, ensure_ascii=False, indent=1), encoding="utf-8")
    return 0 if mortos == len(res) else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--json")
    ap.add_argument("--jobs", type=int, default=2)
    ap.add_argument("--so", help="ids separados por virgula")
    ap.add_argument("--sem-constantes", action="store_true",
                    help="deselect test_constantes_pre_registradas: do the BEHAVIOUR tests alone kill the limit mutants?")
    a = ap.parse_args()
    if a.sem_constantes:
        EXTRA_PYTEST[:] = ["--deselect", "test_analisa_pareado_d.py::test_constantes_pre_registradas"]
    rc, resumo_controle, saida = controle()
    print(f"CONTROLE (sem mutacao): rc={rc} {resumo_controle}")
    if rc != 0:
        print(saida[-3000:])
        return 1
    escolhidos = [m for m in MUTANTES if not a.so or m[0] in a.so.split(",")]
    with concurrent.futures.ThreadPoolExecutor(max_workers=a.jobs) as ex:
        res = list(ex.map(um_mutante, escolhidos))
    return relata(res, resumo_controle, a.json)


if __name__ == "__main__":
    sys.exit(main())
