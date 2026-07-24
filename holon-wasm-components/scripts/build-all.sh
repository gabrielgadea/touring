#!/usr/bin/env bash
# build-all.sh — THSF Fase 4 one-shot builder.
#
# Compiles:
#   1. The three WebAssembly components for `wasm32-wasip2`
#      (spec-version, blast-radius, quality-gate).
#   2. The host `holon-wasm-runner` binary.
#   3. Optionally: the aggregated component via `wac compose`.
#
# Exit 0 on success; non-zero on any failure (offender reported).
set -euo pipefail

BASE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." &>/dev/null && pwd)"
COMPOSE="${1:-compose}"  # pass "no-compose" to skip the wac compose step

cd "${BASE}"

echo "[build-all] step 1/3: compile WASM components (target=wasm32-wasip2)"
cargo build --release --target wasm32-wasip2 \
    -p holon-spec-version -p holon-blast-radius -p holon-quality-gate \
    -p holon-generator-health

echo "[build-all] step 2/3: compile host runner (host target)"
cargo build --release -p holon-wasm-runner

WASM_DIR="${BASE}/target/wasm32-wasip2/release"
HOST_DIR="${BASE}/target/release"

if [[ "${COMPOSE}" != "no-compose" ]]; then
    echo "[build-all] step 3/3: wac compose → holon_aggregate.wasm"
    if ! command -v wac >/dev/null 2>&1; then
        echo "[build-all] WARN: \`wac\` CLI not installed — skipping composition."
        echo "  install via: cargo install wac-cli --locked"
    else
        wac compose compose/aggregate.wac \
            --dep holon:spec-version="${WASM_DIR}/holon_spec_version.wasm" \
            --dep holon:blast-radius="${WASM_DIR}/holon_blast_radius.wasm" \
            --dep holon:quality-gate="${WASM_DIR}/holon_quality_gate.wasm" \
            -o "${WASM_DIR}/holon_aggregate.wasm"
    fi
else
    echo "[build-all] step 3/3: SKIPPED (no-compose)"
fi

echo ""
echo "[build-all] SUMMARY"
echo "  host runner:   ${HOST_DIR}/holon-wasm-runner"
for name in holon_spec_version holon_blast_radius holon_quality_gate holon_generator_health holon_aggregate; do
    path="${WASM_DIR}/${name}.wasm"
    if [[ -f "${path}" ]]; then
        size="$(stat -c%s "${path}")"
        printf "  %-24s  %s bytes\n" "${name}.wasm" "${size}"
    fi
done
