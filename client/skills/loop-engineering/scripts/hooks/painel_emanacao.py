#!/usr/bin/env python3
"""painel_emanacao — F0 dos Mundos da Criação: Atziluth, o Painel de Emanação.

Hook SessionStart: o primeiro turno de toda sessão abre com o estado do mundo
CLASSIFICADO — gravidade × urgência × tendência (GUT), dimensionamento e etapa
— antes de qualquer criação começar, com a opção explícita de ir direto.
Gate humano fechado por Gabriel em 30/08/2026 (F0+F4 primeiro).

Fontes v0 (cada uma com timeout próprio, fail-open — este hook JAMAIS
bloqueia uma sessão):
- ``RETOMAR*.md``  — ponteiros de retomada deliberados em docs/plans/*/
- ``touring kpi -j`` — checks FAIL (não-advisory) e STUB
- espelho cognitivo — ``cognicao_formal.padrao_data()`` (F4)

Fora da v0, documentado: DAGs do decompose (não há superfície de listagem
global com títulos — ``decompose status`` é só contagem) e o scout perpétuo
(já tem hook SessionStart próprio; duplicar seria ruído).

Kill switch humano: ``PAINEL_EMANACAO_DISABLED=1``.
Adesão medida: cada emissão grava ``kind:painel_emitido`` no journal do F4 —
o medidor existe desde o dia 1 (sem contador, o rito não é evidência de nada).
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import Any

SCRIPTS_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(SCRIPTS_DIR))

try:
    import cognicao_formal
except ImportError:  # ambiente degradado: o painel sai mais pobre, nunca cai
    cognicao_formal = None  # type: ignore[assignment]

TIMEOUT_COLETA = 4.0
MAX_ITENS = 8


# ---------------------------------------------------------------------------
# Coletas (cada uma devolve lista de itens {titulo, fonte, g, u, t, extra})
# ---------------------------------------------------------------------------


def coletar_retomar(cwd: Path) -> list[dict[str, Any]]:
    """Ponteiros de retomada: docs/plans/*/RETOMAR*.md, com título e idade."""
    itens: list[dict[str, Any]] = []
    for f in sorted(cwd.glob("docs/plans/*/RETOMAR*.md")):
        try:
            idade_dias = int((time.time() - f.stat().st_mtime) / 86_400)
            # Título: o primeiro heading real. Docs OKF abrem com frontmatter
            # (---) e comentários (<!-- -->) — a primeira linha não-vazia NÃO
            # é o título (o smoke da estreia mostrou "---" como título).
            titulo = f.parent.name
            for line in f.read_text(encoding="utf-8").splitlines()[:30]:
                line = line.strip()
                if line.startswith("#"):
                    titulo = line.lstrip("#").strip()[:70]
                    break
            itens.append({
                "titulo": f"RETOMAR: {titulo}",
                "fonte": str(f.relative_to(cwd)),
                # Um ponteiro deliberado é grave (4); urgência cresce com a
                # idade — quanto mais parado, mais urgente decidir se ainda vale.
                "g": 4,
                "u": min(5, 2 + idade_dias // 7),
                "t": 3,
                "extra": f"{idade_dias}d parado",
            })
        except OSError:
            continue
    return itens


def coletar_kpi() -> list[dict[str, Any]]:
    """Checks FAIL (não-advisory) e STUB do dashboard de commitments."""
    try:
        r = subprocess.run(
            ["touring", "kpi", "-j"], capture_output=True, text=True,
            timeout=TIMEOUT_COLETA, check=False,
        )
        data = json.loads(r.stdout)
    except (OSError, subprocess.SubprocessError, json.JSONDecodeError):
        return []
    itens: list[dict[str, Any]] = []
    for c in data.get("checks", []):
        status = c.get("status")
        if status == "FAIL" and not c.get("advisory"):
            itens.append({
                "titulo": f"KPI FAIL: {c.get('id')}",
                "fonte": "touring kpi",
                "g": 5, "u": 5, "t": 4,
                "extra": f"atual {c.get('actual')} vs piso {c.get('threshold')}",
            })
        elif status == "STUB":
            extra = c.get("stub_reason") or "sem dado"
            itens.append({
                "titulo": f"KPI STUB: {c.get('id')}",
                "fonte": "touring kpi",
                "g": 2, "u": 2, "t": 2,
                "extra": str(extra)[:70],
            })
    return itens


def coletar_espelho() -> dict[str, Any]:
    """O padrão acumulado do F4 (antecipação, ausências) — pode ser vazio."""
    if cognicao_formal is None:
        return {}
    try:
        return cognicao_formal.padrao_data()
    except Exception:  # noqa: BLE001 — espelho é enfeite; painel nunca cai
        return {}


# ---------------------------------------------------------------------------
# Render
# ---------------------------------------------------------------------------


def render(itens: list[dict[str, Any]], espelho: dict[str, Any]) -> str:
    """O Painel: top-N por GUT, sinais transparentes, opções explícitas."""
    linhas = ["🌅 ATZILUTH — Painel de Emanação (o estado do mundo, classificado)"]
    if itens:
        ordenados = sorted(itens, key=lambda i: -(i["g"] * i["u"] * i["t"]))
        linhas.append("Itens vivos, por GUT (gravidade×urgência×tendência):")
        for n, item in enumerate(ordenados[:MAX_ITENS], 1):
            gut = item["g"] * item["u"] * item["t"]
            linhas.append(
                f"  {n}. [{gut:>3}·G{item['g']}U{item['u']}T{item['t']}] "
                f"{item['titulo']} — {item['extra']} | {item['fonte']}"
            )
        if len(ordenados) > MAX_ITENS:
            linhas.append(f"  (+{len(ordenados) - MAX_ITENS} itens de menor GUT não listados)")
    else:
        linhas.append("Nada crítico vivo — campo limpo para criar.")
    if espelho.get("n"):
        partes = [f"n={espelho['n']} medições"]
        if "ratio_medio" in espelho:
            partes.append(f"antecipação média {espelho['ratio_medio']}")
            partes.append(f"últimas 5: {espelho['ratio_ultimos5']}")
        if espelho.get("mais_ausentes"):
            partes.append("mais ausentes: " + ", ".join(espelho["mais_ausentes"]))
        linhas.append("🪞 Espelho (F4): " + " · ".join(partes))
    linhas.append(
        "→ Diga \"retomar N\" para um item, descreva uma nova criação "
        "(o rito Briah será ofertado no tamanho certo), ou vá direto ao trabalho."
    )
    return "\n".join(linhas)


def main() -> int:
    """Entrada do hook SessionStart: stdin JSON → additionalContext; exit 0 sempre."""
    if os.environ.get("PAINEL_EMANACAO_DISABLED") == "1":
        return 0
    try:
        payload = json.load(sys.stdin)
    except (json.JSONDecodeError, OSError):
        payload = {}
    cwd = Path(payload.get("cwd") or os.getcwd())

    with ThreadPoolExecutor(max_workers=3) as pool:
        fut_retomar = pool.submit(coletar_retomar, cwd)
        fut_kpi = pool.submit(coletar_kpi)
        fut_espelho = pool.submit(coletar_espelho)
        itens: list[dict[str, Any]] = []
        for fut in (fut_retomar, fut_kpi):
            try:
                itens.extend(fut.result(timeout=TIMEOUT_COLETA + 1))
            except Exception:  # noqa: BLE001 — coleta quebrada ≠ painel quebrado
                continue
        try:
            espelho = fut_espelho.result(timeout=2)
        except Exception:  # noqa: BLE001
            espelho = {}

    texto = render(itens, espelho)
    if cognicao_formal is not None:
        cognicao_formal.journal_append({
            "kind": "painel_emitido",
            "ts": int(time.time()),
            "session": str(payload.get("session_id", ""))[:12],
            "n_itens": len(itens),
        })
    print(json.dumps({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": texto,
        }
    }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
