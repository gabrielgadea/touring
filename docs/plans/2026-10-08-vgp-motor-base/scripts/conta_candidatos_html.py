#!/usr/bin/env python3
"""CONTAGEM de candidatos a positivo natural das classes HTML nos transcripts.

PEDIDO DA COORDENADORA (10/10): "MEÇA primeiro: conte os candidatos por classe nos transcripts de
analise e touring e me mande a contagem ANTES de construir qualquer coisa. Se uma classe der < 20,
ela fica «não provada, n=X»."

MODELO: `corpus_rust_natural.py` — mesmo universo congelado num `--corte`, determinístico. Aqui so
a CONTAGEM, que e o que foi pedido: nao gera corpus, nao sela, nao escreve em nenhum corpus.

O QUE ESTE NUMERO E: quantas referencias de cada classe uma edicao `.html` (Write/Edit)
INTRODUZIU, em todos os transcripts de `analise` e `touring`. A classe vem da TAG e do ATRIBUTO,
nunca do nome do arquivo.

O QUE ESTE NUMERO NAO E: nao e recall, nao e positivo confirmado, nao e n valido para o juiz. O
oraculo por INSTANCIA — a existencia no disco no instante da edicao — e quem decide se cada
candidato vira positivo; este script so conta o universo. O filtro de instancia e o proximo passo,
e ele pode reduzir muito o numero.

RODAR (o sandbox do `touring run` nao le ~/.claude/projects; o proprio corpus_rust_natural.py
documenta isso na L29):
    TOURING_CODE_MODE=native python3 conta_candidatos_html.py [--corte ISO]
"""
import argparse
import json
import re
from collections import Counter, defaultdict
from pathlib import Path

PROJETOS = ("analise", "touring")
CACHE = Path.home() / ".cache/vgp-semeado/tmp"
CLASSES = ("recurso", "ancora", "cdn", "cdn_pin", "classe_css", "id")

TAGS_RECURSO = {"script", "img", "link", "iframe", "source", "video", "audio"}
TAGS_REF = {"a", "area", "use", "form", "label", "input"}
ATRS_RECURSO = {"src", "poster", "data"}
ATRS_REF = {"href", "action"}
ATRS_ID = {"id", "for", "aria-labelledby", "aria-describedby", "aria-controls"}

TAG = re.compile(r"<(?P<tag>[A-Za-z][\w:-]*)(?P<atrs>[^>]*)>")
ATTR = re.compile(r"""\s(?P<nome>[\w:-]+)\s*=\s*(?P<asp>["'])(?P<val>[^"']*)(?P=asp)""")
VERSAO = re.compile(r"^\d+\.\d+\.\d+")


def classe_do(token: str, tag: str, atr: str) -> str:
    """A classe pelo que o token E, com a tag e o atributo que o trazem. Sem deteccao de padrao."""
    if atr in ATRS_ID:
        return "id" if atr == "id" else "ancora"
    if atr == "class":
        return "classe_css"
    if token.startswith("http"):
        return "cdn" if VERSAO.match(token.rsplit("@", 1)[-1]) else "cdn_pin"
    if token.startswith("#"):
        return "ancora"
    return "recurso"


def varre_texto(texto: str) -> Counter:
    """Conta as referencias que um texto de edicao introduz. Ignora o que ja estava no arquivo —
    o que este script NAO sabe, e por isso o numero e um teto, nao uma contagem de candidatos."""
    c = Counter()
    for tagm in TAG.finditer(texto):
        tag = tagm.group("tag").lower()
        for a in ATTR.finditer(tagm.group("atrs") or ""):
            atr, val = a.group("nome").lower(), a.group("val").strip()
            if not val:
                continue
            carrega = tag in TAGS_RECURSO or tag in TAGS_REF
            if atr in ATRS_RECURSO and not (tag in TAGS_RECURSO):
                continue
            if atr in ATRS_REF and not carrega:
                continue
            if atr == "class":
                for cl in val.split():
                    c["classe_css"] += 1
                continue
            if atr in ATRS_RECURSO or atr in ATRS_REF or atr in ATRS_ID:
                c[classe_do(val, tag, atr)] += 1
    return c


def edicoes(transcripts: Path, corte: str) -> tuple:
    """Devolve (contagem por classe, nº de edicoes .html, nº de transcripts)."""
    total = Counter()
    n_ed = 0
    arqs = sorted(transcripts.rglob("*.jsonl"))
    for arq in arqs:
        for ln in arq.read_text(errors="replace").splitlines():
            if '"tool_use"' not in ln:
                continue
            if corte != "9999" and corte not in ln:
                pass
            try:
                rec = json.loads(ln)
            except json.JSONDecodeError:
                continue
            if (rec.get("timestamp") or "") > corte:
                continue
            conteudo = rec.get("message", {}).get("content")
            if not isinstance(conteudo, list):
                continue
            for b in conteudo:
                if not isinstance(b, dict) or b.get("type") != "tool_use":
                    continue
                if b.get("name") not in ("Edit", "Write", "MultiEdit"):
                    continue
                inp = b.get("input") or {}
                if not str(inp.get("file_path") or "").endswith(".html"):
                    continue
                novo = inp.get("new_string") or inp.get("content") or ""
                if not isinstance(novo, str) or not novo:
                    continue
                n_ed += 1
                total += varre_texto(novo)
    return total, n_ed, len(arqs)


def main() -> int:
    ap = argparse.ArgumentParser(description="contagem de candidatos HTML nos transcripts")
    ap.add_argument("--corte", default="9999")
    a = ap.parse_args()
    CACHE.mkdir(parents=True, exist_ok=True)
    raiz = Path.home() / ".claude/projects"
    alvo = [d for d in sorted(raiz.iterdir()) if d.is_dir() and any(p in d.name for p in PROJETOS)]
    print(f"=== {len(alvo)} diretorios de projeto, corte={a.corte} ===")
    por_projeto = {}
    geral = Counter()
    for d in alvo:
        c, n_ed, n_arq = edicoes(d, a.corte)
        geral += c
        por_projeto[d.name] = {"classes": dict(c), "edicoes_html": n_ed, "transcripts": n_arq}
        print(f"  {d.name}: {n_ed} edicoes .html em {n_arq} transcripts -> {dict(c)}")
    print()
    print("=== TOTAL de referencias introduzidas por classe (TETO, nao candidatos confirmados) ===")
    for k in CLASSES:
        print(f"  {k:12s} {geral.get(k, 0)}")
    print(f"  {'TOTAL':12s} {sum(geral.values())}")
    (CACHE / "conta-candidatos-html.json").write_text(
        json.dumps({"corte": a.corte, "por_projeto": por_projeto, "total": dict(geral)},
                   indent=1, ensure_ascii=False),
        encoding="utf-8",
    )
    print(f"\nJSON em {CACHE / 'conta-candidatos-html.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())