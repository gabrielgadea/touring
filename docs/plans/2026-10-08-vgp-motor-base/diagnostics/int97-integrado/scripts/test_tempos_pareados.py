"""tempos_pareados.py end to end against a FAKE judge and two FAKE motors (no real binary, no corpus).

What it proves: the launcher sits in front of every motor call in the ABBABAAB order, the readings land in the lists of
the binary that produced them (a swap of A and B would show), a missing reading is null (never 0), and `--sem-custo`
keeps the C' file format.
"""

import importlib.util
import json
import stat
import sys
from pathlib import Path

import pytest

AQUI = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("tempos_pareados", AQUI / "tempos_pareados.py")
tp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tp)

JUIZ_FALSO = '''
import argparse, json, subprocess, sys
from pathlib import Path

ARGS_MOTOR = []


def rodar_motor(touring, entrada, corpus):
    r = subprocess.run([*touring, "vgp", "check", "--id", entrada["id"], "-j"],
                       capture_output=True, text=True, timeout=60, check=False)
    try:
        return json.loads(r.stdout)
    except json.JSONDecodeError:
        return {"_erro": f"rc {r.returncode}: {r.stdout[:80]!r}"}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--touring")
    ap.add_argument("--corpus", type=Path)
    a = ap.parse_args()
    for pid in ("a1", "b2", "c3"):
        rodar_motor([a.touring], {"id": pid}, a.corpus)
    return 0
'''


def motor(caminho: Path, voltas: int, registro: Path) -> Path:
    """A shell 'motor' that logs who it is and prints a verdict JSON; `voltas` extra turns make B costlier than A."""
    caminho.write_text(
        "#!/bin/sh\n"
        f"echo {caminho.name[-1].upper()} >> {registro}\n"
        f"i=0; while [ $i -lt {voltas} ]; do i=$((i+1)); done\n"
        'echo \'{"verdict": "allow", "elapsed_ms": 2.0, "findings": []}\'\n'
    )
    caminho.chmod(caminho.stat().st_mode | stat.S_IXUSR)
    return caminho


def rodar(tmp_path: Path, mp: pytest.MonkeyPatch, *extra: str, lancador: Path | None = None) -> dict:
    juiz = tmp_path / "juiz_falso.py"
    juiz.write_text(JUIZ_FALSO)
    registro = tmp_path / "ordem.log"
    a, b = motor(tmp_path / "motor_a", 10, registro), motor(tmp_path / "motor_b", 30000, registro)
    saida = tmp_path / "pareado.json"
    if lancador:
        mp.setattr(tp, "LANCADOR", lancador)
    mp.setattr(sys, "argv", ["tempos_pareados.py", *extra, str(juiz), str(a), str(b), str(saida), "--corpus", str(tmp_path)])
    assert tp.main() == 0
    return json.loads(saida.read_text())


def test_cada_chamada_passa_pelo_lancador_e_a_leitura_vai_para_o_lado_certo(tmp_path, monkeypatch):
    d = rodar(tmp_path, monkeypatch)
    assert (d["protocolo"], d["ordem"]) == ("D", "ABBABAAB")
    assert set(d["por_id"]) == {"a1", "b2", "c3"}
    for reg in d["por_id"].values():
        assert {k: len(reg[k]) for k in ("A", "B", "instr_A", "instr_B", "cpu_A", "cpu_B")} == dict.fromkeys(
            ("A", "B", "instr_A", "instr_B", "cpu_A", "cpu_B"), 4
        )
        assert all(isinstance(x, int) and x > 0 for x in reg["instr_A"] + reg["instr_B"]), reg
        assert all(x >= 0 for x in reg["cpu_A"] + reg["cpu_B"])
        assert min(reg["instr_B"]) > 3 * max(reg["instr_A"]), reg  # B's motor loops 30000 turns: A/B not swapped
        assert reg["verdict_A"] == reg["verdict_B"] == ["allow"] * 4
        assert reg["A"] == reg["B"] == [2.0] * 4


@pytest.fixture
def lancador_que_nao_grava(tmp_path):
    """A launcher that runs the command but leaves no reading: what a crashed launcher looks like."""
    f = tmp_path / "lancador_mudo.py"
    f.write_text("import os, sys\ni = sys.argv.index('--')\nos.execvp(sys.argv[i + 1], sys.argv[i + 1:])\n")
    return f


def test_a_ordem_das_chamadas_e_abbabaab_por_id(tmp_path, monkeypatch):
    rodar(tmp_path, monkeypatch)
    chamadas = (tmp_path / "ordem.log").read_text().split()
    assert chamadas == list("ABBABAAB") * 3  # 3 ids, the balanced order each, A first


def test_leitura_ausente_e_null_nunca_zero(tmp_path, monkeypatch, lancador_que_nao_grava):
    d = rodar(tmp_path, monkeypatch, lancador=lancador_que_nao_grava)
    for reg in d["por_id"].values():
        assert reg["instr_A"] == reg["instr_B"] == reg["cpu_A"] == reg["cpu_B"] == [None] * 4, reg
        assert reg["A"] == [2.0] * 4  # the wall time of the motor itself is untouched


def test_sem_custo_mantem_o_formato_do_c_linha(tmp_path, monkeypatch):
    d = rodar(tmp_path, monkeypatch, "--sem-custo")
    assert d["protocolo"] == "C'" and "lancador" not in d
    for reg in d["por_id"].values():
        assert set(reg) == {"A", "B", "verdict_A", "verdict_B"}
        assert reg["A"] == reg["B"] == [2.0] * 4


def test_o_arquivo_novo_passa_pela_analise_da_fase_d(tmp_path, monkeypatch):
    """Real launcher + fake motors, 3 ids: analisa_pareado reads the file (3 ids decide nothing: only 'no crash')."""
    d = rodar(tmp_path, monkeypatch)
    espec = importlib.util.spec_from_file_location("analisa_pareado_t", AQUI / "analisa_pareado.py")
    ap = importlib.util.module_from_spec(espec)
    espec.loader.exec_module(ap)
    nome, rc, info = ap.veredito_d(d["por_id"], "aa")
    assert nome in {"PROTOCOLO_OK", "PROTOCOLO_INADEQUADO"} and rc in {0, 2}, info
    assert info["ausencia"]["instr"]["faltando"] == 0 and info["ausencia"]["cpu"]["faltando"] == 0
