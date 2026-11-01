#!/usr/bin/env python3
"""Tests for scripts/readiness.py — the attention-list verdict (decision N5, 17/10/2026).

What these prove, rule by rule (each rule has a positive case AND the control that must stay
silent, so a rule that fires on everything — or never — is caught):

* a source that fails / times out / prints non-JSON / prints an error object is an ITEM, never
  an omission, never a zero;
* every item carries source, value, threshold and route;
* a waiver needs item + reason + owner + a real expiry; expired and malformed ones do not waive;
* the old measures survive as detail under distinct names with a definition and a denominator;
* the exit code is the verdict (0 pronto / 2 atencao) and usage errors do not alias it (64).
"""
from __future__ import annotations

import datetime as dt
import importlib.util
import json
import stat
import sys
import textwrap
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parent / "readiness.py"
_spec = importlib.util.spec_from_file_location("readiness_under_test", SCRIPT)
R = importlib.util.module_from_spec(_spec)
sys.modules["readiness_under_test"] = R
_spec.loader.exec_module(R)

HOJE = dt.date(2026, 10, 18)


# --- fixture builders (shapes copied from the live captures of 18/10/2026) --------------------

def status_ok() -> dict:
    return {
        "composite_health_score": 0.9,
        "daemon_health": {"healthy_count": 5, "total_count": 5},
        "index": {"index_generation": {"state": "complete"}},
        "health_delta": {"streak_alert_count": 0, "outstanding": 0},
    }


def fase(nome="index", status="pass", score=0.9, peso=2.0, issues=(), run=5, ok=5) -> dict:
    return {"phase": nome, "status": status, "score": score, "weight": peso, "issues": list(issues),
            "tests_run": run, "tests_passed": ok, "metrics": {}}


def e2e_ok() -> dict:
    return {"overall_status": "pass", "overall_score": 0.9, "phases": [fase("index"), fase("wiring", score=0.81, peso=2.0)]}


def check(cid="touring.x", status="PASS", actual=1.0, direction="gte", threshold=0.5, **extra) -> dict:
    return {"id": cid, "name": f"name of {cid}", "status": status, "actual": actual, "direction": direction,
            "threshold": threshold, "advisory": status == "ADVISORY", "source_kind": "daemon", "rationale": "r", **extra}


def kpi_ok() -> dict:
    return {"checks": [check()], "summary": {"passed": 1, "advisory": 0, "stub": 0, "failed": 0, "regressions": 0, "total": 1}}


def doctor_ok() -> list:
    return [{"name": "daemon_socket", "status": "ok", "detail": "/tmp/x.sock"}]


def harness_ok() -> dict:
    return {"composite": 0.7}


def fontes(**over) -> dict:
    base = {"status": status_ok(), "e2e": e2e_ok(), "kpi": kpi_ok(), "doctor": doctor_ok(), "harness-metric": harness_ok()}
    base.update(over)
    return {n: (v if isinstance(v, R.Fonte) else R.Fonte(n, True, v)) for n, v in base.items()}


def ids(itens) -> list[str]:
    return [i["id"] for i in itens]


def dispensas_file(tmp_path: Path, corpo: str) -> Path:
    p = tmp_path / "dispensas.yaml"
    p.write_text(textwrap.dedent(corpo), encoding="utf-8")
    return p


def avaliar(f, dispensas="dispensas: []\n", tmp_path=None) -> dict:
    p = tmp_path / "d.yaml"
    p.write_text(dispensas, encoding="utf-8")
    return R.avaliar(f, p, HOJE)


# --- the green control ---------------------------------------------------------------------------

def test_all_green_is_pronto_with_no_items(tmp_path):
    r = avaliar(fontes(), tmp_path=tmp_path)
    assert r["veredito"] == "pronto"
    assert r["itens"] == [] and r["dispensados"] == []


# --- status ---------------------------------------------------------------------------------------

def test_status_daemon_health_error_is_a_blocking_item(tmp_path):
    s = status_ok()
    s["daemon_health"] = {"error": "handler `cli-flywheel-status` exceeded its 15s budget"}
    it = {i["id"]: i for i in R.coletar_itens(fontes(status=s))}["status.daemon_health"]
    assert it["severidade"] == "bloqueia"
    assert "15s budget" in it["valor"]


