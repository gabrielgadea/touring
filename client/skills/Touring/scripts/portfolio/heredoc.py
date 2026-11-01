#!/usr/bin/env python3
# #tags: kind:script purpose:gate domain:code-mode process:measure lang:python
"""heredoc.py — the ONE parser of scripts written through a Bash ``cat`` heredoc (P3, S-3.1).

Two forms, the ones the 27/10/2026 inventory measured (``inventario_scripts_sessoes.py``, which imports this
module): ``cat > p <<'EOF'`` and ``cat <<'EOF' > p``, quoted or bare tag, optional ``cat`` flags, body up to the
line that holds only the tag. Anything else — ``tee``, ``printf >``, ``python - <<``, ``>>`` appends, ``<<-``
tab-stripped bodies — is OUTSIDE the declared universe on purpose: a form is added only after it is measured,
never guessed, so the hook and the inventory always count the same thing.

Used by: ``hooks/script_portfolio_gate.py`` (Bash matcher, P3) and the analise inventory.

    python3 heredoc.py < comando.txt      # prints [[path, n_bytes], …]
"""

from __future__ import annotations

import json
import re
import sys

HEREDOC_RES = (
    re.compile(
        r"cat(?:\s+-\w+)*\s*>\s*(?P<path>[^\s<>|]+)\s*<<\s*['\"]?(?P<tag>[A-Za-z_]\w*)['\"]?\s*\n"
        r"(?P<content>.*?)(?:\n(?P=tag)(?:\n|$))",
        re.DOTALL,
    ),
    re.compile(
        r"cat(?:\s+-\w+)*\s*<<\s*['\"]?(?P<tag>[A-Za-z_]\w*)['\"]?\s*>\s*(?P<path>[^\s<>|]+)\s*\n"
        r"(?P<content>.*?)(?:\n(?P=tag)(?:\n|$))",
        re.DOTALL,
    ),
)


def extrair(comando: str) -> list[tuple[str, str]]:
    """(path, content) of every heredoc in the two declared forms; [] when the command has none.

    The ``"<<"`` pre-check keeps the common case (a Bash command with no heredoc) at substring cost: the hook
    runs on every Bash call.
    """
    if "<<" not in comando or "cat" not in comando:
        return []
    return [(m.group("path"), m.group("content")) for rx in HEREDOC_RES for m in rx.finditer(comando)]


def main() -> int:
    print(json.dumps([[p, len(c.encode("utf-8"))] for p, c in extrair(sys.stdin.read())], ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
