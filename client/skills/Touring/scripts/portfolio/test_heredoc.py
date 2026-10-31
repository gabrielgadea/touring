"""The single heredoc parser: both declared forms parse, undeclared forms stay outside (P3, S-3.1)."""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

AQUI = Path(__file__).resolve().parent
sys.path.insert(0, str(AQUI))

import heredoc

CORPO = "import sys\n\nprint(sys.argv)\n"


@pytest.mark.parametrize(
    "comando",
    [
        f"cat > /tmp/x/probe.py <<'PY'\n{CORPO}PY\n",
        f"cat <<'PY' > /tmp/x/probe.py\n{CORPO}PY\n",
        f'cat > /tmp/x/probe.py << "PY"\n{CORPO}PY',
        f"cd /repo && cat >/tmp/x/probe.py <<PY\n{CORPO}PY\npython3 /tmp/x/probe.py",
    ],
)
def test_heredoc_duas_formas(comando: str) -> None:
    assert heredoc.extrair(comando) == [("/tmp/x/probe.py", CORPO.rstrip("\n"))]


def test_dois_heredocs_no_mesmo_comando() -> None:
    comando = f"cat > a.py <<'A'\n{CORPO}A\ncat <<'B' > b.sh\necho oi\nB\n"
    assert sorted(p for p, _ in heredoc.extrair(comando)) == ["a.py", "b.sh"]


@pytest.mark.parametrize(
    "comando",
    [
        "ls -la /tmp",
        f"tee /tmp/x/probe.py <<'PY'\n{CORPO}PY\n",
        f"python3 - <<'PY'\n{CORPO}PY\n",
        f"cat >> /tmp/x/probe.py <<'PY'\n{CORPO}PY\n",
        "printf 'x' > /tmp/x/probe.py",
        f"cat > /tmp/x/probe.py <<'PY'\n{CORPO}",  # unterminated: no closing tag
    ],
)
def test_heredoc_desconhecido_nao_casa(comando: str) -> None:
    assert heredoc.extrair(comando) == []


def test_cli(capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch) -> None:
    import io

    monkeypatch.setattr(sys, "stdin", io.StringIO(f"cat > p.py <<'E'\n{CORPO}E\n"))
    assert heredoc.main() == 0
    assert capsys.readouterr().out.strip() == f'[["p.py", {len(CORPO.rstrip(chr(10)))}]]'
