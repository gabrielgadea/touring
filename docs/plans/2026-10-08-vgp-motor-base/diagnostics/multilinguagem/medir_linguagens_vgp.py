"""Demanda do VGP por linguagem: edições (Edit/Write/MultiEdit) por extensão e erros de "nome inventado" por compilador.

Universo: todo transcript (com subagentes) de ~/.claude/projects/-home-gabrielgadea-projects-{analise,touring}; tool_use
deduplicado pelo id. Erros contados SÓ em tool_result de Bash (um Read que ecoa um erro antigo é menção, não evento), um
por (tool_use_id, código). Controle positivo: o universo e cada classe imprimem o total; universo vazio aborta.
Uso (nativo; o sandbox nega ~/.claude/projects): python3 medir_linguagens_vgp.py
"""

from __future__ import annotations

import collections
import json
import re
from pathlib import Path

PROJ = Path.home() / ".claude/projects"
PROJETOS = ("analise", "touring")
LANG = {
    ".py": "python", ".rs": "rust", ".ts": "ts", ".tsx": "ts", ".mts": "ts", ".js": "js", ".jsx": "js", ".mjs": "js",
    ".html": "html", ".htm": "html", ".css": "css",
}
ERROS = {
    "rust": re.compile(r"error\[(E0425|E0433|E0412|E0432|E0599|E0609|E0560|E0061|E0063|E0026)\]"),
    "ts": re.compile(r"error (TS2304|TS2339|TS2305|TS2307|TS2353|TS2561|TS2554|TS2551|TS2724|TS2552)"),
    "js-runtime": re.compile(r"(ReferenceError: \S+ is not defined|TypeError: \S+ is not a function)"),
    "python": re.compile(r"(AttributeError: .* has no attribute|NameError: name|ImportError: cannot import name)"),
}


def _blocos(rec: dict) -> list[dict]:
    c = (rec.get("message") or {}).get("content")
    return [b for b in c if isinstance(b, dict)] if isinstance(c, list) else []


def _texto_result(b: dict) -> str:
    c = b.get("content")
    if isinstance(c, str):
        return c
    if isinstance(c, list):
        return "\n".join(x.get("text", "") for x in c if isinstance(x, dict))
    return ""


def varrer(projeto: str) -> dict:
    raiz = PROJ / f"-home-gabrielgadea-projects-{projeto}"
    arquivos = sorted(raiz.rglob("*.jsonl"))
    if not arquivos:
        raise SystemExit(f"universo vazio em {raiz} (sandbox? rode nativo)")
    edits: collections.Counter = collections.Counter()
    tmp: collections.Counter = collections.Counter()
    distintos: dict[str, set] = collections.defaultdict(set)
    erros: dict[str, set] = collections.defaultdict(set)
    bash_ids: set[str] = set()
    vistos: set[str] = set()
    for f in arquivos:
        with f.open(errors="replace") as fh:
            for linha in fh:
                if '"tool_use"' not in linha and '"tool_result"' not in linha:
                    continue
                try:
                    rec = json.loads(linha)
                except json.JSONDecodeError:
                    continue
                for b in _blocos(rec):
                    if b.get("type") == "tool_use" and b.get("id") not in vistos:
                        vistos.add(b.get("id"))
                        nome, inp = b.get("name"), b.get("input") or {}
                        if nome == "Bash":
                            bash_ids.add(b.get("id"))
                        elif nome in ("Edit", "Write", "MultiEdit") and isinstance(inp.get("file_path"), str):
                            p = inp["file_path"]
                            lang = LANG.get(Path(p).suffix.lower(), "outro")
                            edits[lang] += 1
                            distintos[lang].add(p)
                            tmp[lang] += p.startswith("/tmp/")
                    elif b.get("type") == "tool_result" and b.get("tool_use_id") in bash_ids:
                        t = _texto_result(b)
                        for lang, rx in ERROS.items():
                            for m in rx.finditer(t):
                                erros[lang].add((b["tool_use_id"], m.group(1)[:60]))
    return {"transcripts": len(arquivos), "edits": edits, "tmp": tmp, "distintos": distintos, "erros": erros}


def main() -> int:
    for projeto in PROJETOS:
        r = varrer(projeto)
        total = sum(r["edits"].values())
        print(f"\n## {projeto}: {r['transcripts']} transcripts · {total} edições")
        for lang, n in r["edits"].most_common():
            print(f"  {lang:7} {n:6} ({n / total:6.1%}) · arquivos {len(r['distintos'][lang]):5} · em /tmp {r['tmp'][lang]:5}")
        for lang, s in r["erros"].items():
            cod = collections.Counter(c for _, c in s)
            top = ", ".join(f"{c.split(':')[0]} {n}" for c, n in cod.most_common(6))
            print(f"  erros {lang:10} {len(s):5} · {top}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
