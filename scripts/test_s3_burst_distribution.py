#!/usr/bin/env python3
"""Testes de `s3_burst_distribution.py` — o instrumento que recalibrou o S3.

A janela e o limiar do gate (`INSPECT_BURST_WINDOW_SECS`, `INSPECT_BURST_DENY_AT`)
foram escolhidos a partir da saída deste script. Um instrumento errado teria
produzido uma política errada com toda a aparência de evidência — e nesta mesma
sessão o mesmo erro aconteceu duas vezes (o probe MCP que não limpava
`CLAUDE_PROJECT_DIR`; o harness de mutação derrotado pelo cache de bytecode).
Provar o instrumento é parte de medir.
"""
from __future__ import annotations

import json
import os
import sys
from datetime import datetime, timedelta, timezone

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import s3_burst_distribution as s3  # noqa: E402

T0 = datetime(2026, 8, 27, 12, 0, 0, tzinfo=timezone.utc)


def transcript(tmp_path, chamadas):
    """Escreve um .jsonl no formato do Claude Code. `chamadas` = [(offset_s, cmd)]."""
    p = tmp_path / "s.jsonl"
    with p.open("w") as f:
        for off, cmd in chamadas:
            f.write(json.dumps({
                "type": "assistant",
                "timestamp": (T0 + timedelta(seconds=off)).isoformat(),
                "message": {"content": [
                    {"type": "tool_use", "name": "Bash", "input": {"command": cmd}}
                ]},
            }) + "\n")
    return str(p)


# ── scan_class: o classificador do instrumento ──────────────────────────────

def test_classifica_as_classes_de_inspecao():
    assert s3.scan_class("grep -rn foo src/") == "grep"
    assert s3.scan_class("rg pattern") == "grep"
    assert s3.scan_class("cat README.md") == "cat"
    assert s3.scan_class("head -5 a.rs") == "cat"
    assert s3.scan_class("find . -name x.rs") == "find"
    assert s3.scan_class("ls -la src/") == "ls"
    assert s3.scan_class("wc -l a.rs") == "wc"


def test_nao_conta_a_propria_rota_como_inspecao():
    """Se `touring run --code 'grep ...'` contasse como grep, o instrumento
    mediria a solução como se fosse o problema — e a rajada convertida
    apareceria inflando exatamente o número que justifica o gate."""
    assert s3.scan_class("touring run --lang bash --code 'grep -rn x src/'") is None
    assert s3.scan_class("touring exec \"cat a.md\"") is None


def test_nao_conta_mutacao_nem_build():
    for cmd in ("cargo build", "cat > out.txt", "python3 script.py", "git status"):
        assert s3.scan_class(cmd) is None, cmd


# ── bursts_of_session: o agrupamento que define "rajada" ────────────────────

def test_duas_chamadas_na_janela_sao_uma_rajada_de_dois(tmp_path):
    p = transcript(tmp_path, [(0, "grep a src/"), (30, "grep b src/")])
    assert s3.bursts_of_session(p, 300, None)["grep"] == [2]


def test_gap_maior_que_a_janela_parte_em_duas_isoladas(tmp_path):
    p = transcript(tmp_path, [(0, "grep a src/"), (400, "grep b src/")])
    assert s3.bursts_of_session(p, 300, None)["grep"] == [1, 1]


def test_a_janela_e_o_gap_entre_consecutivas_nao_o_span_total(tmp_path):
    """Três chamadas espaçadas 200s cada: o span é 400s > janela, mas cada gap
    cabe. É uma rajada de 3 — a mesma leitura que o ledger do gate faz, que
    renova o TTL a cada chamada."""
    p = transcript(tmp_path, [(0, "grep a"), (200, "grep b"), (400, "grep c")])
    assert s3.bursts_of_session(p, 300, None)["grep"] == [3]


def test_touring_run_no_meio_fecha_a_rajada(tmp_path):
    """A condição que o gate usa: seguir a rota zera a contagem. Sem isto o
    instrumento contaria como rajada uma sequência que o modelo JÁ converteu."""
    p = transcript(tmp_path, [
        (0, "grep a src/"),
        (10, "touring run --lang bash --code 'grep b src/'"),
        (20, "grep c src/"),
    ])
    assert s3.bursts_of_session(p, 300, None)["grep"] == [1, 1]


def test_classes_diferentes_nao_se_misturam(tmp_path):
    p = transcript(tmp_path, [(0, "grep a"), (10, "cat b.md"), (20, "grep c")])
    b = s3.bursts_of_session(p, 300, None)
    assert b["grep"] == [2], "as duas do grep somam entre si"
    assert b["cat"] == [1], "o cat no meio é isolado"


def test_since_descarta_o_que_e_anterior(tmp_path):
    p = transcript(tmp_path, [(0, "grep a"), (30, "grep b")])
    assert s3.bursts_of_session(p, 300, "2026-08-28") == {}


def test_sidechain_nao_conta(tmp_path):
    """Subagentes têm transcript próprio; contá-los aqui somaria trabalho que
    nunca passou por ESTE PreToolUse."""
    p = tmp_path / "s.jsonl"
    with p.open("w") as f:
        for i in range(2):
            f.write(json.dumps({
                "type": "assistant", "isSidechain": True,
                "timestamp": (T0 + timedelta(seconds=i * 10)).isoformat(),
                "message": {"content": [
                    {"type": "tool_use", "name": "Bash", "input": {"command": "grep x"}}
                ]},
            }) + "\n")
    assert s3.bursts_of_session(str(p), 300, None) == {}


def test_linha_corrompida_nao_derruba_a_medicao(tmp_path):
    """Transcripts reais têm linhas truncadas. Abortar ali mediria só o prefixo
    do arquivo e reportaria o número como se fosse o total."""
    p = tmp_path / "s.jsonl"
    with p.open("w") as f:
        f.write('{"type": "assistant", "timestamp": "' + T0.isoformat() + '", '
                '"message": {"content": [{"type":"tool_use","name":"Bash",'
                '"input":{"command":"grep a"}}]}}\n')
        f.write("{lixo nao json\n")
        f.write('{"type": "assistant", "timestamp": "'
                + (T0 + timedelta(seconds=10)).isoformat() + '", '
                '"message": {"content": [{"type":"tool_use","name":"Bash",'
                '"input":{"command":"grep b"}}]}}\n')
    assert s3.bursts_of_session(str(p), 300, None)["grep"] == [2]


# ── o invariante que liga o instrumento à política ──────────────────────────

def test_classes_medidas_cobrem_as_que_o_gate_acompanha():
    """Se o Rust passar a acompanhar uma classe que este script não classifica,
    a próxima recalibração mediria um universo menor que o do gate — o modo de
    falha `verificador-usa-menos-que-o-extrator`."""
    import re
    from pathlib import Path
    rust = Path(__file__).resolve().parents[1] / "crates/touring-cli/src/cli_suggester.rs"
    m = re.search(r'CODE_MODE_COLLAPSED_CLASSES:\s*&\[&str\]\s*=\s*&\[([^\]]+)\]',
                  rust.read_text(encoding="utf-8"))
    assert m, "CODE_MODE_COLLAPSED_CLASSES não encontrado"
    do_gate = {c.replace("sed-n", "sed") for c in re.findall(r'"([\w-]+)"', m.group(1))}
    do_script = s3.CLASSES_NEGADAS | s3.CLASSES_QUE_PASSAM
    faltando = do_gate - do_script
    assert not faltando, f"o gate acompanha {faltando}, que este instrumento não mede"
