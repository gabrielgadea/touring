"""The pre-registered decision rules of analisa_pareado.py, on synthetic paired runs with a known answer."""
import importlib.util
import random
from pathlib import Path

import pytest

spec = importlib.util.spec_from_file_location("analisa_pareado", Path(__file__).with_name("analisa_pareado.py"))
ap = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ap)


def corrida(fator_b: float, n: int = 400, ruido: float = 0.03, semente: int = 1) -> dict:
    """n ids with a long tail; each call jitters ±ruido; B = A × fator_b."""
    rng = random.Random(semente)
    por_id = {}
    for i in range(n):
        base = rng.choice([3.0, 5.0, 8.0]) if i % 20 else rng.uniform(100.0, 400.0)
        por_id[f"id{i}"] = {"A": [base * rng.uniform(1 - ruido, 1 + ruido) for _ in range(4)],
                            "B": [base * fator_b * rng.uniform(1 - ruido, 1 + ruido) for _ in range(4)]}
    return por_id


def test_p95_is_the_judges_nearest_rank():
    assert ap.p95(list(range(1, 21))) == 19  # ceil(0.95·20) − 1 = index 18
    assert ap.p95([7.0]) == 7.0


def test_aa_of_identical_behaviour_validates_the_protocol():
    nome, rc, info = ap.veredito(corrida(1.0), "aa")
    assert (nome, rc) == ("PROTOCOLO_OK", 0), info


def test_aa_with_a_systematic_gap_says_the_protocol_is_inadequate():
    nome, rc, _ = ap.veredito(corrida(1.12), "aa")
    assert (nome, rc) == ("PROTOCOLO_INADEQUADO", 2)


@pytest.mark.parametrize("fator, esperado", [(0.8, ("SEM_REGRESSAO", 0)), (1.0, ("SEM_REGRESSAO", 0)),
                                             (1.3, ("REGRESSAO", 1))])
def test_ab_gate(fator, esperado):
    nome, rc, info = ap.veredito(corrida(fator), "ab")
    assert (nome, rc) == esperado, info


def test_a_slower_tail_alone_is_caught_by_r_p95():
    por_id = corrida(1.0)
    for i in range(0, 400, 20):  # only the tail ids get 40% slower
        por_id[f"id{i}"]["B"] = [x * 1.4 for x in por_id[f"id{i}"]["B"]]
    nome, rc, info = ap.veredito(por_id, "ab")
    assert (nome, rc) == ("REGRESSAO", 1), info
    assert info["IC95"]["R_tipico"][1] <= ap.AB_TETO  # the typical id did not move: only R_p95 sees it


def test_missing_times_above_one_percent_are_inconclusive_never_zero():
    por_id = corrida(1.0, n=100)
    for i in range(2):
        por_id[f"id{i}"]["B"][0] = None
    nome, rc, info = ap.veredito(por_id, "ab")
    assert (nome, rc, info["faltando"]) == ("INCONCLUSIVO", 3, 2)
