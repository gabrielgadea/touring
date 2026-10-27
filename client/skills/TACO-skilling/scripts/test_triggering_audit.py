#!/usr/bin/env python3
"""test_triggering_audit.py — regression suite for triggering_audit.py.

The auditor shipped without one and was caught lying twice (25/10/2026, measured
on the 11 project skills of /home/gabrielgadea/projects/analise):

  * It only looked for skills under ``~/.claude/skills``. A PROJECT skill came back
    ``description_present=false`` with ``live/dead keywords = (none)`` — and the
    audit of all 11 ended "rc 0", an approval that measured nothing.
  * It inflated activations: 66 reported for ``external-apis`` against 3 real
    ``tool_use`` of the Skill tool. Every transcript entry that merely contained the
    text ``/external-apis`` (any path ``.../skills/external-apis/...``) counted.

Every test builds SYNTHETIC transcripts under ``tmp_path`` — the real
``~/.claude/projects`` is never read (the autouse fixture redirects it) and the
Touring daemon is never called.

Run: python3 -m pytest test_triggering_audit.py -q -p no:cacheprovider   (from this directory)
"""
from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

import pytest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import lib  # noqa: E402
import triggering_audit as ta  # noqa: E402

SKILL = "external-apis"
DESCRIPTION = "Consulta APIs oficiais do TCU e do BACEN com cache e fallback."


# --- synthetic world --------------------------------------------------------

@pytest.fixture(autouse=True)
def hermetic(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """Redirect every default location to an empty tmp tree; silence the daemon."""
    monkeypatch.setattr(lib, "PROJECTS_DIR", tmp_path / "home" / "projects")
    monkeypatch.setattr(lib, "SKILLS_DIR", tmp_path / "home" / "skills")
    monkeypatch.setattr(lib, "touring_available", lambda: False)
    monkeypatch.chdir(tmp_path)  # a neutral cwd with no .claude/skills


def make_skill(root: Path, name: str = SKILL, description: str = DESCRIPTION) -> Path:
    """Create ``<root>/<name>/SKILL.md`` and return the skill directory."""
    skill_dir = root / name
    skill_dir.mkdir(parents=True)
    (skill_dir / "SKILL.md").write_text(
        f"---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n",
        encoding="utf-8",
    )
    return skill_dir


def write_transcript(name: str, entries: list[dict[str, Any]]) -> Path:
    """Write ``<PROJECTS_DIR>/proj/<name>.jsonl`` (the layout ``iter_transcripts`` globs)."""
    path = lib.PROJECTS_DIR / "proj" / f"{name}.jsonl"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(json.dumps(e) for e in entries) + "\n", encoding="utf-8")
    return path


def user(text: str) -> dict[str, Any]:
    return {"type": "user", "message": {"role": "user", "content": text}}


def assistant_text(text: str) -> dict[str, Any]:
    return {"type": "assistant",
            "message": {"role": "assistant", "content": [{"type": "text", "text": text}]}}


def skill_tool_use(skill: str, tool_id: str) -> dict[str, Any]:
    return {"type": "assistant", "message": {"role": "assistant", "content": [
        {"type": "tool_use", "id": tool_id, "name": "Skill", "input": {"skill": skill}}]}}


def blob_text(text: str) -> dict[str, Any]:
    """A non-user/assistant entry that still carries a message text (api-request-blob)."""
    return {"type": "api-request-blob",
            "message": {"role": "user", "content": [{"type": "text", "text": text}]}}


def typed_slash(skill: str, args: str = "") -> dict[str, Any]:
    """How Claude Code records a user typing ``/skill args``."""
    return user(f"<command-message>{skill}</command-message>\n"
                f"<command-name>/{skill}</command-name>\n"
                f"<command-args>{args}</command-args>")


def run(capsys: pytest.CaptureFixture[str], *argv: str) -> tuple[int, dict[str, Any], str]:
    """Run ``main(--json)``; return (rc, parsed stdout JSON or {}, stderr)."""
    rc = ta.main([*argv, "--json"])
    captured = capsys.readouterr()
    payload = json.loads(captured.out) if captured.out.strip() else {}
    return rc, payload, captured.err


def activations(report: dict[str, Any]) -> int:
    return report["automatic_activations"] + report["explicit_activations"]


# --- defect 1: project skills ----------------------------------------------

def _auto_history() -> None:
    write_transcript("s1", [
        user("consulte as APIs do TCU sobre acórdãos"), skill_tool_use(SKILL, "toolu_a"),
        user("busque a taxa Selic no BACEN"), skill_tool_use(SKILL, "toolu_b"),
    ])


