"""Migra o DAG do plano VGP do daemon per-project do ANALISE para o daemon GLOBAL (cwd ~/projects/touring).

Ordem de Gabriel (09/10/2026): o trabalho do VGP passa às sessões do ~/projects/touring. O DAG
`task_1791484992466538442` foi criado do cwd analise e vive no store dele — invisível ao touring (medido:
"task not found in this project's decompose store"). Este script: (1) lê a origem pelo env da sessão do analise;
(2) cria o DAG no global (cwd touring, SEM o TOURING_DAEMON_SOCKET herdado) e recria cada subtarefa com o mesmo id,
descrição, prioridade, dependências e status; (3) compara origem × destino por código e falha se divergir;
(4) acrescenta à ORIGEM uma subtarefa `MIGRADO` (completed) com o endereço novo — os status antigos ficam como
estavam, congelados, nunca "completados" à força. Toda leitura de campo exige o campo (REGRA ZERO).
Uso: python3 migrar_dag_para_touring.py [--dry-run]
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

ORIGEM = "task_1791484992466538442"
ANALISE = Path.home() / "projects/analise"
TOURING = Path.home() / "projects/touring"
BUNDLE = "~/projects/touring/docs/plans/2026-10-08-vgp-motor-base/"


def _env_global() -> dict[str, str]:
    env = dict(os.environ)
    for k in ("TOURING_DAEMON_SOCKET", "TOURING_DAEMON_SOCK"):
        env.pop(k, None)
    env["TOURING_PROJECT_ROOT"] = str(TOURING)
    return env


def touring(args: list[str], *, destino: bool) -> dict:
    """Roda `touring …` no daemon de origem (analise) ou destino (global) e devolve o JSON; rc≠0 levanta."""
    r = subprocess.run(
        ["touring", *args],
        cwd=TOURING if destino else ANALISE,
        env=_env_global() if destino else dict(os.environ),
        capture_output=True,
        text=True,
        timeout=120,
    )
    if r.returncode != 0:
        raise SystemExit(
            f"touring {' '.join(args)} rc {r.returncode}\nstdout: {r.stdout[-500:]}\nstderr: {r.stderr[-500:]}"
        )
    bruto = r.stdout[r.stdout.find("{") :]
    return json.loads(bruto)


def exigir(d: dict, campo: str):
    if campo not in d:
        raise SystemExit(f"campo {campo!r} ausente; existem: {sorted(d)}")
    return d[campo]


def local(sid: str) -> str:
    return sid.split("::")[-1]


def topologica(subs: list[dict]) -> list[dict]:
    por_id = {local(s["subtask_id"]): s for s in subs}
    feitas: list[str] = []
    while len(feitas) < len(por_id):
        prontas = [
            i
            for i, s in por_id.items()
            if i not in feitas and all(d in feitas for d in s["depends_on"])
        ]
        if not prontas:
            raise SystemExit("ciclo ou dependência inexistente na origem")
        feitas.extend(sorted(prontas))
    return [por_id[i] for i in feitas]


def retrato(d: dict) -> dict[str, tuple]:
    return {
        local(s["subtask_id"]): (
            s["status"],
            tuple(sorted(s["depends_on"])),
            s["description"],
            str(s["priority"]),
        )
        for s in exigir(d, "subtasks")
    }


def main() -> int:
    seco = "--dry-run" in sys.argv
    origem = touring(["decompose", "get", ORIGEM], destino=False)
    tarefa = exigir(origem, "task")
    subs = [
        s for s in exigir(origem, "subtasks") if local(s["subtask_id"]) != "MIGRADO"
    ]
    print(f"origem {ORIGEM}: {len(subs)} subtarefas · tipo {tarefa['task_type']}")
    if seco:
        for s in topologica(subs):
            print(
                f"  {local(s['subtask_id']):6} {s['status']:12} deps={s['depends_on']}"
            )
        return 0
    destino_id = criar_destino(tarefa, subs)
    conferir_destino(subs, destino_id)
    anotar_origem(destino_id)
    return 0


def criar_destino(tarefa: dict, subs: list[dict]) -> str:
    """Cria o DAG no global e recria cada subtarefa (id, prioridade, deps, descrição, status)."""
    desc = f"{tarefa['description']} [migrado de {ORIGEM} (daemon do analise) em 09/10/2026 por ordem do Gabriel; bundle {BUNDLE}]"
    novo = touring(
        [
            "decompose",
            "create",
            tarefa["task_type"],
            desc,
            "--priority",
            "high",
            "--origin",
            "migracao-analise",
        ],
        destino=True,
    )
    destino_id = exigir(novo, "task_id")
    print(f"destino criado: {destino_id}")
    for s in topologica(subs):
        sid = local(s["subtask_id"])
        args = ["decompose", "add", destino_id, sid, "--priority", str(s["priority"])]
        if s["depends_on"]:
            args.append("--depends-on=" + ",".join(s["depends_on"]))
        exigir(touring([*args, s["description"]], destino=True), "subtask_id")
        if s["status"] != "pending":
            r = touring(
                ["decompose", "update", destino_id, sid, "--status", s["status"]],
                destino=True,
            )
            if exigir(r, "subtask_updated") is not True:
                raise SystemExit(f"update de {sid} não aplicado: {r}")
    return destino_id


def conferir_destino(subs: list[dict], destino_id: str) -> None:
    """Origem × destino idênticos por (id, status, deps, descrição, prioridade) e DAG válido sem ciclos."""
    lido = touring(["decompose", "get", destino_id], destino=True)
    a, b = retrato({"subtasks": subs}), retrato(lido)
    difs = sorted(k for k in a.keys() | b.keys() if a.get(k) != b.get(k))
    if difs:
        raise SystemExit(
            f"origem × destino divergem em {difs}: {[(k, a.get(k), b.get(k)) for k in difs[:3]]}"
        )
    val = touring(["decompose", "validate", destino_id], destino=True)
    if exigir(val, "valid") is not True or exigir(val, "has_cycles") is not False:
        raise SystemExit(f"destino inválido: {val}")
    print(
        f"destino idêntico à origem: {len(b)} subtarefas (id, status, deps, descrição, prioridade) · válido, sem ciclos"
    )


def anotar_origem(destino_id: str) -> None:
    """Acrescenta MIGRADO (completed) à origem; os demais status ficam congelados, nunca completados à força."""
    nota = (
        f"DAG TRANSFERIDO para {destino_id} no daemon GLOBAL (cwd ~/projects/touring, sem TOURING_DAEMON_SOCKET) em "
        f"09/10/2026 por ordem do Gabriel; os status aqui congelaram nessa hora — a fonte da verdade é o DAG novo; "
        f"handoff: {BUNDLE}HANDOFF-touring-2026-10-09.md"
    )
    exigir(
        touring(
            ["decompose", "add", ORIGEM, "MIGRADO", "--priority", "high", nota],
            destino=False,
        ),
        "subtask_id",
    )
    r = touring(
        ["decompose", "update", ORIGEM, "MIGRADO", "--status", "completed"],
        destino=False,
    )
    if exigir(r, "subtask_updated") is not True:
        raise SystemExit(f"marca MIGRADO não aplicada: {r}")
    print(f"origem anotada com MIGRADO → {destino_id}")


if __name__ == "__main__":
    raise SystemExit(main())
