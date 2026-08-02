#!/usr/bin/env sh
# install.touring.dev — Touring rustup-pattern installer (Pln2 F5, 2026-07-24).
#
# Security model (rustup-init.sh inspired):
#   - TLS 1.2 pin via curl --proto '=https' --tlsv1.2 (https URLs)
#   - SHA-256 verification: MANDATORY on the production channel; automatic for
#     --from-url / --from-tarball whenever a sibling .sha256 exists
#   - Sigstore verification: runs when `cosign` is installed AND a .sigstore
#     bundle is published; absence downgrades to a loud warning until GA
#   - No sudo — installs under $TOURING_HOME (default ~/.touring)
#   - PATH modification opt-in (--no-modify-path skips)
#
# Sources (exactly one):
#   (default)             GitHub Releases —
#                         ${TOURING_RELEASES_BASE}/v<version>/touring-<triple>.tar.gz
#   --from-url <url>      any tarball URL (https:// or file://)
#   --from-tarball <path> local tarball, fully offline
#
# The tarball layout is the toolchain shape: `bin/{touring,touring-daemon,
# touring-hook}` at the archive root (release.yml packages it this way), so it
# extracts 1:1 into ~/.touring/toolchains/<version>/.
#
# Usage:
#   curl --proto '=https' --tlsv1.2 -sSfL \
#     https://raw.githubusercontent.com/gabrielgadea/touring/main/scripts/packaging/install.touring.dev.sh \
#     | sh -s -- --version 30.3.0
#   sh install.touring.dev.sh --version 30.3.0 --dry-run
#   sh install.touring.dev.sh --version 30.3.0 --from-tarball ./touring.tar.gz

set -eu

# ── Constants ─────────────────────────────────────────────────────────────
TOURING_VERSION="${TOURING_VERSION:-latest}"
# GitHub Releases is the canonical distribution channel: the assets are
# published there with a sibling .sha256 and a CycloneDX SBOM, and they are
# reachable anonymously. Override with TOURING_RELEASES_BASE to mirror them.
RELEASES_BASE="${TOURING_RELEASES_BASE:-https://github.com/gabrielgadea/touring/releases/download}"
TOURING_HOME_DEFAULT="${TOURING_HOME:-$HOME/.touring}"

# ── Flags ─────────────────────────────────────────────────────────────────
DRY_RUN=0
NO_MODIFY_PATH=0
SET_DEFAULT=auto    # auto = set only when no default exists yet
PROFILE="default"   # minimal | default | complete
FROM_URL=""
FROM_TARBALL=""

while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run) DRY_RUN=1 ;;
        --no-modify-path) NO_MODIFY_PATH=1 ;;
        --set-default) SET_DEFAULT=yes ;;
        --version) TOURING_VERSION="$2"; shift ;;
        --profile) PROFILE="$2"; shift ;;
        --toolchain-home) TOURING_HOME_DEFAULT="$2"; shift ;;
        --from-url) FROM_URL="$2"; shift ;;
        --from-tarball) FROM_TARBALL="$2"; shift ;;
        -h|--help)
            cat <<EOF
install.touring.dev — Touring per-project rustup-pattern installer

USAGE:
    install.touring.dev [--dry-run] [--version <VER>] [--profile <P>]
                        [--from-url <URL> | --from-tarball <PATH>]
                        [--no-modify-path] [--set-default] [--toolchain-home <DIR>]

FLAGS:
    --dry-run             Print the install plan, mutate nothing
    --version <VER>       Touring version to install
    --from-url <URL>      Install from a tarball URL (https:// or file://)
    --from-tarball <P>    Install from a local tarball (offline)
    --profile <P>         Component profile: minimal | default | complete
    --set-default         Make this version the default even if one exists
    --no-modify-path      Skip ~/.bashrc / ~/.zshrc PATH modification
    --toolchain-home <D>  Use <D> instead of ~/.touring/

ENV:
    TOURING_VERSION         Same as --version
    TOURING_HOME            Same as --toolchain-home
    TOURING_RELEASES_BASE   Release artifact base URL
                            (default: GitHub Releases for gabrielgadea/touring)
EOF
            exit 0
            ;;
        *)
            printf 'install.touring.dev: unknown flag "%s" (try --help)\n' "$1" >&2
            exit 1
            ;;
    esac
    shift
done

