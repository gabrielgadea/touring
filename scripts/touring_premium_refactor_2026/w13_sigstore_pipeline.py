#!/usr/bin/env python3
"""W13.6 — Configure cosign sign-blob pipeline for release tarballs.

Emits:
  - .github/workflows/sigstore-release.yml (cosign + OIDC keyless signing)
  - staging/w13-cosign-verify.sh (consumer verification script)
  - data/w13-sigstore-manifest.json
"""
from __future__ import annotations

from pathlib import Path

import script_common as sc

_SCRIPT = "w13_sigstore_pipeline"

_ROOT, _DATA_DIR, _STAGING_DIR = sc.ROOT, sc.DATA_DIR, sc.STAGING_DIR

ARTIFACT_GLOB = "release/*.tar.gz"
ISSUER = "https://token.actions.githubusercontent.com"
TARGETS = [
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
]


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    return sc.with_output_dir_and_verbose(p, _DATA_DIR)


def emit_workflow() -> Path:
    """Emit GitHub Actions workflow for keyless signing."""
    workflow_dir = _STAGING_DIR / "w13-github-workflows"
    workflow_dir.mkdir(parents=True, exist_ok=True)
    yml = workflow_dir / "sigstore-release.yml"
    yml.write_text("""# AUTO-GENERATED — cosign sigstore keyless signing pipeline
name: Sigstore Release Signing

on:
  release:
    types: [created]
  workflow_dispatch:

permissions:
  id-token: write   # OIDC token for keyless signing
  contents: write   # upload signed artifacts

jobs:
  sign-artifacts:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        target:
          - x86_64-unknown-linux-gnu
          - aarch64-unknown-linux-gnu
          - x86_64-apple-darwin
          - aarch64-apple-darwin

    steps:
      - uses: actions/checkout@v4
      - name: Install cosign
        uses: sigstore/cosign-installer@v3
      - name: Download release artifact
        run: |
          gh release download "${{ github.event.release.tag_name }}" \\
            --pattern "*-${{ matrix.target }}.tar.gz" \\
            --dir release/
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}

      - name: Sign artifact (keyless OIDC)
        run: |
          for artifact in release/*-${{ matrix.target }}.tar.gz; do
            cosign sign-blob \\
              --yes \\
              --output-signature "${artifact}.sig" \\
              --output-certificate "${artifact}.crt" \\
              "${artifact}"
          done

      - name: Upload signatures to release
        run: |
          gh release upload "${{ github.event.release.tag_name }}" \\
            release/*-${{ matrix.target }}.tar.gz.sig \\
            release/*-${{ matrix.target }}.tar.gz.crt
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
""", encoding="utf-8")
    return yml


def emit_verify_script() -> Path:
    """Emit consumer verification shell script."""
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    sh = _STAGING_DIR / "w13-cosign-verify.sh"
    sh.write_text("""#!/usr/bin/env bash
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
cosign verify-blob \\
    --certificate "$CRT" \\
    --signature "$SIG" \\
    --certificate-identity-regexp "$SUBJECT_REGEX" \\
    --certificate-oidc-issuer "$ISSUER" \\
    "$ARTIFACT"
echo "==> Verification PASSED"
""", encoding="utf-8")
    sh.chmod(0o755)
    return sh


def run(args: sc.Namespace) -> dict:
    """Emit pipeline artifacts."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    workflow_path = emit_workflow()
    verify_script = emit_verify_script()
    manifest = {
        "script": "w13_sigstore_pipeline",
        "wave": "W13", "subtask_refs": ["W13.6"],
        "timestamp": sc.now_iso(),
        "status": "OK",
        "issuer": ISSUER,
        "targets": TARGETS,
        "artifact_glob": ARTIFACT_GLOB,
        "workflow_path": str(workflow_path.relative_to(_ROOT)),
        "verify_script": str(verify_script.relative_to(_ROOT)),
    }
    json_path = args.output_dir / "w13-sigstore-manifest.json"
    return sc.write_json_report(json_path, manifest, _ROOT)


def _run_and_report(args: sc.Namespace) -> int:
    """Run w13_sigstore_pipeline, print its summary; the return value is the exit code."""
    result = run(args)
    sc.emit_json(result)
    return 0


def main() -> int:
    """Entry point of w13_sigstore_pipeline."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
