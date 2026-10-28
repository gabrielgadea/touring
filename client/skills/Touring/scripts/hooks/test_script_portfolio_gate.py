"""Contract tests of script_portfolio_gate.py, run as Claude Code runs it: stdin JSON, a subprocess, exit code.

Isolation: ``HOME`` points to a temp dir (events log, MANIFEST and the injection policy all resolve
under it) and ``PATH`` starts with a fake ``touring`` whose answer each test controls, so no test
touches the live daemon, the real portfolio or the real telemetry.
"""
from __future__ import annotations

import json
import os
import statistics
import subprocess
import sys
import time
from pathlib import Path

import pytest

HOOK = Path(__file__).resolve().parent / "script_portfolio_gate.py"
SCRIPT = "import json\n# baixa a proveniencia das fontes\nprint(json.dumps({'ok': True}))\n"


@pytest.fixture
def ambiente(tmp_path: Path) -> dict:
    home = tmp_path / "home"
    portfolio = home / ".claude" / "scripts-portfolio"
    portfolio.mkdir(parents=True)
    (portfolio / "MANIFEST.json").write_text(json.dumps({"scripts": [
        {"slug": "fetch_proveniencia", "path": "fetch_proveniencia.py",
         "purpose": "baixa a proveniencia das fontes", "domain": "harvest", "tags": "proveniencia"},
    ]}))
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    falso = bin_dir / "touring"
    falso.write_text("#!/bin/sh\nprintf '%s' \"$TOURING_FALSO_SAIDA\"\nexit \"${TOURING_FALSO_RC:-0}\"\n")
    falso.chmod(0o755)
    env = {k: v for k, v in os.environ.items() if not k.startswith("TOURING_")}
    env.update({"HOME": str(home), "PATH": f"{bin_dir}{os.pathsep}{env.get('PATH', '')}",
                "TOURING_FALSO_SAIDA": "[]"})
    return {"env": env, "home": home, "eventos": home / ".claude/touring/portfolio_gate/events.jsonl"}


def _rodar(amb: dict, payload: object, **extra: str) -> subprocess.CompletedProcess[str]:
    entrada = payload if isinstance(payload, str) else json.dumps(payload)
    return subprocess.run([sys.executable, str(HOOK)], input=entrada, capture_output=True, text=True,
                          env={**amb["env"], **extra}, timeout=20, check=False)


def _write(caminho: str, conteudo: str = SCRIPT) -> dict:
    return {"tool_name": "Write", "tool_input": {"file_path": caminho, "content": conteudo}}


def _eventos(amb: dict) -> list[dict]:
    p = amb["eventos"]
    return [json.loads(x) for x in p.read_text().splitlines()] if p.exists() else []


def test_efemero_com_candidato_emite_contexto_e_registra_sem_conteudo(ambiente: dict) -> None:
    r = _rodar(ambiente, _write("/tmp/x/fetch_proveniencia.py"))
    assert r.returncode == 0
    saida = json.loads(r.stdout)["hookSpecificOutput"]
    assert saida["hookEventName"] == "PreToolUse"
    assert "fetch_proveniencia.py" in saida["additionalContext"]
    (evento,) = _eventos(ambiente)
    assert evento["ephemeral"] is True
    assert evento["n_candidates"] == 1
    assert "content" not in evento  # the log never carries the script body


def test_hook_sombra_nunca_bloqueia(ambiente: dict) -> None:
    for caminho in ("/tmp/x/fetch_proveniencia.py", "/tmp/x/probe.py", "/var/tmp/y/dump.py"):
        r = _rodar(ambiente, _write(caminho))
        assert r.returncode == 0
        assert "deny" not in r.stdout
        assert "permissionDecision" not in r.stdout


def test_caminho_canonico_fica_em_silencio(ambiente: dict) -> None:
    r = _rodar(ambiente, _write("/home/u/projects/analise/scripts/util_novo.py"))
    assert (r.returncode, r.stdout) == (0, "")
    assert _eventos(ambiente) == []


def test_nao_script_fica_em_silencio(ambiente: dict) -> None:
    r = _rodar(ambiente, _write("/tmp/x/notas.md"))
    assert (r.returncode, r.stdout) == (0, "")
    assert _eventos(ambiente) == []


def test_kill_switch_desliga_tudo(ambiente: dict) -> None:
    r = _rodar(ambiente, _write("/tmp/x/fetch_proveniencia.py"), TOURING_PORTFOLIO_GATE_DISABLED="1")
    assert (r.returncode, r.stdout) == (0, "")
    assert _eventos(ambiente) == []


def test_hook_fail_open_payload_ilegivel(ambiente: dict) -> None:
    r = _rodar(ambiente, "isto não é json")
    assert (r.returncode, r.stdout) == (0, "")


def test_basename_generico_dispara_fora_do_efemero(ambiente: dict) -> None:
    ambiente["env"]["TOURING_FALSO_SAIDA"] = json.dumps(
        [{"file_path": "scripts/probe_antigo.py", "symbol_name": "sonda antiga"}])
    r = _rodar(ambiente, _write("/home/u/projects/analise/probe.py", "x = 1\n"))
    assert r.returncode == 0
    (evento,) = _eventos(ambiente)
    assert evento["generic_name"] is True
    assert evento["ephemeral"] is False


def test_rota_tantivy_quando_o_manifest_nao_casa(ambiente: dict) -> None:
    ambiente["env"]["TOURING_FALSO_SAIDA"] = json.dumps(
        [{"file_path": "scripts/recalcula_tarifa.py", "symbol_name": "recalcula tarifa"}])
    r = _rodar(ambiente, _write("/tmp/x/zzz_unico.py", "valor = 1\n"))
    assert "recalcula_tarifa.py" in json.loads(r.stdout)["hookSpecificOutput"]["additionalContext"]


@pytest.mark.xfail(strict=True, reason="P1 (S-1.2): hoje uma fonte fora do ar vira silêncio, não inaferível")
def test_fonte_fora_do_ar_registra_inaferivel(ambiente: dict) -> None:
    _rodar(ambiente, _write("/tmp/x/zzz_unico.py", "valor = 1\n"), TOURING_FALSO_RC="1")
    (evento,) = _eventos(ambiente)
    assert evento.get("estado") == "inaferivel"


def test_latencia_mediana_do_processo_cabe_no_orcamento(ambiente: dict) -> None:
    # Budget is p99 <= 150 ms for the hook body; a cold subprocess adds interpreter start-up, so the
    # contract checked here is the generous median of the whole process.
    tempos = []
    for _ in range(7):
        t0 = time.perf_counter()
        _rodar(ambiente, _write("/tmp/x/fetch_proveniencia.py"))
        tempos.append(time.perf_counter() - t0)
    assert statistics.median(tempos) < 0.5
