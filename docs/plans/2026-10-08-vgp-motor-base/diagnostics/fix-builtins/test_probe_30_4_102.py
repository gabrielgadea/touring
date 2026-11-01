"""Testes pareados da sonda 30.4.102, todos com dados sintéticos."""

import json
from types import SimpleNamespace

import probe_30_4_102 as probe


def test_entradas_agrega_inline_sem_apagar_o_arquivo(tmp_path):
    (tmp_path / "entradas.jsonl").write_text('{"id":"arquivo"}\n')
    (tmp_path / "entradas_inline_neg.jsonl").write_text('{"id":"inline"}\n')
    assert [e["id"] for e in probe.entradas(tmp_path)] == ["arquivo", "inline"]


def test_origem_snap_preserva_absoluto_e_ancora_relativo(tmp_path):
    absoluto = tmp_path / "exit.py"
    assert probe.origem_snap({"snap": "snap/caso.py"}, tmp_path) == tmp_path / "snap/caso.py"
    assert probe.origem_snap({"snap": str(absoluto)}, tmp_path) == absoluto


def test_chamada_repassa_estado_e_contexto(monkeypatch, tmp_path):
    (tmp_path / "estado_no_instante").mkdir()
    (tmp_path / "estado_no_instante/manifesto.json").write_text(json.dumps({
        "caso": {"disk_file": "antes.py", "overlays": [{"path": "m.py", "file": "m.py"}]}
    }))
    entrada = {
        "id": "caso", "path": "<string>", "snap": "caso.py", "project_root": "/proj",
        "contexto": [{"path": "ctx.py", "snap": "ctx.py"}],
    }
    capturado = {}

    def run(cmd, **kwargs):
        capturado["cmd"] = cmd
        return SimpleNamespace(stdout='{"verdict":"allow"}')

    monkeypatch.setattr(probe.subprocess, "run", run)
    assert probe.chamada(entrada, tmp_path, "1,2") == {"verdict": "allow"}
    cmd = capturado["cmd"]
    assert f"--disk-file" in cmd and str(tmp_path / "antes.py") in cmd
    assert f"m.py={tmp_path / 'm.py'}" in cmd
    assert f"ctx.py={tmp_path / 'ctx.py'}" in cmd


def test_main_prepara_mini_corpus_sem_apagar_o_caso(monkeypatch, tmp_path):
    ctl = tmp_path / "ctl"
    ctl.mkdir()
    (ctl / "caso.py").write_text("exit()\n")
    binario = tmp_path / "touring"
    binario.write_bytes(b"binario-sintetico")
    base = tmp_path / "base"
    fixtures = base / "diagnostics/fix-builtins/synthetic"
    fixtures.mkdir(parents=True)
    (fixtures / "exit.py").write_text("exit()\n")
    (fixtures / "quit.py").write_text("quit()\n")
    out = tmp_path / "probe.json"
    hot = tmp_path / "hot"
    caso = {"id": "2106f0ebf279", "path": "<string>", "snap": "caso.py", "project_root": "/proj"}
    monkeypatch.setattr(probe, "CTL", ctl)
    monkeypatch.setattr(probe, "B", base)
    monkeypatch.setattr(probe, "BIN", binario)
    monkeypatch.setattr(probe, "OUT", out)
    monkeypatch.setattr(probe, "HOT", hot)
    monkeypatch.setattr(probe, "entradas", lambda corpus: [caso])
    monkeypatch.setattr(probe, "chamada", lambda *args: {"verdict": "allow", "findings": []})
    monkeypatch.setattr(probe.subprocess, "run", lambda *args, **kwargs: SimpleNamespace(stderr="30.4.102"))
    probe.main()
    ids = [json.loads(line)["id"] for line in (hot / "entradas_inline_neg.jsonl").read_text().splitlines()]
    assert ids == ["2106f0ebf279", "sintetico-exit", "sintetico-quit"]
    assert (hot / "caso.py").read_text() == "exit()\n"
    assert json.loads(out.read_text())["caso_2106f0ebf279"]["verdict"] == "allow"
