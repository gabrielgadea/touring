#!/bin/bash
# Gate B: full touring-hooks test suite, no-fail-fast, in one rust slot.
S=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/e2eherm
cd /home/gabrielgadea/projects/touring-wt-e2eherm || exit 1
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-e2eherm
date -u +%FT%TZ
cargo test -p touring-hooks --no-fail-fast > "$S/gatesB.test.log" 2>&1; rc=$?
date -u +%FT%TZ
echo "cargo test rc=$rc"
grep -E "^test result|^error|FAILED|failed|Running " "$S/gatesB.test.log" | grep -vE "Running (unittests|tests/)" | cut -c1-200 | head -60
echo "suites: $(grep -c '^test result' "$S/gatesB.test.log")  failed-lines: $(grep -cE '^test .* FAILED' "$S/gatesB.test.log")"
exit $rc
