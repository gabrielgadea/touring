#!/usr/bin/env python3
"""test_cargo_diagnosis.py — the judge must name WHAT failed, not just THAT.

Every fixture below is verbatim output from a real `cargo` run (rustc 1.97,
23/08/2026) against a throwaway crate built to fail on purpose. Fixtures
invented from memory would test my recollection of cargo's format, which is the
very thing that was wrong: the gate assumed the diagnosis lived on stderr, and
for `cargo test` it lives on stdout.

The verdict is NOT under test here — `_cargo_diagnosis` only builds the detail
string of an already-decided FAIL. What is under test is that the string sends a
reader to a test name or a line of code instead of to a tally.
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from loop_converged import (  # noqa: E402
    CARGO_DETAIL_MAX,
    _cargo_diagnosis,
    _failing_tests,
    _rustc_errors,
)

# ---------------------------------------------------------------- fixtures

TEST_STDOUT = """
running 3 tests
test tests::passa_de_boa ... ok
test tests::este_reprova_de_proposito ... FAILED
test tests::este_tambem_reprova ... FAILED

failures:

---- tests::este_reprova_de_proposito stdout ----

thread 'tests::este_reprova_de_proposito' (348053) panicked at src/lib.rs:8:38:
assertion `left == right` failed: o nome deste teste tem de aparecer
  left: 1
 right: 2
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- tests::este_tambem_reprova stdout ----

thread 'tests::este_tambem_reprova' (348054) panicked at src/lib.rs:10:32:
segundo alvo


failures:
    tests::este_reprova_de_proposito
    tests::este_tambem_reprova

test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
"""

TEST_STDERR = """   Compiling cargofail v0.0.0 (/tmp/cargofail1)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.08s
     Running unittests src/lib.rs (target/debug/deps/cargofail-002a889dab0b8e43)
error: test failed, to rerun pass `--lib`
"""

CHECK_STDERR = """    Checking cargofail v0.0.0 (/tmp/cargofail1)
error[E0308]: mismatched types
 --> src/lib.rs:1:22
  |
1 | pub fn ok() -> u32 { "isto nao e um u32" }
  |                ---   ^^^^^^^^^^^^^^^^^^^ expected `u32`, found `&str`

error[E0425]: cannot find function `nao_existe` in this scope
 --> src/lib.rs:2:25
  |
2 | pub fn outra() -> u32 { nao_existe() }
  |                         ^^^^^^^^^^ not found in this scope

Some errors have detailed explanations: E0308, E0425.
error: could not compile `cargofail` (lib) due to 2 previous errors
"""

CLIPPY_STDERR = """    Checking cargofail v0.0.0 (/tmp/cargofail1)
error: writing `&Vec` instead of `&[_]` involves a new object where a slice will do
 --> src/lib.rs:1:14
  |
  = note: `-D clippy::ptr-arg` implied by `-D warnings`

error: unneeded `return` statement
 --> src/lib.rs:2:35
  |
  = note: `-D clippy::needless-return` implied by `-D warnings`

error: could not compile `cargofail` (lib) due to 2 previous errors
"""

# ---------------------------------------------------------------- the gap itself


def test_a_ultima_linha_do_stderr_nao_nomeia_o_teste():
    """The premise. If this ever fails, the whole fix is unnecessary.

    This is the behaviour the gate had: keep the last stderr line. It names the
    target (`--lib`) and no test — so a reader cannot reproduce without first
    re-running the whole suite themselves.
    """
    ultima = TEST_STDERR.strip().splitlines()[-1]
    assert "test failed" in ultima
    assert "este_reprova_de_proposito" not in ultima
    assert "este_tambem_reprova" not in ultima


def test_os_nomes_dos_testes_vivem_no_stdout():
    """...and they were there the whole time, on the stream that was discarded."""
    assert _failing_tests(TEST_STDOUT) == [
        "tests::este_reprova_de_proposito",
        "tests::este_tambem_reprova",
    ]


# ---------------------------------------------------------------- test step


def test_diagnostico_de_teste_carrega_nomes_contagem_e_cauda():
    d = _cargo_diagnosis("test", TEST_STDOUT, TEST_STDERR)
    assert "2 failing tests" in d, d
    assert "tests::este_reprova_de_proposito" in d, d
    assert "tests::este_tambem_reprova" in d, d
    # nothing that used to be readable stopped being readable
    assert "test failed, to rerun pass `--lib`" in d, d


def test_contagem_vem_do_tally_do_libtest_nao_do_parse():
    """The tally is libtest's own; the list is ours. Divergence stays visible."""
    truncado = TEST_STDOUT.replace("    tests::este_tambem_reprova\n", "")
    d = _cargo_diagnosis("test", truncado, TEST_STDERR)
    assert "2 failing tests" in d, d          # libtest still says 2
    assert "este_tambem_reprova" not in d, d  # but only 1 could be named


