"""Integridade ao fim do HO13f: o selo do 20261013f confere, os arquivos CONGELADOS nao mudaram desde o congelamento, o 20261013
(superado) e o 20261011 seguem intactos, os corpora anteriores nao foram tocados pelo build e as copias dos instrumentos do
20261013 batem com o registro dele. So leitura e hash — sem subprocesso, sem motor, sem juiz.

Uso: python3 -I -B integridade_ho13f.py <saida.json>
"""

import hashlib
import importlib.util
import json
import sys
from pathlib import Path

V = Path.home() / ".claude/touring/vgp"
B = Path(__file__).resolve().parents[2]
F = B / "diagnostics/holdout-20261013f"
O13 = B / "diagnostics/holdout-20261013"
spec = importlib.util.spec_from_file_location("confere_selo", B / "scripts/confere_selo.py")
confere = importlib.util.module_from_spec(spec)
spec.loader.exec_module(confere)


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


selo = json.loads((F / "selo.json").read_text())
meta = json.loads((V / "corpus_holdout_disjunto_20261013f/holdout_meta.json").read_text())
congelados = json.loads((F / "congelados.json").read_text())
antigo = json.loads((O13 / "congelados.json").read_text())
copias = json.loads((F / "instrumentos_do_20261013/ARQUIVOS.json").read_text())
r = {
    "selo_20261013f": {"violacoes": confere.conferir(V / "corpus_holdout_disjunto_20261013f", selo), "sha_tree": selo["sha_tree"]},
    "selo_20261013_superado": {"violacoes": confere.conferir(V / "corpus_holdout_disjunto_20261013", json.loads((O13 / "selo.json").read_text()))},
    "selo_20261011": {"violacoes": confere.conferir(V / "corpus_holdout_disjunto_20261011", json.loads((B / "diagnostics/holdout-20261011/selo.json").read_text()))},
    "congelados_f": {p: sha(Path(p)) == s for p, s in congelados["arquivos"].items()},
    "congelados_json_sha256_igual_ao_do_selo": sha(F / "congelados.json") == selo["congelados_json_sha256"],
    "adjudicador_com_o_mesmo_sha_do_20261013": congelados["arquivos"][str(B / "scripts/adjudica_rotulo.py")] == antigo["arquivos"][str(B / "scripts/adjudica_rotulo.py")],
    "copias_dos_instrumentos_do_20261013_batem_com_o_registro_antigo": {
        n: (sha(F / "instrumentos_do_20261013" / n) == antigo["arquivos"][c["origem"]]) for n, c in copias["copias"].items()
    },
    "arvores_intocadas_durante_o_build": {"vivo_e_temporal": meta["vivo_e_temporal_intocados"], "corpus_excluidos": meta["corpus_excluidos_alem_do_vivo_e_temporal"]},
    "sha_tree_agora": {n: confere.sha_tree(V / n) for n in ("corpus_v0d", "corpus_holdout", "corpus_holdout_disjunto", "corpus_holdout_disjunto_20261011",
                                                              "corpus_holdout_disjunto_20261013", "corpus_holdout_disjunto_20261013f", "corpus_v0d.bak-20261009-pre",
                                                              "corpus_v0d_staging_evidencia", "corpus_v0d.bak-2026-10-09")},
    "dependencias_sha256_agora": {n: sha(B / "scripts" / n) for n in ("corpus_v0d.py", "corpus_v5_inline_neg.py")},
    "dependencias_sha256_no_congelamento": congelados["dependencias_registradas_nao_exigidas"],
}
r["tudo_intacto"] = (
    not r["selo_20261013f"]["violacoes"] and not r["selo_20261013_superado"]["violacoes"] and not r["selo_20261011"]["violacoes"]
    and all(r["congelados_f"].values()) and r["congelados_json_sha256_igual_ao_do_selo"] and r["adjudicador_com_o_mesmo_sha_do_20261013"]
    and all(r["copias_dos_instrumentos_do_20261013_batem_com_o_registro_antigo"].values())
    and r["arvores_intocadas_durante_o_build"]["vivo_e_temporal"]
    and r["arvores_intocadas_durante_o_build"]["corpus_excluidos"]["antes"] == r["arvores_intocadas_durante_o_build"]["corpus_excluidos"]["depois"]
)
Path(sys.argv[1]).write_text(json.dumps(r, indent=1, ensure_ascii=False), encoding="utf-8")
print(json.dumps({k: v for k, v in r.items() if k not in ("congelados_f", "copias_dos_instrumentos_do_20261013_batem_com_o_registro_antigo")}, indent=1, ensure_ascii=False))
print("congelados_f:", {Path(p).name: ok for p, ok in r["congelados_f"].items()})
print("copias_do_20261013:", r["copias_dos_instrumentos_do_20261013_batem_com_o_registro_antigo"])
