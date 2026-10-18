"""C3/C5 (ECC analysis, 17/10/2026): least privilege + SHA pins + CI-on-SHA gate in the GitHub workflows.

Usage: endurece_workflows.py <repo-root>   (idempotent: a second run changes nothing and says so)
"""
import re
import sys
from pathlib import Path

SHAS = {  # git ls-remote https://github.com/dtolnay/rust-toolchain refs/heads/<ref>, 17/10/2026
    "stable": "b725eda525362e7cf34af9a7243cac1e3f01a7ec",
    "nightly": "2c76c034f9e32a5d6e2c104dc4df23a22476db04",
    "1.95": "035fceb3c12fb0c6a9461b52775e16473133e591",
}
USES = re.compile(r"^(\s*-?\s*uses: dtolnay/rust-toolchain)@(stable|nightly|1\.95)\s*$", re.MULTILINE)

CI_PERMS_ANCHOR = "\n# NOTE: no global `RUSTFLAGS: -D warnings`"
CI_PERMS = (
    "\n# Least privilege (17/10/2026, ECC analysis C3): every job reads; `gates` declares its own\n"
    "# `contents: write` for the ARCHITECTURE.md auto-sync push.\n"
    "permissions:\n  contents: read\n"
)

REL_TOP_OLD = """permissions:
  contents: write
  id-token: write # CICD-07: cosign keyless (sigstore OIDC)
  attestations: write # CICD-07: SLSA build-provenance attestation
"""
REL_TOP_NEW = """# Least privilege (17/10/2026, ECC analysis C3): the workflow-level grant used to give `id-token: write`
# to EVERY job, including `build`, which restores a shared cache (Swatinem/rust-cache) — the cache-poisoning
# + OIDC class of the TanStack incident. Only `release` signs, so only `release` holds id-token/attestations.
permissions:
  contents: read
"""
REL_CHECKOUT_OLD = """    steps:
      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4

      - uses: dtolnay/rust-toolchain"""
REL_CHECKOUT_NEW = """    steps:
      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4
        with:
          persist-credentials: false

      - uses: dtolnay/rust-toolchain"""
REL_BUILD_OLD = """jobs:
  build:
    name: build (${{ matrix.target }})
    runs-on: ${{ matrix.os }}
"""
REL_BUILD_NEW = """jobs:
  # C5 (17/10/2026): pushing a v* tag used to build and publish whatever the tag pointed at — the gates of
  # scripts/propagate-release.sh run on the operator's machine, so a pushed tag skipped all of them. The
  # release now waits (bounded) for the `ci` workflow of this exact commit and refuses anything but success;
  # no completed run is a refusal, never a pass (absence is not success).
  verify:
    name: verify (ci green on this commit)
    runs-on: ubuntu-24.04
    timeout-minutes: 70
    permissions:
      contents: read
      actions: read
    steps:
      - name: ci must have succeeded on the released commit
        env:
          GH_TOKEN: ${{ github.token }}
          REPO: ${{ github.repository }}
          SHA: ${{ github.sha }}
        run: |
          set -euo pipefail
          deadline=$(( $(date +%s) + 60 * 60 ))
          while :; do
            runs=$(gh api "repos/${REPO}/actions/workflows/ci.yml/runs?head_sha=${SHA}&per_page=20" \\
              --jq '[.workflow_runs[] | {status, conclusion, event}]')
            echo "ci runs for ${SHA}: ${runs}"
            if jq -e 'any(.[]; .status == "completed" and .conclusion == "success")' <<<"${runs}" >/dev/null; then
              echo "ci succeeded on ${SHA}"
              break
            fi
            if jq -e 'length > 0 and all(.[]; .status == "completed")' <<<"${runs}" >/dev/null; then
              echo "::error::ci did not succeed on ${SHA} — refusing to release"
              exit 1
            fi
            if [ "$(date +%s)" -ge "${deadline}" ]; then
              echo "::error::no completed ci run for ${SHA} within 60 min — refusing to release"
              exit 1
            fi
            sleep 60
          done
      # Report-only: the only existing tag (v30.3.0) is annotated and unsigned, so requiring a signature
      # would block every release. Making it blocking is a policy decision, not a fix.
      - name: tag signature (report-only)
        if: startsWith(github.ref, 'refs/tags/')
        env:
          GH_TOKEN: ${{ github.token }}
          REPO: ${{ github.repository }}
        run: |
          set -euo pipefail
          obj=$(gh api "repos/${REPO}/git/ref/tags/${GITHUB_REF_NAME}" --jq '.object')
          if [ "$(jq -r .type <<<"${obj}")" != "tag" ]; then
            echo "::warning::${GITHUB_REF_NAME} is a lightweight tag: nothing to verify"
            exit 0
          fi
          verified=$(gh api "repos/${REPO}/git/tags/$(jq -r .sha <<<"${obj}")" --jq '.verification.verified')
          if [ "${verified}" = "true" ]; then
            echo "tag signature verified"
          else
            echo "::warning::${GITHUB_REF_NAME} is not signed/verified (report-only)"
          fi

  build:
    name: build (${{ matrix.target }})
    needs: verify
    runs-on: ${{ matrix.os }}
"""


def pin(text: str) -> tuple[str, int]:
    n = 0

    def rep(m: re.Match) -> str:
        nonlocal n
        n += 1
        return f"{m.group(1)}@{SHAS[m.group(2)]} # {m.group(2)}"

    return USES.sub(rep, text), n


def troca(text: str, old: str, new: str, nome: str) -> str:
    if new in text:
        return text
    assert text.count(old) == 1, f"{nome}: âncora encontrada {text.count(old)}x"
    return text.replace(old, new)


def main() -> int:
    root = Path(sys.argv[1])
    wf = root / ".github/workflows"
    resumo = []
    for nome in ("ci.yml", "release.yml", "release-plz.yml", "docs-rs-mirror.yml"):
        p = wf / nome
        t0 = p.read_text(encoding="utf-8")
        t, n = pin(t0)
        if nome == "ci.yml" and "\npermissions:\n  contents: read\n" not in t:
            t = troca(t, CI_PERMS_ANCHOR, CI_PERMS + CI_PERMS_ANCHOR, "ci perms")
        if nome == "release.yml":
            t = troca(t, REL_TOP_OLD, REL_TOP_NEW, "release top perms")
            t = troca(t, REL_CHECKOUT_OLD, REL_CHECKOUT_NEW, "release checkout")
            t = troca(t, REL_BUILD_OLD, REL_BUILD_NEW, "release verify job")
        if t != t0:
            p.write_text(t, encoding="utf-8")
        resumo.append(f"{nome}: pins={n} mudou={t != t0}")
    print(" · ".join(resumo))
    return 0


if __name__ == "__main__":
    sys.exit(main())
