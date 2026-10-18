"""Imprime a tabela de 3 janelas e os controles, a partir do remedicao.json.

Uso: python3 ver_controles.py
"""

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent


def main():
    d = json.loads((HERE / "remedicao.json").read_text())
    print(f"medido_em: {d['medido_em']}   since={d['since']}")
    for g, lin in d["gates"].items():
        ferr = lin["ferramenta_exposta"]
        for j in ("A", "B", "C"):
            c = lin["janelas"][j]
            ic = c["ic95"]
            ics = f"[{ic[0]:.2f}, {ic[1]:.2f}]" if ic else "sem ic"
            rota = c["rota_seguida"]
            rotas = f"{rota}%" if rota is not None else "-"
            print(
                f"{g:<15} {j}  exp={c['exposicao']:>6}  neg={c['negacoes']:>5}  "
                f"taxa={str(c['taxa_por_100']):>14}  ic95={ics:<16} rota={rotas}"
            )
        print(f"{'':<15} ferramenta exposta: {ferr}")
    print(f"\nS3 avisos: {d['s3_avisos']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
