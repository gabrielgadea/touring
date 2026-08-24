#!/usr/bin/env python3
"""Tests for sync-client-skills.py — the guard that keeps `client/` honest.

Two invariants, and they are enforced at different places on purpose:

  · **live == mirror** can only be checked where `~/.claude` exists, i.e. the
    operator's machine. `propagate-release.sh` runs it before any release.
  · **mirror == its own manifest** is checkable anywhere, CI included. It catches
    the other mistake: editing `client/` directly. Such an edit looks like it
    worked, and is silently reverted by the next `--apply` without its author
    ever learning why.

The drift test skips when the live side is absent rather than failing — a gate
that cannot look must not block. The manifest test never skips.
"""

from __future__ import annotations

import importlib.util
import subprocess
import sys
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parent / "sync-client-skills.py"
REPO = SCRIPT.parent.parent


def _module():
    """Import the hyphenated script as a module (its name is not an identifier)."""
    spec = importlib.util.spec_from_file_location("sync_client_skills", SCRIPT)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


sync = _module()


def run(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(SCRIPT), *args], capture_output=True, text=True, cwd=REPO
    )


# ── the invariant CI can enforce ─────────────────────────────────────────────


def test_the_mirror_matches_its_own_manifest():
    """`client/` is generated, never hand-edited.

    An edit made directly in the mirror is not a change to anything that runs: it
    is overwritten by the next sync, silently. This is the check that says so."""
    report = sync.check_manifest(REPO / "client")
    assert report["manifest_present"], (
        "client/MANIFEST.sha256 is missing — run "
        "`python3 scripts/sync-client-skills.py --apply` on the operator's machine"
    )
    assert report["manifest_mismatch"] == [], (
        "the mirror no longer matches its manifest. Edit ~/.claude/... and re-run "
        "--apply; do not edit client/ directly"
    )


def test_no_generated_artifact_sits_in_the_mirror():
    """Regression, 2026-08-18: the 2026-07-25 bulk copy carried 134 generated files
    (73 __pycache__, 46 per-directory `.db`, 15 tool caches — 6.5 MB) into a public
    repo, because `.gitignore` anchored `.claude/` at the root only.

    Scoped to the MIRRORED areas: `client/` also hosts source trees this sync
    does not own (client/omarchy has its own live workstream whose test runs
    regenerate caches on disk mid-suite — observed 2026-08-23). On-disk junk
    there is that workstream's `.gitignore` concern, not mirror drift."""
    junk = [
        f"{area}/{f.relative_to(REPO / 'client' / area).as_posix()}"
        for area, _ in sync.AREAS
        if (REPO / "client" / area).is_dir()
        for f in (REPO / "client" / area).rglob("*")
        if f.is_file() and sync.is_noise(f.relative_to(REPO / "client" / area))
    ]
    assert junk == [], f"generated artifacts in the mirror: {junk[:10]}"


def test_runtime_state_can_never_be_mirrored_again():
    """The exclusion is what makes the cleanup stick."""
    for probe in (
        Path("skills/x/.claude/touring/memory.db"),
        Path("skills/x/__pycache__/y.pyc"),
        Path("skills/x/.pytest_cache/v/cache/nodeids"),
        Path("skills/x/.ruff_cache/CACHEDIR.TAG"),
        Path("skills/x/symbols.db"),
    ):
        assert sync.is_noise(probe), f"{probe} would be mirrored"
    assert not sync.is_noise(Path("skills/Touring/SKILL.md"))
    assert not sync.is_noise(Path("client/MANIFEST.sha256"))


# ── the invariant only the operator's machine can enforce ────────────────────


def test_mirror_is_in_step_with_the_live_skills():
    if not sync.live_root().is_dir():
        pytest.skip(f"{sync.live_root()} absent — the full comparison needs the live side")
    report = sync.survey()
    assert report["drifted"] == [], f"stale in the mirror: {report['drifted']}"
    assert report["missing"] == [], f"never reached the mirror: {report['missing']}"


def test_check_exits_nonzero_on_drift(tmp_path, monkeypatch):
    """The gate has to FAIL, or it is decoration. Proved against a fake pair."""
    live, mirror = tmp_path / "live", tmp_path / "repo" / "client"
    (live / "skills" / "demo").mkdir(parents=True)
    (mirror / "skills" / "demo").mkdir(parents=True)
    (live / "skills" / "demo" / "SKILL.md").write_text("versão nova\n", encoding="utf-8")
    (mirror / "skills" / "demo" / "SKILL.md").write_text("versão velha\n", encoding="utf-8")
    monkeypatch.setattr(sync, "live_root", lambda: live)
    monkeypatch.setattr(sync, "repo_root", lambda: mirror.parent)

    report = sync.survey()
    assert report["drifted"] == ["skills/demo/SKILL.md"]

    sync.apply(report, prune=False)
    assert (mirror / "skills" / "demo" / "SKILL.md").read_text() == "versão nova\n"
    assert sync.survey()["drifted"] == []
    assert (mirror / sync.MANIFEST).is_file(), "--apply must refresh the manifest"


