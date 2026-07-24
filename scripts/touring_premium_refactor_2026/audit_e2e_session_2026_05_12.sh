#!/usr/bin/env bash
# E2E Master Cross-Audit Script — Session 2026-05-12
#
# Validates EVERY deliverable from the touring-premium-refactor-2026 session:
#   W1 (dead code purge), W2 (tooling foundation), W3.1 (rename + shim),
#   W3.x (cleanup), W3.2 Phase 1 (embedding factory boundary),
#   Fix cli_index_rebuild OOM-resilient, Fix w2_centralize apply mode.
#
# Each test prints PASS/FAIL with evidence. Exit code 0 only when all
# pass; non-zero on any failure (CI gateable).
#
# Usage:
#   ./audit_e2e_session_2026_05_12.sh           # full audit
#   ./audit_e2e_session_2026_05_12.sh --quick   # skip expensive cargo test
#
# Authored: Wave Cross-Audit 2026-05-12 (TACO Phase 6).

set -u  # Unset variable = error. NOT `set -e` because we want all
        # checks to run and aggregate failures.

QUICK_MODE=0
if [ "${1:-}" = "--quick" ]; then
    QUICK_MODE=1
fi

ROOT="$HOME/.claude/rust"
SCRIPTS_DIR="$ROOT/scripts/touring_premium_refactor_2026"
PASS=0
FAIL=0
FAILURES=()

# ─────────────────────────────────────────────────────────────────────
# Helpers
# ─────────────────────────────────────────────────────────────────────
section() {
    echo ""
    echo "═══════════════════════════════════════════════════════════════"
    echo "  $1"
    echo "═══════════════════════════════════════════════════════════════"
}

check() {
    local name="$1"; local expected="$2"; local actual="$3"
    if [ "$expected" = "$actual" ]; then
        echo "  ✓ PASS  $name  ($actual)"
        PASS=$((PASS + 1))
    else
        echo "  ✗ FAIL  $name  expected=$expected actual=$actual"
        FAIL=$((FAIL + 1))
        FAILURES+=("$name")
    fi
}

check_ge() {
    local name="$1"; local min="$2"; local actual="$3"
    if [ "$actual" -ge "$min" ] 2>/dev/null; then
        echo "  ✓ PASS  $name  ($actual >= $min)"
        PASS=$((PASS + 1))
    else
        echo "  ✗ FAIL  $name  expected>=$min actual=$actual"
        FAIL=$((FAIL + 1))
        FAILURES+=("$name")
    fi
}

check_file_exists() {
    local name="$1"; local path="$2"
    if [ -e "$path" ]; then
        echo "  ✓ PASS  $name  (exists)"
        PASS=$((PASS + 1))
    else
        echo "  ✗ FAIL  $name  (missing: $path)"
        FAIL=$((FAIL + 1))
        FAILURES+=("$name")
    fi
}

check_file_absent() {
    local name="$1"; local path="$2"
    if [ ! -e "$path" ]; then
        echo "  ✓ PASS  $name  (absent as expected)"
        PASS=$((PASS + 1))
    else
        echo "  ✗ FAIL  $name  (still exists: $path)"
        FAIL=$((FAIL + 1))
        FAILURES+=("$name")
    fi
}

# ─────────────────────────────────────────────────────────────────────
# FASE 0 — Health gate (BLOCKING)
# ─────────────────────────────────────────────────────────────────────
section "FASE 0 — Health gate"
cd "$ROOT"

cargo check --workspace >/dev/null 2>&1
check "F0.1 cargo check --workspace exit=0" "0" "$?"

doctor_ok=$(touring doctor -j 2>/dev/null | python3 -c \
    "import sys,json; d=json.load(sys.stdin); print(sum(1 for c in d if c['status']=='ok'))")
check_ge "F0.2 touring doctor ok components" 5 "${doctor_ok:-0}"

# ─────────────────────────────────────────────────────────────────────
# WAVE 1 — Dead code purge
# ─────────────────────────────────────────────────────────────────────
section "WAVE 1 — Dead code purge"

for c in touring-semantic-spike touring-wasm-client touring-wasm-common touring-wasm-server; do
    check_file_absent "W1 crate $c deleted" "$ROOT/crates/$c"
