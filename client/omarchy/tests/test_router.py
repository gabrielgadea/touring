"""P6 — the memory router (`~/Work/CLAUDE.md` + one `AREA.md` per area).

Two contracts from the plan (S-6.1 / §3c):
  * every markdown link in the router resolves to a file under ~/Work — a
    router that points at a file that is not there sends every headless
    `claude -p` (timers, deck, ADW) down a dead end on its FIRST hop;
  * every AREA.md fits on one page (≤ 80 lines) — the ARMS L2 prompt is a
    router, not a manual; past one page the agent reads less, not more.

The logic is proven here against a fixture; the same checks run against the
real ~/Work on the Omarchy (`validate_p6.sh`), where the fixture becomes the
machine.
"""

from __future__ import annotations

import re
from pathlib import Path

import pytest

LINK = re.compile(r"\[[^\]]*\]\(([^)\s]+)\)")
MAX_AREA_LINES = 80


def router_links(router: Path) -> list[str]:
    """Relative markdown link targets in the router (external URLs ignored)."""
    text = router.read_text(encoding="utf-8")
    return [t for t in LINK.findall(text) if "://" not in t and not t.startswith("#")]


def unresolved_links(work: Path) -> list[str]:
    router = work / "CLAUDE.md"
    return [t for t in router_links(router) if not (work / t).exists()]


def oversized_areas(work: Path) -> dict[str, int]:
    out = {}
    for area in sorted(work.glob("*/AREA.md")):
        n = len(area.read_text(encoding="utf-8").splitlines())
        if n > MAX_AREA_LINES:
            out[area.parent.name] = n
    return out


def _fixture_work(tmp_path: Path) -> Path:
    work = tmp_path / "Work"
    for area in ("touring", "antt-detran", "conteudo", "pessoal"):
        (work / area).mkdir(parents=True)
        (work / area / "AREA.md").write_text(f"# {area}\n\n- onde: `{area}/`\n", encoding="utf-8")
    (work / "CLAUDE.md").write_text(
        "# Router\n\n"
        "| área | arquivo |\n|---|---|\n"
        "| touring | [AREA](touring/AREA.md) |\n"
        "| antt | [AREA](antt-detran/AREA.md) |\n"
        "| conteúdo | [AREA](conteudo/AREA.md) |\n"
        "| pessoal | [AREA](pessoal/AREA.md) |\n"
        "| docs | [plano](https://example.invalid/x) |\n",
        encoding="utf-8",
    )
    return work


def test_router_links_resolve(tmp_path: Path) -> None:
    work = _fixture_work(tmp_path)
    assert unresolved_links(work) == []
    assert max(len(p.read_text(encoding="utf-8").splitlines()) for p in work.glob("*/AREA.md")) <= MAX_AREA_LINES


def test_router_reports_a_dead_link(tmp_path: Path) -> None:
    work = _fixture_work(tmp_path)
    (work / "CLAUDE.md").write_text(
        (work / "CLAUDE.md").read_text(encoding="utf-8") + "| morto | [x](financas/AREA.md) |\n",
        encoding="utf-8",
    )
    assert unresolved_links(work) == ["financas/AREA.md"]


def test_area_over_one_page_is_reported(tmp_path: Path) -> None:
    work = _fixture_work(tmp_path)
    (work / "pessoal" / "AREA.md").write_text("\n".join(f"- linha {i}" for i in range(MAX_AREA_LINES + 1)), encoding="utf-8")
    assert oversized_areas(work) == {"pessoal": MAX_AREA_LINES + 1}


def test_real_work_router_when_present() -> None:
    """On the Omarchy (P6 done) this is the live check; on the Pop it skips."""
    work = Path.home() / "Work"
    if not (work / "CLAUDE.md").is_file():
        pytest.skip("~/Work/CLAUDE.md not present on this machine (P6 not executed here)")
    assert unresolved_links(work) == []
    assert oversized_areas(work) == {}
