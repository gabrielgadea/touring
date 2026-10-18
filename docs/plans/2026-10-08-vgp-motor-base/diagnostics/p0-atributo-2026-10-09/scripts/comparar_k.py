#!/usr/bin/env python3
"""Compara o JSON do juiz de um binario com a referencia `k_*.json` da coordenadora, chave a chave e POR ID.

uso: comparar_k.py <k_ref.json> <juiz.json> [<juiz2.json> ...]   (imprime um bloco por juiz)
Chaves de medida: pronto, pendencias, recall_simbolo, positivos_simbolo, recall_deny_informativo,
forma_fora_do_recall, negativos, avisos_em_negativos, chamadas, erros, falsos (ids), perdidos (ids).
p95_ms e so relatado (depende da maquina)."""
import json
import sys

CHAVES = [
    "pronto", "pendencias", "recall_simbolo", "positivos_simbolo", "recall_deny_informativo",
    "forma_fora_do_recall", "nao_classificado", "negativos", "avisos_em_negativos",
    "defeito_de_corpus", "chamadas", "erros",
]


def ids(lst):
    return sorted(x["id"] if isinstance(x, dict) else x for x in lst)


ref = json.load(open(sys.argv[1]))
for alvo in sys.argv[2:]:
    novo = json.load(open(alvo))
    print(f"== {alvo.split('/')[-2]}/{alvo.split('/')[-1]} contra {sys.argv[1].split('/')[-1]}")
    for k in CHAVES:
        a, b = ref.get(k), novo.get(k)
        print(f"  {'=' if a == b else 'DIFF'} {k}: ref={json.dumps(a)[:70]} novo={json.dumps(b)[:70]}" if a != b else f"  = {k}")
    for k in ("falsos", "perdidos"):
        a, b = ids(ref.get(k, [])), ids(novo.get(k, []))
        so_ref, so_novo = sorted(set(a) - set(b)), sorted(set(b) - set(a))
        print(f"  {'=' if a == b else 'DIFF'} {k}: ref={len(a)} novo={len(b)} so_na_ref={so_ref} so_no_novo={so_novo}")
    print(f"  p95_ms: ref={ref.get('p95_ms')} novo={novo.get('p95_ms')}")
