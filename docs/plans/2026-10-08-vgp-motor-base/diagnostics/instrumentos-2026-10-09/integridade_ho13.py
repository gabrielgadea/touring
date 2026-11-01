"""Integridade ao fim do HO13: o selo do 20261013 confere de novo, os arquivos CONGELADOS nao mudaram, os corpora
anteriores seguem intocados e nao ha motor/juiz no 20261013 (so leitura e hash — sem subprocesso).

Uso: python3 -I -B integridade_ho13.py <saida.json>
"""

import hashlib
import importlib.util
import json
import sys
from pathlib import Path

V = Path.home() / ".claude/touring/vgp"
B = Path(__file__).resolve().parents[2]
O = B / "diagnostics/holdout-20261013"
spec = importlib.util.spec_from_file_location("confere_selo", B / "scripts/confere_selo.py")
confere = importlib.util.module_from_spec(spec)
spec.loader.exec_module(confere)


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


selo = json.loads((O / "selo.json").read_text())
meta = json.loads((V / "corpus_holdout_disjunto_20261013/holdout_meta.json").read_text())
congelados = json.loads((O / "congelados.json").read_text())
r = {
    "selo_20261013": {"violacoes": confere.conferir(V / "corpus_holdout_disjunto_20261013", selo), "sha_tree": selo["sha_tree"]},
    "selo_20261011": {"violacoes": confere.conferir(V / "corpus_holdout_disjunto_20261011", json.loads((B / "diagnostics/holdout-20261011/selo.json").read_text()))},
    "congelados": {p: sha(Path(p)) == s for p, s in congelados["arquivos"].items()},
    "congelados_json_sha256_igual_ao_do_selo": sha(O / "congelados.json") == selo["congelados_json_sha256"],
    "arvores_intocadas_durante_o_build": {"vivo_e_temporal": meta["vivo_e_temporal_intocados"], "corpus_excluidos": meta["corpus_excluidos_alem_do_vivo_e_temporal"]},
    "sha_tree_agora": {n: confere.sha_tree(V / n) for n in ("corpus_v0d", "corpus_holdout", "corpus_holdout_disjunto", "corpus_holdout_disjunto_20261011", "corpus_holdout_disjunto_20261013")},
    "dependencias_sha256_agora": {n: sha(B / "scripts" / n) for n in ("corpus_v0d.py", "corpus_v5_inline_neg.py")},
    "dependencias_sha256_no_congelamento": congelados["dependencias_registradas_nao_exigidas"],
}
r["tudo_intacto"] = (
    not r["selo_20261013"]["violacoes"] and not r["selo_20261011"]["violacoes"] and all(r["congelados"].values())
    and r["congelados_json_sha256_igual_ao_do_selo"] and r["arvores_intocadas_durante_o_build"]["vivo_e_temporal"]
    and r["arvores_intocadas_durante_o_build"]["corpus_excluidos"]["antes"] == r["arvores_intocadas_durante_o_build"]["corpus_excluidos"]["depois"]
)
Path(sys.argv[1]).write_text(json.dumps(r, indent=1, ensure_ascii=False), encoding="utf-8")
print(json.dumps({k: v for k, v in r.items() if k not in ("congelados",)}, indent=1, ensure_ascii=False))
print("congelados:", {Path(p).name: ok for p, ok in r["congelados"].items()})
