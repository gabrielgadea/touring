"""Teste pareado da sonda de builtins, com import estático rastreável."""

import json

import sonda_builtins


def test_main_consome_todas_as_fixtures(monkeypatch, capsys):
    class Run:
        stdout = '{"verdict":"allow","findings":[]}'

    monkeypatch.setattr(sonda_builtins.subprocess, "run", lambda *args, **kwargs: Run())
    sonda_builtins.main()
    out = json.loads(capsys.readouterr().out)
    assert set(out) == {"exec", "exit", "quit", "return", "trap"}
    assert all(v["verdict"] == "allow" for v in out.values())
