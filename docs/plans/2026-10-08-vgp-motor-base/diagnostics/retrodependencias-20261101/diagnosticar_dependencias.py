#!/usr/bin/env python3
"""Diagnóstico retroativo do 20261101: dependência local atual × Git no instante.

Não emite veredito de release nem reclassifica a verdade dos rótulos. Só agrega a deriva e,
no subconjunto alterado, a mudança de veredito efetivo do motor 30.4.102.
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path

B = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base"
SCRIPTS = B / "scripts"
sys.path.insert(0, str(SCRIPTS))
import estado_no_instante_v0d as eni  # noqa: E402
import juiz_motor_vgp as juiz  # noqa: E402

CORPUS = Path.home() / ".claude/touring/vgp/corpus_holdout_disjunto_20261101"
BINARIO = Path.home() / ".claude/touring/vgp/binarios/30.4.102/touring"
TRABALHO = Path.home() / ".claude/touring/vgp/retrodependencias_20261101"
SAIDA = B / "diagnostics/retrodependencias-20261101"
PRODUTO = Path("/home/gabrielgadea/projects/touring/target/debug/touring")


def entradas() -> list[dict]:
    return [
        json.loads(linha)
        for nome in ("entradas.jsonl", "entradas_inline_neg.jsonl")
        for linha in (CORPUS / nome).read_text().splitlines()
        if linha.strip()
    ]


def classe_veredito(l1: dict, l2: dict) -> str:
    if "_erro" in l1 or l1.get("verdict") == "unknown":
        return "unknown_outro"
    resultado = l2.get("resultado", {})
    verdict = resultado.get("verdict")
    motivo = str(resultado.get("motivo") or l2.get("_erro") or "")
    if verdict == "unknown" or not resultado:
        return "inconclusivo_timeout" if "prazo" in motivo.lower() else "unknown_outro"
    if l1.get("verdict") == "deny" or verdict == "deny":
        return "deny"
    return "nao_deny"


def absolutizar_estado(e: dict, novo: list[dict], original: dict) -> dict:
    estado = dict(original)
    if estado.get("disk_file"):
        estado["disk_file"] = str(CORPUS / estado["disk_file"])
    overlays = [
        {**o, "file": str(CORPUS / o["file"])} for o in estado.get("overlays", [])
    ]
    overlays += [{**o, "file": str(TRABALHO / o["file"])} for o in novo]
    estado["overlays"] = overlays
    return estado


def preparar(limite: int | None = None) -> list[dict]:
    TRABALHO.mkdir(parents=True, exist_ok=True)
    cache = TRABALHO / "analise.jsonl"
    prontos = {}
    if cache.is_file():
        prontos = {r["id"]: r for r in map(json.loads, cache.read_text().splitlines())}
    registros = []
    todos = entradas()[:limite]
    with cache.open("a") as out:
        for n, e in enumerate(todos, 1):
            if e["id"] in prontos:
                registros.append(prontos[e["id"]])
                continue
            texto = (CORPUS / e["snap"]).read_text(errors="ignore")
            fecho = eni.fecho_transitivo(e, texto, {})
            faltantes = [str(m["path"]) for m in fecho if m["conteudo"] is None]
            alteradas = []
            for m in fecho:
                if m["conteudo"] is None:
                    continue
                atual = m["path"]
                texto_atual = atual.read_text(errors="ignore") if atual.is_file() else None
                if texto_atual != m["conteudo"]:
                    alteradas.append(str(atual))
            registro = {
                "id": e["id"],
                "balde": "inline" if e["path"].startswith("<") else "arquivo",
                "dependencias": len(fecho),
                "dependencias_alteradas": len(alteradas),
                "inconclusivo_reconstrucao": bool(faltantes),
                "faltantes": len(faltantes),
            }
            if alteradas and not faltantes:
                over, _ = eni.overlays_e_faltas(
                    e, texto, {}, TRABALHO / "estado_no_instante", fecho
                )
                registro["overlays"] = over
            out.write(json.dumps(registro, ensure_ascii=False) + "\n")
            out.flush()
            registros.append(registro)
            if n % 25 == 0:
                print(f"preparadas {n}/{len(todos)}", flush=True)
    return registros


def _l1(e: dict, overlays: list[dict], manifesto: dict) -> dict:
    entrada = dict(e)
    entrada["snap"] = str(CORPUS / e["snap"])
    entrada["contexto"] = [
        {**c, "snap": str(CORPUS / c["snap"])} for c in e.get("contexto") or []
    ]
    estado = absolutizar_estado(e, overlays, manifesto.get(e["id"], {}))
    original = juiz.carregar_estado
    try:
        juiz.carregar_estado = lambda _corpus, estado=estado: {e["id"]: estado}
        return juiz.rodar_motor([str(BINARIO)], entrada, TRABALHO)
    finally:
        juiz.carregar_estado = original


def _l2(e: dict, overlays: list[dict], manifesto: dict) -> dict:
    entrada = dict(e)
    entrada["snap"] = str(CORPUS / e["snap"])
    entrada["contexto"] = [
        {**c, "snap": str(CORPUS / c["snap"])} for c in e.get("contexto") or []
    ]
    estado = absolutizar_estado(e, overlays, manifesto.get(e["id"], {}))
    original_estado = juiz.carregar_estado
    original_args = juiz.ARGS_MOTOR[:]
    try:
        juiz.carregar_estado = lambda _corpus, estado=estado: {e["id"]: estado}
        juiz.ARGS_MOTOR[:] = ["--camadas", "1,2", "--l2-budget-ms", "30000"]
        saida = juiz.rodar_motor([str(PRODUTO)], entrada, TRABALHO)
    finally:
        juiz.carregar_estado = original_estado
        juiz.ARGS_MOTOR[:] = original_args
    camada2 = saida.get("camada2")
    return {
        "resultado": {
            "verdict": "unknown" if camada2 else saida.get("verdict"),
            "motivo": camada2 or saida.get("motivo"),
            "elapsed_ms": saida.get("elapsed_ms"),
        },
        "l2_budget_ms": saida.get("l2_budget_ms"),
    }


def _comparar_um(e: dict, overlays: list[dict], manifesto: dict) -> dict:
    atual_l1 = _l1(e, [], manifesto)
    historico_l1 = _l1(e, overlays, manifesto)
    atual_l2 = _l2(e, [], manifesto)
    historico_l2 = _l2(e, overlays, manifesto)
    return {
        "veredito_atual": classe_veredito(atual_l1, atual_l2),
        "veredito_historico": classe_veredito(historico_l1, historico_l2),
        "motivo_l2_atual": atual_l2.get("resultado", {}).get("motivo") or atual_l2.get("_erro"),
        "motivo_l2_historico": historico_l2.get("resultado", {}).get("motivo") or historico_l2.get("_erro"),
        "l2_elapsed_ms_atual": atual_l2.get("resultado", {}).get("elapsed_ms"),
        "l2_elapsed_ms_historico": historico_l2.get("resultado", {}).get("elapsed_ms"),
        "l2_budget_ms_atual": atual_l2.get("l2_budget_ms"),
        "l2_budget_ms_historico": historico_l2.get("l2_budget_ms"),
    }


def comparar(registros: list[dict]) -> list[dict]:
    manifesto = json.loads((CORPUS / "estado_no_instante/manifesto.json").read_text())
    por_id = {e["id"]: e for e in entradas()}
    cache = TRABALHO / "comparacoes_v4.jsonl"
    if cache.is_file():
        salvas = {r["id"]: r for r in map(json.loads, cache.read_text().splitlines())}
        for r in registros:
            if r["id"] in salvas:
                r.update({k: v for k, v in salvas[r["id"]].items() if k.startswith(("veredito_", "motivo_l2_", "l2_elapsed_", "l2_budget_"))})
    juiz.ARGS_MOTOR[:] = ["--camadas", "1"]
    with cache.open("a") as out:
        for n, r in enumerate(registros, 1):
            if not r.get("overlays") or "veredito_historico" in r:
                continue
            r.update(_comparar_um(por_id[r["id"]], r["overlays"], manifesto))
            chaves = ("id", "veredito_atual", "veredito_historico", "motivo_l2_atual", "motivo_l2_historico", "l2_elapsed_ms_atual", "l2_elapsed_ms_historico", "l2_budget_ms_atual", "l2_budget_ms_historico")
            out.write(json.dumps({k: r[k] for k in chaves}) + "\n")
            out.flush()
            if n % 10 == 0:
                print(f"comparadas {n}/{len(registros)}", flush=True)
    return registros


def _par_conhecido(r: dict) -> bool:
    return r["veredito_atual"] in ("deny", "nao_deny") and r["veredito_historico"] in ("deny", "nao_deny")


def resumir(registros: list[dict]) -> dict:
    alteradas = [r for r in registros if r["dependencias_alteradas"]]
    comparados = [r for r in alteradas if r.get("veredito_historico")]
    conhecidos = [r for r in comparados if _par_conhecido(r)]
    mudancas = Counter(
        (r["veredito_atual"], r["veredito_historico"])
        for r in conhecidos
        if r["veredito_atual"] != r["veredito_historico"]
    )
    return {
        "escopo": "diagnóstico de sensibilidade da reconstrução; não é novo veredito nem reclassificação de rótulos",
        "corpus": "20261101",
        "motor": "30.4.102",
        "entradas": len(registros),
        "por_balde": dict(Counter(r["balde"] for r in registros)),
        "dependencias_locais": sum(r["dependencias"] for r in registros),
        "dependencias_alteradas": sum(r["dependencias_alteradas"] for r in registros),
        "entradas_com_dependencia_alterada": len(alteradas),
        "inconclusivo_reconstrucao": sum(r["inconclusivo_reconstrucao"] for r in registros),
        "alteradas_reconstruiveis": sum(bool(r.get("overlays")) for r in alteradas),
        "comparacoes_motor": len(comparados),
        "unknown_outro_atual": sum(r["veredito_atual"] == "unknown_outro" for r in comparados),
        "unknown_outro_historico": sum(r["veredito_historico"] == "unknown_outro" for r in comparados),
        "inconclusivo_timeout_atual": sum(r["veredito_atual"] == "inconclusivo_timeout" for r in comparados),
        "inconclusivo_timeout_historico": sum(r["veredito_historico"] == "inconclusivo_timeout" for r in comparados),
        "mudanca_veredito_efetivo": sum(mudancas.values()),
        "deny_para_nao_deny": mudancas[("deny", "nao_deny")],
        "nao_deny_para_deny": mudancas[("nao_deny", "deny")],
        "sem_mudanca_conhecida": sum(
            r["veredito_atual"] == r["veredito_historico"] for r in conhecidos
        ),
        "limite": "pacotes de terceiros não são congelados; mudança de veredito mede sensibilidade, não verdade do rótulo",
    }


def selftest() -> int:
    def l2(verdict: str, motivo: str | None = None) -> dict:
        return {"resultado": {"verdict": verdict, "motivo": motivo}}

    assert classe_veredito({"verdict": "allow"}, l2("deny")) == "deny"
    assert classe_veredito({"verdict": "deny"}, l2("allow")) == "deny"
    assert classe_veredito({"verdict": "allow"}, l2("allow")) == "nao_deny"
    assert classe_veredito({"verdict": "allow"}, l2("unknown", "prazo de 30000 ms")) == "inconclusivo_timeout"
    assert classe_veredito({"verdict": "allow"}, l2("unknown", "servidor morreu")) == "unknown_outro"
    assert classe_veredito({"_erro": "x"}, l2("allow")) == "unknown_outro"
    print("selftest: 6 asserções OK")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--limite", type=int)
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--preparar", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    registros = preparar(args.limite)
    if not args.preparar:
        registros = comparar(registros)
    resumo = resumir(registros)
    resumo["controle_positivo_10607309d143"] = next(
        ({"atual": r["veredito_atual"], "historico": r["veredito_historico"]} for r in registros if r["id"] == "10607309d143" and r.get("veredito_historico")),
        {},
    )
    print(json.dumps(resumo, ensure_ascii=False, indent=1))
    if args.limite is None and not args.preparar:
        SAIDA.mkdir(parents=True, exist_ok=True)
        (SAIDA / "retro.json").write_text(json.dumps(resumo, ensure_ascii=False, indent=1) + "\n")
        publico = [
            {k: v for k, v in r.items() if k != "overlays"}
            | {"overlays_gravados": len(r.get("overlays", []))}
            for r in registros
        ]
        (SAIDA / "porid.json").write_text(json.dumps(publico, ensure_ascii=False, indent=1) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
