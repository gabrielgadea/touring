#!/usr/bin/env bash
# touring installer — B.W1.P1.T2 (Master Plan Track B, Day-0 Installability)
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/${TOURING_REPO}/main/scripts/install.sh | sh
#   TOURING_VERSION=v30.0.0 ./install.sh        # pin a version
#   TOURING_PREFIX=~/bin ./install.sh           # custom install dir
#
# Downloads the released binary bundle (touring + touring-daemon + touring-hook)
# for the detected OS/arch, verifies its SHA256 against the published checksum
# (and its cosign/sigstore keyless signature when `cosign` is installed —
# TOURING_REQUIRE_COSIGN=1 to enforce), installs into ${TOURING_PREFIX:-~/.local/bin},
# and smoke-tests `touring --help`.
set -euo pipefail

REPO="${TOURING_REPO:-gabrielgadea/touring}"
VERSION="${TOURING_VERSION:-latest}"
PREFIX="${TOURING_PREFIX:-${HOME}/.local/bin}"
BASE="https://github.com/${REPO}/releases"

log()  { printf '\033[1;34m[touring-install]\033[0m %s\n' "$*" >&2; }
fail() { printf '\033[1;31m[touring-install] ERROR:\033[0m %s\n' "$*" >&2; exit 1; }

# ── 1. Detect platform ────────────────────────────────────────────────────
os="$(uname -s)"
arch="$(uname -m)"
case "${os}-${arch}" in
  Linux-x86_64)   target="x86_64-unknown-linux-musl" ;;
  Darwin-arm64)   target="aarch64-apple-darwin" ;;
  Darwin-aarch64) target="aarch64-apple-darwin" ;;
  *) fail "unsupported platform: ${os}/${arch} (supported: Linux x86_64, macOS arm64)" ;;
esac
archive="touring-${target}.tar.gz"
log "platform: ${os}/${arch} -> ${target}"

# ── 2. Resolve download URL ───────────────────────────────────────────────
if [ "${VERSION}" = "latest" ]; then
  url="${BASE}/latest/download/${archive}"
else
  url="${BASE}/download/${VERSION}/${archive}"
fi

# ── 3. Download + verify checksum ─────────────────────────────────────────
command -v curl >/dev/null 2>&1 || fail "curl is required"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

log "downloading ${url}"
curl -fSL --retry 3 -o "${tmp}/${archive}" "${url}" \
  || fail "download failed — check TOURING_REPO=${REPO} / TOURING_VERSION=${VERSION}"
curl -fSL --retry 3 -o "${tmp}/${archive}.sha256" "${url}.sha256" \
  || fail "checksum download failed"

log "verifying SHA256"
if command -v shasum >/dev/null 2>&1; then
  (cd "${tmp}" && awk '{print $1}' "${archive}.sha256" | sed "s|\$| ${archive}|" | shasum -a 256 -c -) \
    || fail "SHA256 mismatch — refusing to install"
elif command -v sha256sum >/dev/null 2>&1; then
  (cd "${tmp}" && awk '{print $1}' "${archive}.sha256" | sed "s|\$|  ${archive}|" | sha256sum -c -) \
    || fail "SHA256 mismatch — refusing to install"
else
  fail "neither shasum nor sha256sum found"
fi

# ── 3b. Verify cosign signature (CICD-08 — tamper-proof installer) ─────────
# Release archives are signed keyless via sigstore (.github/workflows/release.yml):
# the `.sig` + `.pem` attest the GitHub Actions release-workflow identity. Best-effort
# by default (verifies when `cosign` is installed); set TOURING_REQUIRE_COSIGN=1 to
# hard-fail when cosign is absent or verification fails. SHA256 above is always enforced.
if command -v cosign >/dev/null 2>&1; then
  if curl -fSL --retry 3 -o "${tmp}/${archive}.sig" "${url}.sig" \
     && curl -fSL --retry 3 -o "${tmp}/${archive}.pem" "${url}.pem"; then
    log "verifying cosign signature (sigstore keyless)"
    cosign verify-blob \
      --certificate "${tmp}/${archive}.pem" \
      --signature "${tmp}/${archive}.sig" \
      --certificate-identity-regexp "^https://github.com/${REPO}/" \
      --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
      "${tmp}/${archive}" \
      || fail "cosign signature verification FAILED — refusing to install"
    log "cosign signature OK"
  elif [ "${TOURING_REQUIRE_COSIGN:-0}" = "1" ]; then
    fail "signature artifacts (.sig/.pem) unavailable but TOURING_REQUIRE_COSIGN=1"
  else
    log "WARN: signature artifacts unavailable — proceeding on SHA256 only"
  fi
elif [ "${TOURING_REQUIRE_COSIGN:-0}" = "1" ]; then
  fail "cosign not installed but TOURING_REQUIRE_COSIGN=1"
else
  log "NOTE: cosign not installed — skipping signature check (SHA256 enforced). Install cosign for full supply-chain verification."
fi

# ── 4. Install ────────────────────────────────────────────────────────────
mkdir -p "${PREFIX}"
tar -C "${tmp}" -xzf "${tmp}/${archive}"
for bin in touring touring-daemon touring-hook; do
  [ -f "${tmp}/${bin}" ] || fail "archive missing expected binary: ${bin}"
  install -m 0755 "${tmp}/${bin}" "${PREFIX}/${bin}"
done
log "installed touring + touring-daemon + touring-hook -> ${PREFIX}"

# ── 5. Smoke test ─────────────────────────────────────────────────────────
"${PREFIX}/touring" --help >/dev/null 2>&1 || fail "smoke test failed: ${PREFIX}/touring --help"
log "smoke OK — try: touring init --list-profiles && touring doctor -j"

case ":${PATH}:" in
  *":${PREFIX}:"*) ;;
  *) log "NOTE: ${PREFIX} is not on PATH — add: export PATH=\"${PREFIX}:\${PATH}\"" ;;
esac
