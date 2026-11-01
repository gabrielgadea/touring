"""Testes pareados do runner de mutação, sem executar cargo de verdade."""

import json

import mutante_exit


def test_sha_e_cargo_usam_os_alvos_configurados(monkeypatch, tmp_path):
    alvo = tmp_path / "model.rs"
    alvo.write_text("a")
    monkeypatch.setattr(mutante_exit, "ALVO", alvo)
    antes = mutante_exit.sha()
    alvo.write_text("b")
    assert mutante_exit.sha() != antes

    class Run:
        returncode = 0
        stdout = "ok"
        stderr = ""

    monkeypatch.setattr(mutante_exit, "OUT", tmp_path)
    monkeypatch.setattr(mutante_exit.subprocess, "run", lambda *args, **kwargs: Run())
    assert mutante_exit.cargo("cargo.log") == (0, "ok")
    assert (tmp_path / "cargo.log").read_text() == "ok"


def test_main_mata_o_mutante_e_restaura_os_bytes(monkeypatch, tmp_path):
    original = 'prefixo\n    "exit",\n    "quit",\nsufixo\n'
    alvo = tmp_path / "model.rs"
    alvo.write_text(original)
    monkeypatch.setattr(mutante_exit, "ALVO", alvo)
    monkeypatch.setattr(mutante_exit, "OUT", tmp_path)
    resultados = iter([
        (101, f"FAILED {mutante_exit.TESTE}"),
        (0, "green"),
    ])
    monkeypatch.setattr(mutante_exit, "cargo", lambda nome: next(resultados))
    assert mutante_exit.main() == 0
    assert alvo.read_text() == original
    resultado = json.loads((tmp_path / "mutante_exit.json").read_text())
    assert resultado["morto_por_assertion"] is True
    assert resultado["green_depois_rc"] == 0


def test_main_exige_mutante_diferente(monkeypatch, tmp_path):
    alvo = tmp_path / "model.rs"
    alvo.write_text("sem os builtins\n")
    monkeypatch.setattr(mutante_exit, "ALVO", alvo)
    try:
        mutante_exit.main()
    except AssertionError:
        pass
    else:
        raise AssertionError("mutante inalterado não pode produzir verde")
    assert alvo.read_text() == "sem os builtins\n"
