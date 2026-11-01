"""Regressao: 20-touring-daemon deve sondar o PID, nao o exit code.

Medido no primeiro boot real pos-instalacao (2026-08-23): `touring daemon-ctl
status` retorna **exit 0 mesmo com o daemon morto** — imprime "socket: (dead)"
e "daemon PID: (none)" e sai com sucesso. O guard original

    if timeout 20 touring daemon-ctl status >/dev/null 2>&1; then exit 0; fi

portanto sempre saia por ali, tornando o `daemon-ctl restart` seguinte CODIGO
MORTO: o daemon nunca subia no boot.

Segunda armadilha, encontrada ao corrigir a primeira: o relatorio do `status`
sai inteiro em STDERR (stdout vazio), entao capturar so stdout faz o hook
religar o daemon a todo boot mesmo com ele vivo. Ambos os sentidos sao testados.
"""

from __future__ import annotations

import os
import subprocess
import tempfile
from pathlib import Path

HOOK = Path(__file__).resolve().parents[1] / "hooks" / "post-boot.d" / "20-touring-daemon"

ALIVE = """Touring daemon status
  socket:     /tmp/touring-daemon-1000.sock (alive)
  daemon PID: 31374
  exe:        /home/u/projects/touring/target/release/touring-daemon
"""

DEAD = """Touring daemon status
  socket:     /tmp/touring-daemon-1000.sock (dead)
  daemon PID: (none)
  exe:        (none)
"""


def _run_hook(status_report: str) -> list[str]:
    """Roda o hook com um `touring` stub e devolve os subcomandos invocados.

    O stub reproduz o contrato real: relatorio em STDERR e **exit 0 sempre**,
    inclusive quando o daemon esta morto.
    """
    with tempfile.TemporaryDirectory(prefix="daemon_hook_") as tmp:
        calls = Path(tmp) / "calls.log"
        stub = Path(tmp) / "touring"
        stub.write_text(
            "#!/usr/bin/env bash\n"
            f'echo "$*" >> {calls}\n'
            'if [[ "$*" == "daemon-ctl status" ]]; then\n'
            f"  cat >&2 <<'EOF'\n{status_report}EOF\n"
            "  exit 0\n"          # exit 0 MESMO morto — o contrato real
            "fi\n"
            "exit 0\n"
        )
        stub.chmod(0o755)
        env = dict(os.environ, PATH=f"{tmp}:{os.environ['PATH']}")
        proc = subprocess.run(["bash", str(HOOK)], env=env, capture_output=True, text=True)
        assert proc.returncode == 0, f"hook nunca pode bloquear o boot: {proc.stderr}"
        return calls.read_text().splitlines() if calls.exists() else []


def test_dead_daemon_triggers_restart():
    calls = _run_hook(DEAD)
    assert "daemon-ctl restart" in calls, (
        "daemon morto tem de religar; o guard por exit code tornava isto codigo morto"
    )


def test_alive_daemon_is_left_alone():
    calls = _run_hook(ALIVE)
    assert "daemon-ctl restart" not in calls, (
        "daemon vivo nao pode ser religado — sintoma de ler stdout em vez de stderr"
    )
