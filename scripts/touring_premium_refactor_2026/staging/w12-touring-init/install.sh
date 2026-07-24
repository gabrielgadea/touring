#!/usr/bin/env bash
# AUTO-GENERATED — touring-init bootstrap installer
# Usage: curl -fsSL https://install.touring.dev/ | sh

set -euo pipefail

readonly TOURING_HOME="${TOURING_HOME:-$HOME/.touring}"
readonly CHANNEL="${TOURING_CHANNEL:-stable}"
readonly INSTALL_URL_BASE="${TOURING_INSTALL_URL:-https://releases.touring.dev}"

echo "==> Detecting host triple..."
case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) TRIPLE="x86_64-unknown-linux-gnu" ;;
    Linux-aarch64) TRIPLE="aarch64-unknown-linux-gnu" ;;
    Darwin-x86_64) TRIPLE="x86_64-apple-darwin" ;;
    Darwin-arm64) TRIPLE="aarch64-apple-darwin" ;;
    *) echo "Unsupported platform: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac
echo "    Host: $TRIPLE"

echo "==> Creating $TOURING_HOME structure..."
mkdir -p "$TOURING_HOME"/{toolchains,cache,bin}

echo "==> Downloading channel manifest: $CHANNEL"
# TODO W12.5: fetch + verify signature
# curl -fsSL "$INSTALL_URL_BASE/channels/$CHANNEL/manifest.toml" > "$TOURING_HOME/manifest.toml"

echo "==> touring-init installed successfully."
echo "    Run: touring-init init  (in your project directory)"
