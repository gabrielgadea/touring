#!/usr/bin/env python3
"""Coverage ratchet: fail CI when TOTAL line coverage falls below the recorded floor (decision N2).

Origin (17/10/2026, ECC analysis): the `coverage` job runs cargo-llvm-cov and publishes an lcov that
feeds the F3.1 quality dimension, but nothing failed when coverage fell. A fixed `--fail-under` would
be an arbitrary number (too low protects nothing, too high blocks); a ratchet needs no number chosen
in advance: the floor is the last measured total, minus a tolerance, and it only ever moves up.

    python3 scripts/coverage_ratchet.py --lcov lcov.info --floor docs/kpi/coverage-floor.json
    python3 scripts/coverage_ratchet.py ... --update      # raise the floor to the measured total

`docs/kpi/coverage-floor.json`:  {"floor_pct": null | number, "tolerance_pp": 0.5, ...}

* `floor_pct: null` = never measured in CI. The step RECORDS the total (stdout and the job summary)
  and passes. The first real measurement on main becomes the floor through `--update` — nobody
  invents the initial number.
* Otherwise fail (exit 1) when `total < floor_pct - tolerance_pp`. The 0.5 pp default absorbs the
  run-to-run noise of ignored or conditional tests; calibrate it from three equal runs.
* A missing, unreadable or empty lcov fails (exit 2): absent signal is not zero, and a 0/0 ratio
  must never read as 100 %. `--update` only raises (never lowers) and truncates to two decimals, so
  the stored floor can never exceed the value that produced it; it does not excuse a regression.
* Line quality of the DIFF itself is not this script's job: a new line without a test leaves a
  surviving mutant, which the mutants-diff job reports. One floor, one judge, no duplicate gate.

Exit: 0 ok, 1 below the floor, 2 unusable input, 64 usage.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import math
import os
import sys
from pathlib import Path


class LcovInutilizavel(Exception):
    """The lcov is missing or has no instrumented line: no total can be computed."""


class PisoInvalido(Exception):
    """The floor file is missing or malformed."""


def _inteiro(texto: str) -> int:
    try:
        return int(texto)
    except ValueError as exc:
        raise LcovInutilizavel(f"valor não numérico no lcov: {texto!r}") from exc


def _registro(linhas: list[str]) -> tuple[int, int]:
    """(hit, found) of one SF record: LF/LH when present, else counted from its DA lines."""
    lf = lh = None
    da_found = da_hit = 0
    for ln in linhas:
        if ln.startswith("LF:"):
            lf = _inteiro(ln[3:])
        elif ln.startswith("LH:"):
            lh = _inteiro(ln[3:])
        elif ln.startswith("DA:"):  # DA:<line>,<execution count>[,<checksum>]
            partes = ln[3:].split(",")
            if len(partes) < 2:
                raise LcovInutilizavel(f"linha DA malformada: {ln!r}")
            da_found += 1
            da_hit += 1 if _inteiro(partes[1]) > 0 else 0
    if lf is not None and lh is not None:
        return lh, lf
    return da_hit, da_found


def total_pct(caminho: Path) -> float:
    """Total line coverage in percent, 4 decimals: sum(hit) / sum(found) over every record."""
    try:
        texto = caminho.read_text(encoding="utf-8", errors="replace")
    except OSError as exc:
        raise LcovInutilizavel(f"lcov ilegível: {exc}") from exc
    hit = found = 0
    atual: list[str] = []
    for ln in texto.splitlines():
        ln = ln.strip()
        if ln == "end_of_record":
            h, f = _registro(atual)
            hit, found, atual = hit + h, found + f, []
        else:
            atual.append(ln)
    if atual:  # a trailing record without end_of_record still counts
        h, f = _registro(atual)
        hit, found = hit + h, found + f
    if found == 0:
        raise LcovInutilizavel("lcov sem nenhuma linha instrumentada (found == 0)")
    return round(100.0 * hit / found, 4)


def _numero(v: object) -> bool:
    return isinstance(v, int | float) and not isinstance(v, bool) and math.isfinite(v)


def carregar_piso(caminho: Path) -> dict:
    try:
        doc = json.loads(caminho.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise PisoInvalido(f"piso ilegível: {exc}") from exc
    if not isinstance(doc, dict) or "floor_pct" not in doc or "tolerance_pp" not in doc:
        raise PisoInvalido("piso precisa ser um objeto com floor_pct e tolerance_pp")
    piso, tol = doc["floor_pct"], doc["tolerance_pp"]
    if piso is not None and not (_numero(piso) and 0 <= piso <= 100):
        raise PisoInvalido(f"floor_pct precisa ser null ou número em [0,100], veio {piso!r}")
    if not (_numero(tol) and tol >= 0):
        raise PisoInvalido(f"tolerance_pp precisa ser número >= 0, veio {tol!r}")
    return doc


def _resumo(linha: str) -> None:
    destino = os.environ.get("GITHUB_STEP_SUMMARY")
    if destino:
        with open(destino, "a", encoding="utf-8") as fh:
            fh.write(linha + "\n")


def _atualizar(caminho: Path, doc: dict, total: float) -> None:
    novo = math.floor(total * 100) / 100
    if doc["floor_pct"] is not None and novo <= doc["floor_pct"]:
        print(f"floor stays at {doc['floor_pct']} (measured {total:.4f} does not exceed it)")
        return
    doc = {**doc, "floor_pct": novo, "measured_at": dt.datetime.now(dt.UTC).date().isoformat()}
    caminho.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"floor raised to {novo} (measured {total:.4f})")


def avaliar(total: float, doc: dict) -> int:
    piso, tol = doc["floor_pct"], doc["tolerance_pp"]
    if piso is None:
        msg = f"line coverage {total:.4f}% — floor not measured in CI yet; record it with --update"
        print(f"::notice::{msg}")
        _resumo(f"**Coverage** {total:.4f}% (floor not measured in CI yet)")
        return 0
    limite = round(piso - tol, 4)
    if round(total, 4) < limite:
        print(f"::error::line coverage {total:.4f}% fell below the floor {piso} - {tol} pp = {limite}")
        _resumo(f"**Coverage** {total:.4f}% — BELOW the floor {piso} (- {tol} pp)")
        return 1
    dica = f"; the floor can rise to {math.floor(total * 100) / 100} (--update)" if total > piso else ""
    print(f"line coverage {total:.4f}% >= {limite} (floor {piso} - {tol} pp){dica}")
    _resumo(f"**Coverage** {total:.4f}% (floor {piso}){dica}")
    return 0


class _Parser(argparse.ArgumentParser):
    def error(self, message: str) -> None:  # exit 1/2 mean "below the floor" / "unusable input"
        self.print_usage(sys.stderr)
        self.exit(64, f"{self.prog}: erro: {message}\n")


def main(argv: list[str] | None = None) -> int:
    ap = _Parser(description=__doc__.splitlines()[0])
    ap.add_argument("--lcov", required=True)
    ap.add_argument("--floor", required=True)
    ap.add_argument("--update", action="store_true", help="raise the floor to the measured total (never lowers)")
    args = ap.parse_args(argv)
    try:
        total = total_pct(Path(args.lcov))
        doc = carregar_piso(Path(args.floor))
    except (LcovInutilizavel, PisoInvalido) as exc:
        print(f"::error::coverage ratchet cannot decide: {exc}")
        return 2
    veredito = avaliar(total, doc)
    if args.update:  # _atualizar only ever raises, so a regression is never excused by it
        _atualizar(Path(args.floor), doc, total)
    return veredito


if __name__ == "__main__":
    sys.exit(main())
