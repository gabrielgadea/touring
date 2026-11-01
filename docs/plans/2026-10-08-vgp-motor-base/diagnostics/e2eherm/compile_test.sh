#!/bin/bash
# Compile (not run) the wave25 test binary under the rust hw-slot; print the executable path.
# usage: compile_test.sh <label>   -> writes $S/<label>.compile.log and $S/<label>.exe
S=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/e2eherm
label=${1:?label}
cd /home/gabrielgadea/projects/touring-wt-e2eherm || exit 1
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-e2eherm
/home/gabrielgadea/projects/touring/scripts/hw-slot --class rust --wait 7200 -- \
  cargo test -p touring-hooks --test wave25_task_enrichment_e2e --no-run > "$S/$label.compile.log" 2>&1
rc=$?
echo "compile rc=$rc"
grep -E "Executable|error|warning: unused" "$S/$label.compile.log" | head -20
grep -oE "target/wt-e2eherm/debug/deps/wave25_task_enrichment_e2e-[0-9a-f]+" "$S/$label.compile.log" | tail -1 | sed 's#^#/home/gabrielgadea/projects/touring/#' > "$S/$label.exe"
cat "$S/$label.exe"
exit $rc