done

for c in touring_semantic_spike touring_wasm_client touring_wasm_common touring_wasm_server; do
    hits=$(grep -rln "use $c\|$c::" "$ROOT/crates/" --include='*.rs' 2>/dev/null | wc -l)
    check "W1 $c — zero .rs refs" "0" "$hits"
done

# ─────────────────────────────────────────────────────────────────────
# WAVE 2 — Tooling foundation
# ─────────────────────────────────────────────────────────────────────
section "WAVE 2 — Tooling foundation"

deps_count=$(awk '/^\[workspace\.dependencies\]/,/^\[(?!workspace)/' "$ROOT/Cargo.toml" \
    | grep -cE "^[a-z][a-z0-9_-]*\s*=")
check_ge "W2.1 workspace.dependencies entries" 200 "$deps_count"

ws_inherit=$(grep -l "\.workspace = true" "$ROOT/crates/"*/Cargo.toml 2>/dev/null | wc -l)
check_ge "W2.3 crates inheriting .workspace=true" 30 "$ws_inherit"

clippy_deny=$(grep -A 3 "^\[workspace\.lints\.clippy\]" "$ROOT/Cargo.toml" \
    | grep -c 'all = { level = "deny"')
check "W2.4 clippy::all=deny strict lints" "1" "$clippy_deny"

mut_present=$(grep -c "^\[workspace\.metadata\.mutants\]" "$ROOT/Cargo.toml")
check "W2.7 [workspace.metadata.mutants] present" "1" "$mut_present"

mach_present=$(grep -c "^\[workspace\.metadata\.cargo-machete\]" "$ROOT/Cargo.toml")
check "W2.6 [workspace.metadata.cargo-machete] present" "1" "$mach_present"

check_file_exists "W2.5 deny.toml" "$ROOT/deny.toml"

msrv_job=$(grep -c "^  msrv:" "$ROOT/docs/ci-template.yml")
check "W2.8 msrv job in ci-template.yml" "1" "$msrv_job"

# ─────────────────────────────────────────────────────────────────────
# WAVE 3.1 — Rename + shim
# ─────────────────────────────────────────────────────────────────────
section "WAVE 3.1 — Rename + shim"

check_file_exists "W3.1.1 touring-foundation dir" "$ROOT/crates/touring-foundation/src"

fname=$(grep '^name' "$ROOT/crates/touring-foundation/Cargo.toml" | head -1)
check 'W3.1.2 foundation Cargo.toml name field' \
    'name = "touring-foundation"' "$fname"

shim_pattern=$(grep -c "pub use touring_foundation::\*" \
    "$ROOT/crates/touring-core/src/lib.rs")
check "W3.1.4 shim re-exports foundation::*" "1" "$shim_pattern"

check_file_absent "W3.1.5 touring-core/tests/ removed" "$ROOT/crates/touring-core/tests"

core_orphan_hits=$(grep -rln "touring_core" --include='*.rs' "$ROOT/crates/" 2>/dev/null \
    | grep -v "crates/touring-core/\|crates/touring-foundation/" | wc -l)
check "W3.1.6 zero touring_core hits in consumers" "0" "$core_orphan_hits"

foundation_users=$(grep -rln "touring_foundation::" --include='*.rs' \
    "$ROOT/crates/" 2>/dev/null | grep -v "touring-foundation/" | wc -l)
check_ge "W3.1.7 consumers using touring_foundation" 80 "$foundation_users"

# ─────────────────────────────────────────────────────────────────────
# WAVE 3.x — Cleanup
# ─────────────────────────────────────────────────────────────────────
section "WAVE 3.x — Cleanup"

core_files=$(find "$ROOT/crates/touring-core/src/" -type f 2>/dev/null | wc -l)
check "W3.x touring-core/src/ has only lib.rs" "1" "$core_files"

# ─────────────────────────────────────────────────────────────────────
# WAVE 3.2 — Embedding boundary marker
# ─────────────────────────────────────────────────────────────────────
section "WAVE 3.2 — Embedding boundary"

