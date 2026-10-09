#!/usr/bin/env python3
"""Extrai ocorrências (arquivo/linha/trecho) de cada sinal do delta de drift.

Espelha os regexes de scripts/drift_semantic_scan.py (C1:L116, C4:L180,
C5:L205/211-216, C6:L234) — a paridade entre o conjunto extraído aqui e o
conjunto reportado pelo scanner é verificada item a item (controle: se meu
regex divergir do do scanner, este script falha, não o scanner).

Saída: ocorrencias.json = {cat: {item: [{arquivo, linha, trecho}]}} + relatório
de paridade em stdout (exit 1 se qualquer item do delta não for reencontrado).
"""
import importlib.util
import json
import re
import sys
from pathlib import Path

BUNDLE = Path(__file__).resolve().parent
REPO = BUNDLE.parents[2]
SCANNER_PATH = REPO / "scripts" / "drift_semantic_scan.py"

_spec = importlib.util.spec_from_file_location("drift_semantic_scan", SCANNER_PATH)
if _spec is None or _spec.loader is None:
    raise RuntimeError(f"não consegui carregar o scanner: {SCANNER_PATH}")
dss = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(dss)

# Espelhos dos padrões do scanner (ver cabeçalho para as linhas-fonte).
RE_C1 = re.compile(r"\bv?30\.(\d+)\.(\d+)\b")
RE_C4 = re.compile(r"\btouring ([a-z][a-z0-9_-]+)\b")
RE_FENCE = re.compile(r"```[a-zA-Z]*\n(.*?)```", re.DOTALL)
RE_C5_HIFEN = re.compile(r"\b(touring-[a-z][a-z0-9_-]+)\b")
RE_C5_UNDER = re.compile(r"\b(touring_[a-z][a-z0-9_]+)\b")
RE_C6 = re.compile(r"\bTOURING_[A-Z][A-Z0-9_]+\b")

Occur = dict[str, list]  # item -> [{arquivo, linha, trecho}]


def _trecho(content: str, pos: int) -> tuple[int, str]:
    """(linha 1-based, linha inteira truncada em 140 chars) da posição pos."""
    line_no = content.count("\n", 0, pos) + 1
    start = content.rfind("\n", 0, pos) + 1
    end = content.find("\n", pos)
    if end == -1:
        end = len(content)
    return line_no, content[start:end].strip()[:140]


def _scan_files() -> dict[str, str]:
    """Mesma coleta do scanner (função importada), retorna {rel: conteúdo}."""
    out = {}
    for rel in dss.collect_scope_files():
        try:
            out[rel] = (REPO / rel).read_text(encoding="utf-8", errors="ignore")
        except OSError:
            pass
    return out


def _extract_simple(contents: dict[str, str], pattern: re.Pattern,
                    want: set[str], key_of) -> Occur:
    """Extrator genérico: regex sobre o conteúdo inteiro, filtro pelo delta."""
    occ: Occur = {i: [] for i in want}
    for rel, content in contents.items():
        for m in pattern.finditer(content):
            item = key_of(m)
            if item in want:
                ln, tr = _trecho(content, m.start())
                occ[item].append({"arquivo": rel, "linha": ln, "trecho": tr})
    return occ


def _extract_c5(contents: dict[str, str], want: set[str]) -> Occur:
    """C5: nomes de crate só dentro de code fences (predicado do scanner L205)."""
    occ: Occur = {i: [] for i in want}
    for rel, content in contents.items():
        for block in RE_FENCE.findall(content):
            base = content.find(block)
            for m in RE_C5_HIFEN.finditer(block):
                if m.group(1) in want:
                    ln, tr = _trecho(content, base + m.start())
                    occ[m.group(1)].append({"arquivo": rel, "linha": ln, "trecho": tr})
            for m in RE_C5_UNDER.finditer(block):
                item = m.group(1).replace("_", "-")
                if item in want:
                    ln, tr = _trecho(content, base + m.start())
                    occ[item].append({"arquivo": rel, "linha": ln, "trecho": tr})
    return occ


def extract(delta: dict, contents: dict[str, str]) -> dict[str, Occur]:
    """Para cada item do delta, todas as ocorrências (mesmos predicados do scanner)."""
    return {
        "C1": _extract_simple(contents, RE_C1, set(delta["C1"]),
                              lambda m: f"30.{m.group(1)}.{m.group(2)}"),
        "C3": {i: [] for i in delta["C3"]},  # delta C3 vazio nesta reconciliação
        "C4": _extract_simple(contents, RE_C4, set(delta["C4"]), lambda m: m.group(1)),
        "C5": _extract_c5(contents, set(delta["C5"])),
        "C6": _extract_simple(contents, RE_C6, set(delta["C6"]), lambda m: m.group(0)),
    }


def main() -> int:
    delta = json.loads((BUNDLE / "delta.json").read_text(encoding="utf-8"))["delta"]
    occ = extract(delta, _scan_files())

    # PARIDADE: todo item do delta foi reencontrado em ≥1 arquivo; e o extrator
    # só registra itens ∈ delta por construção (filtro `want`), sem inventar.
    errors = []
    for cat, items in occ.items():
        for item, hits in items.items():
            if cat != "C3" and not hits:
                errors.append(f"{cat} {item}: 0 ocorrências reencontradas")

    total_hits = sum(len(h) for items in occ.values() for h in items.values())
    n_items = sum(len(items) for items in occ.values())
    print(f"itens do delta: {n_items} · ocorrências: {total_hits}")
    for cat in delta:
        n_it = len(occ[cat])
        n_hit = sum(len(h) for h in occ[cat].values())
        print(f"  {cat}: {n_it} itens · {n_hit} ocorrências")
    (BUNDLE / "ocorrencias.json").write_text(
        json.dumps(occ, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"gravado: {BUNDLE / 'ocorrencias.json'}")
    for e in errors:
        print(f"  FAIL {e}")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
