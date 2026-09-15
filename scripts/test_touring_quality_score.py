#!/usr/bin/env python3
"""Guards for `scripts/touring-quality-score` — the cache + lock wrapper that every
`touring-quality score` call on PATH goes through, the convergence judge included.

Until 14/09/2026 the wrapper lived only in ~/.local/bin. Its `flock -n` exited 1
with no output whenever another score was running, and the judge read that as
"quality unmeasured" (tier=None). These tests run the REAL script against a fake
engine, a temporary cache and a temporary lock, so every branch is exercised in CI
without the operator's machine; only the symlink checks need the live side.
"""

from __future__ import annotations

import fcntl
import os
import shutil
import stat
import subprocess
import threading
import time
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parent.parent
SCRIPT = REPO / "scripts" / "touring-quality-score"
INSTALLED = Path.home() / ".local" / "bin" / "touring-quality-score"

FAKE_ENGINE = """#!/usr/bin/env bash
# Records every invocation; behaviour chosen by FAKE_* variables.
echo "$*" >> "$FAKE_LOG"
[[ -n "${FAKE_SLEEP:-}" ]] && sleep "$FAKE_SLEEP"
# A descendant the engine leaves behind (a build server, a daemon).
if [[ -n "${FAKE_GRANDCHILD_SECS:-}" ]]; then
  setsid sleep "$FAKE_GRANDCHILD_SECS" </dev/null >/dev/null 2>&1 &
  echo $! > "$FAKE_LOG.grandchild"
fi
# `--output <file>` writes the report there and nothing on stdout.
prev=""
for a in "$@"; do
  if [[ "$prev" == "--output" ]]; then echo '{"tier": "Gold"}' > "$a"; exit "${FAKE_RC:-0}"; fi
  prev="$a"
done
if [[ "${FAKE_EMPTY:-0}" == "1" ]]; then exit "${FAKE_RC:-0}"; fi
echo "touring 30.4.46"
echo ""
echo '{"tier": "Gold", "composite": 0.9}'
exit "${FAKE_RC:-0}"
"""


def write_engine(path: Path) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(FAKE_ENGINE, encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IXUSR)
    return path


@pytest.fixture
def env(tmp_path: Path) -> dict:
    target = tmp_path / "target"
    target.mkdir()
    (target / "lib.rs").write_text("pub fn probe() {}\n", encoding="utf-8")
    engine = write_engine(tmp_path / "engine" / "touring-quality")
    return {
        "tmp": tmp_path,
        "target": target,
        "engine": engine,
        "log": tmp_path / "engine.log",
        "cache": tmp_path / "cache",
        "lock": tmp_path / "quality.lock",
    }


def run(env: dict, *args: str, script: Path = SCRIPT, extra: dict | None = None,
        bin_override: bool = True) -> subprocess.CompletedProcess:
    child_env = {
        "PATH": os.environ["PATH"],
        "HOME": str(env["tmp"]),
        "FAKE_LOG": str(env["log"]),
        "TOURING_QUALITY_CACHE_DIR": str(env["cache"]),
        "TOURING_QUALITY_LOCK": str(env["lock"]),
    }
    if bin_override:
        child_env["TOURING_QUALITY_BIN"] = str(env["engine"])
    child_env.update(extra or {})
    return subprocess.run(
        [str(script), *args], capture_output=True, text=True, env=child_env, timeout=60
    )


def engine_calls(env: dict) -> int:
    return len(env["log"].read_text().splitlines()) if env["log"].exists() else 0


def cached_entries(env: dict) -> list[Path]:
    return sorted(env["cache"].glob("*.json")) if env["cache"].exists() else []


