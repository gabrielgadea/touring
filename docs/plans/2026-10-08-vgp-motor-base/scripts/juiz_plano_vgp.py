#!/usr/bin/env python3
"""Juiz final J do plano VGP (proposta §6): cada fase pelo exit code do SEU juiz, nunca por leitura.

Escrito pela coordenadora analise-89 em 09/10/2026, ANTES das entregas (o aceite precede a execução): componente
ausente reprova (falha fechada). Sequencial de propósito — rodar juízes em paralelo fabrica contenção.

Saída: uma linha por checagem (fase · rc · última linha útil) e o agregado. Exit:
  0 = o plano inteiro passa · 3 = só a V4 pendente por TEMPO (5 sessões reais ainda não aconteceram) · 1 = falha.
Uso: python3 juiz_plano_vgp.py [--touring <bin>] [--so <fase>[,<fase>]] [--json <saida>]
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path

H = Path.home()
T, A = H / "projects/touring", H / "projects/analise"
VGP = T / "docs/plans/2026-10-08-vgp-motor-base"
S = VGP / "scripts"
DAG = "task_1791484992466538442"
FALHA_ACEITA = {"D1", "D2"}  # premissa refutada no próprio plano (proposta §8)
TEMPORAL = {"V4"}
REGRA_ANALISE = A / ".claude/rules/vgp-protocol.md"
PONTEIRO = VGP / "V6-ponteiro-vgp-protocol.md"
CLEAN = {
    k: v
    for k, v in os.environ.items()
    if k not in ("TOURING_DAEMON_SOCKET", "TOURING_DAEMON_SOCK")
}


@dataclass
class Checagem:
    fase: str
    nome: str
    rc: int
    linha: str


def correr(argv: list[str], cwd: Path, timeout: int = 1800) -> tuple[int, str]:
    """rc e saída combinada; ausência do executável ou prazo estourado = rc 127/124, nunca 0."""
    try:
        r = subprocess.run(
            argv, cwd=cwd, capture_output=True, text=True, timeout=timeout, env=CLEAN
        )
    except FileNotFoundError as e:
        return 127, f"ausente: {e}"
    except subprocess.TimeoutExpired:
        return 124, f"prazo de {timeout}s estourado"
    return r.returncode, (r.stdout + r.stderr).strip()


def ultima(saida: str) -> str:
    """A linha do veredito quando o juiz a imprime; senão a última (aviso no stderr não é veredito)."""
    linhas = [ln for ln in saida.splitlines() if ln.strip()]
    vereditos = [
        ln for ln in linhas if ln.lstrip().lower().startswith(("veredito", "selftest"))
    ]
    return (vereditos or linhas or ["(sem saída)"])[-1][:160]


def script(fase: str, nome: str, rel: str, *args: str, cwd: Path = S) -> Checagem:
    caminho = VGP / rel
    if not caminho.is_file():
        return Checagem(fase, nome, 127, f"ausente: {rel}")
    rc, out = correr([sys.executable, str(caminho), *args], cwd)
    return Checagem(fase, nome, rc, ultima(out))


def dag() -> Checagem:
    rc, out = correr(["touring", "decompose", "get", DAG, "-j"], A)
    if rc != 0:
        return Checagem("DAG", "subtarefas", rc, ultima(out))
    try:
        subs = json.loads(out[out.index("{") :])["subtasks"]
    except (ValueError, KeyError) as e:
        return Checagem("DAG", "subtarefas", 1, f"JSON ilegível: {e}")
    abertas = [
        s["subtask_id"].split("::")[-1]
        for s in subs
        if s.get("status") != "completed"
        and s["subtask_id"].split("::")[-1] not in FALHA_ACEITA
    ]
    fora_do_tempo = [x for x in abertas if x not in TEMPORAL]
    rc = 1 if fora_do_tempo else (3 if abertas else 0)
    return Checagem(
        "DAG",
        "subtarefas",
        rc,
        f"{len(subs)} subtarefas · abertas: {abertas or 'nenhuma'}",
    )


def v6_regra() -> Checagem:
    if not REGRA_ANALISE.is_file() or not PONTEIRO.is_file():
        return Checagem(
            "V6", "regra encolhida", 127, "regra do analise ou ponteiro ausente"
        )
    texto = REGRA_ANALISE.read_text()
    ok = (
        texto == PONTEIRO.read_text()
        and "touring vgp check" in texto
        and len(texto) < 2500
    )
    return Checagem(
        "V6",
        "regra encolhida",
        0 if ok else 1,
        f"{len(texto)} chars · igual ao ponteiro: {ok}",
    )


# Os controles nomeados da V0e5 são eventos datados: o selftest precisa do instante congelado da 1ª remedição.
ATE_V0E5 = "2026-10-09T17:44:00Z"
# {"desde": ISO-Z} gravado quando o modo ativo é ligado
ATIVACAO_V4 = VGP / "diagnostics/v4_ativacao.json"


def v4() -> Checagem:
    """A V4 só se mede depois de ligada; antes disso é pendência TEMPORAL declarada (rc 3), nunca "0 falsos"."""
    if not ATIVACAO_V4.is_file():
        return Checagem(
            "V4",
            "5 sessões reais",
            3,
            "deny ainda não ativado (sem diagnostics/v4_ativacao.json)",
        )
    desde = json.loads(ATIVACAO_V4.read_text())["desde"]
    agora = (
        datetime.now(UTC).strftime("%Y-%m-%dT%H:%M:%S.")
        + f"{datetime.now(UTC).microsecond // 1000:03d}Z"
    )
    return script(
        "V4",
        "5 sessões reais",
        "scripts/juiz_v4_sessoes.py",
        "--desde",
        desde,
        "--ate",
        agora,
    )


def cargo(fase: str, nome: str, *args: str) -> Checagem:
    rc, out = correr(["cargo", "test", "-p", "touring-lsp", *args], T, timeout=2400)
    resultado = [ln for ln in out.splitlines() if ln.startswith("test result")]
    return Checagem(fase, nome, rc, " | ".join(resultado)[-160:] or ultima(out))


def checagens(touring: str) -> list:
    """(fase, fábrica) em ordem; cada fábrica roda só quando a fase é pedida."""
    j2 = str(VGP / "diagnostics/juiz_v2.json")
    return [
        ("DAG", dag),
        ("V0c", lambda: script("V0c", "juiz_v0c", "scripts/juiz_v0c.py")),
        ("V0d", lambda: script("V0d", "juiz_v0d", "scripts/juiz_v0d.py")),
        (
            "V0e",
            lambda: script(
                "V0e", "régua (selftest)", "scripts/v0e_negacoes.py", "--selftest"
            ),
        ),
        (
            "V0e",
            lambda: script(
                "V0e",
                "destino das 606 (selftest)",
                "diagnostics/v0e-fora-de-gate/coleta.py",
                "--selftest",
            ),
        ),
        (
            "V0e",
            lambda: script(
                "V0e",
                "V0e5 remedição (selftest)",
                "diagnostics/v0e5-remedicao/remedicao.py",
                "--selftest",
                "--ate",
                ATE_V0E5,
            ),
        ),
        (
            "V2",
            lambda: script(
                "V2",
                "juiz do motor (camada 1)",
                "scripts/juiz_motor_vgp.py",
                "--touring",
                touring,
                "--json",
                j2,
            ),
        ),
        ("V3", lambda: cargo("V3", "touring-lsp (unit)")),
        ("V3", lambda: cargo("V3", "touring-lsp (pyright real)", "--", "--ignored")),
        (
            "V3",
            lambda: script(
                "V3",
                "juiz comparativo (camadas 1,2)",
                "scripts/juiz_motor_vgp.py",
                "--touring",
                touring,
                "--arg-motor=--camadas",
                "--arg-motor=1,2",
                "--p95-ms",
                "1000",
                "--base",
                j2,
            ),
        ),
        ("V4", v4),
        ("V6", v6_regra),
        (
            "V6",
            lambda: Checagem(
                "V6",
                "drift A1",
                *_rc_linha(
                    correr([sys.executable, "scripts/test_drift_coverage.py"], T)
                ),
            ),
        ),
        (
            "V6",
            lambda: Checagem(
                "V6",
                "drift A4",
                *_rc_linha(
                    correr(
                        [sys.executable, "scripts/test_docs_no_phantom_crates.py"], T
                    )
                ),
            ),
        ),
    ]


def _rc_linha(par: tuple[int, str]) -> tuple[int, str]:
    return par[0], ultima(par[1])


def veredito(cs: list[Checagem]) -> int:
    falhas = [
        c
        for c in cs
        if c.rc != 0 and not (c.fase in TEMPORAL or (c.fase == "DAG" and c.rc == 3))
    ]
    if falhas:
        return 1
    return 3 if any(c.rc != 0 for c in cs) else 0


def rotulo(rc: int, so: set[str]) -> str:
    """Um subconjunto verde nunca se anuncia como o plano: com --so, o veredito nomeia as fases julgadas."""
    escopo = f"fases {sorted(so)}" if so else "PLANO"
    estado = {0: "COMPLETO", 3: "só a V4 pendente (temporal)", 1: "NÃO PRONTO"}[rc]
    return f"veredito: {escopo} — {estado}"


def main() -> int:
    ap = argparse.ArgumentParser(description="Juiz final J do plano VGP")
    ap.add_argument("--touring", default=shutil.which("touring") or "touring")
    ap.add_argument(
        "--so", default="", help="fases separadas por vírgula (ex.: V0e,V6)"
    )
    ap.add_argument("--json", type=Path)
    a = ap.parse_args()
    so = {x.strip() for x in a.so.split(",") if x.strip()}
    cs = [fab() for fase, fab in checagens(a.touring) if not so or fase in so]
    for c in cs:
        print(
            f"  {'OK ' if c.rc == 0 else f'rc{c.rc}'}  {c.fase:4} {c.nome:34} {c.linha}"
        )
    rc = veredito(cs)
    print(rotulo(rc, so))
    if a.json:
        a.json.write_text(
            json.dumps(
                [vars(c) for c in cs] + [{"rc": rc}], ensure_ascii=False, indent=1
            )
        )
    return rc


if __name__ == "__main__":
    sys.exit(main())
