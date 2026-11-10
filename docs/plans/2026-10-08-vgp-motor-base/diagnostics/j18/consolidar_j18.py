#!/usr/bin/env python3
"""Consolida o J18 pelo J15, ABBA v3a e as regras de regressão/efeito do pré-registro.

Entradas ausentes ou ilegíveis são INCONCLUSIVO. A primeira V3 de cada braço (`v3a`) decide;
`v3b` não entra no aceite. Um falso deny do candidato ausente no controle é NÃO PRONTO mesmo
quando o critério base passa. O efeito da correção é apenas evidência.
"""

import argparse
import importlib.util
import json
from pathlib import Path

_J15 = Path(__file__).resolve().parents[1] / "j15" / "consolidar_j15.py"
_spec = importlib.util.spec_from_file_location("consolidar_j15", _J15)
j15 = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(j15)

CANDIDATO = "30_4_108"
CONTROLE = "30_4_104"
RC = {"PRONTO": 0, "NÃO PRONTO": 1, "INCONCLUSIVO": 3}


def _ler(arquivo: Path) -> tuple[dict | None, str | None]:
    try:
        valor = json.loads(arquivo.read_text(encoding="utf-8"))
    except (OSError, ValueError) as erro:
        return None, f"{arquivo.name}: {type(erro).__name__}"
    return (valor, None) if isinstance(valor, dict) else (None, f"{arquivo.name}: não é um objeto JSON")


def _falsos(resumo: dict) -> dict[str, str]:
    """IDs já classificados como falsos deny pelo juiz, com sua mensagem."""
    out = {}
    for falso in resumo.get("falsos") or []:
        mensagem = "; ".join(
            str(a.get("sugestao") or a.get("nome") or "sem mensagem")
            for a in falso.get("achados") or []
        )
        out[falso["id"]] = mensagem or "sem mensagem"
    return out


def _diferenca(candidato: dict[str, str], controle: dict[str, str]) -> list[dict]:
    return [
        {"id": identidade, "mensagem": candidato[identidade]}
        for identidade in sorted(candidato.keys() - controle.keys())
    ]


def consolidar(diretorio: Path, candidato: str = CANDIDATO, controle: str = CONTROLE) -> dict:
    nomes = (
        f"{candidato}_v2.json",
        f"{candidato}_v3a.json",
        f"{controle}_v3a.json",
        "hot.json",
    )
    lidos = [_ler(diretorio / nome) for nome in nomes]
    ausentes = [motivo for _, motivo in lidos if motivo]
    if ausentes:
        return {
            "veredito": "INCONCLUSIVO",
            "pendencias": [],
            "inconclusivo_motivos": [f"entrada ausente: {m}" for m in ausentes],
        }
    v2, v3, controle_v3, hot = (valor for valor, _ in lidos)
    resultado = j15.avaliar(v2, v3, hot)
    falsos_candidato = _falsos(v3)
    falsos_controle = _falsos(controle_v3)
    regressao = _diferenca(falsos_candidato, falsos_controle)
    resultado["regressao_vs_controle"] = regressao
    resultado["efeito_da_correcao"] = _diferenca(falsos_controle, falsos_candidato)
    if regressao:
        resultado["veredito"] = "NÃO PRONTO"
        resultado["pendencias"] += [
            f"regressão vs controle: {item['id']} — {item['mensagem']}" for item in regressao
        ]
    return resultado


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("diretorio", type=Path)
    parser.add_argument("--candidato", default=CANDIDATO)
    parser.add_argument("--controle", default=CONTROLE)
    parser.add_argument("--saida", type=Path)
    args = parser.parse_args()
    resultado = consolidar(args.diretorio, args.candidato, args.controle)
    texto = json.dumps(resultado, ensure_ascii=False, indent=1) + "\n"
    if args.saida:
        args.saida.write_text(texto, encoding="utf-8")
    print(texto, end="")
    return RC[resultado["veredito"]]


if __name__ == "__main__":
    raise SystemExit(main())