class HeldLock:
    """Holds the wrapper's lock (flock(2), the same lock flock(1) takes)."""

    def __init__(self, path: Path, seconds: float):
        self.path, self.seconds = path, seconds
        self.acquired = threading.Event()

    def _hold(self) -> None:
        with open(self.path, "w") as fd:
            fcntl.flock(fd, fcntl.LOCK_EX)
            self.acquired.set()
            time.sleep(self.seconds)

    def start(self) -> threading.Thread:
        thread = threading.Thread(target=self._hold, daemon=True)
        thread.start()
        assert self.acquired.wait(5), "the test could not take the lock"
        return thread


def test_the_script_is_versioned_here_and_executable():
    assert SCRIPT.is_file(), f"{SCRIPT} missing — the quality wrapper must be in the repo"
    assert os.access(SCRIPT, os.X_OK), "the wrapper must be executable"


def test_it_parses():
    proc = subprocess.run(["bash", "-n", str(SCRIPT)], capture_output=True, text=True)
    assert proc.returncode == 0, proc.stderr


@pytest.mark.skipif(shutil.which("shellcheck") is None, reason="shellcheck not installed")
def test_shellcheck_is_clean():
    proc = subprocess.run(["shellcheck", "-S", "warning", str(SCRIPT)], capture_output=True, text=True)
    assert proc.returncode == 0, proc.stdout


def test_no_user_home_is_written_into_the_script():
    code = [l for l in SCRIPT.read_text(encoding="utf-8").splitlines() if not l.strip().startswith("#")]
    offenders = [l for l in code if "/home/" in l]
    assert not offenders, f"a hard-coded home path breaks every other checkout: {offenders}"


def test_the_engine_defaults_to_the_release_binary_of_the_workspace_holding_the_script(env):
    """Copied into another checkout and reached through a symlink, the wrapper runs
    THAT checkout's engine — the workspace is derived from the script's real path."""
    checkout = env["tmp"] / "checkout"
    (checkout / "scripts").mkdir(parents=True)
    copy = checkout / "scripts" / "touring-quality-score"
    shutil.copy2(SCRIPT, copy)
    write_engine(checkout / "target" / "release" / "touring-quality")
    link = env["tmp"] / "bin" / "touring-quality"
    link.parent.mkdir()
    link.symlink_to(copy)

    proc = run(env, "list", script=link, bin_override=False)

    assert proc.returncode == 0, proc.stderr
    assert env["log"].read_text().strip() == "list", "the checkout's own engine answered"


def test_non_score_subcommands_pass_through_untouched(env):
    proc = run(env, "check", "--gate", "F2.1", "--target", "x")
    assert proc.returncode == 0, proc.stderr
    assert env["log"].read_text().strip() == "check --gate F2.1 --target x"
    assert not cached_entries(env)


def test_score_help_reaches_the_engine_instead_of_being_read_as_a_target(env):
    """Cross-audit 14/09/2026 (R2-12): `score --help` failed `readlink` as a target."""
    proc = run(env, "score", "--help")
    assert proc.returncode == 0, proc.stderr
    assert env["log"].read_text().strip() == "score --help"
    assert not cached_entries(env)


def test_an_unchanged_target_is_measured_once_and_then_served_from_the_cache(env):
    first = run(env, "score", str(env["target"]), "--format", "json")
    second = run(env, "score", str(env["target"]), "--format", "json")
    assert first.returncode == 0 and second.returncode == 0, (first.stderr, second.stderr)
    assert engine_calls(env) == 1
    assert first.stdout == second.stdout == '{"tier": "Gold", "composite": 0.9}\n', (
        "a cache hit prints exactly what the measurement printed, banner and blanks dropped"
    )


def test_a_changed_engine_is_never_answered_by_the_old_engine_s_cache(env):
    run(env, "score", str(env["target"]))
    later = time.time() + 120
    os.utime(env["engine"], (later, later))
    run(env, "score", str(env["target"]))
    assert engine_calls(env) == 2