if [ -n "$FROM_URL" ] && [ -n "$FROM_TARBALL" ]; then
    printf 'install.touring.dev: --from-url and --from-tarball are mutually exclusive\n' >&2
    exit 1
fi

# ── OS / arch detection → CI target triple ────────────────────────────────
OS_NAME=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$ARCH" in
    x86_64|amd64)  ARCH=x86_64 ;;
    aarch64|arm64) ARCH=aarch64 ;;
    *)
        printf 'install.touring.dev: unsupported arch "%s"\n' "$ARCH" >&2
        exit 1
        ;;
esac
# Must mirror the release.yml build matrix exactly (W14 expands to musl/darwin).
case "${OS_NAME}-${ARCH}" in
    linux-x86_64)   TARGET_TRIPLE="x86_64-unknown-linux-gnu" ;;
    *)
        printf 'install.touring.dev: no release artifact for "%s-%s" yet (current matrix: linux-x86_64; macOS/musl land with W14)\n' "$OS_NAME" "$ARCH" >&2
        exit 1
        ;;
esac

# ── Source resolution ─────────────────────────────────────────────────────
RELEASE_CHANNEL=0
if [ -n "$FROM_TARBALL" ]; then
    SOURCE_DESC="local tarball ${FROM_TARBALL}"
elif [ -n "$FROM_URL" ]; then
    SOURCE_DESC="url ${FROM_URL}"
else
    if [ "$TOURING_VERSION" = "latest" ]; then
        printf 'install.touring.dev: --version <VER> is required on the production channel until a latest-resolver ships (F5 GA)\n' >&2
        exit 1
    fi
    RELEASE_CHANNEL=1
    # GitHub release tags carry a leading "v" (v30.3.0) while toolchain
    # directories do not (~/.touring/toolchains/30.3.0). Accept either input
    # form and normalise both spellings here.
    case "$TOURING_VERSION" in
        v*) RELEASE_TAG="$TOURING_VERSION"; TOURING_VERSION="${TOURING_VERSION#v}" ;;
        *)  RELEASE_TAG="v${TOURING_VERSION}" ;;
    esac
    FROM_URL="${RELEASES_BASE}/${RELEASE_TAG}/touring-${TARGET_TRIPLE}.tar.gz"
    SOURCE_DESC="release channel ${FROM_URL}"
fi
TOOLCHAIN_DIR="${TOURING_HOME_DEFAULT}/toolchains/${TOURING_VERSION}"

printf 'install.touring.dev — Touring installer\n'
printf '  OS/Arch/Triple  : %s / %s / %s\n' "$OS_NAME" "$ARCH" "$TARGET_TRIPLE"
printf '  Version         : %s\n' "$TOURING_VERSION"
printf '  Source          : %s\n' "$SOURCE_DESC"
printf '  Profile         : %s\n' "$PROFILE"
printf '  TOURING_HOME    : %s\n' "$TOURING_HOME_DEFAULT"
printf '  Toolchain dir   : %s\n' "$TOOLCHAIN_DIR"
printf '  Modify PATH     : %s\n' "$([ "$NO_MODIFY_PATH" = "1" ] && echo no || echo yes)"

if [ "$DRY_RUN" = "1" ]; then
    printf '\n[DRY-RUN] Plan: fetch -> sha256 verify -> (cosign verify) -> extract into %s -> default -> PATH. Nothing was executed.\n' "$TOOLCHAIN_DIR"
    exit 0
fi

# ── Fetch ─────────────────────────────────────────────────────────────────
WORK=$(mktemp -d "${TMPDIR:-/tmp}/touring-install.XXXXXX")
trap 'rm -rf "$WORK"' EXIT INT TERM
TARBALL="$WORK/touring.tar.gz"

fetch() { # $1 url, $2 dest — TLS-pinned for https, plain for file://
    case "$1" in
        https://*) curl --proto '=https' --tlsv1.2 -sSfL "$1" -o "$2" ;;
        file://*)  curl -sSfL "$1" -o "$2" ;;
        *) printf 'install.touring.dev: refusing non-https/file URL: %s\n' "$1" >&2; return 1 ;;
    esac
}

if [ -n "$FROM_TARBALL" ]; then
    [ -f "$FROM_TARBALL" ] || { printf 'install.touring.dev: tarball not found: %s\n' "$FROM_TARBALL" >&2; exit 1; }
    cp "$FROM_TARBALL" "$TARBALL"
    [ -f "${FROM_TARBALL}.sha256" ] && cp "${FROM_TARBALL}.sha256" "$TARBALL.sha256"
