"""propagate-release.sh never inherits the Claude Code session's daemon socket.

CC sessions export TOURING_DAEMON_SOCKET pointing at the PER-PROJECT socket of the
project they have open. On 09/10/2026 the 30.4.94 propagation ran from a session in
~/projects/analise: step 5.5 (behavioral proof) asked the hook about
~/projects/touring, the analise daemon refused ("this daemon is pinned to
.../analise"), the hook answered `{}` and the proof failed 37/40 twice — while the
same proof with the variable unset passed 40/40 against the same binary. The
update-touring fix of 08/10 (S8, test_update_touring.py) scrubbed the variable in
that script only; this one, which calls it, kept inheriting.

Harness = the config prelude plus the calls under test, NEVER the whole script: the
full dispatch operates on the machine's real daemons (S8 incident, 08/10/2026: a
stubbed end-to-end run killed the real global daemon).
"""

import os
import re
import subprocess
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "propagate-release.sh"
SCRUB = "unset TOURING_DAEMON_SOCKET TOURING_DAEMON_SOCK"

ENV_STUB = """#!/usr/bin/env bash
{
  printf '%s argv=%s\\n' "$(basename "$0")" "$*"
  printf 'TOURING_DAEMON_SOCKET=%s\\n' "${TOURING_DAEMON_SOCKET-<absent>}"
  printf 'TOURING_DAEMON_SOCK=%s\\n' "${TOURING_DAEMON_SOCK-<absent>}"
} >> "${PR_DIR}/calls"
exit 0
"""


def script_prelude() -> str:
    """Everything before the first function definition — the config block that
    runs before every step, where the scrub must live."""
    lines = SCRIPT.read_text(encoding="utf-8").splitlines(keepends=True)
    cut = next(
        i
        for i, line in enumerate(lines)
        if re.match(r"^[a-z_][a-z_0-9]*\(\)\s*\{", line)
    )
    return "".join(lines[:cut])


def test_the_session_socket_env_is_scrubbed_in_the_prelude():
    """The unset lives in the prelude (whole process), so `touring doctor`, the
    `touring update` calls and the python proof all start from a clean env."""
    prelude = script_prelude()
    assert SCRUB in prelude, (
        "propagate-release.sh must unset TOURING_DAEMON_SOCKET TOURING_DAEMON_SOCK in its "
        "prelude: a CC session exports its per-project socket and every step here must "
        "resolve the daemon by walk-up from the workspace, not by the caller's env"
    )
    assert prelude.index("set -euo pipefail") < prelude.index(SCRUB)


def test_daemon_and_proof_calls_never_inherit_the_session_socket_env(tmp_path):
    """With both variables pointing at a decoy, the stubs of `touring` and
    `python3` (the two binaries the steps invoke by PATH) record their own env;
    the real prelude is what is under test."""
    stubs = tmp_path / "bin"
    stubs.mkdir()
    for name in ("touring", "python3"):
        stub = stubs / name
        stub.write_text(ENV_STUB)
        stub.chmod(0o755)
    harness = tmp_path / "harness.sh"
    harness.write_text(
        "\n".join(
            [script_prelude(), "touring doctor -j", 'python3 "$WORKSPACE/prova.py"']
        )
    )
    decoy = str(tmp_path / "decoy.sock")
    env = {
        **os.environ,
        "PATH": f"{stubs}:{os.environ['PATH']}",
        "TOURING_DAEMON_SOCKET": decoy,
        "TOURING_DAEMON_SOCK": decoy,
        "PR_DIR": str(tmp_path),
    }
    # Via file, not `bash -c`: the prelude reads ${BASH_SOURCE[0]}, unbound under -c + set -u.
    proc = subprocess.run(
        ["bash", str(harness)], env=env, capture_output=True, text=True, timeout=30
    )
    assert proc.returncode == 0, proc.stderr
    calls_file = tmp_path / "calls"
    assert calls_file.exists(), "positive control: the stubs were never called"
    calls = calls_file.read_text()
    assert "touring argv=doctor -j" in calls and "python3 argv=" in calls, calls
    assert "decoy.sock" not in calls, (
        f"the steps inherited TOURING_DAEMON_SOCKET/SOCK from the session:\n{calls}"
    )
