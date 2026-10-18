#!/usr/bin/env python3
"""Tests for scripts/coverage_ratchet.py and docs/kpi/coverage-floor.json (decision N2, 17/10/2026).

The CI `coverage` job produces an lcov that fed F3.1 but never failed anything. The ratchet:
fail when the TOTAL line coverage falls below `floor_pct - tolerance_pp`; `floor_pct: null` means
"never measured in CI" — record the total, pass; a missing or empty lcov fails (absent signal is
not zero, and a 0/0 ratio must not read as 100 %). The floor only moves up, and only by `--update`.
"""
from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parent / "coverage_ratchet.py"
_spec = importlib.util.spec_from_file_location("coverage_ratchet_under_test", SCRIPT)
C = importlib.util.module_from_spec(_spec)
sys.modules["coverage_ratchet_under_test"] = C
_spec.loader.exec_module(C)

ROOT = Path(__file__).resolve().parent.parent


def lcov(*registros: tuple[int, int]) -> str:
    """One record per (hit, found) pair, in the shape cargo-llvm-cov emits."""
    out = []
    for i, (hit, found) in enumerate(registros):
        out += [f"SF:/w/crate{i}/src/lib.rs", f"LF:{found}", f"LH:{hit}", "end_of_record"]
    return "\n".join(out) + "\n"


def arquivos(tmp_path: Path, texto: str | None, piso: dict | str | None = None):
    l = tmp_path / "lcov.info"
    if texto is not None:
        l.write_text(texto, encoding="utf-8")
    f = tmp_path / "floor.json"
    if piso is not None:
        f.write_text(piso if isinstance(piso, str) else json.dumps(piso), encoding="utf-8")
    return l, f


def rodar(l, f, *extra, capsys=None) -> int:
    return C.main(["--lcov", str(l), "--floor", str(f), *extra])


NULO = {"floor_pct": None, "tolerance_pp": 0.5, "measured_at": None, "note": "n"}


# --- reading the lcov -------------------------------------------------------------------------------

def test_total_is_the_sum_of_hits_over_the_sum_of_found_lines_across_records(tmp_path):
    l, _ = arquivos(tmp_path, lcov((50, 100), (30, 100), (0, 100)))
    assert C.total_pct(l) == pytest.approx(26.6667, abs=1e-4), "80/300, not the mean of the three ratios"


def test_total_is_rounded_to_four_decimals(tmp_path):
    l, _ = arquivos(tmp_path, lcov((1, 3)))
    assert C.total_pct(l) == 33.3333


def test_records_without_lf_lh_fall_back_to_their_da_lines(tmp_path):
    texto = "SF:/a.rs\nDA:1,5\nDA:2,0\nDA:3,1\nDA:4,0\nend_of_record\n"
    l, _ = arquivos(tmp_path, texto)
    assert C.total_pct(l) == 50.0


def test_lf_lh_win_over_da_when_both_exist(tmp_path):
    texto = "SF:/a.rs\nDA:1,1\nDA:2,1\nLF:10\nLH:2\nend_of_record\n"
    l, _ = arquivos(tmp_path, texto)
    assert C.total_pct(l) == 20.0


def test_unrelated_lines_are_ignored(tmp_path):
    texto = "TN:\nSF:/a.rs\nFN:1,foo\nFNDA:3,foo\nBRDA:1,0,0,1\nLF:4\nLH:3\nend_of_record\n"
    l, _ = arquivos(tmp_path, texto)
    assert C.total_pct(l) == 75.0


@pytest.mark.parametrize("texto", ["", "\n\n", "TN:\n", lcov((0, 0)), lcov((0, 0), (0, 0))])
def test_an_lcov_with_no_instrumented_lines_is_unusable_not_100_percent(tmp_path, texto):
    l, _ = arquivos(tmp_path, texto)
    with pytest.raises(C.LcovInutilizavel):
        C.total_pct(l)


def test_a_record_with_lf_but_no_lh_falls_back_to_its_da_lines(tmp_path):
    l, _ = arquivos(tmp_path, "SF:/a.rs\nLF:10\nDA:1,1\nDA:2,0\nend_of_record\n")
    assert C.total_pct(l) == 50.0, "LF alone is an incomplete summary; trust the DA lines, not half a summary"


@pytest.mark.parametrize("da", ["DA:7", "DA:7,muitas", "DA:"])
def test_malformed_da_lines_are_unusable_not_a_crash(tmp_path, da):
    l, _ = arquivos(tmp_path, f"SF:/a.rs\n{da}\nend_of_record\n")
    with pytest.raises(C.LcovInutilizavel):
        C.total_pct(l)


