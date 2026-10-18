"""conta_custo.py (fase D, pre-registro 18/10/2026): retired user-space instructions + tree CPU of ONE command.

Every behaviour the pre-registration depends on has a test that fails when the behaviour is removed (mutants:
mutantes_fase_d.py). Tests that need the hybrid PMUs skip on a machine without them — a skip is NOT a kill.
"""

import errno
import importlib.util
import json
import os
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

import pytest

AQUI = Path(__file__).resolve().parent
LANCADOR = AQUI / "conta_custo.py"
spec = importlib.util.spec_from_file_location("conta_custo", LANCADOR)
cc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cc)

LACO = "for i in range({n}): pass"
# CPython randomises str hashes per process, and a module-level loop's STORE_NAME probes a dict: the same loop retires
# 2070M or 2146M or 2236M instructions run to run (measured 18/10: quantised jumps, 8 identical runs with seed 0).
# That is the workload's noise, not the counter's: the tests fix it.
ENV = {**os.environ, "PYTHONHASHSEED": "0"}


def laco(n: int) -> list[str]:
    return [sys.executable, "-c", LACO.format(n=n)]


def lancar(cmd: list[str], tmp_path: Path, **kw) -> tuple[subprocess.CompletedProcess, dict]:
    """Run the launcher as a separate process (the way the pareado does) and load its JSON."""
    saida = tmp_path / "custo.json"
    cp = subprocess.run(
        [sys.executable, "-I", str(LANCADOR), "--out", str(saida), "--", *cmd],
        capture_output=True,
        text=True,
        check=False,
        env=ENV,
        **kw,
    )
    dados = json.loads(saida.read_text(encoding="utf-8")) if saida.exists() else {}
    return cp, dados


def cpus_do_tipo(tipo: str) -> list[int]:
    """Online CPUs of a hybrid PMU (`cpu_core` / `cpu_atom`) that this process may run on."""
    p = Path(f"/sys/devices/{tipo}/cpus")
    if not p.is_file():
        return []
    cpus: set[int] = set()
    for faixa in p.read_text().strip().split(","):
        lo, _, hi = faixa.partition("-")
        cpus |= set(range(int(lo), int(hi or lo) + 1))
    return sorted(cpus & os.sched_getaffinity(0))


# ---------------------------------------------------------------- the measurement itself


def test_instrucoes_crescem_com_o_trabalho(tmp_path):
    _, a = lancar(laco(4_000_000), tmp_path)
    _, b = lancar(laco(8_000_000), tmp_path)
    assert a["erro"] is None and b["erro"] is None, (a, b)
    assert a["instr"] > 10_000_000  # a python loop of 4M iterations is far above a startup
    assert 1.7 <= b["instr"] / a["instr"] <= 2.3, (a["instr"], b["instr"])


def test_o_neto_e_contado_nas_instrucoes_e_na_cpu(tmp_path):
    """The pyright-langserver is a grandchild of the motor: both the counters and wait4 must include it."""
    neto = [
        sys.executable,
        "-c",
        f"import subprocess,sys; subprocess.run([sys.executable,'-c','{LACO.format(n=8_000_000)}'])",
    ]
    # one core type: the same loop takes 50 ms of CPU on a P-core and 120 ms on an E-core (measured 18/10)
    core = cpus_do_tipo("cpu_core")
    fixa = ["taskset", "-c", str(core[0])] if core and shutil_which("taskset") else []
    _, direto = lancar([*fixa, *laco(8_000_000)], tmp_path)
    _, indireto = lancar([*fixa, *neto], tmp_path)
    _, vazio = lancar([*fixa, sys.executable, "-c", "pass"], tmp_path)
    assert indireto["instr"] >= 0.9 * direto["instr"], (direto["instr"], indireto["instr"])
    assert indireto["instr"] > 4 * vazio["instr"]  # the loop is in the grandchild, not in the parent
    assert indireto["cpu_ms"] >= 0.5 * direto["cpu_ms"], (direto["cpu_ms"], indireto["cpu_ms"])
    assert indireto["cpu_ms"] > 3 * vazio["cpu_ms"], (vazio["cpu_ms"], indireto["cpu_ms"])


