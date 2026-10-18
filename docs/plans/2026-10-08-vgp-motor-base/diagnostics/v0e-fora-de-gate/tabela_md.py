"""Emite a tabela markdown a partir do causas.json, para o relatorio nao divergir do dado.

Uso: python3 tabela_md.py
"""

import json
from pathlib import Path

HERE = Path(__file__).resolve().parent


def main():
    d = json.loads((HERE / "causas.json").read_text())
    out = ["| classe | n | % | destino |", "|---|---:|---:|---|"]
    for t in d["tabela"]:
        pc = t["n"] / d["total_itens"] * 100
        out.append(f"| {t['classe']} | {t['n']} | {pc:.1f}% | {t['destino']} |")
    print("\n".join(out))
    print("\n\n### evidencias (3 por classe, sem segredo)\n")
    for t in d["tabela"]:
        # `evidencias` e um DICT por classe (a funcao recebe a lista inteira e agrupa); nao uma lista.
        evs = (
            t["evidencias"].get(t["classe"], [])
            if isinstance(t["evidencias"], dict)
            else t["evidencias"]
        )
        print(f"**{t['classe']}** ({t['n']})")
        for e in evs:
            print(
                f"- `{e['session']}` / `{e['tool_use_id']}` — `{e['primeira_linha'][:150]}`"
            )
        print()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
