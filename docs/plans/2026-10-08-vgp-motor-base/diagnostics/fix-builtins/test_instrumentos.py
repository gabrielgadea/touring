"""Unit tests for the synthetic probe and the mutant runner helpers."""

import importlib.util
import json
from pathlib import Path

ARQ = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("sonda_builtins", ARQ / "sonda_builtins.py")
sonda = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sonda)
spec = importlib.util.spec_from_file_location("mutante_exit", ARQ / "mutante_exit.py")
mutante = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mutante)
spec = importlib.util.spec_from_file_location("probe_30_4_102", ARQ / "probe_30_4_102.py")
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


def test_sonda_consome_as_quatro_fixtures(monkeypatch, capsys):
    class Run:
        stdout = '{"verdict":"allow","findings":[]}'

    monkeypatch.setattr(sonda.subprocess, "run", lambda *args, **kwargs: Run())
    sonda.main()
    out = json.loads(capsys.readouterr().out)
    assert set(out) == {"exec", "exit", "quit", "return", "trap"}
    assert all(v["verdict"] == "allow" for v in out.values())


def test_sha_muda_com_o_arquivo(monkeypatch, tmp_path):
    monkeypatch.setattr(mutante, "ALVO", tmp_path / "x")
    mutante.ALVO.write_text("a")
    antes = mutante.sha()
    mutante.ALVO.write_text("b")
    assert mutante.sha() != antes


def test_cargo_registra_saida_e_exit_code(monkeypatch, tmp_path):
    class Run:
        returncode = 0
        stdout = "ok"
        stderr = ""

    monkeypatch.setattr(mutante, "OUT", tmp_path)
    monkeypatch.setattr(mutante.subprocess, "run", lambda *args, **kwargs: Run())
    rc, log = mutante.cargo("prova.log")
    assert rc == 0 and log == "ok"
    assert (tmp_path / "prova.log").read_text() == "ok"


def test_probe_resolve_snapshot_relativo_e_absoluto(tmp_path):
    absoluto = tmp_path / "exit.py"
    assert probe.origem_snap({"snap": "snap/caso.py"}, tmp_path) == tmp_path / "snap/caso.py"
    assert probe.origem_snap({"snap": str(absoluto)}, tmp_path) == absoluto