def test_an_empty_cache_entry_is_never_served(env):
    run(env, "score", str(env["target"]))
    [entry] = cached_entries(env)
    entry.write_text("")
    proc = run(env, "score", str(env["target"]))
    assert proc.returncode == 0 and proc.stdout.strip(), proc.stderr
    assert engine_calls(env) == 2


def test_a_held_lock_is_waited_for_then_refused_loudly_with_exit_75(env):
    held = HeldLock(env["lock"], seconds=4).start()
    started = time.monotonic()
    proc = run(env, "score", str(env["target"]), extra={"TOURING_QUALITY_LOCK_WAIT_SECS": "1"})
    elapsed = time.monotonic() - started
    held.join()
    assert proc.returncode == 75, (proc.returncode, proc.stderr)
    assert proc.stdout == ""
    assert "nothing was measured" in proc.stderr, "the refusal names itself"
    assert elapsed >= 1, "the wrapper waited before refusing"
    assert engine_calls(env) == 0


def test_a_lock_released_within_the_wait_is_measured(env):
    held = HeldLock(env["lock"], seconds=1).start()
    proc = run(env, "score", str(env["target"]), extra={"TOURING_QUALITY_LOCK_WAIT_SECS": "20"})
    held.join()
    assert proc.returncode == 0, proc.stderr
    assert '"tier": "Gold"' in proc.stdout
    assert engine_calls(env) == 1


def test_a_failing_engine_prints_its_output_keeps_its_exit_code_and_is_never_cached(env):
    proc = run(env, "score", str(env["target"]), extra={"FAKE_RC": "1"})
    assert proc.returncode == 1
    assert '"tier": "Gold"' in proc.stdout, "a failed --fail-below still shows the report"
    assert not cached_entries(env)
    run(env, "score", str(env["target"]), extra={"FAKE_RC": "1"})
    assert engine_calls(env) == 2, "nothing was cached, so the engine ran again"


def test_an_engine_with_no_output_is_an_error_and_never_cached(env):
    proc = run(env, "score", str(env["target"]), extra={"FAKE_EMPTY": "1"})
    assert proc.returncode == 1
    assert "produced no output" in proc.stderr
    assert not cached_entries(env)
    assert not list(env["cache"].glob("*.tmp")) and not list(env["cache"].glob("*.out"))


def test_different_arguments_never_share_a_cache_entry(env):
    """Cross-audit 14/09/2026 (C1): keyed without the arguments, a cached report
    answered `--fail-below 0.95` with exit 0 on a composite of 0.85 and
    `--dims F2.1` with another invocation's full report."""
    run(env, "score", str(env["target"]), "--format", "json")
    failing = run(env, "score", str(env["target"]), "--format", "json", "--fail-below", "0.95",
                  extra={"FAKE_RC": "1"})
    assert failing.returncode == 1, "the threshold is measured, not served from cache"
    run(env, "score", str(env["target"]), "--dims", "F2.1", "--format", "json")
    assert engine_calls(env) == 3, env["log"].read_text()
    again = run(env, "score", str(env["target"]), "--dims", "F2.1", "--format", "json")
    assert again.returncode == 0 and engine_calls(env) == 3, "the same arguments reuse their entry"


def test_a_report_written_to_a_file_bypasses_the_cache(env):
    out = env["tmp"] / "report.json"
    first = run(env, "score", str(env["target"]), "--output", str(out))
    assert first.returncode == 0, first.stderr
    assert out.read_text().strip(), "the engine wrote the report"
    assert "produced no output" not in first.stderr
    out.unlink()
    second = run(env, "score", str(env["target"]), "--output", str(out))
    assert second.returncode == 0 and out.exists(), "a second run writes the file again"
    assert engine_calls(env) == 2
    assert not cached_entries(env)


