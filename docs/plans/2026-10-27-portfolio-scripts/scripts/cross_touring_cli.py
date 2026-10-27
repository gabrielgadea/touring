#!/usr/bin/env python3
"""Cruza o inventário de scripts/hooks com a superfície CLI do Touring.

Para cada .py de ~/.claude/scripts-portfolio e ~/.claude/skills/Touring/scripts/hooks:
  - propósito (1ª linha da docstring)
  - chamadas subprocess (grep/find/jq/rg/curl/sqlite3…) = candidatas a substituição por CLI
  - uso existente de touring (subprocess ["touring",...] ou import lib_touring)
  - reimplementações detectáveis (parser de JSONL de journal, leitura de sqlite de memory…)

Saída: UM JSON agregado (stdout) + a mesma coisa gravada em cross_analysis.json.
"""
from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path

ALVOS = [
    Path.home() / ".claude" / "scripts-portfolio",
    Path.home() / ".claude" / "skills" / "Touring" / "scripts" / "hooks",
]

# padrão de reimplementação → comando Touring que já faz
SUBSTITUICOES = {
    "grep": "touring tantivy search / index find",
    "rg": "touring tantivy search",
    "find": "touring index status / tantivy search",
    "jq": "touring <cmd> -j (saída já é JSON)",
    "sqlite3": "touring memory query / recall (memory.db é interno)",
    "curl": "fetch_proveniencia.py (portfólio) ou ctx fetch",
    "git log": "touring memory recall (histórico semântico)",
}
REIMPL = {
    "run_journal.jsonl": "touring kpi -j (code_mode_adherence/reuse já agregam o journal)",
    "memory.db": "touring memory query/recall — nunca abrir o SQLite direto",
    "symbols.db": "touring index find / ast meta — nunca abrir o SQLite direto",
}


def analisa(path: Path) -> dict:
    """Extrai propósito, subprocessos, uso de touring e oportunidades de um script."""
    src = path.read_text(encoding="utf-8", errors="ignore")
    doc = ""
    m = re.search(r'"""(.*?)"""', src, re.S)
    if m:
        doc = " ".join(m.group(1).split())[:140]
    procs = sorted(set(re.findall(r'\["(grep|rg|find|jq|sqlite3|curl|git)"', src)))
    procs += sorted(set(re.findall(r'\("(grep|rg|find|jq|sqlite3|curl)"', src)))
    usa_touring = bool(re.search(r'\["touring"|"touring",|lib_touring|touring-hook', src))
    reimpl = [k for k in REIMPL if k in src]
    oportunidades = [f"{p} → {SUBSTITUICOES[p]}" for p in set(procs) if p in SUBSTITUICOES]
    oportunidades += [f"{r} → {REIMPL[r]}" for r in reimpl]
    return {
        "arquivo": str(path).replace(str(Path.home()), "~"),
        "linhas": src.count("\n") + 1,
        "proposito": doc,
        "subprocessos": sorted(set(procs)),
        "usa_touring": usa_touring,
        "reimplementa": reimpl,
        "oportunidades": oportunidades,
    }


def superficie_cli() -> dict:
    """A superfície viva do CLI: subcomandos reais do binário instalado."""
    try:
        p = subprocess.run(["touring", "--help"], capture_output=True, text=True, timeout=15)  # noqa: S603
        linhas = [l.strip() for l in p.stdout.splitlines() if l.strip()]
    except (OSError, subprocess.TimeoutExpired):
        linhas = []
    masters = ["scout", "read", "map", "blast", "investigate", "guard", "audit",
               "explore", "portfolio", "adw", "factory", "run", "exec"]
    return {"help_linhas": len(linhas), "masters": masters}


def main() -> None:
    """Cruza os dois inventários e emite o agregado."""
    scripts = []
    for base in ALVOS:
        for f in sorted(base.glob("*.py")):
            if f.name.startswith("test_"):
                continue
            scripts.append(analisa(f))
    out = {
        "scripts_analisados": len(scripts),
        "usam_touring": sum(1 for s in scripts if s["usa_touring"]),
        "com_oportunidade": sum(1 for s in scripts if s["oportunidades"]),
        "cli": superficie_cli(),
        "detalhe": scripts,
    }
    destino = Path(__file__).resolve().parent / "cross_analysis.json"
    destino.write_text(json.dumps(out, ensure_ascii=False, indent=1))
    resumo = {k: v for k, v in out.items() if k != "detalhe"}
    resumo["oportunidades_por_script"] = {
        s["arquivo"].split("/")[-1]: s["oportunidades"]
        for s in scripts if s["oportunidades"]
    }
    print(json.dumps(resumo, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
