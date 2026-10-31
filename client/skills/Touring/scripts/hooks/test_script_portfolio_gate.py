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
SCRIPT = (
    "import json\n# baixa a proveniencia das fontes\nprint(json.dumps({'ok': True}))\n"
)


@pytest.fixture
def ambiente(tmp_path: Path) -> dict:
    home = tmp_path / "home"
    portfolio = home / ".claude" / "scripts-portfolio"
    portfolio.mkdir(parents=True)
    (portfolio / "MANIFEST.json").write_text(
        json.dumps(
            {
                "scripts": [
                    {
                        "slug": "fetch_proveniencia",
                        "path": "fetch_proveniencia.py",
                        "purpose": "baixa a proveniencia das fontes",
                        "domain": "harvest",
                        "tags": "proveniencia",
                    },
                ]
            }
        )
    )
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    falso = bin_dir / "touring"
    falso.write_text(
        '#!/bin/sh\nprintf \'%s\' "$TOURING_FALSO_SAIDA"\nexit "${TOURING_FALSO_RC:-0}"\n'
    )
    falso.chmod(0o755)
    env = {k: v for k, v in os.environ.items() if not k.startswith("TOURING_")}
    env.update(
        {
            "HOME": str(home),
            "PATH": f"{bin_dir}{os.pathsep}{env.get('PATH', '')}",
            "TOURING_FALSO_SAIDA": "[]",
        }
    )
    return {
        "env": env,
        "home": home,
        "eventos": home / ".claude/touring/portfolio_gate/events.jsonl",
    }


def _rodar(
    amb: dict, payload: object, **extra: str
) -> subprocess.CompletedProcess[str]:
    entrada = payload if isinstance(payload, str) else json.dumps(payload)
    return subprocess.run(
        [sys.executable, str(HOOK)],
        input=entrada,
        capture_output=True,
        text=True,
        env={**amb["env"], **extra},
        timeout=20,
        check=False,
    )


def _write(caminho: str, conteudo: str = SCRIPT) -> dict:
    return {
        "tool_name": "Write",
        "tool_input": {"file_path": caminho, "content": conteudo},
    }


def _eventos(amb: dict) -> list[dict]:
    p = amb["eventos"]
    return [json.loads(x) for x in p.read_text().splitlines()] if p.exists() else []


def test_efemero_com_candidato_emite_contexto_e_registra_sem_conteudo(
    ambiente: dict,
) -> None:
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
    for caminho in (
        "/tmp/x/fetch_proveniencia.py",
        "/tmp/x/probe.py",
        "/var/tmp/y/dump.py",
    ):
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
    r = _rodar(
        ambiente,
        _write("/tmp/x/fetch_proveniencia.py"),
        TOURING_PORTFOLIO_GATE_DISABLED="1",
    )
    assert (r.returncode, r.stdout) == (0, "")
    assert _eventos(ambiente) == []


def test_hook_fail_open_payload_ilegivel(ambiente: dict) -> None:
    r = _rodar(ambiente, "isto não é json")
    assert (r.returncode, r.stdout) == (0, "")


def test_basename_generico_dispara_fora_do_efemero(ambiente: dict) -> None:
    ambiente["env"]["TOURING_FALSO_SAIDA"] = json.dumps(
        [{"file_path": "scripts/probe_antigo.py", "symbol_name": "sonda antiga"}]
    )
    r = _rodar(ambiente, _write("/home/u/projects/analise/probe.py", "x = 1\n"))
    assert r.returncode == 0
    (evento,) = _eventos(ambiente)
    assert evento["generic_name"] is True
    assert evento["ephemeral"] is False


PROGRAMA = "\n".join(
    f"linha_{i} = calcular(valor_{i}, base) + ajuste_{i}" for i in range(12)
)