def test_an_untracked_file_invalidates_the_cache(env):
    """Cross-audit 14/09/2026 (C2): the engine scores untracked files, so the key
    must see them."""
    repo = env["target"]
    git_env = {**os.environ, "HOME": str(env["tmp"]), "GIT_CONFIG_NOSYSTEM": "1"}
    git = ["git", "-c", "user.email=a@b.c", "-c", "user.name=a"]
    subprocess.run(git + ["init", "-q"], cwd=repo, check=True, env=git_env)
    subprocess.run(git + ["add", "lib.rs"], cwd=repo, check=True, env=git_env)
    subprocess.run(git + ["commit", "-qm", "x"], cwd=repo, check=True, env=git_env)
    run(env, "score", str(repo))
    (repo / "new_module.rs").write_text("pub fn fresh() {}\n", encoding="utf-8")
    run(env, "score", str(repo))
    assert engine_calls(env) == 2, "a new untracked file is measured, not served stale"


def test_a_descendant_of_the_engine_never_inherits_the_lock(env):
    """Cross-audit 14/09/2026 (C10): with fd 9 inherited, a long-lived grandchild
    held the lock after the engine exited and every later score exited 75."""
    first = run(env, "score", str(env["target"]), extra={"FAKE_GRANDCHILD_SECS": "20"})
    marker = env["log"].parent / (env["log"].name + ".grandchild")
    grandchild = int(marker.read_text())
    try:
        assert first.returncode == 0, first.stderr
        second = run(env, "score", str(env["target"]),
                     extra={"TOURING_QUALITY_CACHE_DISABLE": "1", "TOURING_QUALITY_LOCK_WAIT_SECS": "1"})
        assert second.returncode == 0, f"the lock was free: {second.stderr}"
    finally:
        try:
            os.kill(grandchild, 9)
        except ProcessLookupError:
            pass


def test_an_unusable_lock_is_busy_named_not_a_mute_failure(env):
    """Cross-audit 14/09/2026 (C11): an unopenable lock exited 1 with no JSON."""
    env["lock"].mkdir()  # a directory cannot be opened for writing
    proc = run(env, "score", str(env["target"]))
    assert proc.returncode == 75, (proc.returncode, proc.stderr)
    assert "cannot open the lock" in proc.stderr
    assert engine_calls(env) == 0


def test_the_default_lock_lives_in_the_user_runtime_directory():
    body = SCRIPT.read_text(encoding="utf-8")
    code = "\n".join(l for l in body.splitlines() if not l.strip().startswith("#"))
    assert "/tmp/touring-quality.lock" not in code
    assert "${XDG_RUNTIME_DIR:-$HOME/.cache}/touring-quality.lock" in code


def test_an_unreadable_target_exits_3_without_measuring(env):
    proc = run(env, "score", str(env["tmp"] / "missing"))
    assert proc.returncode == 3, proc.stderr
    assert engine_calls(env) == 0


def test_the_installed_tool_is_a_symlink_to_this_file():
    """The same structural guard as `update-touring`: the installed path and the repo
    file are one inode. `TOURING_QUALITY_SCORE_REQUIRE_SYMLINK=1` (exported by the
    propagate-release gate 1/6) turns an absent install into a failure, because the
    judge the release is about to trust calls `touring-quality` from PATH."""
    if not INSTALLED.exists():
        if os.environ.get("TOURING_QUALITY_SCORE_REQUIRE_SYMLINK") == "1":
            pytest.fail(
                f"{INSTALLED} absent under TOURING_QUALITY_SCORE_REQUIRE_SYMLINK=1. "
                f"Restore with: ln -sfn {SCRIPT} {INSTALLED}"
            )
        pytest.skip(f"{INSTALLED} absent — needs the operator's machine")
    assert INSTALLED.is_symlink(), (
        f"{INSTALLED} is a regular file again — the wrapper has drifted out of the repo. "
        f"Restore with: ln -sfn {SCRIPT} {INSTALLED}"
    )
    assert INSTALLED.resolve() == SCRIPT.resolve(), f"{INSTALLED} points at {INSTALLED.resolve()}"
