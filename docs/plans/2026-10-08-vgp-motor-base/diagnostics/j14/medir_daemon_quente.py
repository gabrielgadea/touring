#!/usr/bin/env python3
"""Measure VGP layer 2 through the live daemon hook path on a hybrid corpus."""

import argparse
import collections
import json
import math
import os
import shlex
import subprocess
import time
from pathlib import Path

SHIM = Path.home() / ".claude/hooks/touring-hook"
SOMBRA = Path.home() / ".claude/touring/vgp/sombra.jsonl"


def carregar(corpus: Path) -> list[dict]:
    entradas = [
        json.loads(line)
        for line in (corpus / "entradas.jsonl").read_text().splitlines()
        if line.strip()
    ]
    extra = corpus / "entradas_inline_neg.jsonl"
    if extra.is_file():
        entradas += [
            json.loads(line) for line in extra.read_text().splitlines() if line.strip()
        ]
    return entradas


def percentil(valores: list[float], fracao: float) -> float | None:
    if not valores:
        return None
    ordenados = sorted(valores)
    return ordenados[min(len(ordenados) - 1, math.ceil(fracao * len(ordenados)) - 1)]


def extrator_inline(entrada: dict) -> str:
    return entrada.get("extrator") or {
        "<string>": "c",
        "<stdin>": "heredoc",
    }.get(entrada["path"], "")


def comando_inline(entrada: dict, corpus: Path) -> str:
    programa = (corpus / entrada["snap"]).read_text(encoding="utf-8")
    extrator = extrator_inline(entrada)
    if extrator == "c":
        return f"python3 -c {shlex.quote(programa)}"
    if extrator == "heredoc":
        return f"python3 - <<'__VGP_HOT__'\n{programa}\n__VGP_HOT__"
    if extrator == "touring_run":
        return f"touring run --lang python --code {shlex.quote(programa)}"
    return ""


def chamada(
    entrada: dict,
    corpus: Path,
    session: str,
    sequencia: int,
    fase: str,
    trace: Path,
) -> tuple[str, int, float]:
    inline = entrada["path"].startswith("<")
    hook = "pre-bash" if inline else "pre-write"
    tool_use_id = f"{session}:{fase}:{sequencia}:{entrada['id']}"
    if inline:
        tool_input = {"command": comando_inline(entrada, corpus)}
    else:
        conteudo = (corpus / entrada["snap"]).read_text(encoding="utf-8")
        tool_input = {"file_path": entrada["path"], "content": conteudo}
    payload = {
        "session_id": session,
        "tool_use_id": tool_use_id,
        "cwd": entrada["project_root"],
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash" if inline else "Write",
        "tool_input": tool_input,
    }
    env = os.environ.copy()
    env.pop("TOURING_NO_DAEMON", None)
    env["CLAUDE_PROJECT_DIR"] = entrada["project_root"]
    env["TOURING_CODE_MODE"] = "native"
    env["TOURING_HOOK_TRACE_FILE"] = str(trace)
    inicio = time.monotonic()
    try:
        run = subprocess.run(
            [str(SHIM), hook],
            input=json.dumps(payload),
            text=True,
            capture_output=True,
            timeout=65,
            check=False,
            env=env,
        )
        rc = run.returncode
    except (OSError, subprocess.TimeoutExpired):
        rc = 124
    return tool_use_id, rc, round((time.monotonic() - inicio) * 1000, 3)


def argumentos() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--saida", type=Path, required=True)
    parser.add_argument("--por-id", type=Path, required=True)
    parser.add_argument("--trace", type=Path, required=True)
    parser.add_argument("--limite", type=int)
    args = parser.parse_args()
    for path in (args.saida, args.por_id, args.trace):
        if path.exists():
            raise SystemExit(f"destino já existe: {path}")
    return args


def medir_entradas(
    entradas: list[dict], corpus: Path, session: str, trace: Path
) -> tuple[dict, list[dict]]:
    registros = {}
    ignorados = []
    for i, entrada in enumerate(entradas):
        inline = entrada["path"].startswith("<")
        if inline and extrator_inline(entrada) not in ("c", "heredoc", "touring_run"):
            ignorados.append({"i": i, "id": entrada["id"], "motivo": "extrator inline desconhecido"})
            continue
        if not (corpus / entrada["snap"]).stat().st_size:
            ignorados.append({"i": i, "id": entrada["id"], "motivo": "conteúdo vazio"})
            continue
        chamada(entrada, corpus, session, i, "aquecimento", trace)
        tool_id, rc, wall = chamada(entrada, corpus, session, i, "medida", trace)
        registros[tool_id] = {
            "i": i,
            "id": entrada["id"],
            "hook": "pre-bash" if inline else "pre-write",
            "rc": rc,
            "wall_ms": wall,
        }
    return registros, ignorados


