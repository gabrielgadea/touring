#!/usr/bin/env python3
"""Consolida o aceite J15 sem transformar o p95 frio informativo em gate."""

import argparse
import json
import math
import re
from pathlib import Path

P95_FRIO = re.compile(r"p95 \S+ ms > \S+ ms")
LIMITE_QUENTE_MS = 1000.0
PATAMAR_TIMEOUT = 0.20


def _pendencias_reais(frio: dict) -> list[str]:
    """Remove apenas a pendência de latência fria; preserva todos os demais gates."""
    return [p for p in frio.get("pendencias", []) if not P95_FRIO.fullmatch(p)]


def _avaliar_frios(v2: dict, v3: dict) -> tuple[list[str], list[str]]:
    pendencias = [f"v2: {p}" for p in _pendencias_reais(v2)]
    pendencias += [f"V3: {p}" for p in _pendencias_reais(v3)]
    inconclusivo = [
        f"{nome}: {frio.get('inconclusivo_motivo') or 'sem motivo'}"
        for nome, frio in (("v2", v2), ("V3", v3))
        if frio.get("inconclusivo")
    ]
    return pendencias, inconclusivo


def _falhas_rotas(rotas: dict, chamadas: int) -> list[str]:
    falhas = []
    if rotas.get("daemon") != 2 * chamadas:
        falhas.append("quente: rotas daemon não fecham 2× o denominador")
    if sum(rotas.values()) != 2 * chamadas:
        falhas.append("quente: há rota não-daemon ou ausente")
    return falhas


def _falhas_denominador(hot: dict, chamadas: int | None) -> list[str]:
    denominador = hot.get("denominador") or {}
    esperado = denominador.get("entradas_esperadas")
    falhas = [] if hot.get("valido") else ["quente: instrumento inválido ou sinal ausente"]
    if not isinstance(chamadas, int) or chamadas <= 0:
        return falhas + [f"quente: denominador do juiz inválido: {chamadas}"]
    if esperado != chamadas:
        falhas.append(f"quente: denominador {esperado} ≠ chamadas do juiz {chamadas}")
    for campo in ("medidas_com_sombra", "medidas_com_camada2"):
        if denominador.get(campo) != chamadas:
            falhas.append(f"quente: {campo}={denominador.get(campo)} ≠ {chamadas}")
    return falhas + _falhas_rotas(denominador.get("rotas") or {}, chamadas)


def _falha_latencia(hot: dict) -> tuple[list[str], list[str]]:
    p95 = hot.get("p95_motor_ms")
    if not isinstance(p95, (int, float)) or not math.isfinite(p95):
        return [], ["quente: p95 ausente ou não finito"]
    if p95 > LIMITE_QUENTE_MS:
        return [f"p95 quente {p95:.3f} ms > {LIMITE_QUENTE_MS:.0f} ms"], []
    return [], []


def _falha_timeout(hot: dict, chamadas: int | None) -> list[str]:
    timeouts = hot.get("timeouts_camada2")
    if not isinstance(timeouts, int) or not isinstance(chamadas, int) or chamadas <= 0:
        return ["quente: taxa de timeout sem denominador válido"]
    if timeouts / chamadas > PATAMAR_TIMEOUT:
        taxa = timeouts / chamadas
        return [f"quente: timeouts {timeouts}/{chamadas} = {taxa:.2%} > {PATAMAR_TIMEOUT:.0%}"]
    return []


def avaliar(v2: dict, v3: dict, hot: dict) -> dict:
    chamadas = v3.get("chamadas")
    pendencias, inconclusivo = _avaliar_frios(v2, v3)
    inconclusivo += _falhas_denominador(hot, chamadas)
    latencia, latencia_ausente = _falha_latencia(hot)
    pendencias += latencia
    inconclusivo += latencia_ausente
    inconclusivo += _falha_timeout(hot, chamadas)
    veredito = "INCONCLUSIVO" if inconclusivo else ("NÃO PRONTO" if pendencias else "PRONTO")
    return {
        "veredito": veredito,
        "pendencias": pendencias,
        "inconclusivo_motivos": inconclusivo,
        "p95_frio_informativo_ms": {"v2": v2.get("p95_ms"), "V3": v3.get("p95_ms")},
        "p95_quente_ms": hot.get("p95_motor_ms"),
        "timeouts_quente": hot.get("timeouts_camada2"),
        "chamadas": chamadas,
    }


def consolidar(diretorio: Path) -> dict:
    v2 = json.loads((diretorio / "30_4_102_v2.json").read_text(encoding="utf-8"))
    v3 = json.loads((diretorio / "30_4_102_v3.json").read_text(encoding="utf-8"))
    hot = json.loads((diretorio / "hot.json").read_text(encoding="utf-8"))
    return avaliar(v2, v3, hot)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("diretorio", type=Path)
    parser.add_argument("--saida", type=Path)
    args = parser.parse_args()
    resultado = consolidar(args.diretorio)
    texto = json.dumps(resultado, ensure_ascii=False, indent=1) + "\n"
    if args.saida:
        args.saida.write_text(texto, encoding="utf-8")
    print(texto, end="")
    return {"PRONTO": 0, "NÃO PRONTO": 1, "INCONCLUSIVO": 3}[resultado["veredito"]]


if __name__ == "__main__":
    raise SystemExit(main())
