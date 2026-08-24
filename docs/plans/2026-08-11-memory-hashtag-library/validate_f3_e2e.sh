#!/usr/bin/env bash
# F3 E2E gate — typed link graph + tag-filtered hybrid recall (live daemon).
set -u
cd /home/gabrielgadea/projects/touring
TS=$(date +%s)
KA="e2e:f3:map-pattern:$TS"
KB="e2e:f3:diorama-impl:$TS"
fail=0

echo "== 0. seed two tagged memories =="
touring memory store "$KA" "map artifact pattern: layered data_layers + zoom controls + legend" \
  --type lesson --tag "#artifact:map" --tag "#purpose:map-rendering" >/dev/null 2>&1
touring memory store "$KB" "diorama implementation of the map pattern with stacked SVG layers" \
  --type snippet --tag "#artifact:map" --tag "#lang:python" >/dev/null 2>&1

echo "== 1. memory link (typed, deterministic) =="
OUT=$(touring memory link "$KB" "$KA" --rel exemplifies 2>&1)
echo "$OUT"
echo "$OUT" | grep -q '"status":"linked"' || { echo "FAIL: link"; fail=1; }
echo "$OUT" | grep -q "$KB|exemplifies|$KA" || { echo "FAIL: non-deterministic id"; fail=1; }

echo "== 2. re-link is a no-op (dedupe by id) =="
touring memory link "$KB" "$KA" --rel exemplifies >/dev/null 2>&1
OUT=$(touring memory links "$KA" 2>&1)
echo "$OUT"
echo "$OUT" | grep -q '"count":1' || { echo "FAIL: duplicate link created"; fail=1; }

echo "== 3. links 1-hop visible from both sides =="
echo "$OUT" | grep -q '"direction":"in"' || { echo "FAIL: incoming direction missing"; fail=1; }
OUT=$(touring memory links "$KB" 2>&1)
echo "$OUT" | grep -q '"direction":"out"' || { echo "FAIL: outgoing direction missing"; fail=1; }

echo "== 4. recall with tag filter narrows the corpus =="
OUT=$(touring memory recall "diorama #artifact:map" 2>&1)
echo "$OUT" | head -c 600; echo
echo "$OUT" | grep -q '"tag_filter"' || { echo "FAIL: tag_filter absent from payload"; fail=1; }
echo "$OUT" | grep -q "$KB" || { echo "FAIL: tagged memory not in filtered recall"; fail=1; }

echo "== 5. recall carries 1-hop links on entries =="
echo "$OUT" | grep -q '"links"' || { echo "FAIL: links not attached to entries"; fail=1; }
echo "$OUT" | grep -q '"rel":"exemplifies"' || { echo "FAIL: link rel missing in recall"; fail=1; }

echo "== 6. tags-only recall (no free text) =="
OUT=$(touring memory recall "#artifact:map #lang:python" 2>&1)
echo "$OUT" | head -c 400; echo
echo "$OUT" | grep -q "$KB" || { echo "FAIL: tags-only recall missed"; fail=1; }

echo "== 7. impossible filter relaxes and says so =="
OUT=$(touring memory recall "diorama #kind:doesnotexist" 2>&1)
echo "$OUT" | grep -q '"relaxed":true' || { echo "FAIL: impossible filter did not relax"; fail=1; }

echo "== 8. unknown rel rejected with valid list =="
OUT=$(touring memory link "$KA" "$KB" --rel banana 2>&1)
echo "$OUT" | grep -q "unknown rel" || { echo "FAIL: bad rel accepted"; fail=1; }

if [ "$fail" -eq 0 ]; then echo "F3 E2E: ALL PASS (keys $KA $KB)"; else echo "F3 E2E: FAILURES"; exit 1; fi
