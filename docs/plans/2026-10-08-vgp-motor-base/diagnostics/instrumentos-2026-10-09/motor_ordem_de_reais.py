"""A ordem de `findings[].reais` varia entre execucoes do MESMO binario? (os 7 ids que diferiram entre o juiz antigo e o novo)

Chama o motor N vezes por id, pela `rodar_motor` do juiz (a mesma chamada da medicao), e conta quantas ordens distintas de
`reais` — e quantos CONJUNTOS distintos — aparecem. Se a ordem muda no mesmo binario com o mesmo juiz, a diferenca entre as
duas rodadas e do motor, nao do juiz. Tambem confere que fora de `reais` o resto do veredito e identico em todas as chamadas.

Uso: python3 -I motor_ordem_de_reais.py <juiz.py> <binario> <corpus> <saida.json> <n> <id> [<id> ...]
"""

import importlib.util
import json
import sys
from pathlib import Path

JUIZ, BIN, CORPUS, SAIDA, N, *IDS = sys.argv[1:]
spec = importlib.util.spec_from_file_location("juiz_sob_teste", JUIZ)
juiz = importlib.util.module_from_spec(spec)
spec.loader.exec_module(juiz)
corpus = Path(CORPUS)
entradas = {e["id"]: e for e in juiz.carregar(corpus)[0] + juiz.carregar(corpus)[1]}


def sem_reais(saida: dict) -> str:
    achados = [{k: v for k, v in f.items() if k != "reais"} for f in saida.get("findings", [])]
    return json.dumps({"verdict": saida.get("verdict"), "findings": achados}, sort_keys=True, ensure_ascii=False)


r = {}
for i in IDS:
    saidas = [juiz.rodar_motor([BIN], entradas[i], corpus) for _ in range(int(N))]
    ordens = {json.dumps([f.get("reais") for f in s.get("findings", [])]) for s in saidas}
    conjuntos = {json.dumps([sorted(f.get("reais") or []) for f in s.get("findings", [])]) for s in saidas}
    r[i] = {"chamadas": len(saidas), "ordens_distintas_de_reais": len(ordens), "conjuntos_distintos_de_reais": len(conjuntos),
            "resto_do_veredito_identico": len({sem_reais(s) for s in saidas}) == 1}
SAIDA = Path(SAIDA)
SAIDA.write_text(json.dumps(r, ensure_ascii=False, indent=1), encoding="utf-8")
for i, v in r.items():
    print(i, v)
