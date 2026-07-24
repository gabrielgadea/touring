#!/usr/bin/env bash
# package_release.sh — build a Touring toolchain tarball from a built workspace
# (Pln2 F5). Produces the SAME artifact shape as .github/workflows/release.yml:
# `touring-<triple>.tar.gz` containing `bin/{touring,touring-daemon,touring-hook}`
# (+ touring-quality when built), plus a sibling `.sha256`.
#
# This is the local/manual half of the release pipeline: the CI job packages
# tagged releases; this script packages the CURRENT target/release for offline
# installs (`install.touring.dev.sh --from-tarball`) and smoke tests.
#
# Usage: package_release.sh <version> [--workspace <dir>] [--out <dir>]
set -euo pipefail

VERSION="${1:?usage: package_release.sh <version> [--workspace <dir>] [--out <dir>]}"
shift
WORKSPACE="$HOME/projects/touring"
OUT_DIR="$PWD"
while [ $# -gt 0 ]; do
  case "$1" in
    --workspace) WORKSPACE="$2"; shift 2 ;;
    --out) OUT_DIR="$2"; shift 2 ;;
    *) echo "package_release.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

RELEASE="$WORKSPACE/target/release"
for bin in touring touring-daemon touring-hook; do
  [ -x "$RELEASE/$bin" ] || {
    echo "package_release.sh: missing $RELEASE/$bin — build first (cargo build --release)" >&2
    exit 1
  }
done

OS_NAME=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "${OS_NAME}-${ARCH}" in
  linux-x86_64)   TRIPLE="x86_64-unknown-linux-gnu" ;;
  darwin-arm64|darwin-aarch64) TRIPLE="aarch64-apple-darwin" ;;
  *) TRIPLE="${ARCH}-${OS_NAME}" ;;
esac

STAGE=$(mktemp -d "${TMPDIR:-/tmp}/touring-pkg.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT INT TERM
mkdir -p "$STAGE/bin"
cp "$RELEASE/touring" "$RELEASE/touring-daemon" "$RELEASE/touring-hook" "$STAGE/bin/"
# Known optional component — included when the workspace built it.
[ -x "$RELEASE/touring-quality" ] && cp "$RELEASE/touring-quality" "$STAGE/bin/"

ARCHIVE="$OUT_DIR/touring-${TRIPLE}.tar.gz"
mkdir -p "$OUT_DIR"
tar -C "$STAGE" -czf "$ARCHIVE" bin
sha256sum "$ARCHIVE" > "$ARCHIVE.sha256"

echo "packaged: $ARCHIVE ($(du -h "$ARCHIVE" | cut -f1)) version=$VERSION"
echo "checksum: $(awk '{print $1}' "$ARCHIVE.sha256")"