def test_status_daemon_health_missing_key_is_an_item():
    s = status_ok()
    del s["daemon_health"]
    assert "status.daemon_health" in ids(R.coletar_itens(fontes(status=s)))


def test_status_daemon_health_without_counts_is_an_item():
    s = status_ok()
    s["daemon_health"] = {"unexpected": 1}
    assert "status.daemon_health" in ids(R.coletar_itens(fontes(status=s)))


def test_status_daemon_health_error_wins_over_partial_counts():
    s = status_ok()
    s["daemon_health"] = {"error": "partial", "healthy_count": 5, "total_count": 5}
    assert "status.daemon_health" in ids(R.coletar_itens(fontes(status=s)))


def test_status_healthy_daemon_is_silent():
    assert "status.daemon_health" not in ids(R.coletar_itens(fontes()))


@pytest.mark.parametrize("estado", ["building", "partial", None])
def test_status_index_not_complete_blocks(estado):
    s = status_ok()
    s["index"]["index_generation"] = {"state": estado} if estado else {}
    it = {i["id"]: i for i in R.coletar_itens(fontes(status=s))}["status.index_generation"]
    assert it["severidade"] == "bloqueia"


def test_status_index_complete_is_silent():
    assert "status.index_generation" not in ids(R.coletar_itens(fontes()))


def test_status_regression_streak_is_a_warning_and_zero_is_silent():
    s = status_ok()
    s["health_delta"]["streak_alert_count"] = 2
    it = {i["id"]: i for i in R.coletar_itens(fontes(status=s))}["status.regressao_em_serie"]
    assert it["severidade"] == "aviso" and it["valor"] == 2
    assert "status.regressao_em_serie" not in ids(R.coletar_itens(fontes()))


# --- e2e ------------------------------------------------------------------------------------------

def test_e2e_overall_not_pass_blocks_and_pass_is_silent():
    e = e2e_ok()
    e["overall_status"] = "fail"
    it = {i["id"]: i for i in R.coletar_itens(fontes(e2e=e))}["e2e.geral"]
    assert it["severidade"] == "bloqueia"
    assert "e2e.geral" not in ids(R.coletar_itens(fontes()))


def test_e2e_phase_warn_is_a_warning_and_phase_fail_blocks():
    e = e2e_ok()
    e["phases"] = [fase("ast", "warn", 0.77), fase("quality", "fail", 0.1)]
    por_id = {i["id"]: i for i in R.coletar_itens(fontes(e2e=e))}
    assert por_id["e2e.fase.ast"]["severidade"] == "aviso"
    assert por_id["e2e.fase.quality"]["severidade"] == "bloqueia"


def test_e2e_phase_with_failed_inner_tests_is_an_item_even_if_status_pass():
    e = e2e_ok()
    e["phases"] = [fase("knowledge", "pass", 0.9, run=7, ok=6)]
    it = {i["id"]: i for i in R.coletar_itens(fontes(e2e=e))}["e2e.fase.knowledge"]
    assert "6/7" in it["valor"]


def test_e2e_phase_with_issues_is_an_item_and_carries_them():
    e = e2e_ok()
    e["phases"] = [fase("knowledge", "pass", 0.9, issues=["58 hot file(s) edited 3+ times"])]
    it = {i["id"]: i for i in R.coletar_itens(fontes(e2e=e))}["e2e.fase.knowledge"]
    assert "58 hot file" in it["detalhe"]


def test_e2e_clean_phases_are_silent():
    assert not [i for i in R.coletar_itens(fontes()) if i["id"].startswith("e2e.")]


# --- kpi ------------------------------------------------------------------------------------------

def test_kpi_fail_blocks_with_its_value_and_threshold():
    k = kpi_ok()
    k["checks"] = [check("touring.a", "FAIL", actual=0.2, direction="gte", threshold=0.5)]
    it = {i["id"]: i for i in R.coletar_itens(fontes(kpi=k))}["kpi.touring.a"]
    assert it["severidade"] == "bloqueia" and it["valor"] == 0.2 and it["limiar"] == "gte 0.5"


