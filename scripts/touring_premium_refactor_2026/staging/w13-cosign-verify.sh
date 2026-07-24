#!/usr/bin/env bash
# AUTO-GENERATED — verify a Touring release artifact with cosign
# Usage: w13-cosign-verify.sh <artifact.tar.gz>

set -euo pipefail

readonly ARTIFACT="${1:?Usage: $0 <artifact.tar.gz>}"
readonly SIG="${ARTIFACT}.sig"
readonly CRT="${ARTIFACT}.crt"
readonly ISSUER="https://token.actions.githubusercontent.com"
readonly SUBJECT_REGEX="https://github.com/.*/touring/.github/workflows/sigstore-release.yml@refs/tags/v.*"

for f in "$ARTIFACT" "$SIG" "$CRT"; do
    [[ -f "$f" ]] || { echo "Missing: $f" >&2; exit 1; }
done

echo "==> Verifying $ARTIFACT (keyless cosign)"
cosign verify-blob \
    --certificate "$CRT" \
    --signature "$SIG" \
    --certificate-identity-regexp "$SUBJECT_REGEX" \
    --certificate-oidc-issuer "$ISSUER" \
    "$ARTIFACT"
echo "==> Verification PASSED"
