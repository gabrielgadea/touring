#!/usr/bin/env bash
# Sela a camada OSS nova e a confere com o validador ATUAL — um comando, saida na tela.
#
# O selo mora FORA do corpus (o `sha_tree` incluiria o proprio selo, e o `confere_selo.py`
# reprova um selo DENTRO do corpus). A disjuncao e medida contra o historico INTEIRO de
# corpora, com a propria camada de fora da comparacao.
#
# A classe `html` entra com `--classe-sem-adjudicador`: o adjudicador desta camada e o do
# TypeScript, e um arquivo html nao produz diagnostico — o rotulo de "negativo" sairia vazio
# de conteudo, e um negativo vazio nao mede falso deny (item D-8: o html tem adjudicador
# proprio).
set -uo pipefail

BUNDLE=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
VG=$HOME/.claude/touring/vgp
CORPUS="$VG/oss_rotulado_20261101"
SELO="$VG/oss_rotulado_20261101.selo.json"
JSON="$VG/oss_rotulado_20261101.denominadores.json"

export TMPDIR="$HOME/.cache/vgp-semeado/tmp"
mkdir -p "$TMPDIR"

cd "$BUNDLE" || exit 2

[ -d "$CORPUS" ] || { echo "corpus inexistente: $CORPUS" >&2; exit 2; }

python3 -I scripts/selar_camada_oss.py \
  --corpus "$CORPUS" --selo "$SELO" --json "$JSON" \
  --alvo 300 --classe-sem-adjudicador html \
  --verificar-disjuncao-de "$VG"
rc_selo=$?

echo "--- confere_selo.py (o validador atual) ---"
python3 -I scripts/confere_selo.py --corpus "$CORPUS" --selo "$SELO"
rc_confere=$?

echo "--- o veredito por classe (para o RELATORIO) ---"
python3 -I - "$SELO" <<'PYEOF'
import json, sys
selo = json.load(open(sys.argv[1]))
print("| classe | n | negativos | taxa | teto por chave | dentro do teto | situacao |")
print("|---|---:|---:|---:|---:|---|---|")
for lang, c in sorted(selo["classes"].items()):
    print(
        "| `%s` | %d | %d | %.0f %% | %d | %s | **%s** |"
        % (
            lang,
            c["n"],
            c["n_negativos"],
            100 * c["n_negativos"] / max(1, c["n"]),
            c["teto_por_chave"],
            "sim" if c["dentro_do_teto"] else "NAO (%d infrações)" % c["n_infratores"],
            c["situacao"],
        )
    )
d = selo["disjuncao"]
print()
print("disjunção:", "DISJUNTA" if d["disjunta"] else "%d ids em comum" % d["n_ids_em_comum"],
      "| ids da camada:", d["n_ids_da_camada"])
print("sha_tree do selo:", selo["sha_tree"], "| arquivos:", selo["arquivos"], "| bytes:", selo["bytes"])
print("classes sem adjudicador:", selo.get("classes_sem_adjudicador"))
PYEOF

[ "$rc_selo" -eq 0 ] && [ "$rc_confere" -eq 0 ] || { echo "FALHOU (selo=$rc_selo confere=$rc_confere)"; exit 1; }
echo "selo gerado e conferido: rc=0 nos dois"
