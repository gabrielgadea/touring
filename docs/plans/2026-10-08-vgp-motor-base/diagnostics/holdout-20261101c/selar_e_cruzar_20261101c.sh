#!/usr/bin/env bash
# Validate, seal, and cross-check the 20261101c held-out without running a judge.
set -euo pipefail
V=/home/gabrielgadea/.claude/touring/vgp
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
D=$B/diagnostics/holdout-20261101c
C=$V/corpus_holdout_disjunto_20261101c
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
incompletas = meta["estado_completo"]["entradas_incompletas_no_corpus"]
out = {
    "arquivo": arquivo,
    "inline": inline,
    "sem_ts": len(sem_ts),
    "depois_do_corte": len(depois),
    "inconclusivo_reconstrucao_no_corpus": incompletas,
    "corte": meta["corte"],
    "ts_ausente_no_universo_arquivo": meta["arquivo"]["universo"].get("ts_ausente"),
    "overlays": meta["estado_no_instante"]["overlays"],
}
print(json.dumps(out, ensure_ascii=False, indent=1))
assert arquivo >= 300 and inline >= 300, out
assert not sem_ts and not depois and not incompletas, out
PY
python3 -I -B "$B/scripts/confere_selo.py" --selftest > "$D/confere_selo_selftest.log" 2>&1
python3 -I -B "$I/selar_holdout.py" "$C" "$D/selo.json" \
  --extra "adjudicador_sha256=$(sha256sum "$B/scripts/adjudica_rotulo.py" | cut -d' ' -f1)" \
  --extra "estado_no_instante_sha256=$(sha256sum "$B/scripts/estado_no_instante_v0d.py" | cut -d' ' -f1)" \
  --extra "congelados_json_sha256=$(sha256sum "$D/congelados.json" | cut -d' ' -f1)" \
  --extra "seed=20261103" \
  --extra "construtor_sha256=$(sha256sum "$I/construir_disjunto.py" | cut -d' ' -f1)" \
  --extra "regra_de_estado_completo=fecho transitivo de módulos locais pelo Git vigente no ts; src/python/packages; não reconstruível fora do denominador" \
  --extra "preregistro_dependencias_sha256=$(sha256sum "$B/PREREGISTRO-DEPENDENCIAS-LOCAIS-2026-11-01.md" | cut -d' ' -f1)" \
  --extra "correcao_dependencias_commit=ab256a5b" \
  --extra "gasto_anterior=corpus_holdout_disjunto_20261101 (J15)" > "$D/selar_20261101c.out"
python3 -I -B "$B/scripts/confere_selo.py" --corpus "$C" --selo "$D/selo.json" > "$D/confere_selo_antes.log" 2>&1
python3 -I -B "$I/verificar_sobreposicao.py" "$C" "$D/sobreposicao_20261101c.json" \
  --contra "$V/corpus_holdout_disjunto" \
  --contra "$V/corpus_holdout_disjunto_20261011" \
  --contra "$V/corpus_v0d.bak-20261009-pre" \
  --contra "$V/corpus_v0d_staging_evidencia" \
  --contra "$V/corpus_v0d.bak-2026-10-09" \
  --contra "$V/corpus_holdout_disjunto_20261013" \
  --contra "$V/corpus_holdout_disjunto_20261013f" \
  --contra "$V/corpus_holdout_disjunto_20261031" \
  --contra "$V/corpus_holdout_disjunto_20261031b" \
  --contra "$V/corpus_holdout_disjunto_20261101" > "$D/sobreposicao_20261101c.out"
python3 -I -c 'import json; d=json.load(open("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/holdout-20261101c/sobreposicao_20261101c.json")); assert d["OK_zero_em_todos_os_cruzamentos"] and d["controle_positivo_enxerga"], d; print("sobreposicao_zero_e_controle_positivo: ok")'
