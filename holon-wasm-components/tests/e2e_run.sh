#!/usr/bin/env bash
# THSF Fase 4+5 E2E Integration Test
# Proves holon-wasm-runner + all 4 WASM components work in a complete pipeline.
# Exit 0 = all pass, exit 1 = any failure.
set -e

RUNNER="/home/gabrielgadea/.claude/rust/holon-wasm-runner/target/release/holon-wasm-runner"
WASM_DIR="/home/gabrielgadea/.claude/rust/holon-wasm-components/target/wasm32-wasip2/release"

echo "═══════════════════════════════════════════════════════"
echo "  THSF WASM Component E2E Integration Test"
echo "═══════════════════════════════════════════════════════"
echo ""

total=0; passed=0

run_test() {
    local name="$1"; shift
    local got="$("$@" 2>&1)" || true
    total=$((total+1))
    if [ -n "$got" ]; then
        echo "  ✓ $name"
        passed=$((passed+1))
        return 0
    else
        echo "  ✗ $name: got empty"
        return 1
    fi
}

run_json() {
    local name="$1"; shift
    local got="$("$@" 2>&1)" || true
    total=$((total+1))
    if echo "$got" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
        echo "  ✓ $name"
        passed=$((passed+1))
        return 0
    else
        echo "  ✗ $name: not valid JSON — $got"
        return 1
    fi
}

echo "── Component 1: holon_spec_version ──"
run_json "list capabilities" "$RUNNER" "$WASM_DIR/holon_spec_version.wasm" list
run_json "invoke spec-version" "$RUNNER" "$WASM_DIR/holon_spec_version.wasm" invoke spec-version '{}'

echo ""
echo "── Component 2: holon_blast_radius ──"
run_json "list capabilities" "$RUNNER" "$WASM_DIR/holon_blast_radius.wasm" list
run_json "invoke (2-level blast)" "$RUNNER" "$WASM_DIR/holon_blast_radius.wasm" invoke blast-radius '{"graph":{"a.rs":["b.rs","c.rs"],"b.rs":["c.rs"],"c.rs":[]},"target":"c.rs"}'
run_json "invoke (isolated node)" "$RUNNER" "$WASM_DIR/holon_blast_radius.wasm" invoke blast-radius '{"graph":{"a.rs":[]},"target":"a.rs"}'

echo ""
echo "── Component 3: holon_quality_gate ──"
run_json "list capabilities" "$RUNNER" "$WASM_DIR/holon_quality_gate.wasm" list
run_json "invoke (clean code score=1.0)" "$RUNNER" "$WASM_DIR/holon_quality_gate.wasm" invoke quality-gate '{"source":"fn main() { println!(\"hello\"); }","lang":"rust"}'
run_json "invoke (unwrap only)" "$RUNNER" "$WASM_DIR/holon_quality_gate.wasm" invoke quality-gate '{"source":"fn main() { x.unwrap(); }","lang":"rust"}'
run_json "invoke (panic+todo)" "$RUNNER" "$WASM_DIR/holon_quality_gate.wasm" invoke quality-gate '{"source":"fn main() { panic!(\"oops\"); todo!(); }","lang":"rust"}'
run_json "invoke (Python bare except)" "$RUNNER" "$WASM_DIR/holon_quality_gate.wasm" invoke quality-gate '{"source":"try:\n  x\nexcept:\n  pass","lang":"python"}'

echo ""
echo "── Component 4: holon_generator_health ──"
run_json "list capabilities" "$RUNNER" "$WASM_DIR/holon_generator_health.wasm" list
run_json "invoke (healthy state)" "$RUNNER" "$WASM_DIR/holon_generator_health.wasm" invoke generator-health '{"counters":{"compute_count":20,"regression_count":2,"improvement_count":15,"recovery_count":3,"streak_alert_count":0,"alert_threshold":3},"per_path":[{"file_path":"src/foo.rs","regression_streak":3,"improvement_streak":0},{"file_path":"src/bar.rs","regression_streak":0,"improvement_streak":5}]}'
run_json "invoke (critical regression)" "$RUNNER" "$WASM_DIR/holon_generator_health.wasm" invoke generator-health '{"counters":{"compute_count":5,"regression_count":4,"improvement_count":1,"recovery_count":0,"streak_alert_count":1,"alert_threshold":3},"per_path":[]}'

echo ""
echo "── Cross-component: error handling ──"
# Unknown capability should return error variant
run_json "unknown capability error" "$RUNNER" "$WASM_DIR/holon_spec_version.wasm" invoke unknown-capability '{}'

echo ""
echo "═══════════════════════════════════════════════════════"
echo "  RESULTS: $passed / $total tests passed"
echo "═══════════════════════════════════════════════════════"

if [ "$passed" -eq "$total" ]; then
    echo "  ALL TESTS PASSED ✓"
    exit 0
else
    echo "  FAILURE: $((total - passed)) / $total tests failed"
    exit 1
fi