def test_o_que_o_filho_executa_antes_do_exec_nao_conta(tmp_path):
    """`execvp` of a bare name tries every PATH entry in PYTHON; 3000 dead entries before the real one is millions of
    user instructions the child runs before the exec. They must not reach the count: enable_on_exec turns the counter
    on at the first SUCCESSFUL exec only."""
    cc_bin = shutil.which("gcc") or shutil.which("cc")
    if not cc_bin:
        pytest.skip("sem compilador C")
    (tmp_path / "cal.c").write_text(
        'int main(void){ unsigned long n=100000000UL; __asm__ volatile("1: dec %0\\n\\tjnz 1b" : "+r"(n)); return 0; }'
    )
    subprocess.run([cc_bin, "-O0", "-static", "-o", str(tmp_path / "cal_pelado"), str(tmp_path / "cal.c")],
                   check=True, capture_output=True)
    caminho = ":".join([f"/nao/existe/d{i}" for i in range(3000)] + [str(tmp_path)])
    saida = tmp_path / "custo.json"
    subprocess.run([sys.executable, "-I", str(LANCADOR), "--out", str(saida), "--", "cal_pelado"],
                   check=True, capture_output=True, env={**ENV, "PATH": caminho})
    d = json.loads(saida.read_text())
    assert d["erro"] is None and 200_000_000 <= d["instr"] <= 200_800_000, d


def test_a_soma_das_pmus_nao_perde_uma(tmp_path):
    atom, core = cpus_do_tipo("cpu_atom"), cpus_do_tipo("cpu_core")
    if not atom or not core or not shutil_which("taskset"):
        pytest.skip("sem PMU hibrida (cpu_core + cpu_atom) ou sem taskset")
    _, no_atom = lancar(["taskset", "-c", str(atom[0]), *laco(3_000_000)], tmp_path)
    _, no_core = lancar(["taskset", "-c", str(core[0]), *laco(3_000_000)], tmp_path)
    for d in (no_atom, no_core):
        assert d["erro"] is None, d
        assert d["instr"] == d["instr_core"] + d["instr_atom"], d
    assert no_atom["instr_atom"] > 10_000_000 and no_atom["instr_atom"] > 20 * no_atom["instr_core"], no_atom
    assert no_core["instr_core"] > 10_000_000 and no_core["instr_core"] > 20 * no_core["instr_atom"], no_core


def shutil_which(nome: str) -> str | None:
    return shutil.which(nome)


def test_calibracao_com_laco_de_contagem_exata(tmp_path):
    """`dec; jnz` x 100M retires exactly 200M instructions: the counters must read that (± the static startup)."""
    cc_bin = shutil.which("gcc") or shutil.which("cc")
    if not cc_bin:
        pytest.skip("sem compilador C")
    fonte = tmp_path / "cal.c"
    fonte.write_text(
        'int main(void){ unsigned long n=100000000UL; __asm__ volatile("1: dec %0\\n\\tjnz 1b" : "+r"(n)); return 0; }'
    )
    exe = tmp_path / "cal"
    subprocess.run([cc_bin, "-O0", "-static", "-o", str(exe), str(fonte)], check=True, capture_output=True)
    _, d = lancar([str(exe)], tmp_path)
    assert d["erro"] is None, d
    # measured 200_249_146 and 200_249_148 (two runs): 249k of static libc startup + exit. Anything the child runs
    # BEFORE the exec (the python between fork and exec) must not be counted: that is what enable_on_exec buys.
    assert 200_000_000 <= d["instr"] <= 200_800_000, d


def test_a_mesma_carga_da_o_mesmo_numero_de_instrucoes(tmp_path):
    a = lancar(laco(5_000_000), tmp_path)[1]["instr"]
    b = lancar(laco(5_000_000), tmp_path)[1]["instr"]
    assert abs(a - b) / a < 0.005, (a, b)


def test_o_json_tem_o_formato_pre_registrado(tmp_path):
    cp, d = lancar(["sleep", "0.3"], tmp_path)
    assert cp.returncode == 0
    assert {"instr", "instr_core", "instr_atom", "cpu_ms", "wall_ms", "rc", "erro"} <= set(d)
    assert d["erro"] is None and d["rc"] == 0
    assert isinstance(d["instr"], int) and d["instr"] > 0
    assert d["wall_ms"] >= 250  # sleep 0.3
    assert 0 <= d["cpu_ms"] < d["wall_ms"]  # asleep, not computing


def test_cpu_ms_e_utime_mais_stime():
    class Uso:
        ru_utime, ru_stime = 0.1, 0.2

    assert cc.cpu_ms(Uso()) == pytest.approx(300.0)


# ---------------------------------------------------------------- the command is not disturbed


def test_stdout_e_stderr_passam_intactos(tmp_path):
    cp, d = lancar(["sh", "-c", "echo OUT; echo ERR >&2"], tmp_path)
    assert (cp.stdout, cp.stderr) == ("OUT\n", "ERR\n")
    assert d["rc"] == 0


