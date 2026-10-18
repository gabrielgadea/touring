"""Phase D decision rules of analisa_pareado.py (PREREGISTRO-FASE-D-2026-10-18.md) on synthetic paired runs.

One test per row of the pre-registered table, plus the ones that pin a rule against its cheapest wrong implementation
(mutantes_fase_d.py swaps each rule for that implementation and demands a red test).
"""

import importlib.util
import json
import random
import subprocess
import sys
from pathlib import Path

import pytest

AQUI = Path(__file__).resolve().parent
ALVO = AQUI / "analisa_pareado.py"
spec = importlib.util.spec_from_file_location("analisa_pareado_d", ALVO)
ap = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ap)

N = 240  # ids; 8% of them (every 12th) form the long tail the p95 lives in
INSTR_POR_MS = 2.5e6  # a synthetic motor retiring 2.5M user instructions per ms of CPU


def corrida_d(
    fi: float = 1.0,
    fc: float = 1.0,
    fw: float = 1.0,
    cauda_cpu: tuple[float, float] = (60.0, 120.0),
    n: int = N,
    cada: int = 12,
    ruido_i: float = 0.002,
    ruido_c: float = 0.01,
    ruido_w: float = 0.10,
    semente: int = 7,
) -> dict:
    """n ids x (4 calls of A, 4 of B). B = A x factor per measure; each call jitters by its own noise."""
    rng = random.Random(semente)
    por_id = {}
    for i in range(n):
        cpu = rng.uniform(*cauda_cpu) if i % cada == 0 else rng.choice([1.5, 2.5, 4.0])
        reg: dict[str, list[float]] = {}
        for lado, (a, b, c) in {"A": (1.0, 1.0, 1.0), "B": (fi, fc, fw)}.items():
            reg[f"instr_{lado}"] = [cpu * INSTR_POR_MS * a * rng.uniform(1 - ruido_i, 1 + ruido_i) for _ in range(4)]
            reg[f"cpu_{lado}"] = [cpu * b * rng.uniform(1 - ruido_c, 1 + ruido_c) for _ in range(4)]
            reg[lado] = [cpu * c * rng.uniform(1 - ruido_w, 1 + ruido_w) for _ in range(4)]
        por_id[f"id{i}"] = reg
    return por_id


def anula(por_id: dict, campo: str, ids: list[int], pos: int = 0) -> None:
    for i in ids:
        por_id[f"id{i}"][campo][pos] = None


# ---------------------------------------------------------------- pre-registered constants


def test_constantes_pre_registradas():
    assert ap.AA_INSTR_FAIXA == (0.99, 1.01)
    assert ap.AA_CPU_FAIXA == (0.95, 1.05)
    assert ap.AB_TETO == 1.10
    assert ap.FALTA_MAX == 0.01
    assert ap.ABSOLUTO_CPU_MS == 150.0
    assert ap.BOOT_N == 2000
    assert ap.BOOT_SEMENTE_D == 20261018


def test_toda_reamostragem_usa_a_semente_20261018_e_2000_reamostras(monkeypatch):
    por_id = corrida_d(n=60)  # built BEFORE the spy: the fixture's own generator is a Random too
    sementes, reamostras = [], []
    real = random.Random

    class Espia(real):
        def choices(self, *args, **kw):
            reamostras.append(1)
            return super().choices(*args, **kw)

    def espia(semente=None):
        sementes.append(semente)
        return Espia(semente)

    monkeypatch.setattr(ap.random, "Random", espia)
    ap.veredito_d(por_id, "aa")
    assert sementes and set(sementes) == {20261018}, sementes
    assert len(sementes) == 3 and len(reamostras) == 3 * 2000  # instr, cpu and wall: 2000 resamples each


# ---------------------------------------------------------------- A/A: the ruler is chosen here, before the A/B


def test_aa_com_instrucoes_estaveis_escolhe_instrucoes():
    nome, rc, info = ap.veredito_d(corrida_d(), "aa")
    assert (nome, rc, info["regua"]) == ("PROTOCOLO_OK", 0, "instr"), info