def test_project_skill_found_via_cwd(tmp_path, monkeypatch, capsys):
    project = tmp_path / "proj_root"
    skill_dir = make_skill(project / ".claude" / "skills")
    monkeypatch.chdir(project)
    _auto_history()
    rc, report, _ = run(capsys, SKILL)
    assert rc == 0
    assert report["description_present"] is True
    assert report["live_keywords"], "description words seen in real prompts must be reported"
    assert report.get("skill_found") is True
    assert report.get("skill_dir") == str(skill_dir)


def test_project_skill_found_via_project_flag(tmp_path, capsys):
    project = tmp_path / "elsewhere"
    skill_dir = make_skill(project / ".claude" / "skills")
    _auto_history()
    rc, report, _ = run(capsys, SKILL, "--project", str(project))
    assert rc == 0
    assert report["description_present"] is True
    assert report.get("skill_dir") == str(skill_dir)


def test_project_skill_found_via_skills_dir_flag(tmp_path, capsys):
    skills_root = tmp_path / "custom_skills"
    skill_dir = make_skill(skills_root)
    _auto_history()
    rc, report, _ = run(capsys, SKILL, "--skills-dir", str(skills_root))
    assert rc == 0
    assert report["description_present"] is True
    assert report.get("skill_dir") == str(skill_dir)


def test_project_skill_found_via_path_argument(tmp_path, capsys):
    skill_dir = make_skill(tmp_path / "anywhere" / ".claude" / "skills")
    _auto_history()
    rc, report, _ = run(capsys, str(skill_dir))
    assert rc == 0
    assert report["description_present"] is True
    assert report["skill"] == SKILL, "the name comes from the SKILL.md, not the path"
    assert activations(report) == 2


def test_global_skill_still_found(capsys):
    """Regression: the pre-existing behaviour (skill under SKILLS_DIR) is preserved."""
    make_skill(lib.SKILLS_DIR)
    _auto_history()
    rc, report, _ = run(capsys, SKILL)
    assert rc == 0
    assert report["description_present"] is True
    assert report["live_keywords"]
    assert activations(report) == 2


def test_report_names_the_audited_skill_dir(capsys):
    """Which SKILL.md was read must be visible — a same-named skill may shadow another."""
    skill_dir = make_skill(lib.SKILLS_DIR)
    rc, report, _ = run(capsys, SKILL)
    assert rc == 0
    assert report.get("skill_found") is True
    assert report.get("skill_dir") == str(skill_dir)


def test_missing_skill_is_an_explicit_error(capsys):
    """Control (c): an unknown skill is NOT an empty 'rc 0' report."""
    _auto_history()
    rc, report, err = run(capsys, "no-such-skill")
    assert rc == 3
    assert report.get("skill_found") is False
    assert report.get("searched_dirs"), "the report must say where it looked"
    assert "not found" in err.lower()


def test_missing_skill_text_mode_is_loud(capsys):
    rc = ta.main(["no-such-skill"])
    err = capsys.readouterr().err
    assert rc == 3
    assert "not found" in err.lower()


# --- defect 2: activations --------------------------------------------------

def test_only_real_tool_use_counts(capsys):
    """(b) 1 real tool_use + 5 path mentions in other entries = 1 activation, not 6."""
    make_skill(lib.SKILLS_DIR)
    path = "/home/u/.claude/skills/external-apis/SKILL.md"
    write_transcript("s1", [
        user("consulte as APIs do TCU sobre acórdãos"),
        skill_tool_use(SKILL, "toolu_real"),
        assistant_text(f"li {path} agora"),
        user(f"o arquivo {path} existe?"),
        blob_text(f"Base directory for this skill: {path}"),
        assistant_text(f"veja .../skills/{SKILL}/references/x.md"),
        blob_text(f"<system-reminder>skill at {path}</system-reminder>"),
    ])
    rc, report, _ = run(capsys, SKILL)
    assert rc == 0
    assert activations(report) == 1
    assert report["automatic_activations"] == 1
    assert report["explicit_activations"] == 0


def test_skill_name_prefix_is_not_a_match(capsys):
    """``external-apis-extra`` / ``plan-protocol`` are other skills, not substring hits."""
    make_skill(lib.SKILLS_DIR)
    make_skill(lib.SKILLS_DIR, name="plan", description="Plan things.")
    write_transcript("s1", [
        user("faça algo"), skill_tool_use("external-apis-extra", "toolu_1"),
        user("planeje"), skill_tool_use("plan-protocol", "toolu_2"),
    ])
    _, report_ext, _ = run(capsys, SKILL)
    _, report_plan, _ = run(capsys, "plan")
    assert activations(report_ext) == 0
    assert activations(report_plan) == 0


def test_plugin_namespaced_tool_use_counts(capsys):
    """``plugin:external-apis`` is the same skill invoked through a plugin namespace."""
    make_skill(lib.SKILLS_DIR)
    write_transcript("s1", [user("consulte o TCU"), skill_tool_use(f"myplugin:{SKILL}", "toolu_1")])
    _, report, _ = run(capsys, SKILL)
    assert activations(report) == 1


