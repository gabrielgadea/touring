#!/usr/bin/env python3
"""Consolidate the full hot run with the single repaired warm-up/measurement pair."""

import collections
import importlib.util
import json
from pathlib import Path

AQUI = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("medir_daemon_quente", AQUI / "medir_daemon_quente.py")
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)


def exigir(condicao: bool, mensagem: str) -> None:
    if not condicao:
        raise SystemExit(mensagem)


def ler_jsonl(path: Path, session: str) -> list[dict]:
    out = []
    for line in path.read_text().splitlines():
        if not line.strip():
            continue
        registro = json.loads(line)
        if registro.get("session_id") == session:
            out.append(registro)
    return out


def carregar() -> tuple[dict, dict, dict, list[dict], list[dict], int, dict]:
    original = json.loads((AQUI / "hot_daemon.json").read_text())
    reparo = json.loads((AQUI / "hot_reparo_134.json").read_text())
    exigir(reparo["valido"] and reparo["denominador"]["rotas"] == {"daemon": 2}, "reparo inválido")
    por_original = json.loads((AQUI / "hot_daemon.porid.json").read_text())
    por_reparo = json.loads((AQUI / "hot_reparo_134.porid.json").read_text())["por_id"]
    exigir(len(por_reparo) == 1, "reparo precisa ter uma entrada")
    entrada_reparo = next(iter(por_reparo.values()))
    por_id = por_original["por_id"]
    chaves = [key for key, value in por_id.items() if value["id"] == entrada_reparo["id"]]
    exigir(len(chaves) == 1, "id do reparo ausente ou duplicado no original")
    entrada_reparo = dict(entrada_reparo, i=por_id[chaves[0]]["i"])
    por_id[chaves[0]] = entrada_reparo
    trace = ler_jsonl(AQUI / "hot_daemon.trace.jsonl", original["session"])
    standalone = [(i, r) for i, r in enumerate(trace) if r.get("route") != "daemon"]
    exigir(len(standalone) == 1 and standalone[0][0] == 268, "anomalia original mudou")
    posicao, linha_standalone = standalone[0]
    reparo_trace = ler_jsonl(AQUI / "hot_reparo_134.trace.jsonl", reparo["session"])
    exigir(
        len(reparo_trace) == 2 and all(r["route"] == "daemon" for r in reparo_trace),
        "trace do reparo não é integralmente daemon",
    )
    return original, reparo, por_id, trace, reparo_trace, posicao, linha_standalone


def construir_resumo(
    original: dict,
    reparo: dict,
    por_id: dict,
    trace: list[dict],
    posicao: int,
    linha_standalone: dict,
) -> dict:
    exigir(len(trace) == 2128, "denominador do trace consolidado mudou")
    registros = list(por_id.values())
    medidas = [r for r in registros if "elapsed_ms" in r]
    com_camada2 = [r for r in medidas if 2 in (r.get("camadas") or [])]
    rotas = collections.Counter(r.get("route") or "ausente" for r in trace)
    esperado = len(registros)
    valido = mod.medicao_valida(esperado, medidas, com_camada2, rotas)
    tempos = [float(r["elapsed_ms"]) for r in com_camada2]
    wall = [float(r["wall_ms"]) for r in com_camada2]
    entrada_reparo = next(r for r in registros if r["i"] == 134)
    resumo = dict(original)
    resumo["session"] = f"{original['session']} + reparo {reparo['session']}"
    resumo["metodo"] += "; entrada 134 repetida e substituída após um aquecimento standalone"
    resumo["denominador"] = dict(original["denominador"], rotas=dict(rotas))
    resumo["p50_motor_ms"] = mod.percentil(tempos, 0.50)
    resumo["p95_motor_ms"] = mod.percentil(tempos, 0.95)
    resumo["p95_wall_shim_ms"] = mod.percentil(wall, 0.95)
    resumo["timeouts_camada2"] = sum(
        str(r.get("camada2") or "").startswith("unknown: prazo de") for r in com_camada2
    )
    resumo["camada2_sem_veredito"] = sum(
        str(r.get("camada2") or "").startswith("unknown:") for r in com_camada2
    )
    resumo["reparo"] = {
        "id": entrada_reparo["id"],
        "posicao_no_trace": posicao,
        "standalone_original": linha_standalone,
        "rotas_do_reparo": [r["route"] for r in trace[posicao : posicao + 2]],
        "p95_antes_do_reparo": original["p95_motor_ms"],
        "elapsed_ms_da_entrada_depois": entrada_reparo["elapsed_ms"],
    }
    resumo["valido"] = valido
    resumo["veredito"] = "OK" if valido else "INCONCLUSIVO"
    return resumo


def main() -> int:
    original, reparo, por_id, trace, reparo_trace, posicao, standalone = carregar()
    trace[posicao : posicao + 2] = reparo_trace
    resumo = construir_resumo(original, reparo, por_id, trace, posicao, standalone)
    (AQUI / "hot_reparado.json").write_text(json.dumps(resumo, ensure_ascii=False, indent=1) + "\n")
    (AQUI / "hot_reparado.porid.json").write_text(
        json.dumps({"session": resumo["session"], "por_id": por_id}, ensure_ascii=False, indent=1) + "\n"
    )
    (AQUI / "hot_reparado.trace.jsonl").write_text(
        "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in trace)
    )
    campos = ("denominador", "p50_motor_ms", "p95_motor_ms", "p95_wall_shim_ms", "timeouts_camada2", "camada2_sem_veredito", "valido", "veredito")
    print(json.dumps({k: resumo[k] for k in campos}, ensure_ascii=False))
    return 0 if resumo["valido"] else 3


if __name__ == "__main__":
    raise SystemExit(main())
