"""Testes vendados de skill_picker_hook.py (S-5.1 registration)."""

from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path

import pytest

HOOK = Path(__file__).resolve().parents[1] / "skill_picker_hook.py"
spec = importlib.util.spec_from_file_location("sph", HOOK)
sph = importlib.util.module_from_spec(spec)
sys.modules["sph"] = sph
spec.loader.exec_module(sph)


@pytest.fixture()
def sandbox(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> dict:
    sug = tmp_path / "suggestion.json"
    jou = tmp_path / "journal.jsonl"
    monkeypatch.setattr(sph, "SUGGESTION_FILE", sug)
    monkeypatch.setattr(sph, "JOURNAL_FILE", jou)
    return {"sug": sug, "jou": jou}


def test_said_false_is_absolute_silence_but_journaled(sandbox: dict) -> None:
    """said:false → nada no contexto; silent_reason SEMPRE no journal."""
    sandbox["sug"].write_text(json.dumps({"said": False, "silent_reason": "below_tau"}))
    assert sph._deliver_pending_suggestion() is None
    assert not sandbox["sug"].exists()  # consumido — não repete


def test_slash_command_prompt_discards_the_pending_suggestion(
    sandbox: dict, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture
) -> None:
    """Retro 11/11/2026: quem digita um slash command já escolheu a skill; a
    sugestão pendente é consumida em silêncio, não entregue por cima dele."""
    import io
    monkeypatch.setattr(sph, "_spawn_background_judge", lambda *_a, **_k: None)
    for prompt in ("<command-message>x:retro</command-message>", "/retro"):
        sandbox["sug"].write_text(json.dumps({"said": True, "picked": "Touring", "confidence": 0.64}))
        monkeypatch.setattr(sys, "stdin", io.StringIO(json.dumps({"prompt": prompt, "session_id": "s1"})))
        assert sph.main() == 0
        assert capsys.readouterr().out == ""
        assert not sandbox["sug"].exists()


@pytest.mark.parametrize("prompt,expected", [
    ("<command-message>x:retro</command-message>", True),
    ("<command-name>/retro</command-name>", True),
    ("/retro", True),
    ("/mattpocock-skills:code-review main", True),
    # Texto livre que só COMEÇA parecido: caminho absoluto, tag que não é de comando.
    ("/home/x/lib.rs não compila", False),
    ("<div> quebrou no render", False),
    ("/", False),
    ("/9lives", False),
    ("corrija o /retro", False),
    ("", False),
])
def test_slash_command_predicate_matches_the_rust_one(prompt: str, expected: bool) -> None:
    """O mesmo conceito tem um predicado em Rust (`prompt_enhance::is_slash_invocation`):
    um caminho ou uma tag qualquer é texto livre, e o usuário que o digita não escolheu skill."""
    assert sph._is_slash_command(prompt) is expected


def test_suggestion_from_another_session_is_not_delivered(sandbox: dict) -> None:
    """A sugestão é sobre o prompt ANTERIOR desta sessão; uma pendência deixada
    por outra sessão não tem prompt anterior aqui."""
    sandbox["sug"].write_text(json.dumps(
        {"said": True, "picked": "Touring", "confidence": 0.64, "session_id": "outra"}))
    assert sph._deliver_pending_suggestion(session_id="esta") is None
    assert not sandbox["sug"].exists()
    sandbox["sug"].write_text(json.dumps(
        {"said": True, "picked": "Touring", "confidence": 0.64, "session_id": "esta"}))
    assert sph._deliver_pending_suggestion(session_id="esta") is not None


def test_said_true_delivers_once(sandbox: dict) -> None:
    sandbox["sug"].write_text(json.dumps({"said": True, "picked": "loop-engineering", "confidence": 0.87}))
    out = sph._deliver_pending_suggestion()
    assert out is not None
    ctx = out["hookSpecificOutput"]["additionalContext"]
    assert "loop-engineering" in ctx
    assert "0.87" in ctx
    assert sph._deliver_pending_suggestion() is None  # entrega é única


def test_journal_records_silent_reason(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    jou = tmp_path / "j.jsonl"
    monkeypatch.setattr(sph, "JOURNAL_FILE", jou)
    sph._journal({"ts": "t", "said": False, "silent_reason": "below_tau"})
    rec = json.loads(jou.read_text().strip())
    assert rec["silent_reason"] == "below_tau"


def test_corrupt_suggestion_file_is_silence(sandbox: dict) -> None:
    sandbox["sug"].write_text("{não é json")
    assert sph._deliver_pending_suggestion() is None


def test_main_never_raises(sandbox: dict, monkeypatch: pytest.MonkeyPatch) -> None:
    """Fail-open de última linha: stdin quebrado não derruba."""
    monkeypatch.setattr(sys, "stdin", type("S", (), {"read": lambda self: "{quebrado"})())
    assert sph.main() == 0
