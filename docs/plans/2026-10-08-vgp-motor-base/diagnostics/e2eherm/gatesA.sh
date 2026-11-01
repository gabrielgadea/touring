#!/bin/bash
# Gate set A, one hw-slot acquisition: fmt, clippy (all targets, -D warnings), compile the wave25 test, tight-window proof.
# usage: gatesA.sh <label>      (call as: hw-slot --class rust --wait 7200 -- bash gatesA.sh <label>)
S=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/e2eherm
label=${1:?label}
cd /home/gabrielgadea/projects/touring-wt-e2eherm || exit 1
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-e2eherm

echo "### fmt";    cargo fmt --all -- --check > "$S/$label.fmt.log" 2>&1;  echo "fmt rc=$?"
echo "### clippy"; cargo clippy -p touring-hooks --all-targets -- -D warnings > "$S/$label.clippy.log" 2>&1; echo "clippy rc=$?"
tail -3 "$S/$label.clippy.log" | cut -c1-200
echo "### compile wave25"
cargo test -p touring-hooks --test wave25_task_enrichment_e2e --no-run > "$S/$label.compile.log" 2>&1; echo "compile rc=$?"
grep -oE "target/wt-e2eherm/debug/deps/wave25_task_enrichment_e2e-[0-9a-f]+" "$S/$label.compile.log" | tail -1 | sed 's#^#/home/gabrielgadea/projects/touring/#' > "$S/$label.exe"
cat "$S/$label.exe"
bash "$S/proof_inner.sh" "$label"