def test_copia_editada_achada_pelo_tantivy_e_reprovada(
    ambiente: dict, tmp_path: Path
) -> None:
    existente = tmp_path / "recalcula_tarifa.py"
    existente.write_text(PROGRAMA)
    ambiente["env"]["TOURING_FALSO_SAIDA"] = json.dumps(
        [{"file_path": str(existente), "symbol_name": "recalcula tarifa"}]
    )
    r = _rodar(
        ambiente,
        _write("/tmp/x/zzz_unico.py", PROGRAMA.replace("ajuste_3", "correcao_3")),
    )
    contexto = json.loads(r.stdout)["hookSpecificOutput"]["additionalContext"]
    assert "reprovado" in contexto
    assert "recalcula_tarifa.py" in contexto
    (evento,) = _eventos(ambiente)
    assert evento["estado"] == "reprovado"
    assert evento["would_avoid"] is True


def test_fonte_fora_do_ar_registra_inaferivel(ambiente: dict) -> None:
    _rodar(ambiente, _write("/tmp/x/zzz_unico.py", "valor = 1\n"), TOURING_FALSO_RC="1")
    (evento,) = _eventos(ambiente)
    assert evento.get("estado") == "inaferivel"
    assert any("tantivy" in f for f in evento["fontes_falhas"])


def test_hook_registra_estado_do_veredito(ambiente: dict) -> None:
    _rodar(ambiente, _write("/tmp/x/zzz_unico.py", "valor = 1\n"))
    (evento,) = _eventos(ambiente)
    assert evento["estado"] in ("reprovado", "aprovado", "inaferivel")
    assert "motivo" in evento


def test_manifest_nao_curto_circuita_mais(ambiente: dict, tmp_path: Path) -> None:
    # The old hook skipped tantivy whenever the MANIFEST matched; now every source is consulted.
    marca = tmp_path / "tantivy_consultado"
    falso = Path(ambiente["env"]["PATH"].split(os.pathsep)[0]) / "touring"
    falso.write_text(f"#!/bin/sh\ntouch {marca}\nprintf '[]'\n")
    _rodar(ambiente, _write("/tmp/x/fetch_proveniencia.py"))
    assert marca.exists()


def test_stats_conta_por_estado_e_legado(ambiente: dict) -> None:
    ambiente["eventos"].parent.mkdir(parents=True, exist_ok=True)
    ambiente["eventos"].write_text(
        json.dumps({"n_candidates": 1, "would_avoid": True}) + "\n"
    )
    _rodar(ambiente, _write("/tmp/x/zzz_unico.py", "valor = 1\n"))
    r = subprocess.run(
        [sys.executable, str(HOOK), "--stats"],
        capture_output=True,
        text=True,
        env=ambiente["env"],
        timeout=20,
        check=False,
    )
    resumo = json.loads(r.stdout)
    assert resumo["por_estado"]["legado"] == 1
    assert sum(resumo["por_estado"].values()) == resumo["eventos"] == 2


def test_stats_separa_eventos_de_medicao(ambiente: dict) -> None:
    # Latency/audit runs feed real-looking events into the shadow log (206 on 31/10/2026); the P5 numbers
    # must not count them, but they stay visible as their own bucket instead of vanishing.
    linhas = [
        {
            "file": "/tmp/medicao-latencia-hook/a.py",
            "estado": "reprovado",
            "would_avoid": True,
        },
        {"file": "/tmp/auditoria-portfolio/b.py", "estado": "aprovado"},
        {"file": "/home/u/proj/c.py", "estado": "aprovado", "n_candidates": 2},
    ]
    ambiente["eventos"].parent.mkdir(parents=True, exist_ok=True)
    ambiente["eventos"].write_text("".join(json.dumps(x) + "\n" for x in linhas))
    r = subprocess.run(
        [sys.executable, str(HOOK), "--stats"],
        capture_output=True,
        text=True,
        env=ambiente["env"],
        timeout=20,
        check=False,
    )
    resumo = json.loads(r.stdout)
    assert resumo["sinteticos"] == 2
    assert resumo["eventos"] == 1
    assert resumo["would_avoid"] == 0
    assert resumo["por_estado"] == {"aprovado": 1}


