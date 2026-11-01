#!/usr/bin/env bash
# Validate, seal, and cross-check the 20261101 held-out without running a judge.
set -euo pipefail
V=/home/gabrielgadea/.claude/touring/vgp
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
D=$B/diagnostics/holdout-20261101
C=$V/corpus_holdout_disjunto_20261101
I=$B/diagnostics/instrumentos-2026-10-09
python3 -I - "$C" > "$D/validacao_pre_selo.json" <<'PY'
import json
import sys
from pathlib import Path

corpus = Path(sys.argv[1])
meta = json.loads((corpus / "holdout_meta.json").read_text())
entradas = []
for nome in ("entradas.jsonl", "entradas_inline_neg.jsonl"):
    entradas += [json.loads(x) for x in (corpus / nome).read_text().splitlines() if x.strip()]
arquivo = len([e for e in entradas if e["path"] and not e["path"].startswith("<")])
inline = len(entradas) - arquivo
sem_ts = [e["id"] for e in entradas if not e.get("ts")]
depois = [e["id"] for e in entradas if e.get("ts") and e["ts"] > meta["corte"]]
out = {
    "arquivo": arquivo,
    "inline": inline,
    "sem_ts": len(sem_ts),
    "depois_do_corte": len(depois),
    "corte": meta["corte"],
    "ts_ausente_no_universo_arquivo": meta["arquivo"]["universo"].get("ts_ausente"),
}
print(json.dumps(out, ensure_ascii=False, indent=1))
assert arquivo >= 300 and inline >= 300, out
assert not sem_ts and not depois, out
PY
python3 -I -B "$B/scripts/confere_selo.py" --selftest > "$D/confere_selo_selftest.log" 2>&1
python3 -I -B "$I/selar_holdout.py" "$C" "$D/selo.json" \
  --extra "adjudicador_sha256=fabc36c9e470c3598c35d869ac26e96714b101f60dab963f74b0a9a9e76fca4c" \
  --extra "estado_no_instante_sha256=cc4516d0c008d3fad41659ed3d8a506417c19d0aa24f85454e3aad25e7d87891" \
  --extra "congelados_json_sha256=$(sha256sum "$D/congelados.json" | cut -d' ' -f1)" \
  --extra "seed=20261101" \
  --extra "construtor_sha256=$(sha256sum "$I/construir_disjunto.py" | cut -d' ' -f1)" \
  --extra "regra_de_estado_completo=fecho transitivo: todo modulo do projeto alcancavel pelos imports, em qualquer profundidade, no conteudo do instante" \
  --extra "gasto_anterior=corpus_holdout_disjunto_20261031b (J14)" \
  --extra "correcao_timestamps_commits=443564a3,1256b7f0" > "$D/selar_20261101.out"
python3 -I -B "$B/scripts/confere_selo.py" --corpus "$C" --selo "$D/selo.json" > "$D/confere_selo_antes.log" 2>&1
python3 -I -B "$I/verificar_sobreposicao.py" "$C" "$D/sobreposicao_20261101.json" \
  --contra "$V/corpus_holdout_disjunto" \
  --contra "$V/corpus_holdout_disjunto_20261011" \
  --contra "$V/corpus_v0d.bak-20261009-pre" \
  --contra "$V/corpus_v0d_staging_evidencia" \
  --contra "$V/corpus_v0d.bak-2026-10-09" \
  --contra "$V/corpus_holdout_disjunto_20261013" \
  --contra "$V/corpus_holdout_disjunto_20261013f" \
  --contra "$V/corpus_holdout_disjunto_20261031" \
  --contra "$V/corpus_holdout_disjunto_20261031b" > "$D/sobreposicao_20261101.out"
python3 -I -c 'import json; d=json.load(open("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/holdout-20261101/sobreposicao_20261101.json")); assert d["OK_zero_em_todos_os_cruzamentos"] and d["controle_positivo_enxerga"], d; print("sobreposicao_zero_e_controle_positivo: ok")'