def test_kpi_stub_is_sem_medida_and_carries_the_stub_reason():
    k = kpi_ok()
    k["checks"] = [check("touring.b", "STUB", actual=None, stub_reason="stale — measured 50d ago")]
    it = {i["id"]: i for i in R.coletar_itens(fontes(kpi=k))}["kpi.touring.b"]
    assert it["severidade"] == "sem_medida" and "50d" in it["valor"]


def test_kpi_stub_without_reason_still_says_it_has_no_measure():
    k = kpi_ok()
    k["checks"] = [check("touring.b", "STUB", actual=None)]
    it = {i["id"]: i for i in R.coletar_itens(fontes(kpi=k))}["kpi.touring.b"]
    assert "sem medição" in it["valor"]


def test_kpi_advisory_is_a_warning_with_the_measured_value():
    k = kpi_ok()
    k["checks"] = [check("touring.c", "ADVISORY", actual=0.05)]
    it = {i["id"]: i for i in R.coletar_itens(fontes(kpi=k))}["kpi.touring.c"]
    assert it["severidade"] == "aviso" and it["valor"] == 0.05


def test_kpi_pass_is_silent_and_unknown_status_blocks():
    k = kpi_ok()
    k["checks"] = [check("touring.ok", "PASS"), check("touring.weird", "MAYBE")]
    por_id = {i["id"]: i for i in R.coletar_itens(fontes(kpi=k))}
    assert "kpi.touring.ok" not in por_id
    assert por_id["kpi.touring.weird"]["severidade"] == "bloqueia"
    assert "MAYBE" in por_id["kpi.touring.weird"]["valor"]


def test_kpi_regressions_block():
    k = kpi_ok()
    k["summary"]["regressions"] = 2
    assert "kpi.regressoes" in ids(R.coletar_itens(fontes(kpi=k)))
    assert "kpi.regressoes" not in ids(R.coletar_itens(fontes()))


def test_kpi_external_check_route_points_at_the_feeder_file():
    k = kpi_ok()
    k["checks"] = [check("touring.coverage.line", "STUB", actual=None, source_kind="external")]
    it = {i["id"]: i for i in R.coletar_itens(fontes(kpi=k))}["kpi.touring.coverage.line"]
    assert "docs/kpi/external/touring.coverage.line.json" in it["rota"]
    k["checks"] = [check("touring.d", "STUB", actual=None)]
    it = {i["id"]: i for i in R.coletar_itens(fontes(kpi=k))}["kpi.touring.d"]
    assert "docs/kpi/external" not in it["rota"]


# --- doctor ---------------------------------------------------------------------------------------

def test_doctor_statuses_map_to_severities():
    d = doctor_ok() + [
        {"name": "a", "status": "warn", "detail": "x"},
        {"name": "b", "status": "missing", "detail": "y"},
        {"name": "c", "status": "error", "detail": "z"},
    ]
    por_id = {i["id"]: i for i in R.coletar_itens(fontes(doctor=d))}
    assert por_id["doctor.a"]["severidade"] == "aviso"
    assert por_id["doctor.b"]["severidade"] == "sem_medida"
    assert por_id["doctor.c"]["severidade"] == "bloqueia"
    assert "doctor.daemon_socket" not in por_id


def test_doctor_non_list_is_an_item():
    assert "doctor.formato" in ids(R.coletar_itens(fontes(doctor={"oops": 1})))


# --- absent signal is not zero --------------------------------------------------------------------

@pytest.mark.parametrize("nome", ["status", "e2e", "kpi", "doctor"])
def test_failed_core_source_is_a_blocking_item_and_is_not_evaluated(nome):
    f = fontes()
    f[nome] = R.Fonte(nome, False, None, "excedeu 120s")
    itens = R.coletar_itens(f)
    por_id = {i["id"]: i for i in itens}
    assert por_id[f"fonte.{nome}"]["severidade"] == "bloqueia"
    assert por_id[f"fonte.{nome}"]["valor"] == "excedeu 120s"
    assert len(itens) == 1, "a failed source must not also produce items from data it never delivered"


def test_failed_detail_source_is_a_warning_not_an_omission():
    f = fontes()
    f["harness-metric"] = R.Fonte("harness-metric", False, None, "budget_exceeded")
    it = {i["id"]: i for i in R.coletar_itens(f)}["fonte.harness-metric"]
    assert it["severidade"] == "aviso"