def _bash(comando: str, cwd: str = "/home/u/projects/analise") -> dict:
    return {"tool_name": "Bash", "tool_input": {"command": comando}, "cwd": cwd}


def test_bash_heredoc_dispara_como_write(ambiente: dict) -> None:
    # P3: the same script written through `cat > … <<EOF` reaches the same decision as a Write.
    r = _rodar(
        ambiente, _bash(f"cat > /tmp/x/fetch_proveniencia.py <<'PY'\n{SCRIPT}PY\n")
    )
    assert r.returncode == 0
    assert (
        "fetch_proveniencia.py"
        in json.loads(r.stdout)["hookSpecificOutput"]["additionalContext"]
    )
    (evento,) = _eventos(ambiente)
    assert evento["origem"] == "Bash.heredoc"
    assert evento["file"] == "/tmp/x/fetch_proveniencia.py"
    assert "content" not in evento


def test_bash_nao_emite_updated_input_nem_bloqueia(
    ambiente: dict, tmp_path: Path
) -> None:
    # The touring-hook already rewrites Bash commands: a second rewriter would be last-write-wins.
    existente = tmp_path / "recalcula_tarifa.py"
    existente.write_text(PROGRAMA)
    ambiente["env"]["TOURING_FALSO_SAIDA"] = json.dumps(
        [{"file_path": str(existente), "symbol_name": "recalcula tarifa"}]
    )
    corpo = PROGRAMA.replace("ajuste_3", "correcao_3")
    r = _rodar(ambiente, _bash(f"cat <<'PY' > /tmp/x/zzz_unico.py\n{corpo}\nPY\n"))
    assert r.returncode == 0
    saida = json.loads(r.stdout)["hookSpecificOutput"]
    assert (
        "reprovado" in saida["additionalContext"]
    )  # control: the case did reach the strongest verdict
    assert set(saida) == {"hookEventName", "additionalContext"}
    assert "updatedInput" not in r.stdout
    assert "permissionDecision" not in r.stdout


def test_bash_sem_heredoc_fica_em_silencio_e_nao_registra(ambiente: dict) -> None:
    for comando in (
        "ls -la /tmp",
        "python3 /tmp/x/probe.py",
        "tee /tmp/x/probe.py <<'E'\nx\nE\n",
    ):
        r = _rodar(ambiente, _bash(comando))
        assert (r.returncode, r.stdout) == (0, "")
    assert _eventos(ambiente) == []


def test_bash_caminho_relativo_resolvido_pelo_cwd(ambiente: dict) -> None:
    _rodar(ambiente, _bash(f"cat > probe.py <<'PY'\n{SCRIPT}PY\n", cwd="/tmp/sessao"))
    (evento,) = _eventos(ambiente)
    assert evento["file"] == "/tmp/sessao/probe.py"
    assert evento["ephemeral"] is True


def test_bash_dois_heredocs_dois_eventos(ambiente: dict) -> None:
    comando = f"cat > /tmp/x/a.py <<'A'\n{SCRIPT}A\ncat > /home/u/notas.md <<'B'\nnota\nB\ncat > /tmp/x/b.sh <<'C'\necho oi\nC\n"
    _rodar(ambiente, _bash(comando))
    assert sorted(e["file"] for e in _eventos(ambiente)) == [
        "/tmp/x/a.py",
        "/tmp/x/b.sh",
    ]


def test_write_registra_origem_write(ambiente: dict) -> None:
    _rodar(ambiente, _write("/tmp/x/zzz_unico.py", "valor = 1\n"))
    (evento,) = _eventos(ambiente)
    assert evento["origem"] == "Write"


