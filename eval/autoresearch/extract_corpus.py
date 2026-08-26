#!/usr/bin/env python3
"""Extrai o corpus CONGELADO de chamadas Bash dos transcripts do Claude Code.

O verificador do research loop precisa ser determinístico e independente do
sistema avaliado. Um corpus histórico e imutável é a independência mais forte
disponível aqui — e é preciso dizer o que ela NÃO é: os comandos vieram do
próprio modelo cujo gate está sendo calibrado. O que o congelamento garante é
que a métrica não pode ser movida DURANTE a campanha; não que a distribuição
seja neutra.

Saída: JSON `{"sessions": [[cmd, ...], ...], "meta": {...}}`, uma lista de
comandos por sessão, na ordem em que foram emitidos.
"""
from __future__ import annotations

import argparse
import glob
import json
import os
import sys


def bash_commands(path: str) -> list[str]:
    """Comandos Bash de um transcript, em ordem."""
    out: list[str] = []
    with open(path, errors="ignore") as fh:
        for linha in fh:
            try:
                d = json.loads(linha)
            except ValueError:
                continue
            msg = d.get("message")
            if not isinstance(msg, dict):
                continue
            conteudo = msg.get("content")
            if not isinstance(conteudo, list):
                continue
            for c in conteudo:
                if (isinstance(c, dict) and c.get("type") == "tool_use"
                        and c.get("name") == "Bash"):
                    cmd = (c.get("input") or {}).get("command")
                    if isinstance(cmd, str) and cmd.strip():
                        out.append(cmd)
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--root", default=os.path.expanduser("~/.claude/projects"))
    ap.add_argument("--limit", type=int, default=40,
                    help="quantos transcripts mais recentes (0 = todos)")
    ap.add_argument("--min-cmds", type=int, default=5,
                    help="ignora sessões curtas demais para conter rajada")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    arquivos = sorted(glob.glob(os.path.join(args.root, "*", "*.jsonl")),
                      key=os.path.getmtime, reverse=True)
    if args.limit:
        arquivos = arquivos[: args.limit]

    sessoes, total = [], 0
    for f in arquivos:
        cmds = bash_commands(f)
        if len(cmds) >= args.min_cmds:
            sessoes.append(cmds)
            total += len(cmds)

    payload = {
        "sessions": sessoes,
        "meta": {
            "transcripts_lidos": len(arquivos),
            "sessoes_com_rajada": len(sessoes),
            "comandos": total,
            # O corpus é CONGELADO por construção: quem o consome não o escreve.
            "congelado_em": max((os.path.getmtime(f) for f in arquivos), default=0),
        },
    }
    with open(args.out, "w") as fh:
        json.dump(payload, fh)
    print(json.dumps(payload["meta"], indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