def test_classify_non_json_output_fails_with_the_rc_and_a_snippet():
    f = R.classify("kpi", 3, "Traceback (most recent call last):\n boom")
    assert not f.ok and "não é JSON" in f.erro and "rc=3" in f.erro and "Traceback" in f.erro


def test_classify_empty_output_fails():
    f = R.classify("kpi", 0, "")
    assert not f.ok and "saída vazia" in f.erro


def test_classify_budget_error_object_is_a_failure_not_data():
    out = json.dumps({"budget_secs": 15, "error": "handler exceeded its 15s budget", "error_kind": "budget_exceeded"})
    f = R.classify("harness-metric", 0, out)
    assert not f.ok and "15s budget" in f.erro


def test_classify_bare_error_object_is_a_failure():
    f = R.classify("status", 1, json.dumps({"error": "daemon down"}))
    assert not f.ok and f.erro == "daemon down"


def test_classify_real_data_with_nested_error_key_is_still_data():
    f = R.classify("status", 0, json.dumps({"daemon_health": {"error": "x"}, "index": {}}))
    assert f.ok


def test_classify_top_level_error_key_next_to_real_data_is_still_data():
    f = R.classify("e2e", 0, json.dumps({"error": None, "overall_status": "pass", "phases": []}))
    assert f.ok


def test_classify_tolerates_log_noise_before_the_json():
    f = R.classify("doctor", 0, 'INFO starting\n[{"name": "a", "status": "ok", "detail": ""}]')
    assert f.ok and f.data[0]["name"] == "a"


def test_classify_keeps_rc_for_good_data():
    f = R.classify("e2e", 1, json.dumps({"overall_status": "fail"}))
    assert f.ok and f.rc == 1


# --- every item is actionable ---------------------------------------------------------------------

def test_every_item_carries_source_value_threshold_and_route(tmp_path):
    s = status_ok()
    s["daemon_health"] = {"error": "x"}
    s["index"]["index_generation"] = {"state": "building"}
    s["health_delta"]["streak_alert_count"] = 3
    e = e2e_ok()
    e["overall_status"] = "fail"
    e["phases"] = [fase("ast", "warn", 0.7, issues=["i"], run=2, ok=1)]
    k = kpi_ok()
    k["summary"]["regressions"] = 1
    k["checks"] = [check("a", "FAIL", actual=None), check("b", "STUB", actual=None), check("c", "ADVISORY")]
    d = doctor_ok() + [{"name": "x", "status": "missing", "detail": ""}]
    f = fontes(status=s, e2e=e, kpi=k, doctor=d)
    f["harness-metric"] = R.Fonte("harness-metric", False, None, "boom")
    itens = R.coletar_itens(f)
    assert len(itens) >= 10
    for it in itens:
        for campo in ("id", "fonte", "severidade", "valor", "limiar", "rota"):
            assert it.get(campo) not in (None, ""), (campo, it)
        assert it["severidade"] in R.SEVERIDADES


def test_item_value_is_never_none_and_floats_are_rounded():
    it = R._item("x", "s", "aviso", None, "l", "r")
    assert it["valor"] == "ausente"
    assert R._item("x", "s", "aviso", 0.7737999999999999, "l", "r")["valor"] == 0.7738
    assert R._item("x", "s", "aviso", 0.123456789, "l", "r")["valor"] == 0.1235


# --- waivers --------------------------------------------------------------------------------------

def _um_item_kpi():
    k = kpi_ok()
    k["checks"] = [check("touring.cache.hit_ratio", "ADVISORY", actual=0.05)]
    return fontes(kpi=k)


def test_valid_waiver_moves_the_item_to_dispensados_and_verdict_is_pronto(tmp_path):
    r = avaliar(_um_item_kpi(), """
        dispensas:
          - item: kpi.touring.cache.hit_ratio
            motivo: cache em memória zera a cada restart
            dono: gabriel
            valido_ate: 2026-12-01
    """, tmp_path)
    assert r["veredito"] == "pronto" and r["itens"] == []
    assert r["dispensados"][0]["dispensa"] == {"motivo": "cache em memória zera a cada restart", "dono": "gabriel", "valido_ate": "2026-12-01"}