def test_typed_slash_command_is_explicit(capsys):
    make_skill(lib.SKILLS_DIR)
    write_transcript("s1", [
        user("uma pergunta sem relação"),
        typed_slash(SKILL, "consulte o BACEN"),
    ])
    _, report, _ = run(capsys, SKILL)
    assert report["explicit_activations"] == 1
    assert report["automatic_activations"] == 0


def test_plain_typed_slash_is_explicit(capsys):
    make_skill(lib.SKILLS_DIR)
    write_transcript("s1", [user(f"/{SKILL} consulte o BACEN")])
    _, report, _ = run(capsys, SKILL)
    assert report["explicit_activations"] == 1
    assert report["automatic_activations"] == 0


def test_path_in_preceding_prompt_does_not_make_it_explicit(capsys):
    """A real automatic tool_use stays automatic when the user merely quoted a path."""
    make_skill(lib.SKILLS_DIR)
    write_transcript("s1", [
        user(f"revise /home/u/.claude/skills/{SKILL}/SKILL.md e consulte o TCU"),
        skill_tool_use(SKILL, "toolu_1"),
    ])
    _, report, _ = run(capsys, SKILL)
    assert report["automatic_activations"] == 1
    assert report["explicit_activations"] == 0


def test_same_tool_use_id_across_transcripts_counts_once(capsys):
    """Resumed sessions re-copy history: 47 tool_use / 41 distinct ids in the real corpus."""
    make_skill(lib.SKILLS_DIR)
    entries = [user("consulte o TCU"), skill_tool_use(SKILL, "toolu_dup")]
    write_transcript("original", entries)
    write_transcript("resumed", entries)
    _, report, _ = run(capsys, SKILL)
    assert activations(report) == 1


def test_same_typed_slash_uuid_across_transcripts_counts_once(capsys):
    make_skill(lib.SKILLS_DIR)
    entry = {**typed_slash(SKILL, "consulte o BACEN"), "uuid": "11111111-aaaa"}
    write_transcript("original", [entry])
    write_transcript("resumed", [entry])
    _, report, _ = run(capsys, SKILL)
    assert report["explicit_activations"] == 1


def test_skill_call_that_expands_a_typed_slash_is_the_same_activation(capsys):
    """Typing /skill and the Skill call that follows it are ONE activation, not two."""
    make_skill(lib.SKILLS_DIR)
    write_transcript("s1", [
        typed_slash(SKILL, "consulte o BACEN"),
        skill_tool_use(SKILL, "toolu_expand"),
    ])
    _, report, _ = run(capsys, SKILL)
    assert activations(report) == 1
    assert report["explicit_activations"] == 1


def test_later_automatic_call_after_a_typed_slash_still_counts(capsys):
    """The dedupe is scoped to the typed prompt: a later, separate prompt is its own activation."""
    make_skill(lib.SKILLS_DIR)
    write_transcript("s1", [
        typed_slash(SKILL, "consulte o BACEN"), skill_tool_use(SKILL, "toolu_expand"),
        user("agora busque o acórdão do TCU"), skill_tool_use(SKILL, "toolu_later"),
    ])
    _, report, _ = run(capsys, SKILL)
    assert report["explicit_activations"] == 1
    assert report["automatic_activations"] == 1


# --- evidence statement -----------------------------------------------------

def test_no_activation_says_no_evidence_in_json(capsys):
    make_skill(lib.SKILLS_DIR)
    write_transcript("s1", [user("nada a ver"), assistant_text("ok")])
    rc, report, _ = run(capsys, SKILL)
    assert rc == 0
    assert report.get("evidence") == "none"
    assert report.get("transcripts_scanned") == 1
    assert activations(report) == 0


def test_no_activation_says_no_evidence_in_text(capsys):
    make_skill(lib.SKILLS_DIR)
    write_transcript("s1", [user("nada a ver")])
    rc = ta.main([SKILL])
    out = capsys.readouterr().out
    assert rc == 0
    assert "NO EVIDENCE" in out
    assert "not an approval" in out.lower()


def test_explicit_only_is_not_evidence_for_the_description(capsys):
    make_skill(lib.SKILLS_DIR)
    write_transcript("s1", [typed_slash(SKILL, "consulte o BACEN")])
    _, report, _ = run(capsys, SKILL)
    assert report.get("evidence") == "explicit_only"
    rc = ta.main([SKILL])
    out = capsys.readouterr().out
    assert rc == 0
    assert "NO AUTOMATIC EVIDENCE" in out


def test_automatic_activation_is_evidence(capsys):
    make_skill(lib.SKILLS_DIR)
    _auto_history()
    _, report, _ = run(capsys, SKILL)
    assert report.get("evidence") == "automatic"
    assert report["automatic_activations"] == 2
