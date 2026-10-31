"""Unit tests for fail-closed accounting in the hot-daemon measurement."""

import collections
import importlib.util
import shlex
from pathlib import Path

ARQ = Path(__file__).resolve().with_name("medir_daemon_quente.py")
spec = importlib.util.spec_from_file_location("medir_daemon_quente", ARQ)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)


def test_medicao_valida_exige_sombra_camada2_e_rota_daemon_completa():
    assert mod.medicao_valida(2, [{}, {}], [{}, {}], collections.Counter(daemon=4))


def test_ausencia_de_sombra_ou_camada2_nao_vira_zero_valido():
    assert not mod.medicao_valida(2, [{}], [{}, {}], collections.Counter(daemon=4))
    assert not mod.medicao_valida(2, [{}, {}], [{}], collections.Counter(daemon=4))


def test_standalone_ou_trace_ausente_reprova():
    assert not mod.medicao_valida(2, [{}, {}], [{}, {}], collections.Counter(daemon=3, standalone=1))
    assert not mod.medicao_valida(2, [{}, {}], [{}, {}], collections.Counter())


def test_percentil_usa_posto_mais_proximo_do_juiz():
    assert mod.percentil([1, 2, 3, 4], 0.95) == 4
    assert mod.percentil([], 0.95) is None


def test_comando_inline_usa_o_snapshot_integral(tmp_path):
    programa = "x = 1\n" * 500
    (tmp_path / "snap.py").write_text(programa)
    entrada = {"snap": "snap.py", "extrator": "c"}
    comando = mod.comando_inline(entrada, tmp_path)
    assert len(comando) > 2000
    assert shlex.split(comando)[2] == programa


def test_positivo_sem_extrator_e_inferido_pelo_path():
    assert mod.extrator_inline({"path": "<string>"}) == "c"
    assert mod.extrator_inline({"path": "<stdin>"}) == "heredoc"
    assert mod.extrator_inline({"path": "<stdin>", "extrator": "touring_run"}) == "touring_run"
