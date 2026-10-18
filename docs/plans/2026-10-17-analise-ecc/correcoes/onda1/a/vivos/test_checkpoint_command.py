"""test_checkpoint_command.py — o bloco bash do novo /checkpoint, executado de verdade.

O comando antigo (`~/.claude/commands/checkpoint.md`) chamava
`~/.claude/tools/Touring-native tooling/workflows/checkpoint.sh`, que não existe — o `/checkpoint` falhava sempre.
O novo é um prompt cujo bloco ```bash é a execução; aqui o bloco é EXTRAÍDO do .md proposto e roda num repositório
git real, com `touring` e `date` substituídos por stubs no PATH (o estado vivo do Gabriel nunca é tocado).

Uso (do worktree espelho):
    SNAPSHOT_SAFETY=<worktree>/scripts/snapshot_safety.py CHECKPOINT_MD=<este dir>/checkpoint.md \
        python3 -m pytest test_checkpoint_command.py -q
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent
COMMAND_MD = Path(os.environ.get("CHECKPOINT_MD") or HERE / "checkpoint.md")
SNAPSHOT_SAFETY = Path(os.environ.get("SNAPSHOT_SAFETY") or "/home/gabrielgadea/projects/touring-wt-onda1/scripts/snapshot_safety.py")
OLD_COMMAND = Path.home() / ".claude" / "commands" / "checkpoint.md"
GIT_ENV = {"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t",
           "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_SYSTEM": "/dev/null"}
FIXED_DAY, FIXED_STAMP = "2026-10-18", "120000"


def _block() -> str:
    match = re.search(r"```bash\n(.*?)\n```", COMMAND_MD.read_text(encoding="utf-8"), re.DOTALL)
    assert match, "o .md não tem bloco bash"
    return match.group(1)


def _git(repo: Path, *args: str) -> str:
    done = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True, env={**os.environ, **GIT_ENV}, check=True)
    return done.stdout.strip()


@pytest.fixture
def world(tmp_path: Path) -> dict[str, Path]:
    home = tmp_path / "home"
    scripts = home / "projects" / "touring" / "scripts"
    scripts.mkdir(parents=True)
    (scripts / "snapshot_safety.py").symlink_to(SNAPSHOT_SAFETY)
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    calls = tmp_path / "touring_calls.jsonl"
    (bin_dir / "touring").write_text(
        "#!/usr/bin/env python3\nimport json, os, sys\n"
        "rec = {'argv': sys.argv[1:]}\n"
        f"open({str(calls)!r}, 'a').write(json.dumps(rec) + '\\n')\n"
        "rc = int(os.environ.get('FAKE_TOURING_MEMORY_RC', '0')) if sys.argv[1:3] == ['memory', 'store'] else 0\n"
        "sys.exit(rc)\n")
    (bin_dir / "date").write_text(
        f"#!/bin/sh\ncase \"$1\" in +%F) echo {FIXED_DAY};; +%H%M%S) echo {FIXED_STAMP};; *) exec /usr/bin/date \"$@\";; esac\n")
    for name in ("touring", "date"):
        (bin_dir / name).chmod(0o755)
    repo = tmp_path / "repo"
    repo.mkdir()
    _git(repo, "init", "-q", "-b", "main")
    (repo / "a.txt").write_text("v1\n")
    _git(repo, "add", "-A")
    _git(repo, "commit", "-q", "-m", "init")
    return {"home": home, "bin": bin_dir, "calls": calls, "repo": repo, "tmp": tmp_path}


SAMPLE_SUMMARY = "Estado: hooks prontos. Próximo: rodar a suíte."


def _run(world: dict[str, Path], topic: str = "onda1 hooks", *, cwd: Path | None = None, home: Path | None = None,
         env: dict[str, str] | None = None, summary: str = SAMPLE_SUMMARY) -> subprocess.CompletedProcess[str]:
    script = _block().replace("$ARGUMENTS", topic).replace("<RESUMO>", summary)
    full_env = {**os.environ, **GIT_ENV, "HOME": str(home or world["home"]), "PATH": f"{world['bin']}{os.pathsep}{os.environ['PATH']}",
                **(env or {})}
    return subprocess.run(["bash", "-c", script], cwd=cwd or world["repo"], capture_output=True, text=True, env=full_env,
                          timeout=60, check=False)


def _touring_calls(world: dict[str, Path]) -> list[list[str]]:
    path = world["calls"]
    return [json.loads(line)["argv"] for line in path.read_text().splitlines()] if path.exists() else []


def _refs(repo: Path) -> list[str]:
    return _git(repo, "for-each-ref", "--format=%(refname)", "refs/heads/safety").splitlines()


@pytest.mark.skipif(not OLD_COMMAND.exists(), reason="o comando vivo antigo não está nesta máquina")
def test_the_old_command_pointed_at_a_script_that_does_not_exist() -> None:
    text = OLD_COMMAND.read_text(encoding="utf-8")
    if "Touring-native tooling/workflows/checkpoint.sh" not in text:
        pytest.skip("o comando vivo já foi substituído")
    assert not (Path.home() / ".claude" / "tools" / "Touring-native tooling" / "workflows" / "checkpoint.sh").exists()


def test_a_dirty_repository_gets_a_safety_ref_and_a_semantic_memory_and_nothing_else_moves(world: dict[str, Path]) -> None:
    repo = world["repo"]
    (repo / "a.txt").write_text("v2 dirty\n")
    (repo / "new.py").write_text("print(1)\n")
    _git(repo, "add", "a.txt")
    before = {"head": _git(repo, "rev-parse", "HEAD"), "staged": _git(repo, "diff", "--cached", "--name-only"),
              "a": (repo / "a.txt").read_text(), "branch": _git(repo, "rev-parse", "--abbrev-ref", "HEAD")}
    done = _run(world)
    assert done.returncode == 0, done.stdout + done.stderr
    assert 'SNAPSHOT: {' in done.stdout and '"status": "OK"' in done.stdout
    assert _refs(repo) == [f"refs/heads/safety/{FIXED_DAY}/onda1-hooks-{FIXED_STAMP}"]
    assert _git(repo, "show", f"{_refs(repo)[0]}:new.py") == "print(1)"
    after = {"head": _git(repo, "rev-parse", "HEAD"), "staged": _git(repo, "diff", "--cached", "--name-only"),
             "a": (repo / "a.txt").read_text(), "branch": _git(repo, "rev-parse", "--abbrev-ref", "HEAD")}
    assert after == before and _git(repo, "stash", "list") == ""
    store, reward = _touring_calls(world)
    assert store[:2] == ["memory", "store"] and store[2] == f"checkpoint:onda1-hooks:{FIXED_DAY}-{FIXED_STAMP}"
    assert "--tier" in store and store[store.index("--tier") + 1] == "semantic"
    assert "Estado: hooks prontos" in store[3] and f"snapshot=refs/heads/safety/{FIXED_DAY}" in store[3]
    assert reward == ["learning", "reward", "checkpoint", "1.0", "onda1-hooks"]


def test_a_clean_repository_is_covered_by_head_and_still_remembered(world: dict[str, Path]) -> None:
    done = _run(world)
    assert done.returncode == 0 and "COBERTO-PELO-HEAD" in done.stdout
    assert _refs(world["repo"]) == []
    assert [c[:2] for c in _touring_calls(world)] == [["memory", "store"], ["learning", "reward"]]


def test_no_topic_prints_usage_and_does_nothing(world: dict[str, Path]) -> None:
    (world["repo"] / "a.txt").write_text("dirty\n")
    done = _run(world, "   ")
    assert done.returncode == 2 and "Uso: /checkpoint <tópico>" in done.stdout
    assert _touring_calls(world) == [] and _refs(world["repo"]) == []


def test_outside_a_repository_only_the_memory_is_written_and_no_reward(world: dict[str, Path]) -> None:
    plain = world["tmp"] / "plain"
    plain.mkdir()
    done = _run(world, cwd=plain)
    assert done.returncode == 0 and "SNAPSHOT: n/a" in done.stdout
    assert [c[:2] for c in _touring_calls(world)] == [["memory", "store"]]


def test_a_missing_snapshot_script_is_loud_keeps_the_memory_and_withholds_the_reward(world: dict[str, Path]) -> None:
    bare_home = world["tmp"] / "bare-home"
    bare_home.mkdir()
    (world["repo"] / "a.txt").write_text("dirty\n")
    done = _run(world, home=bare_home)
    assert done.returncode == 1 and "SNAPSHOT: FALHOU" in done.stdout
    assert [c[:2] for c in _touring_calls(world)] == [["memory", "store"]]


def test_a_refused_snapshot_is_a_failure_not_a_checkpoint(world: dict[str, Path]) -> None:
    repo = world["repo"]
    (repo / "a.txt").write_text("dirty\n")
    _git(repo, "update-ref", f"refs/heads/safety/{FIXED_DAY}/onda1-hooks-{FIXED_STAMP}", "HEAD")  # the same name again
    done = _run(world)
    assert done.returncode == 1 and '"status": "RECUSADO"' in done.stdout
    assert [c[:2] for c in _touring_calls(world)] == [["memory", "store"]]  # no reward on an unmeasured outcome


def test_a_memory_that_did_not_store_fails_the_checkpoint_and_withholds_the_reward(world: dict[str, Path]) -> None:
    (world["repo"] / "a.txt").write_text("dirty\n")
    done = _run(world, env={"FAKE_TOURING_MEMORY_RC": "3"})
    assert done.returncode == 1 and "rc=3" in done.stdout
    assert [c[:2] for c in _touring_calls(world)] == [["memory", "store"]]
    assert len(_refs(world["repo"])) == 1  # the git side still happened


@pytest.mark.parametrize("topic", ["x'; touch PWNED_A; echo '", "$(touch PWNED_B)", "`touch PWNED_C`", "a\" ; touch PWNED_D ; \"",
                                   "__TOPIC__; touch PWNED_E"])
def test_a_hostile_topic_is_data_never_code(world: dict[str, Path], topic: str) -> None:
    done = _run(world, topic)
    assert done.returncode == 0, done.stdout + done.stderr
    assert not list(world["repo"].glob("PWNED*")) and not list(world["tmp"].glob("PWNED*"))
    key = _touring_calls(world)[0][2]
    assert re.fullmatch(r"checkpoint:[a-z0-9-]+:2026-10-18-120000", key), key


def test_a_summary_full_of_shell_syntax_is_stored_literally(world: dict[str, Path]) -> None:
    """The summary is model-written and routinely carries backticks, `$VAR` and `$(…)`: it must reach memory verbatim."""
    hostile = "Rodar `touch PWNED_F`; custo $(touch PWNED_G) em $HOME; \\n literal; 'aspas' e \"duplas\""
    done = _run(world, summary=hostile)
    assert done.returncode == 0, done.stdout + done.stderr
    assert not list(world["repo"].glob("PWNED*")) and not list(world["tmp"].glob("PWNED*"))
    assert hostile in _touring_calls(world)[0][3]


def test_accents_become_a_plain_slug_and_a_long_topic_is_capped(world: dict[str, Path]) -> None:
    _run(world, "Integração Óbvia — Fase 2")
    assert _touring_calls(world)[0][2] == f"checkpoint:integracao-obvia-fase-2:{FIXED_DAY}-{FIXED_STAMP}"
    _run(world, "palavra " * 40)
    assert len(_touring_calls(world)[2][2].split(":")[1]) <= 48


def test_the_command_never_stashes() -> None:
    assert "stash" not in _block()
    assert "git stash" in COMMAND_MD.read_text(encoding="utf-8")  # only as the prohibition, in prose


@pytest.mark.skipif(shutil.which("shellcheck") is None, reason="shellcheck ausente")
def test_the_block_is_shellcheck_clean(tmp_path: Path) -> None:
    script = tmp_path / "block.sh"
    script.write_text("#!/bin/bash\n" + _block().replace("$ARGUMENTS", "t").replace("<RESUMO>", "r") + "\n")
    done = subprocess.run(["shellcheck", "-s", "bash", str(script)], capture_output=True, text=True, check=False)
    assert done.returncode == 0, done.stdout
