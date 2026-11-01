#!/usr/bin/env python3
"""Mirror integrity AT HEAD (30.4.69, touring-36).

`sync-client-skills.py --check` reads the WORKING TREE — a mirror committed
with a stale manifest passed (measured 25/09: a commit landed 3 mirror files
and left the manifest ` M`; the lesson `espelho-commit-manifest-junto` was
written the same afternoon and recidivated within hours — a lesson written
holds nothing; the executor does). This checker reads BOTH sides from HEAD:
every manifest line's recorded digest must equal the blob's digest at
`HEAD:client/<path>`, and every file under `client/` at HEAD must be in the
manifest. Any divergence exits 1 naming the files.

Runs at gate 1/6 of `propagate-release.sh` (and in CI via its own test).
"""

from __future__ import annotations

import hashlib
import subprocess
import sys

MANIFEST_NAME = "MANIFEST" + ".sha256"


def _git(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", *args], capture_output=True)


def main() -> int:
    proc = _git("show", "HEAD:client/" + MANIFEST_NAME)
    if proc.returncode != 0:
        print("espelho sem manifesto no HEAD: client/" + MANIFEST_NAME + " ausente")
        return 1
    manifest: dict[str, str] = {}
    for line in proc.stdout.decode().splitlines():
        line = line.strip()
        if not line:
            continue
        sha, _, path = line.partition("  ")
        manifest[path] = sha

    proc = _git("ls-tree", "-r", "--name-only", "HEAD", "--", "client/")
    prefix = "client/"
    # The sweep is scoped to the subtrees the manifest DECLARES it covers
    # (measured on the real HEAD: `client/omarchy/` lives there but is not the
    # skills mirror — flagging it is a false positive, 90+ of them).
    declared_dirs = sorted({p.split("/", 1)[0] for p in manifest})
    declared = tuple(d + "/" for d in declared_dirs)
    files = [
        f[len(prefix):]
        for f in proc.stdout.decode().splitlines()
        if f
        and f != prefix + MANIFEST_NAME
        and f[len(prefix):].startswith(declared)
    ]

    problems: list[str] = []
    for path, recorded in manifest.items():
        blob = _git("show", "HEAD:client/" + path)
        if blob.returncode != 0:
            problems.append(f"{path}: no manifesto mas ausente no HEAD")
            continue
        actual = hashlib.sha256(blob.stdout).hexdigest()
        if actual != recorded:
            problems.append(f"{path}: hash diverge do HEAD (manifesto velho)")
    for f in files:
        if f not in manifest:
            problems.append(f"{f}: commitado sem entrada no manifesto")

    if problems:
        print("espelho client/ incoerente NO HEAD:")
        for pr in problems[:30]:
            print(f"  ✗ {pr}")
        if len(problems) > 30:
            print(f"  … e mais {len(problems) - 30}")
        print("remédio: python3 scripts/sync-client-skills.py --apply "
              "e commit do espelho COM o manifesto (viajam juntos)")
        return 1
    print(f"espelho no HEAD íntegro ({len(manifest)} arquivos conferidos)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
