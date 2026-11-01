#!/usr/bin/env python3
"""Classificação assistida dos 111 sinais do delta de drift.

Para cada item, mede o que a disposição exige decidir:
  - paths distintos das ocorrências (doc vivo vs registro datado);
  - C4: quantas ocorrências estão em CONTEXTO DE COMANDO (code fence, backticks
    inline, linha iniciada por $/#) vs prosa solta — prosa é candidata a (b);
  - C6: a var existe no código-fonte do repo (.rs/.py/.sh/.toml)? Se sim e ela
    não está no env do scanner, é candidata a (b) (ground truth errado);
  - C1: o trecho é mesmo uma versão do projeto (vs número de seção/lei).

Saída: classificacao.json (sugestão por item, com evidência) + TABELA.md para
revisão humana. A decisão final é minha (sessão), não do script — ele mede.
"""
import json
import re
import sys
from collections import Counter
from pathlib import Path

BUNDLE = Path(__file__).resolve().parent
REPO = BUNDLE.parents[2]

RE_FENCE = re.compile(r"```[a-zA-Z]*\n(.*?)```", re.DOTALL)
RE_INLINE = re.compile(r"`[^`\n]+`")

# Registro datado legítimo (disposição c): histórico que NÃO se reescreve.
RE_PATH_DATADO = re.compile(
    r"^(docs/plans/\d{4}-\d{2}-\d{2}-|docs/plans-archive/|docs/audits/|docs/kpi/|"
    r"docs/sessions/|\.remember/|CHANGELOG|docs/indefinidos/)"
)


def _spans_protegidos(content: str) -> list[tuple[int, int]]:
    """Spans de code fence e código inline — contexto de comando/código."""
    spans = [(m.start(), m.end()) for m in RE_FENCE.finditer(content)]
    for m in RE_INLINE.finditer(RE_FENCE.sub(lambda f: " " * (f.end() - f.start()), content)):
        spans.append((m.start(), m.end()))
    return spans


def _em_contexto_codigo(content: str, trecho: str, spans) -> bool:
    """A ocorrência (localizada pelo trecho) está em fence/inline ou linha $/#?"""
    idx = content.find(trecho)
    if idx == -1:
        return False
    for s, e in spans:
        if s <= idx < e:
            return True
    # linha de shell: primeiro char não-espaço da linha é $ ou #
    start = content.rfind("\n", 0, idx) + 1
    line = content[start:idx + len(trecho)].lstrip()
    return line.startswith(("$", "#", "❯", ">"))


def vars_touring_no_codigo() -> set[str]:
    """Ground truth ESTÁTICO de env vars: TOURING_* referenciadas no código/docs de código."""
    found: set[str] = set()
    exts = {".rs", ".py", ".sh", ".toml", ".json"}
    for p in REPO.rglob("*"):
        if not p.is_file() or p.suffix not in exts:
            continue
        rel = p.relative_to(REPO).as_posix()
        if "/target/" in rel or "/node_modules/" in rel or rel.startswith("docs/plans/2026-10-08-"):
            continue
        try:
            found.update(re.findall(r"\bTOURING_[A-Z][A-Z0-9_]+\b", p.read_text(encoding="utf-8", errors="ignore")))
        except OSError:
            pass
    return found


def main() -> int:
    occ = json.loads((BUNDLE / "ocorrencias.json").read_text(encoding="utf-8"))
    gt_vars = vars_touring_no_codigo()
    print(f"ground truth estático: {len(gt_vars)} vars TOURING_* no código", file=sys.stderr)

    tabela = ["# Tabela de classificação — delta de drift 31/08→08/10",
              "", "(gerado por classificar.py — sugestão medida; decisão é da sessão)", ""]
    sugestoes: dict[str, dict] = {}
    stats = Counter()

    for cat in ("C1", "C3", "C4", "C5", "C6"):
        tabela.append(f"\n## {cat} ({len(occ[cat])} itens)\n")
        for item in sorted(occ[cat]):
            hits = occ[cat][item]
            paths = sorted({h["arquivo"] for h in hits})
            n_datado = sum(1 for p in paths if RE_PATH_DATADO.match(p))
            vivos = [p for p in paths if not RE_PATH_DATADO.match(p)]
            sugestao, evidencia = "c", f"{n_datado}/{len(paths)} paths datados"
            if cat == "C6":
                em_codigo = item in gt_vars
                if em_codigo:
                    sugestao, evidencia = "b", "var EXISTE no código-fonte; ausente só do env do scanner"
                elif vivos:
                    sugestao, evidencia = "a?", f"var inexistente no código, citada em doc vivo: {vivos[:2]}"
            elif cat == "C4":
                n_ctx = 0
                for h in hits:
                    content = (REPO / h["arquivo"]).read_text(encoding="utf-8", errors="ignore")
                    if _em_contexto_codigo(content, h["trecho"], _spans_protegidos(content)):
                        n_ctx += 1
                if n_ctx == 0:
                    sugestao, evidencia = "b", f"0/{len(hits)} ocorrências em contexto de comando (prosa)"
                elif vivos:
                    sugestao, evidencia = "a?", f"{n_ctx}/{len(hits)} em ctx comando; doc vivo: {vivos[:2]}"
                else:
                    sugestao, evidencia = "c", f"{n_ctx}/{len(hits)} em ctx comando; todos paths datados"
            elif vivos:
                sugestao, evidencia = "a?", f"doc vivo: {vivos[:3]}"
            sugestoes.setdefault(cat, {})[item] = {
                "sugestao": sugestao, "evidencia": evidencia,
                "n_ocorrencias": len(hits), "paths": paths,
            }
            stats[f"{cat}:{sugestao}"] += 1
            tabela.append(f"| `{item}` | {len(hits)} | {sugestao} | {evidencia} |")

    (BUNDLE / "classificacao.json").write_text(
        json.dumps(sugestoes, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    (BUNDLE / "TABELA.md").write_text("\n".join(tabela) + "\n", encoding="utf-8")
    print("sugestões:", dict(sorted(stats.items())))
    print(f"gravado: {BUNDLE / 'TABELA.md'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