def test_latencia_bash_sem_heredoc(ambiente: dict) -> None:
    # The Bash matcher fires on EVERY Bash call: the no-heredoc path must stay at interpreter start-up cost.
    tempos = []
    for _ in range(7):
        t0 = time.perf_counter()
        _rodar(ambiente, _bash("git status --short"))
        tempos.append(time.perf_counter() - t0)
    assert statistics.median(tempos) < 0.15


def test_latencia_mediana_do_processo_cabe_no_orcamento(ambiente: dict) -> None:
    # Budget is p99 <= 150 ms for the hook body; a cold subprocess adds interpreter start-up, so the
    # contract checked here is the generous median of the whole process.
    tempos = []
    for _ in range(7):
        t0 = time.perf_counter()
        _rodar(ambiente, _write("/tmp/x/fetch_proveniencia.py"))
        tempos.append(time.perf_counter() - t0)
    assert statistics.median(tempos) < 0.5


def _plano(corpo: object, tmp_path: Path) -> str:
    projeto = tmp_path / "proj"
    (projeto / ".claude").mkdir(parents=True)
    (projeto / ".claude" / "plano_ativo.json").write_text(
        corpo if isinstance(corpo, str) else json.dumps(corpo)
    )
    return str(projeto)


def test_write_efemero_vai_para_diagnostics_com_conteudo_identico(
    ambiente: dict, tmp_path: Path
) -> None:
    # Shadow: the hook SUGGESTS the destination; it never rewrites the input, so the content is untouched.
    cwd = _plano({"versao": 1, "bundle": "docs/plans/2026-10-31-x"}, tmp_path)
    r = _rodar(ambiente, {**_write("/tmp/x/zzz_unico.py", "valor = 1\n"), "cwd": cwd})
    assert r.returncode == 0
    saida = json.loads(r.stdout)["hookSpecificOutput"]
    alvo = f"{cwd}/docs/plans/2026-10-31-x/diagnostics/zzz_unico.py"
    assert alvo in saida["additionalContext"]
    assert set(saida) == {
        "hookEventName",
        "additionalContext",
    }  # no rewrite field: input stays byte-identical
    (evento,) = _eventos(ambiente)
    assert evento["destino_sugerido"] == alvo
    assert "valor = 1" not in json.dumps(evento)  # the log never carries content


def test_sem_plano_ativo_nao_redireciona(ambiente: dict, tmp_path: Path) -> None:
    (tmp_path / "vazio").mkdir()
    r = _rodar(
        ambiente,
        {
            **_write("/tmp/x/zzz_unico.py", "valor = 1\n"),
            "cwd": str(tmp_path / "vazio"),
        },
    )
    assert r.returncode == 0
    assert "plano ativo" not in r.stdout
    (evento,) = _eventos(ambiente)
    assert "destino_sugerido" not in evento
    assert "plano_ativo_erro" not in evento


@pytest.mark.parametrize(
    "corpo",
    [
        "{nao json",
        {"versao": 2, "bundle": "docs/plans/x"},
        {"versao": 1, "bundle": "../fora"},
        {"versao": 1, "bundle": "/abs"},
    ],
)
def test_plano_ativo_invalido_so_avisa_e_registra(
    ambiente: dict, tmp_path: Path, corpo: object
) -> None:
    cwd = _plano(corpo, tmp_path)
    r = _rodar(ambiente, {**_write("/tmp/x/zzz_unico.py", "valor = 1\n"), "cwd": cwd})
    assert r.returncode == 0
    assert "plano ativo" not in r.stdout
    (evento,) = _eventos(ambiente)
    assert evento["plano_ativo_erro"]
    assert "destino_sugerido" not in evento


def test_destino_nao_sugerido_fora_do_efemero(ambiente: dict, tmp_path: Path) -> None:
    cwd = _plano({"versao": 1, "bundle": "docs/plans/x"}, tmp_path)
    _rodar(ambiente, {**_write("/home/u/projects/probe.py", "valor = 1\n"), "cwd": cwd})
    (evento,) = _eventos(ambiente)
    assert "destino_sugerido" not in evento