def test_a_new_file_inside_a_mirrored_skill_is_adopted(tmp_path, monkeypatch):
    """Scope rule for `skills`: the directory is mirrored, so everything in it is."""
    live, mirror = tmp_path / "live", tmp_path / "repo" / "client"
    (live / "skills" / "demo" / "scripts").mkdir(parents=True)
    (mirror / "skills" / "demo").mkdir(parents=True)
    (mirror / "skills" / "demo" / "SKILL.md").write_text("x\n", encoding="utf-8")
    (live / "skills" / "demo" / "SKILL.md").write_text("x\n", encoding="utf-8")
    (live / "skills" / "demo" / "scripts" / "novo.py").write_text("print(1)\n", encoding="utf-8")
    monkeypatch.setattr(sync, "live_root", lambda: live)
    monkeypatch.setattr(sync, "repo_root", lambda: mirror.parent)

    assert sync.survey()["missing"] == ["skills/demo/scripts/novo.py"]


def test_an_unmirrored_skill_stays_out(tmp_path, monkeypatch):
    """A skill without a directory in `client/` is not product — a personal skill
    must not be dragged into a public repo by the sync."""
    live, mirror = tmp_path / "live", tmp_path / "repo" / "client"
    (live / "skills" / "pessoal").mkdir(parents=True)
    (live / "skills" / "pessoal" / "SKILL.md").write_text("privado\n", encoding="utf-8")
    (mirror / "skills").mkdir(parents=True)
    monkeypatch.setattr(sync, "live_root", lambda: live)
    monkeypatch.setattr(sync, "repo_root", lambda: mirror.parent)

    report = sync.survey()
    assert report["missing"] == [] and report["drifted"] == []


def test_a_new_live_rule_is_not_adopted_automatically(tmp_path, monkeypatch):
    """Scope rule for `rules`/`agents`: the mirror is a curated SELECTION, so a new
    global rule stays out until someone puts it there deliberately."""
    live, mirror = tmp_path / "live", tmp_path / "repo" / "client"
    (live / "rules").mkdir(parents=True)
    (mirror / "rules").mkdir(parents=True)
    (live / "rules" / "espelhada.md").write_text("a\n", encoding="utf-8")
    (mirror / "rules" / "espelhada.md").write_text("a\n", encoding="utf-8")
    (live / "rules" / "pessoal.md").write_text("privado\n", encoding="utf-8")
    monkeypatch.setattr(sync, "live_root", lambda: live)
    monkeypatch.setattr(sync, "repo_root", lambda: mirror.parent)

    assert sync.survey()["missing"] == []


def test_apply_never_deletes_without_prune(tmp_path, monkeypatch):
    """A file gone from the live side is usually a half-finished rename, not a
    deletion — so removal is opt-in."""
    live, mirror = tmp_path / "live", tmp_path / "repo" / "client"
    (live / "skills" / "demo").mkdir(parents=True)
    (mirror / "skills" / "demo").mkdir(parents=True)
    (mirror / "skills" / "demo" / "sumiu.md").write_text("x\n", encoding="utf-8")
    monkeypatch.setattr(sync, "live_root", lambda: live)
    monkeypatch.setattr(sync, "repo_root", lambda: mirror.parent)

    report = sync.survey()
    assert report["orphaned"] == ["skills/demo/sumiu.md"]
    sync.apply(report, prune=False)
    assert (mirror / "skills" / "demo" / "sumiu.md").is_file(), "não deve apagar sem --prune"
    sync.apply(sync.survey(), prune=True)
    assert not (mirror / "skills" / "demo" / "sumiu.md").exists()


def test_cli_check_returns_one_when_drifted(tmp_path, monkeypatch):
    """The exit code is what a gate consumes (`propagate-release.sh` gate 2/6).

    The old body only asserted 0 on a clean repo — the FAILURE branch of
    `main()` was never exercised, so the very signal the release gate relies
    on had zero coverage (T3, cross-audit 2026-08-23). This drives both."""
    live, mirror = tmp_path / "live", tmp_path / "repo" / "client"
    (live / "skills" / "demo").mkdir(parents=True)
    (mirror / "skills" / "demo").mkdir(parents=True)
    (live / "skills" / "demo" / "SKILL.md").write_text("v2\n", encoding="utf-8")
    (mirror / "skills" / "demo" / "SKILL.md").write_text("v1\n", encoding="utf-8")
    monkeypatch.setattr(sync, "live_root", lambda: live)
    monkeypatch.setattr(sync, "repo_root", lambda: mirror.parent)

    assert sync.main(["--check"]) == 1, "drift must surface as exit 1"
    report = sync.survey()
    sync.apply(report, prune=False)
    assert sync.main(["--check"]) == 0, "after apply the same gate must clear"


def test_cli_check_passes_on_this_repo():
    """End-to-end through the real CLI on the real repo: stays clean."""
    assert run("--check").returncode == 0, "o espelho deste repo deve estar limpo"



