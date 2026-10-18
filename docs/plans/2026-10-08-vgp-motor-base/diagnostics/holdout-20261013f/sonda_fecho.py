"""Sonda SO-LEITURA do fecho transitivo sobre o corpus VIVO (nenhum held-out): tamanho, bytes, tempo, antes/depois.

Uso: python3 -I -B sonda_fecho.py <n_amostra> — amostra deterministica (passo fixo) das entradas do vivo.
Imprime UM json agregado (nada de conteudo)."""

import hashlib
import json
import sys
import time
from collections import Counter
from pathlib import Path

B = Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base")
sys.path.insert(0, str(B / "scripts"))
sys.path.insert(0, str(B / "diagnostics/instrumentos-2026-10-09"))
import construir_disjunto as cd  # noqa: E402
import corpus_v0d as c0  # noqa: E402
import estado_no_instante_v0d as eni  # noqa: E402

N = int(sys.argv[1])
vivo = Path.home() / ".claude/touring/vgp/corpus_v0d"
entradas = eni.carregar_entradas(vivo)
passo = max(1, len(entradas) // N)
amostra = entradas[::passo][:N]
memo = cd.memoizar_git()
t0 = time.monotonic()
indice = c0.varrer_transcripts()
t_indice = time.monotonic() - t0
tamanhos, bytes_total, unicos, prof_max = [], 0, {}, Counter()
antes, depois = Counter(), Counter()
t1 = time.monotonic()
for e in amostra:
    texto = (vivo / e["snap"]).read_text(errors="ignore")
    fecho = eni.fecho_transitivo(e, texto, indice)
    tamanhos.append(len(fecho))
    prof_max[max((m["profundidade"] for m in fecho), default=0)] += 1
    for m in fecho:
        if m["conteudo"] is not None:
            b = m["conteudo"].encode()
            bytes_total += len(b)
            unicos[hashlib.sha256(b).hexdigest()] = len(b)
    antes[eni.avaliar(fecho, 1)[1]] += 1
    depois[eni.avaliar(fecho)[1]] += 1
t_fecho = time.monotonic() - t1
print(json.dumps({
    "amostra": len(amostra), "universo_vivo": len(entradas), "t_indice_s": round(t_indice, 1), "t_fecho_s": round(t_fecho, 1),
    "s_por_entrada": round(t_fecho / max(1, len(amostra)), 3), "git_memo": memo,
    "tamanho": eni.distribuicao(tamanhos), "profundidade_maxima": dict(sorted(prof_max.items())),
    "bytes_total_overlays": bytes_total, "bytes_unicos_por_conteudo": sum(unicos.values()), "conteudos_unicos": len(unicos),
    "antes_regra_prof1": dict(antes), "depois_fecho": dict(depois),
}, ensure_ascii=False, indent=1))
