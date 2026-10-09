#!/usr/bin/env python3
"""Guard estrutural: doc do sistema/repo/infra NUNCA cita crate que não existe.

Origem (26/08/2026, loop `2026-08-26-documentacao-touring`, fase F3): 63 de 93
arquivos de documentação (`ARCHITECTURE.md`, `crates/*/README.md`,
`crates/*/CLAUDE.md`, `crates/*/ARCHITECTURE*.md`, RFCs, CONSTITUTION) citavam
crates fundidos meses atrás — `touring-ast`, `touring-learning`, `touring-core`,
`touring-cognitive`, `touring-index`, `touring-wasm` e outros — como se ainda
existissem. Mais de 500 menções, e a correção NÃO foi mecânica: 3 dos 5
primeiros palpites por nome de diretório homônimo estavam errados (mapa
completo em `docs/plans/2026-08-26-documentacao-touring/f1-inventario.json`).

Este guard fecha a classe: doc corrigida hoje relata verdade hoje; sem ele, a
próxima fusão de crate reabre exatamente o mesmo drift, um crate por vez, até
alguém rodar outra auditoria manual em meses.

Estratégia: extrai `touring-<nome>` de cada arquivo no escopo, compara contra
`os.listdir('crates')` (ground truth executado, não uma lista hardcoded — uma
lista fixa envelheceria no primeiro crate novo). Um nome com nota de
proveniência ("era `touring-X`", "fundido", "verificado", data no formato
DD/MM) é tolerado — é exatamente assim que F2 documentou a história da fusão;
proibir isso destruiria a correção que este guard deveria proteger.

Uso:
    python3 scripts/test_docs_no_phantom_crates.py          # exit 1 se violado
    python3 scripts/test_docs_no_phantom_crates.py --json
    python3 -m pytest scripts/test_docs_no_phantom_crates.py -q
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import TypedDict

REPO = Path(__file__).resolve().parent.parent
CRATES_DIR = REPO / "crates"

# Escopo: sistema/repo/infra — nunca docs/plans, docs/audits, client/ (gerado),
# docs/internal/sessions (histórico), docs/indefinidos (arquivado).
SCOPE_GLOBS = [
    "ARCHITECTURE.md",
    "README.md",
    "CLAUDE.md",
    "ARCHITECTURE_PLAN.md",
    "crates/*/README.md",
    "crates/*/CLAUDE.md",
    "crates/*/.claude/CLAUDE.md",
    "crates/*/*ARCHITECTURE*.md",
    "docs/RFC-*.md",
    "docs/CONSTITUTION*.md",
    "docs/adr/*.md",
]

# Nomes que casam o padrão `touring-<palavra>` mas não são crates — binários,
# subagentes, threads, ferramentas externas ao crate graph, ou dependências
# técnicas (tantivy/capnp são bibliotecas de terceiro, "touring-X" aqui é o
# nome de uma FEATURE ou de um adapter, não de um crate do workspace).
NOISE = {
    "daemon", "hook", "cli", "quality", "mcp", "scriber", "engineer",
    "auditor", "architect", "scouter", "daemon-1000", "hook-runtime",
    "mcp-worker", "rayon", "project-actor", "pretool", "elite",
    "antt", "tantivy", "zip", "capnp",
}

# Extensões que, coladas ao nome capturado, denunciam "isto é um PATH de
# arquivo (doc, script, config), não uma referência a crate". Sem este filtro
# o guard confundia `docs/touring-system.md` ou `~/.claude/rules/
# touring-cli-index.md` com o crate `touring-system`/`touring-cli-index`.
FILE_EXTENSIONS = (".md", ".rs", ".sh", ".toml", ".json", ".py", ".yaml", ".yml")

# Marcadores de proveniência que o guard aceita como "documentado, não drift".
PROVENANCE_MARKERS = (
    "era ", "fundido", "verificado", "manutenção", "não existe",
    "peeled", "confirmado", "correção", "corrigido", "Reconciliado",
    "próprio Cargo.toml",
    # Equivalentes em inglês: os CLAUDE.md por crate (`crates/*/.claude/CLAUDE.md`)
    # são escritos em inglês e documentam a fusão como "(ex-touring-ast)" /
    # "fused" — proveniência legítima que o guard reprovava (08/10/2026).
    "(ex-", "fused", "merged into",
)


def real_crates() -> set[str]:
    if not CRATES_DIR.is_dir():
        return set()
    return {p.name for p in CRATES_DIR.iterdir() if p.is_dir()}


def scope_files() -> list[Path]:
    seen: set[Path] = set()
    for pattern in SCOPE_GLOBS:
        seen.update(REPO.glob(pattern))
    return sorted(seen)


class Offender(TypedDict):
    """Uma citação de crate inexistente sem nota de proveniência."""

    file: str
    line: int
    crate: str
    context: str


def _context(flat: str, m: re.Match[str]) -> str:
    return flat[max(0, m.start() - 320): m.end() + 40]


def _is_not_phantom(flat: str, m: re.Match[str], name: str, crates: set[str]) -> bool:
    """True quando o casamento `touring-<name>` NÃO é citação de crate fantasma.

    Passam: ruído conhecido, crate real, caminho de arquivo/diretório (por extensão,
    por barra final, ou por ser componente de caminho FORA de `crates/`) e menção com
    nota de proveniência por perto. Sob `crates/` o nome continua checado —
    `crates/touring-x` é exatamente o crate fantasma.
    """
    if not name or name in NOISE or f"touring-{name}" in crates:
        return True
    tail = flat[m.end(): m.end() + 6]
    if any(tail.startswith(ext) for ext in FILE_EXTENSIONS) or tail.startswith("/"):
        return True  # path de arquivo/diretório (doc/plans/script), não crate
    head = flat[max(0, m.start() - 7): m.start()]
    if head.endswith("/") and not head.endswith("crates/"):
        return True  # `scripts/touring-quality-score`, `/tmp/touring-run-*` (08/10/2026)
    return any(marker in _context(flat, m) for marker in PROVENANCE_MARKERS)


def offenders() -> list[Offender]:
    crates = real_crates()
    found: list[Offender] = []
    for path in scope_files():
        text = path.read_text(encoding="utf-8", errors="ignore")
        # Blockquotes markdown ("> " no início de cada linha continuada)
        # partem uma nota de proveniência em fragmentos que a janela de
        # contexto não via como uma frase só. Normalizado antes de procurar.
        flat = re.sub(r"\n>\s*", " ", text)
        for m in re.finditer(r"\btouring[-_]([a-z0-9][a-z0-9-]{2,})\b", flat):
            # Hífen final órfão: "touring-hooks-ARCHITECTURE.md" tem maiúsculas
            # (quebram [a-z0-9-]), então o regex para no hífen antes delas —
            # sem isso "touring-hooks-" nunca bate "touring-hooks" nos crates reais.
            name = m.group(1).replace("_", "-").rstrip("-")
            if _is_not_phantom(flat, m, name, crates):
                continue
            found.append(
                {
                    "file": str(path.relative_to(REPO)),
                    "line": flat[: m.start()].count("\n") + 1,
                    "crate": f"touring-{name}",
                    "context": _context(flat, m).strip()[-150:],
                }
            )
    return found


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    bad = offenders()
    if args.json:
        json.dump({"ok": not bad, "offenders": bad}, sys.stdout, indent=2, ensure_ascii=False)
        print()
    elif bad:
        print("VIOLAÇÃO — doc cita crate que não existe, sem nota de proveniência:")
        for entry in bad:
            print(f"  {entry['file']}:{entry['line']}  {entry['crate']}")
            print(f"    contexto: {entry['context'][:110]}")
        print(
            "\nOu (a) o crate existe e o nome real difere do citado — corrigir o "
            "nome; ou (b) o crate foi fundido e a menção precisa de uma nota de "
            "proveniência (\"era touring-X, fundido em <destino>, verificado "
            "<data>\") — ver docs/plans/2026-08-26-documentacao-touring/"
            "f1-inventario.json para o padrão."
        )
    else:
        print(f"OK — {len(scope_files())} arquivos do escopo sistema/repo/infra, nenhum crate fantasma sem nota")
    return 1 if bad else 0


# --------------------------------------------------------------------------
# Entrada pytest.
# --------------------------------------------------------------------------


def test_docs_cite_no_phantom_crates_without_provenance():
    bad = offenders()
    assert not bad, "docs citando crate inexistente sem nota: " + ", ".join(
        f"{e['file']}:{e['line']} ({e['crate']})" for e in bad
    )


def test_guard_detects_a_reintroduced_phantom_crate():
    """Prova por mutação: reintroduzir um crate fantasma SEM nota deve reprovar."""
    target = REPO / "ARCHITECTURE.md"
    original = target.read_text(encoding="utf-8")
    # Injeta uma menção nova, isolada, sem nenhum marcador de proveniência por perto.
    mutated = original + "\n\nEsta seção descreve o pipeline touring-ast puro.\n"
    assert mutated != original
    try:
        target.write_text(mutated, encoding="utf-8")
        bad = offenders()
        assert any(e["file"] == "ARCHITECTURE.md" for e in bad), (
            "o guard não detectou o crate fantasma reintroduzido"
        )
    finally:
        target.write_text(original, encoding="utf-8")
        assert target.read_text(encoding="utf-8") == original


def test_path_outside_crates_is_not_a_crate_but_crates_path_still_is():
    """Caminho fora de crates/ passa; o mesmo nome sob crates/ continua reprovando
    (a isenção não pode virar brecha para o crate fantasma) — 08/10/2026."""
    target = REPO / "ARCHITECTURE.md"
    original = target.read_text(encoding="utf-8")
    cases = {
        "touring-fantasmapath": ("\n\nRode `scripts/touring-fantasmapath` e veja `/tmp/touring-fantasmapath-*`.\n", False),
        "touring-fantasmacrate": ("\n\nO código vive em `crates/touring-fantasmacrate`.\n", True),
        "touring-fantasmaex": ("\n\nThe `ast` module (ex-touring-fantasmaex) parses sources.\n", False),
    }
    try:
        for crate, (snippet, must_flag) in cases.items():
            target.write_text(original + snippet, encoding="utf-8")
            flagged = any(e["file"] == "ARCHITECTURE.md" and e["crate"] == crate for e in offenders())
            assert flagged is must_flag, f"{crate}: esperado flagged={must_flag}, obtido {flagged}"
    finally:
        target.write_text(original, encoding="utf-8")
        assert target.read_text(encoding="utf-8") == original


if __name__ == "__main__":
    sys.exit(main())
