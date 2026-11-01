#!/usr/bin/env bash
# F1 E2E gate — store → auto-tag → tag → tags → query, against the LIVE daemon.
# Exit 0 only when every step proves itself on the deployed binary.
set -u
cd /home/gabrielgadea/projects/touring
K="e2e:hashtag-library:f1:$(date +%s)"
fail=0

echo "== 1. store with explicit tags (auto-derive must also fire) =="
OUT=$(touring memory store "$K" "render diorama map artifact with layered data_layers and zoom controls" \
  --type lesson --tag "#purpose:map-rendering" --tag "#artifact:map" 2>&1)
echo "$OUT"
echo "$OUT" | grep -q '"status":"stored"' || { echo "FAIL: store"; fail=1; }

echo "== 2. tags list — explicit + derived =="
OUT=$(touring memory tags "$K" 2>&1)
echo "$OUT"
for t in "purpose:map-rendering" "artifact:map" "kind:lesson" "status:stable"; do
  echo "$OUT" | grep -q "$t" || { echo "FAIL: missing tag $t"; fail=1; }
done

echo "== 3. memory tag add on an existing key =="
OUT=$(touring memory tag "$K" "#domain:portfolio" 2>&1)
echo "$OUT"
echo "$OUT" | grep -q '"status":"tagged"' || { echo "FAIL: tag add"; fail=1; }

echo "== 4. conjunctive query: tags only =="
OUT=$(touring memory query "#kind:lesson #artifact:map" 2>&1)
echo "$OUT" | head -3
echo "$OUT" | grep -q "$K" || { echo "FAIL: tag-only query did not find $K"; fail=1; }

echo "== 5. hybrid query: text + tag =="
OUT=$(touring memory query "diorama #kind:lesson" 2>&1)
echo "$OUT" | head -3
echo "$OUT" | grep -q "$K" || { echo "FAIL: hybrid query did not find $K"; fail=1; }

echo "== 6. invalid tag is rejected with suggestion =="
OUT=$(touring memory tag "$K" "#knd:snippet" 2>&1)
echo "$OUT"
echo "$OUT" | grep -qi "invalid tag" || { echo "FAIL: invalid tag accepted"; fail=1; }

echo "== 7. unseeded value warns but is accepted =="
OUT=$(touring memory tag "$K" "#domain:hashtag-library" 2>&1)
echo "$OUT"
echo "$OUT" | grep -q '"status":"tagged"' || { echo "FAIL: unseeded tag rejected"; fail=1; }

if [ "$fail" -eq 0 ]; then echo "F1 E2E: ALL PASS (key=$K)"; else echo "F1 E2E: FAILURES"; exit 1; fi
