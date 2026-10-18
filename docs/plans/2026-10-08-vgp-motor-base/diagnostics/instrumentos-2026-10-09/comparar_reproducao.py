"""Prova de que `construir_disjunto.py` SEM as flags novas continua o mesmo: o corpus reconstruido (mesma semente, mesmos
parametros, saida em outro lugar) tem de coincidir com o corpus gravado em 09/10 — entradas, snaps e meta de selecao.

Compara: entradas.jsonl e entradas_inline_neg.jsonl (byte a byte), cada arquivo de snap/ e snap_inline_neg/ (sha), e os
campos de selecao do holdout_meta.json (parametros, exclusoes, selecao de arquivo e inline). Ignora so o que depende do
relogio/saida: `saida`, `wall_s`, `*_sha_tree_*`. Nao imprime conteudo, so contagens e ids de divergencia.

Uso: python3 -I comparar_reproducao.py <corpus_gravado> <corpus_reconstruido> <saida.json>
"""

import hashlib
import json
import sys
from pathlib import Path

GRAVADO, NOVO, SAIDA = (Path(a) for a in sys.argv[1:4])


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def arquivos(base: Path, sub: str) -> dict[str, str]:
    return {str(p.relative_to(base)): sha(p) for p in sorted((base / sub).rglob("*")) if p.is_file()}


r = {"entradas.jsonl_igual": sha(GRAVADO / "entradas.jsonl") == sha(NOVO / "entradas.jsonl"),
     "entradas_inline_neg.jsonl_igual": sha(GRAVADO / "entradas_inline_neg.jsonl") == sha(NOVO / "entradas_inline_neg.jsonl")}
for sub in ("snap", "snap_inline_neg"):
    a, b = arquivos(GRAVADO, sub), arquivos(NOVO, sub)
    r[sub] = {"arquivos_gravado": len(a), "arquivos_reconstruido": len(b), "so_no_gravado": len(set(a) - set(b)),
              "so_no_reconstruido": len(set(b) - set(a)), "conteudo_diferente": sorted(k for k in a if k in b and a[k] != b[k])[:20]}
ma = json.loads((GRAVADO / "holdout_meta.json").read_text(encoding="utf-8"))
mb = json.loads((NOVO / "holdout_meta.json").read_text(encoding="utf-8"))
campos = ("corte", "parametros", "exclusoes_tamanho")
r["meta"] = {c: ma.get(c) == mb.get(c) for c in campos}


def pool_sem_o_pos_corte(m: dict) -> dict:
    """O universo inline CRESCE com o tempo (sessoes novas): so o que e posterior ao corte muda. O resto tem de ser igual."""
    p = dict(m["pool_inline"])
    p.pop("universo_total")
    p["excluidos_por_chave"] = {k: v for k, v in p["excluidos_por_chave"].items() if k != "depois do corte"}
    return p


r["meta"]["pool_inline (fora do que e posterior ao corte)"] = pool_sem_o_pos_corte(ma) == pool_sem_o_pos_corte(mb)
cresceu_total = mb["pool_inline"]["universo_total"] - ma["pool_inline"]["universo_total"]
cresceu_pos_corte = mb["pool_inline"]["excluidos_por_chave"]["depois do corte"] - ma["pool_inline"]["excluidos_por_chave"]["depois do corte"]
r["universo_inline_cresceu"] = {"total": cresceu_total, "depois_do_corte": cresceu_pos_corte, "todo_o_crescimento_e_posterior_ao_corte": cresceu_total == cresceu_pos_corte}
r["meta"]["crescimento_so_posterior_ao_corte"] = cresceu_total == cresceu_pos_corte
r["meta"]["arquivo.selecao"] = ma["arquivo"]["selecao"] == mb["arquivo"]["selecao"]
r["meta"]["inline.selecao"] = ma["inline"]["selecao"] == mb["inline"]["selecao"]
r["meta_chaves_so_de_um_lado"] = sorted(set(ma) ^ set(mb))
r["IDENTICO"] = (r["entradas.jsonl_igual"] and r["entradas_inline_neg.jsonl_igual"] and all(v for v in r["meta"].values())
                 and all(r[s]["so_no_gravado"] == r[s]["so_no_reconstruido"] == 0 and not r[s]["conteudo_diferente"] for s in ("snap", "snap_inline_neg")))
SAIDA.write_text(json.dumps(r, ensure_ascii=False, indent=1), encoding="utf-8")
for k, v in r.items():
    print(f"{k}: {v}")
raise SystemExit(0 if r["IDENTICO"] else 1)