def test_a_trailing_record_without_end_of_record_still_counts(tmp_path):
    l, _ = arquivos(tmp_path, "SF:/a.rs\nLF:10\nLH:5\nend_of_record\nSF:/b.rs\nLF:10\nLH:10\n")
    assert C.total_pct(l) == 75.0


def test_da_with_a_checksum_field_counts(tmp_path):
    l, _ = arquivos(tmp_path, "SF:/a.rs\nDA:1,3,abcd\nDA:2,0,efgh\nend_of_record\n")
    assert C.total_pct(l) == 50.0


def test_a_missing_lcov_is_unusable(tmp_path):
    with pytest.raises(C.LcovInutilizavel):
        C.total_pct(tmp_path / "nao-existe.info")


def test_non_numeric_lf_lh_is_unusable_not_a_crash(tmp_path):
    l, _ = arquivos(tmp_path, "SF:/a.rs\nLF:muitas\nLH:3\nend_of_record\n")
    with pytest.raises(C.LcovInutilizavel):
        C.total_pct(l)


# --- the floor file ---------------------------------------------------------------------------------

def test_the_shipped_floor_file_is_valid_and_starts_unmeasured():
    piso = C.carregar_piso(ROOT / "docs" / "kpi" / "coverage-floor.json")
    assert piso["floor_pct"] is None, "the first real CI measurement sets it; nobody invents a number"
    assert piso["tolerance_pp"] == 0.5


@pytest.mark.parametrize("conteudo", [
    "nao e json", "[]", "{}", '{"floor_pct": "70", "tolerance_pp": 0.5}', '{"floor_pct": 120, "tolerance_pp": 0.5}',
    '{"floor_pct": -1, "tolerance_pp": 0.5}', '{"floor_pct": 70, "tolerance_pp": -0.1}', '{"floor_pct": 70, "tolerance_pp": "x"}',
    '{"floor_pct": 70}', '{"floor_pct": true, "tolerance_pp": 0.5}', '{"floor_pct": 70, "tolerance_pp": true}',
    '{"tolerance_pp": 0.5}', '{"floor_pct": NaN, "tolerance_pp": 0.5}', '{"floor_pct": 70, "tolerance_pp": NaN}',
    '{"floor_pct": Infinity, "tolerance_pp": 0.5}', '{"floor_pct": 70, "tolerance_pp": Infinity}',
])
def test_invalid_floor_file_is_rejected(tmp_path, conteudo):
    _, f = arquivos(tmp_path, None, conteudo)
    with pytest.raises(C.PisoInvalido):
        C.carregar_piso(f)


def test_missing_floor_file_is_rejected(tmp_path):
    with pytest.raises(C.PisoInvalido):
        C.carregar_piso(tmp_path / "nao-existe.json")


# --- the ratchet decision -----------------------------------------------------------------------------

def test_floor_null_records_and_passes(tmp_path, capsys):
    l, f = arquivos(tmp_path, lcov((7, 10)), NULO)
    assert rodar(l, f) == 0
    out = capsys.readouterr().out
    assert "70.0000" in out and "not measured in CI yet" in out and "--update" in out


def test_floor_null_never_fails_whatever_the_total(tmp_path):
    l, f = arquivos(tmp_path, lcov((0, 10)), NULO)
    assert rodar(l, f) == 0


def test_total_above_the_floor_passes_and_says_the_floor_can_rise(tmp_path, capsys):
    l, f = arquivos(tmp_path, lcov((80, 100)), {**NULO, "floor_pct": 70.0})
    assert rodar(l, f) == 0
    assert "can rise" in capsys.readouterr().out


def test_total_exactly_at_the_floor_passes_without_suggesting_a_rise(tmp_path, capsys):
    l, f = arquivos(tmp_path, lcov((70, 100)), {**NULO, "floor_pct": 70.0})
    assert rodar(l, f) == 0
    assert "can rise" not in capsys.readouterr().out


def test_total_inside_the_tolerance_band_passes(tmp_path):
    l, f = arquivos(tmp_path, lcov((695, 1000)), {**NULO, "floor_pct": 70.0})  # 69.5 == floor - 0.5
    assert rodar(l, f) == 0


