"""D3 (analise-92, 06/10/2026): session_env_setup.sh must not overwrite a per-project daemon socket.

Measured: the bash environ of an analise session started with `<analise>/.touring/daemon.sock` (from its
settings.local.json), and after sourcing CLAUDE_ENV_FILE `$TOURING_DAEMON_SOCKET` was the GLOBAL socket — CLI
calls from Bash went to the global daemon while hooks went to the project's: two daemons over one symbols.db.
The script now follows the same three layers as `TouringConfig::resolve_daemon_socket_path`
(touring-foundation/src/config/paths.rs): env override, walk-up (socket exists or `[daemon] per_project = true`),
global fallback.
"""
from __future__ import annotations

import os
import subprocess
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "session_env_setup.sh"
GLOBAL = f"/tmp/touring-daemon-{os.getuid()}.sock"


def _run(tmp_path: Path, cwd: Path, **env: str) -> dict[str, str]:
    env_file = tmp_path / "claude.env"
    env_file.write_text("export OTHER=1\n", encoding="utf-8")
    base = {k: v for k, v in os.environ.items()
            if k not in ("TOURING_DAEMON_SOCKET", "TOURING_DAEMON_SOCK", "TOURING_PROJECT_ROOT", "CLAUDE_PROJECT_DIR")}
    p = subprocess.run(["bash", str(SCRIPT)], input='{"session_id": "s-d3"}', capture_output=True, text=True,
                       cwd=cwd, env={**base, "CLAUDE_ENV_FILE": str(env_file), **env}, timeout=20, check=False)
    assert p.returncode == 0, p.stderr
    out = {}
    for line in env_file.read_text(encoding="utf-8").splitlines():
        if line.startswith("export "):
            k, _, v = line[len("export "):].partition("=")
            out[k] = v.strip('"')
    return out


def _project(root: Path, per_project: bool) -> Path:
    (root / ".touring").mkdir(parents=True)
    (root / ".touring" / "touring.toml").write_text(f"[daemon]\nper_project = {str(per_project).lower()}\n",
                                                    encoding="utf-8")
    sub = root / "a" / "b"
    sub.mkdir(parents=True)
    return sub


def test_an_inherited_socket_is_respected(tmp_path):
    out = _run(tmp_path, tmp_path, TOURING_DAEMON_SOCKET="/x/proj/.touring/daemon.sock")
    assert out["TOURING_DAEMON_SOCKET"] == "/x/proj/.touring/daemon.sock"
    assert out["OTHER"] == "1" and out["TOURING_SESSION_ID"] == "s-d3"


def test_per_project_opt_in_is_found_by_walking_up(tmp_path):
    root = tmp_path / "proj"
    sub = _project(root, per_project=True)
    out = _run(tmp_path, sub)
    assert out["TOURING_DAEMON_SOCKET"] == str(root / ".touring" / "daemon.sock")


def test_an_existing_project_socket_counts_without_opt_in(tmp_path):
    root = tmp_path / "proj"
    sub = _project(root, per_project=False)
    (root / ".touring" / "daemon.sock").write_text("", encoding="utf-8")  # existence is what the resolver reads
    out = _run(tmp_path, sub)
    assert out["TOURING_DAEMON_SOCKET"] == str(root / ".touring" / "daemon.sock")


def test_no_opt_in_and_no_socket_is_the_global_daemon(tmp_path):
    sub = _project(tmp_path / "proj", per_project=False)
    assert _run(tmp_path, sub)["TOURING_DAEMON_SOCKET"] == GLOBAL


def test_claude_project_dir_is_the_walk_start(tmp_path):
    root = tmp_path / "proj"
    _project(root, per_project=True)
    out = _run(tmp_path, tmp_path, CLAUDE_PROJECT_DIR=str(root))
    assert out["TOURING_DAEMON_SOCKET"] == str(root / ".touring" / "daemon.sock")


def test_an_inherited_project_root_is_respected(tmp_path):
    out = _run(tmp_path, tmp_path, TOURING_PROJECT_ROOT="/x/proj")
    assert out["TOURING_PROJECT_ROOT"] == "/x/proj"
    assert _run(tmp_path, tmp_path)["TOURING_PROJECT_ROOT"] == str(tmp_path)