def test_waiver_valid_on_its_last_day_and_dead_the_day_after(tmp_path):
    corpo = """
        dispensas:
          - {item: kpi.touring.cache.hit_ratio, motivo: m, dono: d, valido_ate: 2026-10-18}
    """
    assert avaliar(_um_item_kpi(), corpo, tmp_path)["veredito"] == "pronto"
    p = tmp_path / "d.yaml"
    p.write_text(corpo.replace("2026-10-18", "2026-10-17"), encoding="utf-8")
    r = R.avaliar(_um_item_kpi(), p, HOJE)
    assert r["veredito"] == "atencao"
    assert r["itens"][0]["dispensa_expirada"] == "2026-10-17"


def test_waiver_horizon_is_capped_at_90_days(tmp_path):
    def com(data):
        return avaliar(_um_item_kpi(), f"dispensas:\n  - {{item: kpi.touring.cache.hit_ratio, motivo: m, dono: d, valido_ate: {data}}}\n", tmp_path)

    limite = (HOJE + dt.timedelta(days=90)).isoformat()  # literal on purpose: the cap is a contract, not a constant to echo
    depois = (HOJE + dt.timedelta(days=91)).isoformat()
    assert com(limite)["veredito"] == "pronto"
    r = com(depois)
    assert r["veredito"] == "atencao"
    assert any(i["id"].startswith("dispensas.invalida") for i in r["itens"])
    assert "kpi.touring.cache.hit_ratio" in ids(r["itens"]), "an invalid waiver must not waive"


@pytest.mark.parametrize("campo", ["item", "motivo", "dono", "valido_ate"])
def test_waiver_missing_a_required_field_does_not_waive_and_becomes_an_item(tmp_path, campo):
    d = {"item": "kpi.touring.cache.hit_ratio", "motivo": "m", "dono": "d", "valido_ate": "2026-12-01"}
    del d[campo]
    r = avaliar(_um_item_kpi(), "dispensas:\n  - " + json.dumps(d) + "\n", tmp_path)
    assert r["veredito"] == "atencao"
    assert "kpi.touring.cache.hit_ratio" in ids(r["itens"])
    assert any(i["id"].startswith("dispensas.invalida") and campo in i["valor"] for i in r["itens"])


def test_waiver_with_blank_reason_does_not_waive(tmp_path):
    r = avaliar(_um_item_kpi(), 'dispensas:\n  - {item: kpi.touring.cache.hit_ratio, motivo: "  ", dono: d, valido_ate: 2026-12-01}\n', tmp_path)
    assert "kpi.touring.cache.hit_ratio" in ids(r["itens"])


def test_waiver_with_garbage_date_does_not_waive(tmp_path):
    r = avaliar(_um_item_kpi(), "dispensas:\n  - {item: kpi.touring.cache.hit_ratio, motivo: m, dono: d, valido_ate: amanhã}\n", tmp_path)
    assert "kpi.touring.cache.hit_ratio" in ids(r["itens"])


def test_waiver_that_is_not_a_mapping_is_an_item(tmp_path):
    r = avaliar(fontes(), "dispensas:\n  - so-uma-string\n", tmp_path)
    assert r["veredito"] == "atencao" and ids(r["itens"]) == ["dispensas.invalida.0"]


def test_waiver_for_an_item_that_no_longer_exists_is_reported(tmp_path):
    r = avaliar(fontes(), "dispensas:\n  - {item: kpi.sumiu, motivo: m, dono: d, valido_ate: 2026-12-01}\n", tmp_path)
    assert r["veredito"] == "pronto"
    assert r["dispensas_sem_item"] == ["kpi.sumiu"]


def test_waiver_only_waives_the_named_item(tmp_path):
    k = kpi_ok()
    k["checks"] = [check("touring.a", "ADVISORY", actual=1), check("touring.b", "ADVISORY", actual=1)]
    r = avaliar(fontes(kpi=k), "dispensas:\n  - {item: kpi.touring.a, motivo: m, dono: d, valido_ate: 2026-12-01}\n", tmp_path)
    assert ids(r["itens"]) == ["kpi.touring.b"] and ids(r["dispensados"]) == ["kpi.touring.a"]


def test_missing_waiver_file_is_an_item_not_a_pass(tmp_path):
    r = R.avaliar(fontes(), tmp_path / "nao-existe.yaml", HOJE)
    assert r["veredito"] == "atencao" and ids(r["itens"]) == ["dispensas.arquivo"]