def test_singular_quando_um_so():
    stdout = """
failures:
    rlm::pipeline::async_executor::tests::test_parallel_beats_sequential

test result: FAILED. 1997 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
"""
    d = _cargo_diagnosis("test", stdout, "error: test failed, to rerun pass `-p kazuba-rlm --lib`")
    assert "1 failing test —" in d, d
    assert "async_executor::tests::test_parallel_beats_sequential" in d, d
    assert "-p kazuba-rlm --lib" in d, d


# ---------------------------------------------------------------- check / clippy


def test_diagnostico_de_check_carrega_codigo_local_e_mensagem():
    assert _rustc_errors(CHECK_STDERR) == [
        "E0308 src/lib.rs:1:22 mismatched types",
        "E0425 src/lib.rs:2:25 cannot find function `nao_existe` in this scope",
    ]
    d = _cargo_diagnosis("check", "", CHECK_STDERR)
    assert "2 errors" in d, d
    assert "src/lib.rs:1:22" in d, d


def test_a_contagem_de_rustc_e_descartada_do_corpo():
    """`could not compile ... due to 2 previous errors` sends nobody anywhere."""
    for linha in _rustc_errors(CHECK_STDERR):
        assert "could not compile" not in linha
        assert "previous errors" not in linha


def test_clippy_sem_codigo_de_erro_ainda_localiza():
    d = _cargo_diagnosis("clippy", "", CLIPPY_STDERR)
    assert "2 errors" in d, d
    assert "src/lib.rs:1:14 writing `&Vec`" in d, d
    assert "src/lib.rs:2:35 unneeded `return` statement" in d, d


# ---------------------------------------------------------------- bounds & fallback


def test_o_que_nao_cabe_e_contado_nunca_omitido_em_silencio():
    n = CARGO_DETAIL_MAX + 4
    nomes = "\n".join(f"    crate::tests::t{i}" for i in range(n))
    stdout = f"failures:\n{nomes}\n\ntest result: FAILED. 0 passed; {n} failed;\n"
    d = _cargo_diagnosis("test", stdout, "")
    assert f"{n} failing tests" in d, d
    assert d.count("crate::tests::t") == CARGO_DETAIL_MAX, d
    assert f"(+{n - CARGO_DETAIL_MAX} not listed)" in d, d


def test_formato_desconhecido_degrada_para_o_comportamento_antigo():
    """A cargo that changes its output must cost a worse message, never a wrong verdict."""
    ruido = "algo totalmente diferente\nultima linha do futuro\n"
    assert _cargo_diagnosis("test", "sem bloco de failures", ruido) == "ultima linha do futuro"
    assert _cargo_diagnosis("check", "", ruido) == "ultima linha do futuro"


def test_streams_vazios_nao_explodem():
    assert _cargo_diagnosis("test", "", "") == ""
    assert _cargo_diagnosis("check", None, None) == ""
    assert _failing_tests(None) == []
    assert _rustc_errors(None) == []


def test_o_diagnostico_nunca_levanta():
    """Fail-open is the file's contract: a diagnosis may not break a verdict."""

    class Explode(str):
        def splitlines(self):  # noqa: D102
            raise RuntimeError("boom")

    assert _cargo_diagnosis("test", Explode("x"), "") == ""
