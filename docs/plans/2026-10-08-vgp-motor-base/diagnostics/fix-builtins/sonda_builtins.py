#!/usr/bin/env python3
"""Run the current VGP binary on the four synthetic builtin fixtures."""

import json
import subprocess
from pathlib import Path

AQUI = Path(__file__).resolve().parent
BIN = Path.home() / ".claude/touring/vgp/binarios/a6b9a550/touring"


def main() -> None:
    out = {}
    for fixture in sorted((AQUI / "synthetic").glob("*.py")):
        run = subprocess.run(
            [
                str(BIN),
                "vgp",
                "check",
                "--path",
                "<string>",
                "--content-file",
                str(fixture),
                "--project-root",
                "/home/gabrielgadea/projects/analise",
                "--lang",
                "python",
                "-j",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        out[fixture.stem] = json.loads(run.stdout)
    print(json.dumps(out, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
