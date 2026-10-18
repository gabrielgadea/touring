"""Tabela (d) COMPLETA: `comparar()` do medir_programa_inline com TODOS os motivos (o --comparar corta nos 5 maiores).

Por classe: total, extraídos (igual+diferente), compila(novo) e a taxa, e cada motivo de recusa. NATIVO.
Uso: cd ~/projects/analise && .venv/bin/python <este arquivo>  →  tabela_d.txt
"""

from __future__ import annotations

import sys
from pathlib import Path

SC = Path(__file__).parent
sys.path.insert(0, str(SC))
sys.path.insert(0, str(Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"))
from medir_programa_inline import comandos, comparar  # noqa: E402


def main() -> int:
    cmds = comandos()
    r = comparar(cmds)
    linhas = [f"universo: {len(cmds)} comandos Bash"]
    tot_total = tot_extr = tot_comp = 0
    for chave, cont in sorted(r["tab"].items(), key=lambda kv: -sum(kv[1].values())):
        total = sum(cont.values())
        extr = cont["igual"] + cont["diferente"] + cont["novo extrai"]
        comp = r["comp_novo"][chave]
        tot_total += total
        tot_extr += extr
        tot_comp += comp
        taxa = f"{100 * comp / extr:.1f}%" if extr else "—"
        linhas.append(f"\n{chave[0]:14} {chave[1][:34]:34} total {total:5} · extraídos {extr:5} · compila {comp:5} ({taxa})")
        for k, n in cont.most_common():
            linhas.append(f"      {n:5}  {k}")
    linhas.append(f"\nSOMA das classes: total {tot_total} · extraídos {tot_extr} · compila {tot_comp} ({100 * tot_comp / max(tot_extr, 1):.1f}%)")
    linhas.append(f"novo extrai onde o antigo não extraía: {r['so_novo']}")
    texto = "\n".join(linhas)
    (SC / "tabela_d.txt").write_text(texto)
    print(texto[-400:])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