def test_measured_runtime_state_is_never_mirrored():
    """`.baseline/` holds the orphan counts `loop_converged.py` measures for a
    scope — machine-written and machine-specific. Running the convergence gate
    from inside a mirrored tree is enough to create one (observed 2026-08-19),
    so the exclusion has to be structural rather than a habit of not running
    things in the wrong directory."""
    assert sync.is_noise(Path(".baseline/orphans-scoped.txt"))
    assert sync.is_noise(Path("skills/loop-engineering/scripts/.baseline/orphans-scoped.txt"))
    assert not sync.is_noise(Path("skills/loop-engineering/scripts/judge_attest.py"))

if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-v", "--tb=short", "-p", "no:cacheprovider"]))


# ── the noise filter must be evaluated where the file LIVES, not where it sits ──


def test_is_noise_refuses_an_absolute_path():
    """The real live root is `~/.claude`, and `.claude` is in IGNORED_DIRS — so an
    absolute live path classified EVERY file as noise. The live-side walk then
    contributed nothing and a new file inside a mirrored skill was never adopted,
    silently, because updates to already-known paths kept working. Refusing the
    absolute path makes that mistake unrepresentable rather than merely fixed."""
    with pytest.raises(ValueError, match="relative to its area root"):
        sync.is_noise(Path.home() / ".claude" / "skills" / "X" / "SKILL.md")


def test_a_new_file_is_adopted_even_when_the_live_root_is_named_like_noise(tmp_path, monkeypatch):
    """The regression the old test could not catch: its fake root was a bare
    tmp_path, so the condition that actually holds in production — a root whose
    own name is on the ignore list — was never exercised."""
    live = tmp_path / ".claude"                      # named exactly like the real one
    mirror = tmp_path / "repo" / "client"
    (live / "skills" / "demo" / "fragments").mkdir(parents=True)
    (mirror / "skills" / "demo").mkdir(parents=True)
    for side in (live, mirror):
        (side / "skills" / "demo" / "SKILL.md").write_text("x\n", encoding="utf-8")
    (live / "skills" / "demo" / "fragments" / "novo.toml").write_text("a=1\n", encoding="utf-8")
    monkeypatch.setattr(sync, "live_root", lambda: live)
    monkeypatch.setattr(sync, "repo_root", lambda: mirror.parent)

    assert sync.survey()["missing"] == ["skills/demo/fragments/novo.toml"], (
        "a file under a root whose name is on the ignore list must still be seen")


# ── client/omarchy is a SOURCE tree, not a mirror ────────────────────────────


def test_prune_never_touches_client_omarchy(tmp_path, monkeypatch):
    """`client/omarchy/` (plano 2026-08-23-omarchyos-agentico) inverts the mirror's
    direction: the repo is the origin and the Omarchy machine is provisioned from
    it. Nothing live corresponds to it, so a sync that treated it as an orphaned
    mirror would delete the only copy. `AREAS` keeps it out by construction; this
    test makes that a promise rather than an accident — `--apply --prune` with a
    live side that knows nothing about omarchy must leave every file in place and
    must not list it in the manifest either."""
    live, mirror = tmp_path / "live", tmp_path / "repo" / "client"
    (live / "skills" / "demo").mkdir(parents=True)
    (live / "skills" / "demo" / "SKILL.md").write_text("live\n", encoding="utf-8")
    (mirror / "skills" / "demo").mkdir(parents=True)
    (mirror / "skills" / "demo" / "SKILL.md").write_text("live\n", encoding="utf-8")
    source = mirror / "omarchy"
    (source / "bin").mkdir(parents=True)
    (source / "bin" / "validate_p0.sh").write_text("#!/usr/bin/env bash\n", encoding="utf-8")
    (source / "README.md").write_text("fonte, não espelho\n", encoding="utf-8")
    # NOISE inside the source tree: the old fixture only planted non-noise
    # files, so it proved less than the name promised — the junk sweep swept
    # ALL of client/ and would have deleted this from the only copy (T6).
    (source / "bin" / "__pycache__").mkdir(parents=True)
    (source / "bin" / "__pycache__" / "cc_build.cpython-312.pyc").write_bytes(b"\x00")
    (source / "state.db").write_bytes(b"\x00")
    monkeypatch.setattr(sync, "live_root", lambda: live)
    monkeypatch.setattr(sync, "repo_root", lambda: mirror.parent)

    report = sync.survey()
    assert not any(e.startswith("omarchy/") for e in report["orphaned"] + report["junk"])
    sync.apply(report, prune=True)
    assert (source / "bin" / "validate_p0.sh").is_file()
    assert (source / "README.md").is_file()
    assert (source / "bin" / "__pycache__" / "cc_build.cpython-312.pyc").is_file(), (
        "junk inside a SOURCE tree is not the sync's to delete")
    assert (source / "state.db").is_file()
    manifest = (mirror / sync.MANIFEST).read_text(encoding="utf-8")
    assert "omarchy/" not in manifest, "a fonte não entra no manifesto do espelho"
