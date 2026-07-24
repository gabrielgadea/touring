#!/usr/bin/env bash
# validate_phase5.sh — Pln2 F5 cross-audit gate: distribuição & versionamento.
#
# Proves the distribution half that exists WITHOUT the release server:
# packager → tarball(bin/ layout, CI-identical) → installer end-to-end
# (--from-tarball offline + --from-url file://), sha256 tamper refusal,
# release-channel fail-closed, dry-run coherence, staged workflows lint,
# and supply-chain gate (cargo-deny). Git boundary (Gabriel): promote
# release-plz.yml, tag SemVer, publish artifacts.
# Pattern: capture-then-grep (SIGPIPE gotcha).
set -uo pipefail

PASS=0; FAIL=0
WS=/home/gabrielgadea/projects/touring
INSTALLER="$WS/scripts/packaging/install.touring.dev.sh"
PACKAGER="$WS/scripts/packaging/package_release.sh"

check() { if [ "$2" -eq 0 ]; then PASS=$((PASS+1)); echo "  ✅ $1"; else FAIL=$((FAIL+1)); echo "  ❌ $1"; fi }

echo "=== validate_phase5 — F5 distribuição ($(date -Iseconds)) ==="
SANDBOX=$(mktemp -d /tmp/validate-phase5-XXXXXX)

# 1. Packager produces the CI-identical artifact shape (bin/ layout + sha256)
OUT=$(bash "$PACKAGER" 30.3.0 --workspace "$WS" --out "$SANDBOX" 2>&1); RC=$?
TARBALL=$(ls "$SANDBOX"/touring-*.tar.gz 2>/dev/null | head -1)
OK=1
[ $RC -eq 0 ] && [ -n "$TARBALL" ] && [ -f "$TARBALL.sha256" ] \
  && LIST=$(tar -tzf "$TARBALL") && echo "$LIST" | grep -q "^bin/touring$" \
  && echo "$LIST" | grep -q "^bin/touring-daemon$" && OK=0
check "1. package_release: tarball bin/-layout + .sha256 ($(basename "${TARBALL:-none}"))" $OK

# 2. Installer dry-run: coherent plan, zero mutation
TH2="$SANDBOX/th-dry"
OUT=$(sh "$INSTALLER" --version 30.3.0 --dry-run --toolchain-home "$TH2" --no-modify-path 2>&1); RC=$?
OK=1
[ $RC -eq 0 ] && echo "$OUT" | grep -q "DRY-RUN" && [ ! -d "$TH2" ] && OK=0
check "2. installer --dry-run coherent + mutates nothing" $OK

# 3. Offline install end-to-end: --from-tarball → binário responde
TH3="$SANDBOX/th-tarball"
OUT=$(sh "$INSTALLER" --version 30.3.0 --from-tarball "$TARBALL" --toolchain-home "$TH3" --no-modify-path 2>&1); RC=$?
OK=1
[ $RC -eq 0 ] && echo "$OUT" | grep -q "SHA-256 verified" \
  && V=$("$TH3/toolchains/30.3.0/bin/touring" --version 2>&1) \
  && echo "$V" | grep -q "touring" \
  && [ "$(cat "$TH3/default")" = "30.3.0" ] \
  && grep -q "installer:" "$TH3/toolchains/30.3.0/meta.toml" && OK=0
check "3. install --from-tarball E2E: sha256 + bin responde ($V) + default + meta" $OK

# 4. URL install (file:// exercises the full fetch path, no network)
TH4="$SANDBOX/th-url"
OUT=$(sh "$INSTALLER" --version 30.3.0 --from-url "file://$TARBALL" --toolchain-home "$TH4" --no-modify-path 2>&1); RC=$?
OK=1
[ $RC -eq 0 ] && [ -x "$TH4/toolchains/30.3.0/bin/touring-hook" ] && OK=0
check "4. install --from-url file:// E2E" $OK

# 5. Tampered tarball → sha256 mismatch → REFUSED
CORRUPT="$SANDBOX/corrupt.tar.gz"
cp "$TARBALL" "$CORRUPT"; cp "$TARBALL.sha256" "$CORRUPT.sha256"
printf 'tamper' >> "$CORRUPT"
TH5="$SANDBOX/th-tamper"
OUT=$(sh "$INSTALLER" --version 30.3.0 --from-tarball "$CORRUPT" --toolchain-home "$TH5" --no-modify-path 2>&1); RC=$?
OK=1
[ $RC -ne 0 ] && echo "$OUT" | grep -q "MISMATCH" && [ ! -d "$TH5/toolchains/30.3.0" ] && OK=0
check "5. tampered tarball refused (sha256 mismatch, nothing installed)" $OK

# 6. Release channel is fail-closed: version required; missing artifacts refuse
OUT=$(sh "$INSTALLER" --toolchain-home "$SANDBOX/th-rc" --no-modify-path 2>&1); RC=$?
OK=1
[ $RC -ne 0 ] && echo "$OUT" | grep -q "version" && OK=0
check "6. release channel without --version refused loud" $OK

# 7. Installer refuses a non-toolchain tarball (no bin/touring inside)
BOGUS="$SANDBOX/bogus.tar.gz"; BOGUS_DIR=$(mktemp -d "$SANDBOX/bogus-XXXX")
touch "$BOGUS_DIR/random.txt"; tar -C "$BOGUS_DIR" -czf "$BOGUS" random.txt
TH7="$SANDBOX/th-bogus"
OUT=$(sh "$INSTALLER" --version 9.9.9 --from-tarball "$BOGUS" --toolchain-home "$TH7" --no-modify-path 2>&1); RC=$?
OK=1
[ $RC -ne 0 ] && echo "$OUT" | grep -q "bin/touring" && [ ! -d "$TH7/toolchains/9.9.9" ] && OK=0
check "7. non-toolchain tarball refused + no partial left" $OK

# 8. Staged workflows are valid YAML + promotion README present (git boundary)
OK=0
for f in "$WS/scripts/touring_premium_refactor_2026/staging/w13-github-workflows/release-plz.yml" \
         "$WS/scripts/touring_premium_refactor_2026/staging/w13-github-workflows/docs-rs-mirror.yml" \
         "$WS/.github/workflows/release.yml"; do
  python3 -c "import yaml,sys; yaml.safe_load(open('$f'))" 2>/dev/null || OK=1
done
[ -f "$WS/scripts/touring_premium_refactor_2026/staging/w13-github-workflows/PROMOTION-README.md" ] || OK=1
check "8. release.yml + staged workflows parse as YAML; PROMOTION-README present" $OK

# 9. release.yml packages the SAME layout the installer expects (bin/)
RY=$(cat "$WS/.github/workflows/release.yml")
OK=1
echo "$RY" | grep -q 'mkdir -p "${STAGE}/bin"' \
  && echo "$RY" | grep -q 'czf "${{ matrix.archive }}" bin' \
  && echo "$RY" | grep -q '"${SMOKE}/bin/touring" --help' && OK=0
check "9. release.yml CI artifact = bin/ layout (installer-compatible)" $OK

# 10. Supply-chain gate: cargo-deny advisories green (P0 F2.5 substrate)
OUT=$(cd "$WS" && cargo deny check advisories 2>&1); RC=$?
OK=1
[ $RC -eq 0 ] && OK=0 || { echo "$OUT" | tail -3; }
check "10. cargo deny check advisories green" $OK

echo ""
echo "=== validate_phase5: $PASS PASS / $FAIL FAIL ==="
[ $FAIL -eq 0 ] && echo "ALL PASS" || echo "GATE FAILED"
exit $FAIL
