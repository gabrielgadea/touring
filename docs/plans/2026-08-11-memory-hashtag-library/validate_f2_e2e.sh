#!/usr/bin/env bash
# F2 E2E gate — codetag anchor → snippet memory → tag query → tombstone.
# Proves the full F2 loop against the LIVE daemon: a `#tags:` anchor in a
# source file becomes a tag-searchable snippet without opening the file,
# and removing the anchor removes the snippet (code is source of truth).
set -u
cd /home/gabrielgadea/projects/touring
D="docs/plans/2026-08-11-memory-hashtag-library/fixture"
mkdir -p "$D"
F="$D/codetag_fixture.py"
fail=0

cat > "$F" << 'PYEOF'
# #tags: kind:script lang:python purpose:map-rendering artifact:map domain:portfolio status:experimental
def render_diorama(layers):
    """Render the diorama map from layered data_layers."""
    canvas = []
    for layer in layers:
        canvas.append(layer.render())
    return canvas
PYEOF

echo "== 1. sync-tags --file harvests the anchor =="
OUT=$(touring memory sync-tags --file "$F" 2>&1)
echo "$OUT"
echo "$OUT" | grep -q '"status":"synced"' || { echo "FAIL: sync"; fail=1; }
echo "$OUT" | grep -q '"upserted":1' || { echo "FAIL: upserted != 1"; fail=1; }

echo "== 2. snippet findable by facet query =="
OUT=$(touring memory query "#kind:script #artifact:map" 2>&1)
echo "$OUT" | head -3
echo "$OUT" | grep -q "codetag_fixture" || { echo "FAIL: snippet not found by tags"; fail=1; }
echo "$OUT" | grep -q "render_diorama" || { echo "FAIL: snippet BODY not in memory (value)"; fail=1; }

echo "== 3. snippet carries derived tags too =="
KEY="snippet:$F#L1"
OUT=$(touring memory tags "$KEY" 2>&1)
echo "$OUT"
for t in "kind:script" "lang:python" "purpose:map-rendering" "status:experimental"; do
  echo "$OUT" | grep -q "$t" || { echo "FAIL: missing $t on $KEY"; fail=1; }
done

echo "== 4. anchor removed → tombstone =="
cat > "$F" << 'PYEOF'
def render_diorama(layers):
    return [layer.render() for layer in layers]
PYEOF
OUT=$(touring memory sync-tags --file "$F" 2>&1)
echo "$OUT"
echo "$OUT" | grep -q '"tombstoned":1' || { echo "FAIL: tombstone"; fail=1; }
OUT=$(touring memory tags "$KEY" 2>&1)
echo "$OUT" | grep -q '"count":0' || { echo "FAIL: tags survived tombstone"; fail=1; }

rm -rf "$D"
if [ "$fail" -eq 0 ]; then echo "F2 E2E: ALL PASS"; else echo "F2 E2E: FAILURES"; exit 1; fi
