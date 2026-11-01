"""Metricas do VIES do fecho transitivo (HO13f): baldes, recusas por motivo antes/depois, tamanho do fecho e mix de projetos
contra o vivo e contra o 20261013. So leitura de metadados e do manifesto; nao abre o conteudo dos snapshots, nao roda motor
nem juiz.

Uso: python3 -I -B metricas_vies_ho13f.py <saida.json>
"""

import json
import sys
from collections import Counter
from pathlib import Path

SP = Path(__file__).resolve().parent
sys.path.insert(0, str(SP))
sys.path.insert(0, str(SP.parents[1] / "scripts"))
import estado_no_instante_v0d as eni  # noqa: E402
import exclusoes_disjunto as ex  # noqa: E402

V = ex.VGP
F, ANT, VIVO = V / "corpus_holdout_disjunto_20261013f", V / "corpus_holdout_disjunto_20261013", ex.VIVO
LIMITE_BALDE = 300


def projeto(e: dict) -> str:
    return Path(e.get("project_root") or "").name or "(sem project_root)"


def mix(entradas: list[dict]) -> dict[str, int]:
    return dict(Counter(projeto(e) for e in entradas).most_common())


def baldes(base: Path) -> dict:
    arq = ex.ler(base / "entradas.jsonl")
    inl = ex.ler(base / "entradas_inline_neg.jsonl")
    return {"arquivo": len(arq), "inline": len(inl), "inline_por_estrato": dict(Counter(e.get("extrator") or e.get("origem") for e in inl))}


def pct(a: dict, total: int) -> dict:
    return {k: round(100 * v / total, 1) for k, v in a.items()} if total else {}


meta = json.loads((F / "holdout_meta.json").read_text())
manifesto = json.loads((F / "estado_no_instante/manifesto.json").read_text())
f_arq, f_inl = ex.ler(F / "entradas.jsonl"), ex.ler(F / "entradas_inline_neg.jsonl")
a_arq, a_inl = ex.ler(ANT / "entradas.jsonl"), ex.ler(ANT / "entradas_inline_neg.jsonl")
v_neg_arq = [e for e in ex.ler(VIVO / "entradas.jsonl") if e["rotulo"] == "negativo"]
v_neg_inl = ex.ler(VIVO / "entradas_inline_neg.jsonl")
meta_ant = json.loads((ANT / "holdout_meta.json").read_text())

tam = [m["fecho"]["tamanho"] for m in manifesto.values()]
prof = Counter(m["fecho"]["profundidade_max"] for m in manifesto.values())
b = baldes(F)
chaves_f, chaves_a = ex.chaves(F), ex.chaves(ANT)
ids_f, ids_a = chaves_f["ids"], chaves_a["ids"]
r = {
    "baldes": {**b, "limite_para_selar": LIMITE_BALDE, "arquivo_ok": b["arquivo"] >= LIMITE_BALDE, "inline_ok": b["inline"] >= LIMITE_BALDE,
               "baldes_do_20261013": baldes(ANT)},
    "recusas_por_estado_incompleto": {
        "20261013_regra_de_profundidade_1": meta_ant["estado_completo"]["recusados_por_estado_incompleto"],
        "20261013f_fecho": meta["estado_completo"]["recusados_por_estado_incompleto"],
    },
    "vies_do_fecho_por_balde (mesma corrente de candidatos avaliados, regra de profundidade 1 x fecho)": meta["vies_do_fecho"],
    "tamanho_do_fecho_das_entradas_aceitas": eni.distribuicao(tam),
    "entradas_aceitas_por_profundidade_maxima": {str(k): v for k, v in sorted(prof.items())},
    "entradas_aceitas_com_fecho_vazio": sum(1 for t in tam if t == 0),
    "entradas_aceitas_com_modulo_de_profundidade_2_ou_mais": sum(1 for m in manifesto.values() if m["fecho"]["profundidade_max"] >= 2),
    "verificador_independente (tem de ser 0)": {"entradas_com_lacuna": sum(bool(m["nao_cobertas_prof2"]) for m in manifesto.values()),
                                                 "entradas_incompletas": sum(not m["completo"] for m in manifesto.values())},
    "mix_de_projetos": {
        "arquivo_20261013f": mix(f_arq), "arquivo_20261013": mix(a_arq), "arquivo_vivo_negativos": mix(v_neg_arq),
        "arquivo_20261013f_pct": pct(mix(f_arq), len(f_arq)), "arquivo_vivo_negativos_pct": pct(mix(v_neg_arq), len(v_neg_arq)),
        "inline_20261013f": mix(f_inl), "inline_20261013": mix(a_inl), "inline_vivo_negativos": mix(v_neg_inl),
    },
    "parentesco_com_o_20261013 (informativo; ele nunca foi aberto)": {
        "ids_em_comum": len(ids_f & ids_a), "ids_20261013f": len(ids_f), "ids_20261013": len(ids_a),
        "so_no_20261013": len(ids_a - ids_f), "so_no_20261013f": len(ids_f - ids_a),
        "fe5620ecee0a_no_20261013f": "fe5620ecee0a" in ids_f,
    },
    "arquivos_distintos_e_teto": {"20261013f": {"max_por_arquivo": meta["arquivo"]["selecao"]["max_por_arquivo"], "arquivos_distintos": meta["arquivo"]["selecao"]["arquivos_distintos"]},
                                   "20261013": {"max_por_arquivo": meta_ant["arquivo"]["selecao"]["max_por_arquivo"], "arquivos_distintos": meta_ant["arquivo"]["selecao"]["arquivos_distintos"]}},
    "selecao_inline": {est: {k: s[k] for k in ("quota", "aceitos", "pool", "faltou")} for est, s in meta["inline"]["selecao"].items()},
    "selecao_inline_20261013": {est: {k: s[k] for k in ("quota", "aceitos", "pool", "faltou")} for est, s in meta_ant["inline"]["selecao"].items()},
}
Path(sys.argv[1]).write_text(json.dumps(r, indent=1, ensure_ascii=False), encoding="utf-8")
print(json.dumps(r, indent=1, ensure_ascii=False))