embed_mod="$ROOT/crates/touring-foundation/src/embedding/mod.rs"
factory_present=$(grep -c "^pub fn make_embedder" "$embed_mod")
check "W3.2 make_embedder factory present" "1" "$factory_present"

config_present=$(grep -c "^pub struct EmbedderConfig" "$embed_mod")
check "W3.2 EmbedderConfig struct present" "1" "$config_present"

# ─────────────────────────────────────────────────────────────────────
# FIX — cli_index_rebuild OOM-resilient
# ─────────────────────────────────────────────────────────────────────
section "FIX — cli_index_rebuild OOM resilient"

cir="$ROOT/crates/touring-hooks/src/cli_handlers_index.rs"
chunk_def=$(grep -c "const CHUNK_SIZE: usize = 100" "$cir")
check "FIX-CIR.1 CHUNK_SIZE constant" "1" "$chunk_def"

hard_def=$(grep -c "MEMORY_HARD_MB: f64 = 3000" "$cir")
check "FIX-CIR.2 MEMORY_HARD_MB threshold" "1" "$hard_def"

probe_call=$(grep -c "memory_stats_probe::snapshot()" "$cir")
check_ge "FIX-CIR.3 memory_stats_probe::snapshot() called" 1 "$probe_call"

red_call=$(grep -c "gate_metrics::record_pressure_red()" "$cir")
check "FIX-CIR.4 record_pressure_red() called on abort" "1" "$red_call"

abort_json=$(grep -c '"aborted_memory_pressure"' "$cir")
check "FIX-CIR.5 JSON output telemetry" "1" "$abort_json"

# ─────────────────────────────────────────────────────────────────────
# FIX — w2_centralize_workspace_deps.py
# ─────────────────────────────────────────────────────────────────────
section "FIX — w2_centralize_workspace_deps apply"

wcw="$SCRIPTS_DIR/w2_centralize_workspace_deps.py"
func_exists=$(grep -c "^def apply_to_root_cargo_toml" "$wcw")
check "FIX-W2.1 apply_to_root_cargo_toml present" "1" "$func_exists"

helpers=$(grep -c "^def _section_bounds\|^def _existing_deps_in_section" "$wcw")
check "FIX-W2.2 _section_bounds + _existing_deps helpers" "2" "$helpers"

stub_removed=$(grep -c "apply mode not yet implemented" "$wcw")
check "FIX-W2.3 stub removed" "0" "$stub_removed"

# ─────────────────────────────────────────────────────────────────────
# TEST INTEGRATION
# ─────────────────────────────────────────────────────────────────────
if [ "$QUICK_MODE" = "0" ]; then
    section "TEST integration"

    # Foundation tests
    cd "$ROOT"
    test_out=$(cargo test -p touring-foundation 2>&1 | grep -E "^test result" \
        | head -1 | awk '{print $4}')
    check_ge "TI.1 touring-foundation tests passed" 25 "${test_out:-0}"

    # Embedding tests specifically (E2E + boundary)
    embed_out=$(cargo test -p touring-foundation --lib embedding 2>&1 \
        | grep -E "^test result: ok" | tail -1 | awk '{print $4}')
    check_ge "TI.2 embedding tests passed" 8 "${embed_out:-0}"

    # Pytest sub-scripts
    cd "$SCRIPTS_DIR"
    py_out=$(python3 -m pytest tests/ -q 2>&1 | grep "passed" \
        | tail -1 | awk '{print $1}')
    check_ge "TI.3 pytest sub-scripts passed" 100 "${py_out:-0}"
fi

# ─────────────────────────────────────────────────────────────────────
# Summary
# ─────────────────────────────────────────────────────────────────────
section "SUMMARY"
TOTAL=$((PASS + FAIL))
echo "  TOTAL: $TOTAL checks"
echo "  PASS:  $PASS"
echo "  FAIL:  $FAIL"

if [ "$FAIL" -gt 0 ]; then
    echo ""
    echo "  Failures:"
    for f in "${FAILURES[@]}"; do
        echo "    - $f"
    done
    exit 1
fi

echo ""
echo "  🎉 All checks passed — session 2026-05-12 deliverables verified."
exit 0
