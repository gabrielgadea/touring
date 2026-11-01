"""Tests of reconcilia_int97i.py: each cause is recognised, and a divergence with no cause fails the reconciliation."""

from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import reconcilia_int97i as r

ACHADO = {"classe": "nome", "nome": "foo", "linha": 3, "severidade": "deny"}


def rec(verdict: str, achados: int = 0) -> dict:
    return {"verdict": verdict, "findings": [ACHADO] * achados}


def grava(tmp: Path, nome: str, por_id: dict, envelope: bool = True) -> str:
    p = tmp / nome
    p.write_text(json.dumps({"por_id": por_id} if envelope else por_id), encoding="utf-8")
    return str(p)


def roda(tmp: Path, int97: dict, novo: dict, **kw) -> tuple[int, dict]:
    args = ["--int", grava(tmp, "int.json", int97), "--novo", grava(tmp, "novo.json", novo), "--json", str(tmp / "o.json")]
    if "p0b" in kw:
        args += ["--p0b", grava(tmp, "p0b.json", kw["p0b"])]
    if "frag" in kw:
        base, frag = kw["frag"]
        args += ["--frag-base", grava(tmp, "fb.json", base, envelope=False), "--frag", grava(tmp, "f.json", frag, envelope=False)]
    for p in kw.get("previsto", []):
        args += ["--previsto", p]
    rc = r.main(args)
    return rc, json.loads((tmp / "o.json").read_text(encoding="utf-8"))


def test_identical_runs_have_no_divergence(tmp_path):
    rc, out = roda(tmp_path, {"a": rec("allow")}, {"a": rec("allow")})
    assert rc == 0 and out["divergencias"] == 0


def test_each_cause_is_recognised(tmp_path):
    int97 = {"p": rec("warn", 1), "f": rec("allow"), "d": rec("warn", 1), "x": rec("allow")}
    novo = {"p": rec("allow"), "f": rec("deny", 1), "d": rec("deny", 1), "x": rec("allow")}
    rc, out = roda(tmp_path, int97, novo,
                   p0b={"p": rec("allow")},
                   frag=({"f": rec("allow")}, {"f": rec("deny", 1)}),
                   previsto=["d:warn:deny"])
    causas = {d["id"]: d["causa"] for d in out["todas"]}
    assert causas == {"p": "P0B", "f": "FRAG", "d": "previsto (warn -> deny)"}
    assert rc == 0 and out["previstos_observados"] == {"d": True}


def test_a_divergence_nobody_explains_fails(tmp_path):
    rc, out = roda(tmp_path, {"z": rec("allow")}, {"z": rec("deny", 1)}, p0b={"z": rec("allow")})
    assert rc == 1 and [d["id"] for d in out["sem_causa"]] == ["z"]


def test_isolated_run_landing_elsewhere_is_not_a_cause(tmp_path):
    # FRAG changed the id, but to a different signature than the integrated release: not an explanation.
    rc, out = roda(tmp_path, {"f": rec("allow")}, {"f": rec("deny", 2)},
                   frag=({"f": rec("allow")}, {"f": rec("deny", 1)}))
    assert rc == 1 and out["sem_causa"][0]["id"] == "f"


def test_a_prediction_that_did_not_happen_is_reported(tmp_path):
    rc, out = roda(tmp_path, {"d": rec("warn", 1)}, {"d": rec("warn", 1)}, previsto=["d:warn:deny"])
    assert rc == 0 and out["previstos_observados"] == {"d": False}


def test_unreadable_input_is_exit_3(tmp_path):
    assert r.main(["--int", str(tmp_path / "nao-existe.json"), "--novo", str(tmp_path / "x.json"),
                   "--json", str(tmp_path / "o.json")]) == 3


def test_layer2_timeout_that_only_lost_findings_is_a_mechanical_cause(tmp_path):
    base = {"t": {**rec("deny", 1), "camada2": None}}
    novo = {"t": {**rec("allow"), "camada2": "unknown: prazo de 1000 ms estourado esperando publishDiagnostics"}}
    rc, out = roda(tmp_path, base, novo)
    assert rc == 0 and out["todas"][0]["causa"].startswith("camada 2 sem resposta")


def test_layer2_timeout_does_not_explain_a_gained_finding(tmp_path):
    base = {"t": {**rec("allow"), "camada2": None}}
    novo = {"t": {**rec("deny", 1), "camada2": "unknown: prazo de 1000 ms estourado esperando publishDiagnostics"}}
    rc, out = roda(tmp_path, base, novo)
    assert rc == 1 and out["sem_causa"][0]["id"] == "t"


def test_prediction_is_read_against_the_p0b_state_it_was_made_on(tmp_path):
    # INT97 said deny via another class; P0B moved it to warn; P0B-d predicted warn -> deny on top of P0B.
    int97 = {"d": {"verdict": "deny", "findings": [{"classe": "membro", "nome": "x", "linha": 3, "severidade": "deny"}]}}
    p0b = {"d": {"verdict": "warn", "findings": [{"classe": "atributo", "nome": "x", "linha": 3, "severidade": "warn"}]}}
    novo = {"d": {"verdict": "deny", "findings": [{"classe": "atributo", "nome": "x", "linha": 3, "severidade": "deny"}]}}
    rc, out = roda(tmp_path, int97, novo, p0b=p0b, previsto=["d:warn:deny"])
    assert rc == 0 and out["todas"][0]["causa"] == "previsto (warn -> deny)"


def test_a_base_that_timed_out_explains_the_findings_the_new_run_has(tmp_path):
    base = {"t": {**rec("allow"), "camada2": "unknown: prazo de 1000 ms estourado esperando publishDiagnostics"}}
    novo = {"t": {**rec("warn", 1), "camada2": None}}
    rc, out = roda(tmp_path, base, novo)
    assert rc == 0 and out["todas"][0]["causa"].startswith("camada 2 sem resposta")


def test_both_sides_answering_is_never_a_timeout_cause(tmp_path):
    rc, _out = roda(tmp_path, {"t": {**rec("allow"), "camada2": None}}, {"t": {**rec("warn", 1), "camada2": None}})
    assert rc == 1
