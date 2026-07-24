#!/usr/bin/env bash
# validate_phase0.sh — Productization Fase 0 cross-audit gate.
# Plan: ~/.claude/plans/giggly-drifting-kahn.md (§ Fase 0) — 5 checks:
#  (a) no NEW runtime hardcode of the historical workspace root — the only
#      allowed literals are resolver fallbacks (same line as unwrap_or_else),
#      doc-comment lines, and known #[test] fixtures
#  (b) TOURING_WORKSPACE_ROOT override does not break the binary (runtime probe)
#  (c) `[toolchain] channel` in a project touring.toml reaches detect_layered
#  (d) `touring --version` matches [workspace.package] version (single source)
#  (e) the 6 edited sites carry the env-resolved pattern (citation re-audit)
set -uo pipefail
WS="${TOURING_WS:-$HOME/.claude/rust}"
BIN="${TOURING_BIN:-$WS/target/release/touring}"
cd "$WS" || { echo "FAIL: workspace $WS unreadable"; exit 3; }
fail=0

echo "── (a) runtime hardcodes of the historical root"
# Allowed: resolver fallbacks (unwrap_or_else lines), doc-comment lines,
# #[test] fixtures in-file, and E2E test binaries under /tests/ (the plan
# defers test-const migration to Fase 4, when the source actually moves).
hits=$(grep -rn '/home/gabrielgadea/\.claude/rust' crates/ --include='*.rs' \
  | grep -v 'unwrap_or_else' \
  | grep -vE '^[^:]+:[0-9]+:\s*//' \
  | grep -v '/tests/' \
  | grep -cvE 'quality_rules\.rs|txn\.rs|gotcha_loader\.rs')
if [ "$hits" -eq 0 ]; then echo "  PASS (0 unexpected)"; else
  echo "  FAIL: $hits unexpected runtime hardcodes:"; \
  grep -rn '/home/gabrielgadea/\.claude/rust' crates/ --include='*.rs' \
    | grep -v 'unwrap_or_else' | grep -vE '^[^:]+:[0-9]+:\s*//' \
    | grep -v '/tests/' \
    | grep -vE 'quality_rules\.rs|txn\.rs|gotcha_loader\.rs' | head -5; fail=1; fi

echo "── (b) TOURING_WORKSPACE_ROOT override runtime probe"
if TOURING_WORKSPACE_ROOT=/tmp/tf-phase0-fake "$BIN" --version >/dev/null 2>&1; then
  echo "  PASS (binary healthy under override)"; else echo "  FAIL (binary errored under override)"; fail=1; fi

echo "── (c) toolchain pin read by detect_layered (unit test)"
if cargo test -q -p touring-foundation test_layered_reads_project_toolchain_pin >/dev/null 2>&1; then
  echo "  PASS"; else echo "  FAIL (unit test red)"; fail=1; fi

echo "── (d) binary version == workspace version (single source of truth)"
ws_ver=$(grep -A8 '^\[workspace.package\]' Cargo.toml | grep '^version' | head -1 | cut -d'"' -f2)
# NOTE: the custom dispatch prints `--version` on STDERR (measured 2026-07-01;
# stdout carries 0 bytes) — capture both streams.
bin_ver=$("$BIN" --version 2>&1 | head -1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)
if [ -n "$ws_ver" ] && [ "$ws_ver" = "$bin_ver" ]; then
  echo "  PASS ($ws_ver)"; else echo "  FAIL (workspace=$ws_ver binary=$bin_ver)"; fail=1; fi

echo "── (e) edited sites carry the env-resolved pattern"
ok=0
for f in crates/touring-storage/src/knowledge_wiring.rs \
         crates/touring-hooks-core/src/knowledge_wiring.rs \
         crates/touring-cli/src/cli/gotcha.rs \
         crates/touring-hook-handlers/src/hooks/session_hooks.rs \
         crates/touring-server/src/cli/profile.rs; do
  grep -q 'TOURING_WORKSPACE_ROOT' "$f" && ok=$((ok+1))
done
grep -q 'channel = ' crates/touring-server/src/cli/init_project.rs && ok=$((ok+1))
if [ "$ok" -eq 6 ]; then echo "  PASS (6/6 sites)"; else echo "  FAIL ($ok/6 sites)"; fail=1; fi

if [ "$fail" -eq 0 ]; then echo "✅ PHASE 0 VALIDATE: ALL PASS"; exit 0
else echo "❌ PHASE 0 VALIDATE: FAILURES ABOVE"; exit 1; fi
