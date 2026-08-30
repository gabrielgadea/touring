#!/usr/bin/env bash
# validate_p1_p5_e2e.sh — prova comportamental do contrato do grafo (P1-P5)
# contra o binário touring INSTALADO. Re-executável a cada release (um
# validador que roda uma vez decai em certificado falso).
set -u
LC_ALL=C
PASS=0; FAIL=0
chk() { if [ "$1" = "ok" ]; then PASS=$((PASS+1)); echo "  ✅ $2"; else FAIL=$((FAIL+1)); echo "  ❌ $2"; fi; }
TS=$(date +%s)

echo "== P1 store: faceta inválida vira ignored_facets na RESPOSTA =="
OUT=$(touring memory store "audit:sonda-p1-$TS:2026-08-30" "sonda do cross-audit" --tier semantic --tag "#kind:lesson" --tag "#classe:prova" 2>/dev/null)
echo "$OUT" | python3 -c "
import json, sys
d = json.load(sys.stdin)
ok = isinstance(d.get('ignored_facets'), list) and len(d['ignored_facets']) == 1
item = (d.get('ignored_facets') or [{}])[0]
teaches = 'kind, purpose, lang, domain, process, artifact, status' in str(item.get('reason'))
print('ok' if ok and teaches else 'no')
" | grep -q ok && chk ok "(P1a) ignored_facets presente, 1 item, razão ensina as 7" || chk no "(P1a) resposta: $OUT"

echo "== P1 store: tudo aceito = array VAZIO (presença constante) =="
OUT=$(touring memory store "audit:sonda-p1b-$TS:2026-08-30" "sonda limpa" --tier episodic --tag "#kind:observation" 2>/dev/null)
echo "$OUT" | python3 -c "
import json, sys; d = json.load(sys.stdin)
print('ok' if d.get('ignored_facets') == [] else 'no')
" | grep -q ok && chk ok "(P1b) array vazio quando tudo aceito (ausência ≠ daemon velho)" || chk no "(P1b) $OUT"

echo "== P1 query: token desconhecido declarado em unknown_facets =="
OUT=$(touring memory query "#kind:lesson #classe:prova" 2>/dev/null)
echo "$OUT" | python3 -c "
import json, sys; d = json.load(sys.stdin)
uf = d.get('unknown_facets')
ok = isinstance(uf, list) and any('classe' in str(u.get('raw','')) for u in uf)
print('ok' if ok else 'no')
" | grep -q ok && chk ok "(P1c) query declara o token que caiu para texto" || chk no "(P1c) $OUT"

echo "== P1.5 supersede reaponta arestas =="
touring memory store "audit:sonda-old-$TS:2026-08-30" "v1" --tier semantic >/dev/null 2>&1
touring memory store "audit:sonda-peer-$TS:2026-08-30" "peer" --tier semantic >/dev/null 2>&1
touring memory link "audit:sonda-old-$TS:2026-08-30" "audit:sonda-peer-$TS:2026-08-30" --rel relates-to >/dev/null 2>&1
touring memory store "audit:sonda-new-$TS:2026-08-30" "v2" --tier semantic --supersedes "audit:sonda-old-$TS:2026-08-30" >/dev/null 2>&1
OUT=$(touring memory links "audit:sonda-new-$TS:2026-08-30" 2>/dev/null)
echo "$OUT" | python3 -c "
import json, sys; d = json.load(sys.stdin)
ls = d.get('links') or []
print('ok' if any('sonda-peer' in str(l.get('dst','')) or 'sonda-peer' in str(l.get('src','')) for l in ls) else 'no')
" | grep -q ok && chk ok "(P1.5) aresta do nó antigo reapontada ao sucessor" || chk no "(P1.5) $OUT"

echo "== P3 advisory contract no store curado =="
OUT=$(touring memory store "audit:sonda-p3-$TS:2026-08-30" "decisão de sonda" --tier semantic --tag "#kind:decision" 2>/dev/null)
echo "$OUT" | python3 -c "
import json, sys; d = json.load(sys.stdin)
c = d.get('contract')
ok = isinstance(c, dict) and c.get('key_shape') is True and 'linked' in c and 'provenance' in c
print('ok' if ok else 'no')
" | grep -q ok && chk ok "(P3) contract:{key_shape,faceted,linked,provenance} presente e key_shape=true" || chk no "(P3) $OUT"

echo "== P5 suggest-links responde (nunca erro) =="
OUT=$(touring memory suggest-links --min-co 99 2>/dev/null)
echo "$OUT" | python3 -c "
import json, sys; d = json.load(sys.stdin)
print('ok' if 'suggestions' in d and 'error' not in d else 'no')
" | grep -q ok && chk ok "(P5) suggest-links devolve shape honesto" || chk no "(P5) $OUT"

echo "== KPI: braços novos medem =="
touring kpi -j 2>/dev/null > /tmp/kpi_audit_$TS.json
python3 -c "
import json
d = json.load(open('/tmp/kpi_audit_$TS.json'))
found = set()
def walk(o):
    if isinstance(o, dict):
        i = str(o.get('id',''))
        for k in ('graph_contract_share','edge_density'):
            if k in i: found.add(k)
        for v in o.values(): walk(v)
    elif isinstance(o, list):
        for v in o: walk(v)
walk(d)
print('ok' if found == {'graph_contract_share','edge_density'} else f'no {found}')
" | grep -q '^ok' && chk ok "(P3/P5) commitments graph_contract_share + edge_density avaliados no kpi -j" || chk no "(KPI) braços ausentes"
rm -f /tmp/kpi_audit_$TS.json

echo; echo "RESULTADO: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ] && echo "P1_P5_E2E_OK" || echo "P1_P5_E2E_FALHOU"
exit $FAIL
