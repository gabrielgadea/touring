"""Testes do emissor Python `gen_sdk.py`.

A comparação com o leitor Rust (a referência) mora em
`crates/touring-code/tests/sdk_cross_emitter.rs`; aqui ficam o que só o Python tem — a
extensão declarada `--extras` — e a leitura estrita do que é um run.
"""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parent / "gen_sdk.py"

_spec = importlib.util.spec_from_file_location("gen_sdk", SCRIPT)
assert _spec is not None and _spec.loader is not None
gen_sdk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(gen_sdk)

RUN = {"ts": 1, "language": "bash", "exit_code": 0, "duration_ms": 5}


def _journal(tmp_path: Path, lines: list[str]) -> Path:
    path = tmp_path / "run_journal.jsonl"
    path.write_text("".join(f"{line}\n" for line in lines), encoding="utf-8")
    return path


def _run(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(SCRIPT), *args], capture_output=True, text=True, check=False
    )


def test_extras_is_opt_in_and_carries_the_byte_totals(tmp_path: Path) -> None:
    journal = _journal(
        tmp_path,
        [
            json.dumps({**RUN, "bytes_elided": 3, "code_hash_stdout_bytes": 10}),
            json.dumps({**RUN, "bytes_elided": 4}),
        ],
    )
    default = json.loads(_run("--journal", str(journal)).stdout)
    assert "_extras" not in default, "the default output has the Rust SignalReport's keys"
    with_extras = json.loads(_run("--journal", str(journal), "--extras").stdout)
    assert with_extras["_extras"] == {
        "total_bytes_elided": 7,
        "total_code_hash_stdout_bytes": 10,
    }
    with_extras.pop("_extras")
    for report in (default, with_extras):
        report.pop("generated_at_unix")
    assert default == with_extras, "--extras only ADDS a key"


@pytest.mark.parametrize(
    "line",
    [
        '{"repeat_blocked":{"hash":"abc"}}',
        "123",
        "[1]",
        "null",
        "not json",
        '{"ts":1,"language":"bash","exit_code":0}',
        '{"ts":1,"language":"bash","exit_code":0,"duration_ms":1.5}',
        '{"ts":-1,"language":"bash","exit_code":0,"duration_ms":1}',
        '{"ts":1,"language":"bash","exit_code":true,"duration_ms":1}',
        '{"ts":1,"language":"bash","exit_code":0,"duration_ms":1,"bytes_elided":null}',
        '{"ts":1,"language":"bash","exit_code":0,"duration_ms":NaN}',
        '{"ts":1,"language":"bash","exit_code":0,"duration_ms":4294967296}',
        '{"ts":1,"language":"bash","exit_code":2147483648,"duration_ms":1}',
    ],
)
def test_what_is_not_a_run_is_not_counted(line: str, tmp_path: Path) -> None:
    journal = _journal(tmp_path, [json.dumps(RUN), line])
    entries, first_malformed = gen_sdk.read_journal(journal)
    assert len(entries) == 1, line
    assert first_malformed == 1, line


def test_the_optional_fields_default_when_absent_and_pass_when_typed(tmp_path: Path) -> None:
    full = {
        **RUN,
        "failure_kind": None,
        "source": "file",
        "file": None,
        "harvest": None,
        "brief": True,
        "orchestrate": False,
        "tmp_bytes": 0,
        "hint": {"key": "k", "sim": 1},
        "a_field_from_the_future": [1, 2],
    }
    journal = _journal(tmp_path, [json.dumps(RUN), json.dumps(full)])
    entries, first_malformed = gen_sdk.read_journal(journal)
    assert len(entries) == 2
    assert first_malformed is None


def test_a_journal_where_nothing_is_a_run_is_refused(tmp_path: Path) -> None:
    journal = _journal(tmp_path, ["not json", '{"repeat_blocked":{}}'])
    out = _run("--journal", str(journal))
    assert out.returncode == 1
    assert "no line is a run" in out.stderr
