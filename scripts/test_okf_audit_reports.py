#!/usr/bin/env python3
"""Guard: todo report de auditoria é um documento OKF — sem exceção.

Origem (30/08/2026): o cross-audit-2026-08-30.md foi escrito SEM frontmatter
OKF e Gabriel pegou; o censo achou outros 4 iguais (o mais antigo de junho).
A regra do loop-engineering — "every .md the loop writes is an OKF document" —
era declaração sem executor para docs/audits/ (o loop_doc_link_gate valida
bundles de docs/plans/, nunca este diretório). Este guard é o executor:
"certifique-se de não fazer mais isso" vira exit code, não promessa (D8).

Contrato mínimo exigido por report (o formato canônico dos audits recentes):
frontmatter YAML flat com `type:`, `title:`, `description:` e `timestamp:`.
"""

from __future__ import annotations

from pathlib import Path

import pytest

AUDITS_DIR = Path(__file__).resolve().parent.parent / "docs" / "audits"
CAMPOS_OBRIGATORIOS = ("type:", "title:", "description:", "timestamp:")


def _reports() -> list[Path]:
    return sorted(p for p in AUDITS_DIR.glob("*.md"))


def test_o_diretorio_de_audits_existe_e_tem_reports():
    # Guarda de não-degeneração: se o glob esvaziar (dir movido/renomeado),
    # os testes parametrizados virariam no-op em silêncio.
    assert AUDITS_DIR.is_dir(), f"docs/audits sumiu? {AUDITS_DIR}"
    assert len(_reports()) >= 5, "menos reports que o censo de 30/08 — glob degenerado?"


@pytest.mark.parametrize("report", _reports(), ids=lambda p: p.name)
def test_todo_report_de_auditoria_e_um_documento_okf(report: Path):
    head = report.read_text(encoding="utf-8", errors="ignore")[:2000]
    assert head.startswith("---\n"), (
        f"{report.name}: sem frontmatter OKF — todo .md de auditoria abre com "
        f"'---' (type/title/description/timestamp). A regra 'every .md the "
        f"loop writes is an OKF document' vale para docs/audits/ também."
    )
    fm = head.split("---", 2)[1] if head.count("---") >= 2 else ""
    faltando = [c for c in CAMPOS_OBRIGATORIOS if c not in fm]
    assert not faltando, (
        f"{report.name}: frontmatter OKF incompleto — faltam {faltando}"
    )


if __name__ == "__main__":
    import sys
    sys.exit(pytest.main([__file__, "-q"]))
