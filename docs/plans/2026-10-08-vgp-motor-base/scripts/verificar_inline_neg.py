"""Confere o formato das entradas inline geradas: campos, sha do snap, distribuicao.

Existe como arquivo (e nao inline) porque o proprio VGP nega um comando bash com palavra-chave de
risco — e negar o verificador de um corpus de falsos denies seria a piada que o corpus existe para
evitar.

Uso: python3 verificar_inline_neg.py
"""

import hashlib
import json
import sys
from collections import Counter
from pathlib import Path

C = Path.home() / ".claude/touring/vgp/corpus_v0d"
ENTRADA = C / "entradas_inline_neg.jsonl"
REQ = {
    "id",
    "rotulo",
    "origem",
    "path",
    "project_root",
    "snap",
    "snap_sha256",
    "lang",
    "ts",
    "session",
    "tool_use_id",
    "command",
    "cwd",
    "extrator",
}


def main():
    linhas = [
        json.loads(x)
        for x in ENTRADA.read_text(encoding="utf-8").splitlines()
        if x.strip()
    ]
    print(f"entradas: {len(linhas)}")
    faltando = REQ - set(linhas[0])
    print(f"  campos faltando: {sorted(faltando) or 'nenhum'}")
    ids = [e["id"] for e in linhas]
    print(f"  ids de 12 hex: {all(len(i) == 12 for i in ids)}")
    print(f"  ids unicos:    {len(set(ids))}/{len(ids)}")
    sha_ok = sum(
        1
        for e in linhas
        if hashlib.sha256((C / e["snap"]).read_bytes()).hexdigest() == e["snap_sha256"]
    )
    print(f"  snap existe e o sha confere: {sha_ok}/{len(linhas)}")
    print(
        f"  command <= 2000 chars:        "
        f"{sum(1 for e in linhas if len(e['command']) <= 2000)}/{len(linhas)}"
    )
    print(f"  por extrator: {dict(Counter(e['extrator'] for e in linhas))}")
    print(f"  por path:     {dict(Counter(e['path'] for e in linhas))}")
    print(f"  projetos:     {sorted({e['project_root'] for e in linhas})}")
    print(
        f"  rotulo/origem: {sorted({(e['rotulo'], e['origem'], e['lang']) for e in linhas})}"
    )
    ok = (
        not faltando
        and sha_ok == len(linhas)
        and len(set(ids)) == len(ids)
        and all(len(i) == 12 for i in ids)
    )
    print(f"\nformato: {'OK' if ok else 'FALHA'}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