def test_o_codigo_de_saida_e_o_do_filho(tmp_path):
    cp, d = lancar(["sh", "-c", "exit 7"], tmp_path)
    assert (cp.returncode, d["rc"]) == (7, 7)
    cp, d = lancar(["sh", "-c", "kill -9 $$"], tmp_path)
    assert (cp.returncode, d["rc"]) == (137, 137)  # 128 + SIGKILL, as a shell reports it
    assert d["instr"] is not None  # a command that dies still has a measurable cost


def test_comando_que_nao_executa_nao_vira_zero(tmp_path):
    cp, d = lancar(["/nao/existe/mesmo"], tmp_path)
    assert cp.returncode == 127 and d["rc"] == 127
    assert d["instr"] is None and d["instr_core"] is None and d["instr_atom"] is None
    assert d["erro"] and "FileNotFoundError" in d["erro"] and "/nao/existe/mesmo" in d["erro"], d


def test_matar_o_lancador_mata_o_comando(tmp_path):
    """The judge's subprocess.run(timeout) kills ONLY the launcher: the motor must not outlive it."""
    p = subprocess.Popen(
        [sys.executable, "-I", str(LANCADOR), "--out", str(tmp_path / "x.json"), "--", "sleep", "30"]
    )
    filho = None
    for _ in range(100):
        txt = Path(f"/proc/{p.pid}/task/{p.pid}/children").read_text().split()
        if txt:
            filho = int(txt[0])
            break
        time.sleep(0.05)
    assert filho, "o lancador nao criou o filho"
    p.send_signal(signal.SIGKILL)
    p.wait()
    for _ in range(100):
        try:
            if Path(f"/proc/{filho}/stat").read_text().split()[2] == "Z":
                return
        except FileNotFoundError:  # reaped already: gone
            return
        time.sleep(0.05)
    os.kill(filho, signal.SIGKILL)
    pytest.fail("o comando sobreviveu ao lancador")


def test_uso_errado_sai_com_64(tmp_path):
    sem_comando = [sys.executable, "-I", str(LANCADOR), "--out", str(tmp_path / "x.json")]
    cp = subprocess.run(sem_comando, capture_output=True, check=False)
    assert cp.returncode == 64
    cp = subprocess.run([sys.executable, "-I", str(LANCADOR), "--", "true"], capture_output=True, check=False)
    assert cp.returncode == 64


# ---------------------------------------------------------------- an unavailable counter is null, never 0


@pytest.mark.parametrize("codigo", [errno.EACCES, errno.ENOENT, errno.EMFILE])
def test_contador_indisponivel_vira_null_com_o_motivo(tmp_path, monkeypatch, codigo):
    def nega(attr, pid):  # what the kernel does under perf_event_paranoid=3 / no PMU / fd exhaustion
        raise OSError(codigo, os.strerror(codigo))

    monkeypatch.setattr(cc, "_perf_event_open", nega)
    saida = tmp_path / "custo.json"
    rc = cc.main(["--out", str(saida), "--", "sh", "-c", "exit 3"])
    d = json.loads(saida.read_text())
    assert rc == 3 and d["rc"] == 3  # the command still ran and its rc still travels
    assert d["instr"] is None and d["instr_core"] is None and d["instr_atom"] is None
    assert os.strerror(codigo) in d["erro"]
    assert d["cpu_ms"] is not None  # the CPU comes from wait4, which does not need the counter


def test_uma_pmu_que_nao_abre_anula_a_soma(tmp_path, monkeypatch):
    """Hybrid: if cpu_atom cannot be opened the sum would silently lose it → the whole count is null."""
    real = cc._perf_event_open
    chamadas = []

    def so_a_primeira(attr, pid):
        chamadas.append(1)
        if len(chamadas) == 2:
            raise OSError(errno.ENODEV, os.strerror(errno.ENODEV))
        return real(attr, pid)

    if len(cc.pmus()) < 2:
        pytest.skip("sem PMU hibrida")
    monkeypatch.setattr(cc, "_perf_event_open", so_a_primeira)
    saida = tmp_path / "custo.json"
    cc.main(["--out", str(saida), "--", "true"])
    d = json.loads(saida.read_text())
    assert d["instr"] is None and d["erro"]


def test_leitura_zero_depois_de_um_exec_valido_nao_vira_zero(tmp_path, monkeypatch):
    """A counter that reads 0 after a successful exec never counted (enable_on_exec did not fire): null, not 0."""
    monkeypatch.setattr(cc, "ler_contador", lambda fd: (0, 0, 0))
    saida = tmp_path / "custo.json"
    cc.main(["--out", str(saida), "--", "true"])
    d = json.loads(saida.read_text())
    assert d["instr"] is None and "leu 0" in d["erro"], d