def test_unparseable_waiver_file_is_an_item(tmp_path):
    r = avaliar(fontes(), "dispensas: [unclosed\n", tmp_path)
    assert r["veredito"] == "atencao" and ids(r["itens"]) == ["dispensas.arquivo"]


def test_waiver_file_without_the_key_is_an_item(tmp_path):
    r = avaliar(fontes(), "outra_coisa: 1\n", tmp_path)
    assert r["veredito"] == "atencao" and ids(r["itens"]) == ["dispensas.arquivo"]


def test_the_shipped_waiver_file_parses_and_waives_nothing():
    dispensas, problemas = R.carregar_dispensas(R.DISPENSAS, HOJE)
    assert problemas == [] and dispensas == [], "no waiver is pre-approved: accepting an item is the owner's act"


# --- ordering and the old measures ---------------------------------------------------------------

def test_items_are_ordered_blocking_first_then_sem_medida_then_aviso(tmp_path):
    k = kpi_ok()
    k["checks"] = [check("z", "ADVISORY"), check("y", "STUB", actual=None), check("a", "FAIL")]
    r = avaliar(fontes(kpi=k), tmp_path=tmp_path)
    assert [i["severidade"] for i in r["itens"]] == ["bloqueia", "sem_medida", "aviso"]


def test_old_measures_have_distinct_names_definition_and_denominator():
    ms = R.medidas(fontes())
    nomes = [m["nome"] for m in ms]
    assert len(set(nomes)) == len(nomes) == 5
    assert not {"composite_health_score", "overall_score", "health"} & set(nomes), "the old overloaded names must not survive"
    for m in ms:
        assert m["mede"].strip() and m["denominador"].strip() and m["fonte"].strip()


def test_old_measures_map_to_the_right_values():
    k = kpi_ok()
    ms = {m["nome"]: m for m in R.medidas(fontes(kpi=k))}
    assert ms["saude_composta_daemon"]["valor"] == 0.9
    assert ms["e2e_nota_ponderada"]["valor"] == 0.9
    assert ms["harness_composto"]["valor"] == 0.7
    assert ms["nota_fase_wiring"]["valor"] == 0.81
    assert ms["kpi_compromissos"]["valor"] == {"pass": 1, "advisory": 0, "stub": 0, "fail": 0, "total": 1}
    assert "= 4" in ms["e2e_nota_ponderada"]["denominador"], "the e2e denominator is the sum of the executed phase weights (2+2)"


def test_old_measure_of_a_missing_source_is_none_never_zero():
    f = fontes()
    for n in ("status", "e2e", "kpi", "harness-metric"):
        f[n] = R.Fonte(n, False, None, "x")
    for m in R.medidas(f):
        assert m["valor"] is None, m


# --- live fetch (fake `touring` binary) -----------------------------------------------------------

def fake_touring(tmp_path: Path, corpo: str) -> str:
    p = tmp_path / "touring"
    p.write_text(f"#!{sys.executable}\nimport json, os, sys, time\n{textwrap.dedent(corpo)}", encoding="utf-8")
    p.chmod(p.stat().st_mode | stat.S_IEXEC)
    return str(p)


def test_fetch_live_runs_the_command_in_the_project_root(tmp_path):
    root = tmp_path / "proj"
    root.mkdir()
    exe = fake_touring(tmp_path, 'print(json.dumps({"cwd": os.getcwd(), "args": sys.argv[1:]}))')
    f = R.fetch_live("kpi", root, exe, 10)
    assert f.ok and Path(f.data["cwd"]).resolve() == root.resolve()
    assert f.data["args"] == ["kpi", "-j"]
    assert f.duracao_s is not None


def test_fetch_live_timeout_is_an_item_input(tmp_path):
    exe = fake_touring(tmp_path, "time.sleep(30)")
    f = R.fetch_live("status", tmp_path, exe, 1)
    assert not f.ok and "excedeu 1s" in f.erro


def test_fetch_live_missing_binary_is_an_item_input(tmp_path):
    f = R.fetch_live("status", tmp_path, str(tmp_path / "nao-existe"), 5)
    assert not f.ok and "não executou" in f.erro


