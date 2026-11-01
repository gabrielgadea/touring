"""Explica a diferenca entre a soma dos 9 gates da tabela e o total do instrumento.

Soma dos 9 gates != total porque a tabela cobre 9 gates e o instrumento classifica TODOS. A diferenca
e a contribuicao dos gates de fora da lista — nomeados aqui, para o dado fechar.

Uso: python3 explicar_regua.py
"""

import json
import sys
from collections import Counter
from datetime import datetime
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCRIPTS = HERE.parents[1] / "scripts"
assert (SCRIPTS / "inventario_erros.py").exists(), f"scripts errado: {SCRIPTS}"
sys.path.insert(0, str(SCRIPTS))
from inventario_erros import ROOTS, SINCE, _text  # noqa: E402
from v0e_negacoes import COMPILED, HOOK_DENIAL  # noqa: E402

# Os 9 gates da TABELA, pelo nome exato de GATES no v0e_negacoes. O S3 NAO esta aqui: o padrao amplo do
# G1 o credita ao balde do G1 (achado da V0e5), entao ele nao existe como chave separada no
# instrumento — a tabela o lista como linha de taxa derivada, nao como gate independente.
NA_TABELA = {
    "G1 rajada-de-inspeção",
    "G10 exec-burst",
    "G11 orçamento de bypass",
    "rm guard",
    "G7 re-inspeção (3ª leitura)",
    "G3 edit sem Read recente",
    "python_pre_gate (F82 etc.)",
    "write→run (script escrito-e-executado)",
}


def conta(ate: str) -> Counter:
    """Negacoes da janela A por gate, com o instrumento inteiro."""
    desde = datetime.fromisoformat(SINCE).timestamp()
    total = Counter()
    for root in ROOTS:
        for f in root.rglob("*.jsonl"):
            if f.stat().st_mtime < desde:
                continue
            for raw in f.open(encoding="utf-8", errors="replace"):
                if '"tool_result"' not in raw:
                    continue
                try:
                    rec = json.loads(raw)
                except json.JSONDecodeError:
                    continue
                ts = str(rec.get("timestamp", ""))
                if ts < SINCE or ts >= ate:
                    continue
                for c in (rec.get("message") or {}).get("content") or []:
                    if not (
                        isinstance(c, dict)
                        and c.get("type") == "tool_result"
                        and c.get("is_error")
                    ):
                        continue
                    t = _text(c)
                    if HOOK_DENIAL.search(t):
                        total[
                            next((nm for nm, p, r in COMPILED if p.search(t)), "?")
                        ] += 1
    return total


def main():
    d = json.loads((HERE / "remedicao.json").read_text())
    total = conta(d["janelas"]["A"][1])
    soma = sum(lin["janelas"]["A"]["negacoes"] for lin in d["gates"].values())
    print(f"total do instrumento na janela A: {sum(total.values())}")
    print(f"soma dos 9 gates da tabela:        {soma}")
    print(f"fora da tabela:                    {sum(total.values()) - soma}\n")
    for g, n in total.most_common():
        print(f"  {n:>5}  {g}{'' if g in NA_TABELA else '   <- fora da tabela'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
