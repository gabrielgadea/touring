"""--stop-before-propagate: the pause that lets a project owner grant a window.

propagate-release.sh used to be one continuous pipeline: step 5 restarts the
per-project daemon of EVERY consumer, with no point where a human (or the other
project's coordinator) could say "now". The flag stops after gates/build/freeze/
default and leaves step 5 for an explicit second run.
"""
import os
import re
import subprocess
from pathlib import Path

# PROPAGATE_RELEASE_SCRIPT lets a mutation proof point the same tests at a mutated
# copy (it must live in scripts/ — the workspace root is derived from its path).
SCRIPT = Path(os.environ.get("PROPAGATE_RELEASE_SCRIPT")
              or Path(__file__).resolve().parent / "propagate-release.sh")
BASE = ["--dry-run", "--skip-build", "--skip-gates", "--skip-freeze"]


def run(*flags):
    env = dict(os.environ, NO_COLOR="1")
    return subprocess.run(
        [str(SCRIPT), "30.4.90", *flags], env=env, capture_output=True, text=True, timeout=120,
    )


def test_stop_flag_halts_before_step_5_and_names_the_second_run():
    r = run(*BASE, "--stop-before-propagate")
    assert r.returncode == 0, r.stderr
    out = r.stdout + r.stderr
    assert "5/6 PROPAGATE" not in out
    assert "PAUSA" in out
    # the message must teach the next command, not just stop
    assert "--skip-gates --skip-build --skip-freeze" in out


def test_without_the_flag_step_5_still_runs():
    out = (lambda r: r.stdout + r.stderr)(run(*BASE))
    assert "5/6 PROPAGATE" in out


def test_guard_sits_between_step_4_and_step_5_in_the_source():
    src = SCRIPT.read_text()
    default_at = src.index("4/6 DEFAULT")
    guard_at = src.index('"$STOP_BEFORE_PROPAGATE" -eq 1')
    step5_at = src.index("5/6 PROPAGATE")
    assert default_at < guard_at < step5_at


def test_flag_is_documented_in_the_usage_block():
    assert re.search(r"^#\s+scripts/propagate-release\.sh <versão> --stop-before-propagate", SCRIPT.read_text(), re.M)
