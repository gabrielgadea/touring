"""Descreve o holdout: n por balde, janela, concentracao por sessao, sobreposicao com o conjunto de AJUSTE (vivo) e
duplicatas de conteudo. Leitura apenas. Saida: holdout_descricao.json ao lado deste script.
"""

import json
from collections import Counter
from pathlib import Path

VGP = Path.home() / ".claude/touring/vgp"
VIVO, HOLD = VGP / "corpus_v0d", VGP / "corpus_holdout"
SAIDA = Path(__file__).resolve().parent / "holdout_descricao.json"


def out(*partes) -> None:
    print(" ".join(str(p) for p in partes))


def ler(arq: Path) -> list[dict]:
    return [json.loads(ln) for ln in arq.read_text(encoding="utf-8").splitlines() if ln.strip()]


def todos(base: Path) -> list[dict]:
    return ler(base / "entradas.jsonl") + ler(base / "entradas_inline_neg.jsonl")


def balde(e: dict) -> str:
    return "inline" if e["origem"] == "inline" else "arquivo"  # a regra do juiz (balde(e))


meta = json.loads((HOLD / "holdout_meta.json").read_text(encoding="utf-8"))
h, v = todos(HOLD), todos(VIVO)
neg_h = [e for e in h if e["rotulo"] == "negativo"]
pos_h = [e for e in h if e["rotulo"] == "positivo"]
r: dict = {"corte": meta["corte"], "ate": meta["ate"], "positivos": len(pos_h)}
r["n_por_balde"] = dict(Counter(balde(e) for e in neg_h))
r["n_por_origem"] = dict(Counter(e["origem"] for e in neg_h))
r["inline_por_estrato"] = dict(Counter(e["extrator"] for e in neg_h if e["origem"] == "inline"))
ts = sorted(e["ts"] for e in neg_h)
r["janela_efetiva"] = [ts[0], ts[-1]]
r["por_hora_utc"] = dict(sorted(Counter(t[:13] for t in ts).items()))

# concentracao: inline tem a sessao; o atribuido nao (por construcao do vivo) — usa o repositorio e o arquivo
sess = Counter(e["session"] for e in neg_h if e["origem"] == "inline")
n_in = sum(sess.values())
r["inline_sessoes_distintas"] = len(sess)
r["inline_top_sessoes"] = [(s[:8], n, round(100 * n / n_in, 1)) for s, n in sess.most_common(5)]
arq = [e for e in neg_h if e["origem"] == "atribuido"]
r["atribuido_arquivos_distintos"] = len({e["path"] for e in arq})
r["atribuido_por_projeto"] = dict(Counter(Path(e.get("project_root") or "?").name for e in arq))
r["atribuido_top_arquivos"] = Counter(Path(e["path"]).name for e in arq).most_common(5)

# sobreposicao com o conjunto de AJUSTE: o ARQUIVO ja foi visto (em outro instante)? o CONTEUDO foi visto?
vp = {e["path"] for e in v if e["origem"] == "atribuido"}
vsha = {e["snap_sha256"] for e in v}
r["atribuido_com_arquivo_ja_no_vivo"] = sum(1 for e in arq if e["path"] in vp)
r["conteudo_identico_a_snap_do_vivo"] = {"total": sum(1 for e in neg_h if e["snap_sha256"] in vsha),
                                           "por_balde": dict(Counter(balde(e) for e in neg_h if e["snap_sha256"] in vsha))}
vcmd = {e["command"] for e in v if e["origem"] == "inline"}
r["inline_comando_identico_ao_do_vivo"] = sum(1 for e in neg_h if e["origem"] == "inline" and e["command"] in vcmd)
r["duplicatas_internas_de_conteudo"] = sum(n - 1 for n in Counter(e["snap_sha256"] for e in neg_h).values() if n > 1)
r["tamanho_snap_mediano_chars"] = {b: sorted(len((HOLD / e["snap"]).read_text(errors="ignore")) for e in neg_h if balde(e) == b)[
    len([1 for e in neg_h if balde(e) == b]) // 2] for b in ("arquivo", "inline")}
vrel = json.loads((VIVO / "relatorio.json").read_text(encoding="utf-8"))
r["poda_vivo"] = vrel["negativos"].get("podas")
r["poda_holdout"] = meta["v0d"]["negativos"].get("podas")
r["universo_holdout"] = meta["v0d"]["negativos"]["universo"]
r["positivos_v0a"] = meta["positivos_v0a"]
SAIDA.write_text(json.dumps(r, ensure_ascii=False, indent=1), encoding="utf-8")
for k, val in r.items():
    out(f"{k}: {val}")
