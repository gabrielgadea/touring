#!/usr/bin/env python3
"""Roda o juiz OFICIAL (`scripts/juiz_motor_vgp.py::main`, as mesmas flags) e grava, de brinde, a saída do motor POR ID.

O juiz só agrega (taxas, listas de falsos e perdidos): para dizer QUAIS ids mudaram entre dois binários é preciso a saída
de cada entrada. Em vez de uma 2a passada (738 chamadas a mais por configuração), o gancho embrulha `rodar_motor` — o
`medir()` do juiz o chama pelo global do módulo —, de modo que o JSON do juiz e as linhas por id saem da MESMA execução.

    juiz_por_id.py --por-id <saida.json> [flags do juiz...]      # ex.: --touring BIN --corpus DIR --v5-estrito --json J
"""

import importlib.util
import json
import sys
from pathlib import Path

B = Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base")


def carregar_juiz():
    spec = importlib.util.spec_from_file_location("juiz_motor_vgp", B / "scripts/juiz_motor_vgp.py")
    juiz = importlib.util.module_from_spec(spec)
    sys.modules["juiz_motor_vgp"] = juiz
    spec.loader.exec_module(juiz)
    return juiz


def main(argv: list[str]) -> int:
    if "--por-id" not in argv:
        print("uso: juiz_por_id.py --por-id <saida.json> [flags do juiz]", file=sys.stderr)
        return 2
    i = argv.index("--por-id")
    destino = Path(argv[i + 1])
    resto = argv[:i] + argv[i + 2 :]
    juiz = carregar_juiz()
    linhas: dict[str, dict] = {}
    original = juiz.rodar_motor

    def gravando(touring, entrada, corpus):
        saida = original(touring, entrada, corpus)
        linhas[entrada["id"]] = {
            "path": entrada["path"],
            "project_root": entrada["project_root"],
            "origem": entrada.get("origem"),
            "rotulo": entrada["rotulo"],
            "lang": entrada.get("lang", "python"),
            "nome_esperado": entrada.get("nome_esperado"),
            "verdict": saida.get("verdict"),
            "erro": saida.get("_erro"),
            "camada2": saida.get("camada2"),
            "findings": [
                {k: f.get(k) for k in ("classe", "nome", "linha", "severidade")}
                for f in saida.get("findings", [])
            ],
        }
        return saida

    juiz.rodar_motor = gravando
    rc = juiz.main(resto)
    destino.write_text(json.dumps(linhas, ensure_ascii=False, sort_keys=True))
    print(f"[por-id] {len(linhas)} entradas gravadas em {destino}")
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