def test_fetch_live_nonzero_rc_with_prose_is_a_failure(tmp_path):
    exe = fake_touring(tmp_path, 'print("daemon indisponivel"); sys.exit(7)')
    f = R.fetch_live("doctor", tmp_path, exe, 5)
    assert not f.ok and "rc=7" in f.erro


def test_fetch_dir_missing_file_is_a_failure(tmp_path):
    f = R.fetch_dir("kpi", tmp_path)
    assert not f.ok and "arquivo ausente" in f.erro


# --- CLI ------------------------------------------------------------------------------------------

def _escrever_capturas(pasta: Path, **over) -> Path:
    base = {"status": status_ok(), "e2e": e2e_ok(), "kpi": kpi_ok(), "doctor": doctor_ok(), "harness-metric": harness_ok()}
    base.update(over)
    for n, v in base.items():
        (pasta / f"{n}.json").write_text(json.dumps(v), encoding="utf-8")
    return pasta


def test_cli_exit_0_when_pronto_and_2_when_atencao(tmp_path, capsys):
    pasta = _escrever_capturas(tmp_path)
    disp = tmp_path / "d.yaml"
    disp.write_text("dispensas: []\n", encoding="utf-8")
    assert R.main(["--from-dir", str(pasta), "--dispensas", str(disp), "--hoje", "2026-10-18", "--json"]) == 0
    assert json.loads(capsys.readouterr().out)["veredito"] == "pronto"
    k = kpi_ok()
    k["checks"] = [check("touring.a", "FAIL", actual=0.1)]
    _escrever_capturas(tmp_path, kpi=k)
    assert R.main(["--from-dir", str(pasta), "--dispensas", str(disp), "--hoje", "2026-10-18", "--json"]) == 2
    out = json.loads(capsys.readouterr().out)
    assert out["veredito"] == "atencao" and ids(out["itens"]) == ["kpi.touring.a"]


def test_cli_text_output_has_verdict_routes_and_measures(tmp_path, capsys):
    k = kpi_ok()
    k["checks"] = [check("touring.a", "FAIL", actual=0.1)]
    pasta = _escrever_capturas(tmp_path, kpi=k)
    disp = tmp_path / "d.yaml"
    disp.write_text("dispensas: []\n", encoding="utf-8")
    assert R.main(["--from-dir", str(pasta), "--dispensas", str(disp), "--hoje", "2026-10-18", "--text"]) == 2
    out = capsys.readouterr().out
    assert "PRONTIDÃO: ATENCAO" in out and "[bloqueia] 1" in out
    assert "rota: touring kpi -j" in out and "saude_composta_daemon" in out and "denominador:" in out


def test_cli_a_failed_source_still_exits_2_with_the_item(tmp_path, capsys):
    pasta = _escrever_capturas(tmp_path)
    (pasta / "e2e.json").unlink()
    disp = tmp_path / "d.yaml"
    disp.write_text("dispensas: []\n", encoding="utf-8")
    assert R.main(["--from-dir", str(pasta), "--dispensas", str(disp), "--hoje", "2026-10-18", "--json"]) == 2
    out = json.loads(capsys.readouterr().out)
    assert "fonte.e2e" in ids(out["itens"]) and out["fontes"]["e2e"]["ok"] is False


def test_cli_usage_error_does_not_alias_the_verdict(capsys):
    with pytest.raises(SystemExit) as exc:
        R.main(["--bogus"])
    assert exc.value.code == 64
    with pytest.raises(SystemExit) as exc:
        R.main(["--hoje", "nao-e-data", "--from-dir", "/nonexistent"])
    assert exc.value.code == 64


def test_text_lists_waivers_without_item_and_expired_marker(tmp_path):
    r = avaliar(fontes(), "dispensas:\n  - {item: kpi.sumiu, motivo: m, dono: d, valido_ate: 2026-12-01}\n", tmp_path)
    assert "kpi.sumiu" in R.texto(r)
    r2 = avaliar(_um_item_kpi(), "dispensas:\n  - {item: kpi.touring.cache.hit_ratio, motivo: m, dono: d, valido_ate: 2026-01-01}\n", tmp_path)
    assert "dispensa expirou em 2026-01-01" in R.texto(r2)