def test_aa_instrucoes_dentro_de_meio_por_cento_ainda_e_instrucoes():
    nome, rc, info = ap.veredito_d(corrida_d(fi=1.005), "aa")
    assert (nome, rc, info["regua"]) == ("PROTOCOLO_OK", 0, "instr"), info


def test_aa_com_instrucoes_fora_de_1pct_cai_para_a_cpu():
    nome, rc, info = ap.veredito_d(corrida_d(fi=1.03), "aa")
    assert (nome, rc, info["regua"]) == ("PROTOCOLO_OK", 0, "cpu"), info
    assert info["aa"]["instr"]["ok"] is False and info["aa"]["cpu"]["ok"] is True


def test_aa_com_instrucoes_3pct_mais_baixas_tambem_cai_para_a_cpu():
    """The window is two-sided: a binary that retires FEWER instructions in an A/A is just as unreliable a ruler."""
    nome, rc, info = ap.veredito_d(corrida_d(fi=0.97), "aa")
    assert (nome, rc, info["regua"]) == ("PROTOCOLO_OK", 0, "cpu"), info


def test_aa_cpu_ruim_nao_importa_se_as_instrucoes_valem():
    nome, rc, info = ap.veredito_d(corrida_d(fc=1.3), "aa")
    assert (nome, rc, info["regua"]) == ("PROTOCOLO_OK", 0, "instr"), info


def test_aa_com_as_duas_reguas_reprovadas_e_protocolo_inadequado():
    nome, rc, info = ap.veredito_d(corrida_d(fi=1.03, fc=1.10), "aa")
    assert (nome, rc) == ("PROTOCOLO_INADEQUADO", 2), info
    assert info["regua"] is None


def test_aa_cpu_a_3pct_ainda_vale_e_a_8pct_nao():
    assert ap.veredito_d(corrida_d(fi=1.03, fc=1.03), "aa")[:2] == ("PROTOCOLO_OK", 0)
    assert ap.veredito_d(corrida_d(fi=1.03, fc=1.08), "aa")[:2] == ("PROTOCOLO_INADEQUADO", 2)


# ---------------------------------------------------------------- A/B on the ruler the A/A chose


@pytest.mark.parametrize("fator, esperado", [(0.8, ("SEM_REGRESSAO", 0)), (1.0, ("SEM_REGRESSAO", 0)),
                                             (1.08, ("SEM_REGRESSAO", 0)), (1.15, ("REGRESSAO", 1)),
                                             (1.3, ("REGRESSAO", 1))])
def test_ab_na_regua_das_instrucoes(fator, esperado):
    nome, rc, info = ap.veredito_d(corrida_d(fi=fator), "ab", regua="instr")
    assert (nome, rc) == esperado, info


@pytest.mark.parametrize("fator, esperado", [(1.0, ("SEM_REGRESSAO", 0)), (1.06, ("SEM_REGRESSAO", 0)),
                                             (1.15, ("REGRESSAO", 1))])
def test_ab_na_regua_da_cpu(fator, esperado):
    nome, rc, info = ap.veredito_d(corrida_d(fc=fator), "ab", regua="cpu")
    assert (nome, rc) == esperado, info


def test_a_regua_do_aa_nao_se_troca_depois_do_ab():
    """Instructions say 'regression', CPU says 'fine': the ruler the A/A chose decides, in both directions."""
    por_id = corrida_d(fi=1.3, fc=1.0)
    assert ap.veredito_d(por_id, "ab", regua="instr")[:2] == ("REGRESSAO", 1)
    assert ap.veredito_d(por_id, "ab", regua="cpu")[:2] == ("SEM_REGRESSAO", 0)
    por_id = corrida_d(fi=1.0, fc=1.3)
    assert ap.veredito_d(por_id, "ab", regua="instr")[:2] == ("SEM_REGRESSAO", 0)
    assert ap.veredito_d(por_id, "ab", regua="cpu")[:2] == ("REGRESSAO", 1)


def test_ab_sem_regua_do_aa_e_recusado():
    with pytest.raises(ValueError, match="regua"):
        ap.veredito_d(corrida_d(), "ab")
    with pytest.raises(ValueError, match="regua"):
        ap.veredito_d(corrida_d(), "ab", regua="parede")


