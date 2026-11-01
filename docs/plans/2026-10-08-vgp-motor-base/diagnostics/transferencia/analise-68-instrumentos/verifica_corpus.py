"""Verificador do corpus semeado: esquema do juiz + o classificador novo, item a item.

Nao e o juiz (que chama o motor): e a auditoria do CORPUS — existe snap, existe disk_file,
o id e unico, o positivo reproduz, e a mensagem classifica como `simbolo` sob o juiz novo.
"""

import json
import sys
from collections import Counter
from pathlib import Path

B = Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts")
sys.path.insert(0, str(B))
from juiz_motor_vgp import classe_erro  # noqa: E402

C = Path("/home/gabrielgadea/.claude/touring/vgp/corpus_semeado")
REQ = {
    "id", "rotulo", "reproduz", "classe", "nome_esperado", "origem", "lang",
    "path", "project_root", "snap", "mensagem", "contexto",
}

for lang in ("typescript", "tsx", "javascript", "html"):
    d = C / lang
    f = d / "entradas.jsonl"
    if not f.is_file():
        print(f"{lang}: SEM entradas.jsonl")
        continue
    ls = [json.loads(x) for x in f.read_text(encoding="utf-8").splitlines() if x.strip()]
    pos = [e for e in ls if e["rotulo"] == "positivo" and str(e.get("reproduz")).lower() == "true"]
    neg = [e for e in ls if e["rotulo"] == "negativo"]
    m = d / "estado_no_instante" / "manifesto.json"
    man = json.loads(m.read_text(encoding="utf-8")) if m.is_file() else {}
    faltando_campos = sorted(REQ - set(ls[0])) if ls else ["(vazio)"]
    sem_snap = [e["snap"] for e in ls if not (d / e["snap"]).is_file()]
    sem_disk = [i for i, v in man.items() if not (d / v["disk_file"]).is_file()]
    pos_sem_man = [e["id"] for e in pos if e["id"] not in man]
    print(f"\n### {lang}")
    print(f"  entradas={len(ls)} positivos={len(pos)} negativos={len(neg)} manifesto={len(man)}")
    print(f"  campos faltando: {faltando_campos or 'nenhum'}")
    print(f"  ids unicos: {len({e['id'] for e in ls}) == len(ls)}")
    print(f"  snap ausente: {len(sem_snap)} | disk_file ausente: {len(sem_disk)}"
          f" | positivo fora do manifesto: {len(pos_sem_man)}")
    print(f"  origem: {dict(Counter(e['origem'] for e in ls))}")
    print(f"  classe (positivos): {dict(Counter(e['classe'] for e in pos))}")
    print(f"  classificador: {dict(Counter(classe_erro(e['mensagem'], e['lang'], e['classe']) for e in pos))}")
    vistos = set()
    for e in pos:
        if e["classe"] in vistos:
            continue
        vistos.add(e["classe"])
        nome = e["nome_esperado"]
        print(f"    {e['classe']:<11} nome={nome!r:<26} msg={e['mensagem'][:86]!r}")