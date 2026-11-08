#!/usr/bin/env python3
"""Consolida o aceite J17 pelo critério V3-QUENTE sem reescrevê-lo.

`avaliar` é o do J15 (fonte única do critério pré-registrado: p95 frio informativo, p95 quente <= 1.000 ms, timeouts
> 20% ou sinal ausente = INCONCLUSIVO). Aqui só muda o NOME do candidato e a regra de que um arquivo de entrada ausente
ou ilegível é INCONCLUSIVO — sinal ausente nunca vira PRONTO nem zero.
Uso: python3 -I consolidar_j17.py <diretorio> [--candidato 30_4_104] [--saida aceite.json]
"""

import argparse
import importlib.util
import json
from pathlib import Path

_J15 = Path(__file__).resolve().parents[1] / "j15" / "consolidar_j15.py"
_spec = importlib.util.spec_from_file_location("consolidar_j15", _J15)
j15 = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(j15)

CANDIDATO = "30_4_104"
RC = {"PRONTO": 0, "NÃO PRONTO": 1, "INCONCLUSIVO": 3}


def _ler(arquivo: Path) -> tuple[dict | None, str | None]:
    """(conteúdo, motivo de ausência): arquivo faltando ou JSON inválido não é zero, é falta de sinal."""
    try:
        valor = json.loads(arquivo.read_text(encoding="utf-8"))
    except (OSError, ValueError) as erro:
        return None, f"{arquivo.name}: {type(erro).__name__}"
    return (valor, None) if isinstance(valor, dict) else (None, f"{arquivo.name}: não é um objeto JSON")


def consolidar(diretorio: Path, candidato: str = CANDIDATO) -> dict:
    nomes = (f"{candidato}_v2.json", f"{candidato}_v3.json", "hot.json")
    lidos = [_ler(diretorio / nome) for nome in nomes]
    ausentes = [motivo for _, motivo in lidos if motivo]
    if ausentes:
        return {"veredito": "INCONCLUSIVO", "pendencias": [], "inconclusivo_motivos": [f"entrada ausente: {m}" for m in ausentes]}
    v2, v3, hot = (valor for valor, _ in lidos)
    return j15.avaliar(v2, v3, hot)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("diretorio", type=Path)
    parser.add_argument("--candidato", default=CANDIDATO)
    parser.add_argument("--saida", type=Path)
    args = parser.parse_args()
    resultado = consolidar(args.diretorio, args.candidato)
    texto = json.dumps(resultado, ensure_ascii=False, indent=1) + "\n"
    if args.saida:
        args.saida.write_text(texto, encoding="utf-8")
    print(texto, end="")
    return RC[resultado["veredito"]]


if __name__ == "__main__":
    raise SystemExit(main())
