#!/usr/bin/env python3
"""Fallback ISO 9660 (Joliet + Rock Ridge) builder for the cidata drive.

Used by `cidata_build.sh` only when none of genisoimage / mkisofs / xorrisofs is
installed (the Pop!_OS 24.04 base has none of them; `pip install --user pycdlib`
needs no root). The on-disk contract is the same as the shell tools produce:
volume id `cidata`, every file of the staging directory at the root.

`pycdlib` is the ONE external dependency in this tree, imported lazily so the
rest of the scripts stay stdlib-only; without it this script fails closed with
`FAIL missing-tool pycdlib` (exit 2) and the caller reports that.

Usage: cidata_iso.py <staging-dir> <out.iso>
"""

from __future__ import annotations

import sys
from pathlib import Path


def build(staging: Path, out: Path) -> int:
    try:
        import pycdlib  # type: ignore[import-not-found]
    except ModuleNotFoundError:
        print("FAIL missing-tool pycdlib (pip install --user pycdlib)", file=sys.stderr)
        return 2
    iso = pycdlib.PyCdlib()
    iso.new(interchange_level=3, joliet=3, rock_ridge="1.09", vol_ident="cidata")
    for n, path in enumerate(sorted(p for p in staging.iterdir() if p.is_file())):
        # ISO 9660 level 3 names are 8.3 uppercase; Joliet and Rock Ridge carry the
        # real name, which is what the installer reads.
        iso.add_file(
            str(path),
            f"/F{n:06d}.;1",
            joliet_path=f"/{path.name}",
            rr_name=path.name,
        )
    iso.write(str(out))
    iso.close()
    return 0


def main(argv: list[str]) -> int:
    if len(argv) != 3 or argv[1] in ("-h", "--help"):
        print(__doc__.strip())
        return 0 if len(argv) == 2 else 1
    staging, out = Path(argv[1]), Path(argv[2])
    if not staging.is_dir():
        print(f"FAIL staging dir not found: {staging}", file=sys.stderr)
        return 1
    return build(staging, out)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