def test_uma_cauda_mais_lenta_so_nas_instrucoes_e_pega_pelo_r_p95():
    por_id = corrida_d()
    for i in range(0, N, 12):
        por_id[f"id{i}"]["instr_B"] = [x * 1.4 for x in por_id[f"id{i}"]["instr_B"]]
    nome, rc, info = ap.veredito_d(por_id, "ab", regua="instr")
    assert (nome, rc) == ("REGRESSAO", 1), info
    assert info["instr"]["IC95"]["R_tipico"][1] <= ap.AB_TETO  # the typical id did not move: only R_p95 sees it


def test_o_r_tipico_sozinho_tambem_reprova():
    """A quarter of the ids form the tail (so the p95 stays in it in every resample) and only the others get slower."""
    por_id = corrida_d(cada=4)
    for i in range(N):
        if i % 4:
            por_id[f"id{i}"]["instr_B"] = [x * 1.4 for x in por_id[f"id{i}"]["instr_B"]]
    nome, rc, info = ap.veredito_d(por_id, "ab", regua="instr")
    assert (nome, rc) == ("REGRESSAO", 1), info
    assert info["instr"]["IC95"]["R_tipico"][0] > ap.AB_TETO  # it is the typical ratio that failed...
    assert info["instr"]["IC95"]["R_p95"][1] <= ap.AB_TETO  # ...while the tail did not move at all


def test_a_parede_e_relatada_com_ic_e_nunca_decide():
    nome, rc, info = ap.veredito_d(corrida_d(fw=3.0), "ab", regua="instr")  # wall 3x slower, instructions identical
    assert (nome, rc) == ("SEM_REGRESSAO", 0), info
    assert info["parede"]["R_tipico"] > 2.5 and set(info["parede"]["IC95"]) == {"R_tipico", "R_p95"}


# ---------------------------------------------------------------- absent measurement: INCONCLUSIVO, never zero


@pytest.mark.parametrize("modo, kw", [("aa", {}), ("ab", {"regua": "instr"})])
def test_contador_indisponivel_e_inconclusivo(modo, kw):
    por_id = corrida_d()
    for reg in por_id.values():
        reg["instr_A"] = [None] * 4
        reg["instr_B"] = [None] * 4
    nome, rc, info = ap.veredito_d(por_id, modo, **kw)
    assert (nome, rc) == ("INCONCLUSIVO", 3), info
    assert info["ausencia"]["instr"]["faltando"] == 8 * N


def test_ausencia_acima_de_1pct_das_chamadas_e_inconclusiva():
    por_id = corrida_d()  # 240 ids x 8 = 1920 calls; 1% = 19.2 -> 20 missing is above
    anula(por_id, "instr_B", list(range(20)))
    nome, rc, info = ap.veredito_d(por_id, "ab", regua="instr")
    assert (nome, rc) == ("INCONCLUSIVO", 3), info


def test_ausencia_ate_1pct_nao_bloqueia_e_nunca_vira_zero():
    por_id = corrida_d()
    anula(por_id, "instr_B", list(range(1, 19)))  # 18 of 1920 calls = 0.94%
    nome, rc, info = ap.veredito_d(por_id, "ab", regua="instr")
    assert (nome, rc) == ("SEM_REGRESSAO", 0), info
    assert info["ausencia"]["instr"]["faltando"] == 18
    assert len(info["ausencia"]["instr"]["ids_excluidos"]) == 18
    assert info["instr"]["n_ids"] == N - 18  # the ids with a hole are left out, not imputed as 0


def test_id_sem_parede_sai_de_todas_as_reguas_mesmo_abaixo_de_1pct():
    """A motor call that left no wall time failed: its instructions are the cost of a failure, not of a verdict."""
    por_id = corrida_d()
    anula(por_id, "B", [3, 4])  # 2 of 240 ids = 0.83%: not inconclusive
    nome, rc, info = ap.veredito_d(por_id, "ab", regua="instr")
    assert (nome, rc) == ("SEM_REGRESSAO", 0), info
    assert info["instr"]["n_ids"] == info["cpu"]["n_ids"] == info["parede"]["n_ids"] == N - 2
    assert info["n_ids_excluidos"] == 2