def test_total_just_below_the_tolerance_band_fails(tmp_path, capsys):
    l, f = arquivos(tmp_path, lcov((6949, 10000)), {**NULO, "floor_pct": 70.0})  # 69.49 < 69.5
    assert rodar(l, f) == 1
    out = capsys.readouterr().out
    assert "::error::" in out and "69.49" in out and "70.0" in out


def test_the_tolerance_is_the_files_not_a_constant(tmp_path):
    l, f = arquivos(tmp_path, lcov((695, 1000)), {**NULO, "floor_pct": 70.0, "tolerance_pp": 0.1})
    assert rodar(l, f) == 1


def test_missing_lcov_exits_2_not_a_silent_pass(tmp_path, capsys):
    l, f = arquivos(tmp_path, None, {**NULO, "floor_pct": 70.0})
    assert rodar(l, f) == 2
    assert "::error::" in capsys.readouterr().out


def test_empty_lcov_exits_2_even_when_the_floor_is_null(tmp_path):
    l, f = arquivos(tmp_path, "", NULO)
    assert rodar(l, f) == 2


def test_invalid_floor_exits_2(tmp_path):
    l, f = arquivos(tmp_path, lcov((7, 10)), "lixo")
    assert rodar(l, f) == 2


def test_step_summary_gets_the_total_when_github_provides_it(tmp_path, monkeypatch):
    resumo = tmp_path / "summary.md"
    monkeypatch.setenv("GITHUB_STEP_SUMMARY", str(resumo))
    l, f = arquivos(tmp_path, lcov((7, 10)), NULO)
    rodar(l, f)
    assert "70.0000" in resumo.read_text(encoding="utf-8")


def test_no_step_summary_is_fine(tmp_path, monkeypatch):
    monkeypatch.delenv("GITHUB_STEP_SUMMARY", raising=False)
    l, f = arquivos(tmp_path, lcov((7, 10)), NULO)
    assert rodar(l, f) == 0


# --- --update: the only way up --------------------------------------------------------------------------

def test_update_sets_a_null_floor_to_the_measured_total_truncated_to_two_decimals(tmp_path):
    l, f = arquivos(tmp_path, lcov((2, 3)), NULO)  # 66.6667
    assert rodar(l, f, "--update") == 0
    novo = json.loads(f.read_text(encoding="utf-8"))
    assert novo["floor_pct"] == 66.66, "truncated, never rounded up past the real value"
    assert novo["measured_at"] and novo["tolerance_pp"] == 0.5 and novo["note"] == "n"


def test_update_raises_the_floor_when_the_total_is_higher(tmp_path):
    l, f = arquivos(tmp_path, lcov((8, 10)), {**NULO, "floor_pct": 70.0})
    rodar(l, f, "--update")
    assert json.loads(f.read_text(encoding="utf-8"))["floor_pct"] == 80.0


def test_update_with_a_total_equal_to_the_floor_does_not_rewrite_the_file(tmp_path):
    l, f = arquivos(tmp_path, lcov((70, 100)), {**NULO, "floor_pct": 70.0})
    antes = f.read_text(encoding="utf-8")
    assert rodar(l, f, "--update") == 0
    assert f.read_text(encoding="utf-8") == antes


def test_update_never_lowers_the_floor(tmp_path):
    l, f = arquivos(tmp_path, lcov((695, 1000)), {**NULO, "floor_pct": 70.0})
    antes = f.read_text(encoding="utf-8")
    assert rodar(l, f, "--update") == 0
    assert f.read_text(encoding="utf-8") == antes


def test_update_does_not_run_the_check_below_the_floor_as_a_pass(tmp_path):
    l, f = arquivos(tmp_path, lcov((50, 100)), {**NULO, "floor_pct": 70.0})
    antes = f.read_text(encoding="utf-8")
    assert rodar(l, f, "--update") == 1, "--update raises the floor; it does not excuse a regression"
    assert f.read_text(encoding="utf-8") == antes


def test_without_update_the_floor_file_is_never_written(tmp_path):
    l, f = arquivos(tmp_path, lcov((9, 10)), {**NULO, "floor_pct": 70.0})
    antes = f.read_text(encoding="utf-8")
    rodar(l, f)
    assert f.read_text(encoding="utf-8") == antes


# --- usage -----------------------------------------------------------------------------------------------

def test_usage_errors_exit_64_so_they_never_alias_a_ratchet_failure(capsys):
    with pytest.raises(SystemExit) as exc:
        C.main(["--bogus"])
    assert exc.value.code == 64
    with pytest.raises(SystemExit) as exc:
        C.main([])
    assert exc.value.code == 64