else
    printf 'Fetching %s\n' "$FROM_URL"
    fetch "$FROM_URL" "$TARBALL"
    fetch "${FROM_URL}.sha256" "$TARBALL.sha256" 2>/dev/null || rm -f "$TARBALL.sha256"
fi

# ── Verify: SHA-256 (mandatory on the release channel) ────────────────────
if [ -f "$TARBALL.sha256" ]; then
    EXPECTED=$(awk '{print $1}' "$TARBALL.sha256")
    ACTUAL=$(sha256sum "$TARBALL" | awk '{print $1}')
    if [ "$EXPECTED" != "$ACTUAL" ]; then
        printf 'install.touring.dev: SHA-256 MISMATCH — refusing to install.\n  expected %s\n  actual   %s\n' "$EXPECTED" "$ACTUAL" >&2
        exit 1
    fi
    printf 'SHA-256 verified: %s\n' "$ACTUAL"
elif [ "$RELEASE_CHANNEL" = "1" ]; then
    printf 'install.touring.dev: release channel requires a .sha256 next to the tarball — refusing.\n' >&2
    exit 1
else
    printf 'WARN: no .sha256 found next to the source — integrity NOT verified (local/dev install).\n' >&2
fi

# ── Verify: sigstore (cosign) — warns until GA publishes bundles ──────────
if command -v cosign >/dev/null 2>&1 && [ -z "$FROM_TARBALL" ]; then
    if fetch "${FROM_URL}.sigstore" "$TARBALL.sigstore" 2>/dev/null; then
        cosign verify-blob --bundle "$TARBALL.sigstore" "$TARBALL" \
            || { printf 'install.touring.dev: sigstore verification FAILED — refusing.\n' >&2; exit 1; }
        printf 'sigstore verified.\n'
    else
        printf 'WARN: no .sigstore bundle published for this artifact yet (pre-GA).\n' >&2
    fi
fi

# ── Extract into the toolchain layout ─────────────────────────────────────
if [ -d "$TOOLCHAIN_DIR" ]; then
    printf 'install.touring.dev: %s already installed — remove it or pick another --version.\n' "$TOOLCHAIN_DIR" >&2
    exit 1
fi
mkdir -p "$TOOLCHAIN_DIR"
tar -xzf "$TARBALL" -C "$TOOLCHAIN_DIR"
if [ ! -x "$TOOLCHAIN_DIR/bin/touring" ]; then
    rm -rf "$TOOLCHAIN_DIR"
    printf 'install.touring.dev: archive did not contain bin/touring — not a Touring toolchain tarball.\n' >&2
    exit 1
fi
printf 'version = "%s"\ninstalled_at = %s\nsource = "installer:%s"\n' \
    "$TOURING_VERSION" "$(date +%s)" "$SOURCE_DESC" > "$TOOLCHAIN_DIR/meta.toml"

# ── Default channel ───────────────────────────────────────────────────────
DEFAULT_FILE="${TOURING_HOME_DEFAULT}/default"
if [ "$SET_DEFAULT" = "yes" ] || [ ! -s "$DEFAULT_FILE" ]; then
    printf '%s\n' "$TOURING_VERSION" > "$DEFAULT_FILE"
    printf 'default -> %s\n' "$TOURING_VERSION"
fi

# ── PATH (opt-in) ─────────────────────────────────────────────────────────
PATH_LINE='export PATH="$HOME/.touring/toolchains/$(cat "$HOME/.touring/default" 2>/dev/null)/bin:$PATH"'
if [ "$NO_MODIFY_PATH" = "0" ]; then
    for rc in "$HOME/.bashrc" "$HOME/.zshrc"; do
        [ -f "$rc" ] || continue
        grep -qF '.touring/toolchains' "$rc" 2>/dev/null && continue
        printf '\n# Touring toolchain (install.touring.dev)\n%s\n' "$PATH_LINE" >> "$rc"
        printf 'PATH line appended to %s\n' "$rc"
    done
else
    printf 'PATH untouched (--no-modify-path). Add manually:\n  %s\n' "$PATH_LINE"
fi

printf '\nTouring %s installed at %s\n' "$TOURING_VERSION" "$TOOLCHAIN_DIR"
printf 'Next: `touring init-project` inside a project, or `touring toolchain list`.\n'