def test_cpu_ausente_tambem_e_inconclusivo():
    por_id = corrida_d()
    anula(por_id, "cpu_A", list(range(30)))
    assert ap.veredito_d(por_id, "aa")[:2] == ("INCONCLUSIVO", 3)


def test_parede_ausente_acima_de_1pct_dos_ids_e_inconclusiva_como_no_c_linha():
    por_id = corrida_d()
    anula(por_id, "B", list(range(5)))  # 5 of 240 ids = 2.1% have no wall time (the C' rule)
    assert ap.veredito_d(por_id, "aa")[:2] == ("INCONCLUSIVO", 3)


# ---------------------------------------------------------------- the 150 ms absolute: gate on v2, reported on v3


def test_absoluto_v2_acima_de_150ms_reprova_mesmo_sem_regressao_relativa():
    por_id = corrida_d(cauda_cpu=(200.0, 400.0))
    nome, rc, info = ap.veredito_d(por_id, "ab", regua="instr", juiz="v2")
    assert (nome, rc) == ("REGRESSAO", 1), info
    assert info["absoluto"]["cpu_p95_B_ms"] > 150 and info["absoluto"]["portao"] is True
    assert info["portoes"] == {"relativo": True, "absoluto": False}  # the relative gate was fine: the absolute failed


def test_absoluto_v2_dentro_de_150ms_passa():
    nome, rc, info = ap.veredito_d(corrida_d(), "ab", regua="instr", juiz="v2")
    assert (nome, rc) == ("SEM_REGRESSAO", 0), info
    assert info["absoluto"]["cpu_p95_B_ms"] <= 150


def test_absoluto_v3_e_so_relatado():
    por_id = corrida_d(cauda_cpu=(200.0, 400.0))
    nome, rc, info = ap.veredito_d(por_id, "ab", regua="instr", juiz="v3")
    assert (nome, rc) == ("SEM_REGRESSAO", 0), info
    assert info["absoluto"]["cpu_p95_B_ms"] > 150 and info["absoluto"]["portao"] is False
    assert info["portoes"] == {"relativo": True, "absoluto": None}  # reported, not a gate


def test_o_absoluto_e_o_p95_das_chamadas_de_b_nao_das_medianas():
    """One call in ten is slow: the per-id median hides it, the per-call p95 (what the spec says) sees it."""
    por_id = corrida_d()
    for i in range(N):
        por_id[f"id{i}"]["cpu_B"][0] = 200.0 if i % 3 == 0 else por_id[f"id{i}"]["cpu_B"][0]
    info = ap.veredito_d(por_id, "ab", regua="instr", juiz="v2")[2]
    assert info["absoluto"]["cpu_p95_B_ms"] >= 150


# ---------------------------------------------------------------- the CLI


def cli(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, "-I", str(ALVO), *args], capture_output=True, text=True, check=False)


def grava(tmp_path: Path, nome: str, por_id: dict, sha_a="aaaa", sha_b="bbbb", args=None) -> Path:
    p = tmp_path / nome
    p.write_text(json.dumps({"por_id": por_id, "sha_A": sha_a, "sha_B": sha_b, "args": args or []}))
    return p


def test_cli_aa_depois_ab_usa_a_regua_gravada_no_veredito_do_aa(tmp_path):
    aa = grava(tmp_path, "aa.json", corrida_d(fi=1.03), sha_b="aaaa")  # instructions off by 3% -> CPU ruler
    r = cli(str(aa), "--modo", "aa", "--json", str(tmp_path / "v_aa.json"))
    assert r.returncode == 0, r.stdout
    assert json.loads((tmp_path / "v_aa.json").read_text())["regua"] == "cpu"
    # the A/B has instructions 30% worse but CPU fine: with the CPU ruler it passes; it must not be re-chosen
    ab = grava(tmp_path, "ab.json", corrida_d(fi=1.3, fc=1.0))
    r = cli(str(ab), "--modo", "ab", "--aa", str(tmp_path / "v_aa.json"), "--json", str(tmp_path / "v_ab.json"))
    assert r.returncode == 0, r.stdout
    saida = json.loads((tmp_path / "v_ab.json").read_text())
    assert (saida["veredito"], saida["regua"], saida["fase"]) == ("SEM_REGRESSAO", "cpu", "D")