def jsonl_da_session(path: Path, session: str) -> list[dict]:
    if not path.is_file():
        return []
    out = []
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        try:
            registro = json.loads(line)
        except json.JSONDecodeError:
            continue
        if registro.get("session_id") == session:
            out.append(registro)
    return out


def enriquecer_com_sombra(registros: dict, session: str) -> None:
    sombra_por_id = {r.get("tool_use_id"): r for r in jsonl_da_session(SOMBRA, session)}
    for tool_id, registro in registros.items():
        sombra = sombra_por_id.get(tool_id)
        if sombra is not None:
            registro.update(
                {
                    "elapsed_ms": sombra.get("elapsed_ms"),
                    "camadas": sombra.get("camadas"),
                    "camada2": sombra.get("camada2"),
                    "verdict": sombra.get("verdict"),
                }
            )


def medicao_valida(esperado: int, medidas: list, com_camada2: list, rotas: collections.Counter) -> bool:
    return (
        esperado > 0
        and len(medidas) == esperado
        and len(com_camada2) == esperado
        and rotas.get("daemon", 0) == 2 * esperado
        and sum(rotas.values()) == 2 * esperado
    )


def resumir(
    args: argparse.Namespace,
    session: str,
    registros: dict,
    ignorados: list[dict],
) -> dict:
    rotas = collections.Counter(
        r.get("route") or "ausente" for r in jsonl_da_session(args.trace, session)
    )
    medidas = [r for r in registros.values() if "elapsed_ms" in r]
    com_camada2 = [r for r in medidas if 2 in (r.get("camadas") or [])]
    tempos = [float(r["elapsed_ms"]) for r in com_camada2]
    wall = [float(r["wall_ms"]) for r in com_camada2]
    esperado = len(registros)
    valido = medicao_valida(esperado, medidas, com_camada2, rotas)
    return {
        "session": session,
        "corpus": str(args.corpus),
        "metodo": "duas chamadas consecutivas por entrada no shim; a 1ª aquece e a 2ª mede; elapsed_ms da sombra VGP (L1+L2), p95 por posto mais próximo; trace comprova a rota",
        "denominador": {
            "entradas_esperadas": esperado,
            "chamadas_hook_esperadas_com_aquecimento": 2 * esperado,
            "medidas_com_sombra": len(medidas),
            "medidas_com_camada2": len(com_camada2),
            "ignoradas_antes_da_chamada": len(ignorados),
            "rotas": dict(rotas),
        },
        "p50_motor_ms": percentil(tempos, 0.50),
        "p95_motor_ms": percentil(tempos, 0.95),
        "p95_wall_shim_ms": percentil(wall, 0.95),
        "timeouts_camada2": sum(
            str(r.get("camada2") or "").startswith("unknown: prazo de") for r in com_camada2
        ),
        "camada2_sem_veredito": sum(
            str(r.get("camada2") or "").startswith("unknown:") for r in com_camada2
        ),
        "limitacao": "o hook não aceita --overlay/--disk-file: mesmas entradas, paths, roots e conteúdos, mas sem replay dos overlays nem do disco histórico do juiz; o comando inline é reconstruído do snapshot integral",
        "valido": valido,
        "veredito": "OK" if valido else "INCONCLUSIVO",
        "ignorados": ignorados,
    }


def main() -> int:
    args = argumentos()
    entradas = carregar(args.corpus)
    if args.limite is not None:
        entradas = entradas[: args.limite]
    session = f"j14-hot-{os.getpid()}-{time.time_ns()}"
    registros, ignorados = medir_entradas(entradas, args.corpus, session, args.trace)
    enriquecer_com_sombra(registros, session)
    resumo = resumir(args, session, registros, ignorados)
    args.saida.write_text(json.dumps(resumo, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    args.por_id.write_text(
        json.dumps({"session": session, "por_id": registros}, ensure_ascii=False, indent=1) + "\n",
        encoding="utf-8",
    )
    print(json.dumps(resumo, ensure_ascii=False))
    return 0 if resumo["valido"] else 3


if __name__ == "__main__":
    raise SystemExit(main())
