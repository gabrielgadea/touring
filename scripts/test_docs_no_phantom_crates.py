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


def offenders() -> list[dict[str, object]]:
    crates = real_crates()
    found: list[dict[str, object]] = []
    for path in scope_files():
        text = path.read_text(encoding="utf-8", errors="ignore")
        # Blockquotes markdown ("> " no início de cada linha continuada)
        # partem uma nota de proveniência em fragmentos que a janela de
        # contexto não via como uma frase só. Normalizado antes de procurar.
        flat = re.sub(r"\n>\s*", " ", text)
        for m in re.finditer(r"\btouring[-_]([a-z0-9][a-z0-9-]{2,})\b", flat):
            name = m.group(1).replace("_", "-").rstrip("-")
            # Hífen final órfão: "touring-hooks-ARCHITECTURE.md" tem maiúsculas
            # (quebram [a-z0-9-]), então o regex para no hífen antes delas —
            # sem isso "touring-hooks-" nunca bate "touring-hooks" nos crates reais.
            if not name or name in NOISE or f"touring-{name}" in crates:
                continue
            tail = flat[m.end(): m.end() + 6]
            if any(tail.startswith(ext) for ext in FILE_EXTENSIONS) or tail.startswith("/"):
                continue  # path de arquivo/diretório (doc/plans/script), não crate
            start = max(0, m.start() - 320)
            context = flat[start:m.end() + 40]
            if any(marker in context for marker in PROVENANCE_MARKERS):
                continue
            line = flat[: m.start()].count("\n") + 1
            found.append(
                {
                    "file": str(path.relative_to(REPO)),
                    "line": line,
                    "crate": f"touring-{name}",
                    "context": context.strip()[-150:],
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


if __name__ == "__main__":
    sys.exit(main())