def test_cli_aa_inadequado_nao_le_o_ab(tmp_path):
    aa = grava(tmp_path, "aa.json", corrida_d(fi=1.03, fc=1.10), sha_b="aaaa")
    assert cli(str(aa), "--modo", "aa", "--json", str(tmp_path / "v_aa.json")).returncode == 2
    ab = grava(tmp_path, "ab.json", corrida_d(fi=1.0))  # a perfectly fine A/B that must NOT be read
    r = cli(str(ab), "--modo", "ab", "--aa", str(tmp_path / "v_aa.json"))
    assert r.returncode == 2 and json.loads(r.stdout)["veredito"] == "PROTOCOLO_INADEQUADO"
    assert "R_tipico" not in r.stdout  # nothing of the A/B was computed


def test_cli_ab_sem_aa_e_erro_de_uso(tmp_path):
    ab = grava(tmp_path, "ab.json", corrida_d())
    assert cli(str(ab), "--modo", "ab").returncode == 4


def test_cli_ab_com_referencia_diferente_da_do_aa_e_erro_de_uso(tmp_path):
    aa = grava(tmp_path, "aa.json", corrida_d(), sha_a="ref1", sha_b="ref1")
    assert cli(str(aa), "--modo", "aa", "--json", str(tmp_path / "v_aa.json")).returncode == 0
    ab = grava(tmp_path, "ab.json", corrida_d(), sha_a="OUTRA", sha_b="novo")
    assert cli(str(ab), "--modo", "ab", "--aa", str(tmp_path / "v_aa.json")).returncode == 4


def test_cli_deduz_o_juiz_v3_dos_args_do_pareado(tmp_path):
    por_id = corrida_d(cauda_cpu=(200.0, 400.0))
    aa = grava(tmp_path, "aa.json", por_id, sha_b="aaaa")
    cli(str(aa), "--modo", "aa", "--json", str(tmp_path / "v_aa.json"))
    v3 = ["--arg-motor=--camadas", "--arg-motor=1,2", "--p95-ms", "1000"]
    ab3 = grava(tmp_path, "ab3.json", por_id, args=v3)
    ab2 = grava(tmp_path, "ab2.json", por_id, args=["--v5-estrito"])
    r3 = cli(str(ab3), "--modo", "ab", "--aa", str(tmp_path / "v_aa.json"))
    r2 = cli(str(ab2), "--modo", "ab", "--aa", str(tmp_path / "v_aa.json"))
    assert (r3.returncode, json.loads(r3.stdout)["juiz"]) == (0, "v3")
    assert (r2.returncode, json.loads(r2.stdout)["juiz"]) == (1, "v2")


@pytest.mark.parametrize("args, esperado", [
    (["--v5-estrito", "--corpus", "/c"], "v2"),
    (["--arg-motor=--camadas", "--arg-motor=1,2", "--p95-ms", "1000"], "v3"),
    (["--arg-motor", "--camadas", "--arg-motor", "1,2"], "v3"),
    (["--arg-motor=--camadas=1,2"], "v3"),
    (["--arg-motor=--camadas", "--arg-motor=1"], "v2"),  # layer 1 only is still the v2 judge
    ([], "v2"),
])
def test_juiz_dos_args(args, esperado):
    assert ap.juiz_dos_args(args) == esperado


def test_cli_fase_c_reproduz_o_comportamento_antigo(tmp_path):
    antigo = {f"id{i}": {"A": [3.0, 3.1, 2.9, 3.0], "B": [3.0, 3.1, 2.9, 3.0]} for i in range(200)}
    arq = grava(tmp_path, "c.json", antigo)
    r = cli(str(arq), "--modo", "aa", "--fase", "c")
    assert r.returncode == 0 and json.loads(r.stdout)["veredito"] == "PROTOCOLO_OK"
    esperado = ap.veredito(antigo, "aa")
    assert json.loads(r.stdout)["R_tipico"] == esperado[2]["R_tipico"]
    # the same file read by phase D has no counters at all: absent, not zero
    r = cli(str(arq), "--modo", "aa")
    assert r.returncode == 3 and json.loads(r.stdout)["veredito"] == "INCONCLUSIVO"
