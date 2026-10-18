#!/usr/bin/env python3
"""Tests for scripts/journal_gc.py and docs/kpi/journals.toml (decision N6, 17/10/2026).

The thing being protected is *data that a KPI reads*: pruning a journal must never change a
number someone depends on, never run behind a reader that needs the bytes, and never be the
first time anyone learns that a writer has no reader. So the suite proves, in this order:

1. the manifest is consistent with itself AND with the code it names (retention >= every
   reader's window; writer/reader files exist and really mention the journal);
2. dry-run (the default) changes nothing;
3. `--apply` removes exactly the expired lines, keeps order, keeps the undatable, archives and
   rolls up first, and a KPI computed from the rollup equals the one computed from raw;
4. it is idempotent and crash-safe (a retry never double-counts), concurrent appends survive,
   and a journal held open for writing is skipped.

Nothing here touches ~/.claude/touring: every test runs on a tmp base.
"""
from __future__ import annotations

import datetime as dt
import gzip
import hashlib
import importlib.util
import json
import os
import subprocess
import sys
import time
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parent / "journal_gc.py"
_spec = importlib.util.spec_from_file_location("journal_gc_under_test", SCRIPT)
J = importlib.util.module_from_spec(_spec)
sys.modules["journal_gc_under_test"] = J
_spec.loader.exec_module(J)

REPO = Path(__file__).resolve().parent.parent
UTC = dt.timezone.utc
AGORA = dt.datetime(2026, 10, 18, 12, 0, tzinfo=UTC)


def dia(offset: int) -> dt.date:
    """The UTC day `offset` days before AGORA's day."""
    return AGORA.date() - dt.timedelta(days=offset)


def ms(offset: int, hora: int = 10) -> int:
    d = dia(offset)
    return int(dt.datetime(d.year, d.month, d.day, hora, tzinfo=UTC).timestamp() * 1000)


def linha_trace(offset: int, hook="cli-suggest", route="daemon", elapsed=10, hora=10) -> bytes:
    return (json.dumps({"ts_ms": ms(offset, hora), "hook": hook, "route": route, "exit_reason": "ok", "elapsed_ms": elapsed}) + "\n").encode()


def journal(**over) -> dict:
    base = {
        "id": "trace", "caminho": "trace.jsonl", "tipo": "jsonl",
        "escritores": ["w.rs"], "leitores": [{"arquivo": "r.py", "janela_dias": 2, "verificada": True}],
        "retencao_dias": 30, "expira": "arquivar",
        "ts": {"campo": "ts_ms", "unidade": "epoch_ms"},
        "rollup": {"agrupar_por": ["hook", "route"], "somar": ["elapsed_ms"]},
    }
    base.update(over)
    return base


def manifesto(*journals, censo=None) -> dict:
    return {"versao": 1, "base": "~/x", "censo": censo or {"diretorios": ["."], "extensoes": [".jsonl", ".log"]}, "journal": list(journals)}


@pytest.fixture
def mundo(tmp_path):
    """A fake repo whose writer/reader files really mention the journal, and an empty base."""
    repo, base = tmp_path / "repo", tmp_path / "base"
    repo.mkdir()
    base.mkdir()
    (repo / "w.rs").write_text('const P: &str = "trace.jsonl"; // writer\n', encoding="utf-8")
    (repo / "r.py").write_text('PATH = "trace.jsonl"  # reader\n', encoding="utf-8")
    return repo, base


def problemas(m, repo, base=None, nivel=None):
    out = J.verificar(m, repo, base)
    return [p for p in out if nivel is None or p["nivel"] == nivel]


def codigos(m, repo, base=None, nivel=None):
    return sorted(p["codigo"] for p in problemas(m, repo, base, nivel))


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


# --- 1. the manifest, against the code it names ---------------------------------------------------

def test_the_shipped_manifest_has_no_errors_against_the_real_repo():
    m = J.carregar(REPO / "docs" / "kpi" / "journals.toml")
    erros = [p for p in J.verificar(m, REPO, None) if p["nivel"] == "erro"]
    assert erros == [], erros


def test_the_shipped_manifest_covers_the_big_journals_and_reports_window_blockers():
    m = J.carregar(REPO / "docs" / "kpi" / "journals.toml")
    por_id = {j["id"]: j for j in m["journal"]}
    assert {"hook_trace", "run_journal", "run_subcalls", "memwatch"} <= set(por_id)
    assert por_id["memwatch"]["leitores"] == [], "memwatch.log has zero readers: the manifest must say so, not hide it"
    assert any(l["janela_dias"] == "toda" for l in por_id["run_journal"]["leitores"]), "run_journal is read whole by touring-code/journal.rs"
    assert por_id["hook_trace"]["retencao_dias"] == 30 and por_id["hook_trace"]["expira"] == "arquivar"


def test_consistent_manifest_has_no_problems_except_the_info(mundo):
    repo, base = mundo
    assert codigos(manifesto(journal()), repo, base, "erro") == []


def test_duplicate_id_is_an_error(mundo):
    repo, _ = mundo
    assert "id_duplicado" in codigos(manifesto(journal(), journal(caminho="outro.jsonl")), repo, nivel="erro")


@pytest.mark.parametrize("campo", ["id", "caminho", "tipo", "retencao_dias", "expira", "escritores"])
def test_missing_required_field_is_an_error(mundo, campo):
    repo, _ = mundo
    j = journal()
    del j[campo]
    assert "campo_ausente" in codigos(manifesto(j), repo, nivel="erro")


def test_unknown_type_or_expiry_policy_is_an_error(mundo):
    repo, _ = mundo
    assert "valor_invalido" in codigos(manifesto(journal(tipo="xml")), repo, nivel="erro")
    assert "valor_invalido" in codigos(manifesto(journal(expira="queimar")), repo, nivel="erro")


def test_retention_shorter_than_a_reader_window_is_an_error_and_equal_is_fine(mundo):
    repo, _ = mundo
    lt = journal(retencao_dias=1)
    assert "retencao_menor_que_janela" in codigos(manifesto(lt), repo, nivel="erro")
    eq = journal(retencao_dias=2)
    assert "retencao_menor_que_janela" not in codigos(manifesto(eq), repo, nivel="erro")


def test_a_reader_window_of_zero_does_not_constrain_retention(mundo):
    repo, _ = mundo
    j = journal(retencao_dias=1, leitores=[{"arquivo": "r.py", "janela_dias": 0, "verificada": True}])
    assert codigos(manifesto(j), repo, nivel="erro") == []


def test_deleting_without_rollup_behind_a_reader_is_an_error(mundo):
    repo, _ = mundo
    assert "poda_sem_rollup" in codigos(manifesto(journal(expira="apagar", rollup=None)), repo, nivel="erro")
    j = journal(expira="apagar", rollup=None, leitores=[])
    assert "poda_sem_rollup" not in codigos(manifesto(j), repo, nivel="erro")
    assert "poda_sem_rollup" not in codigos(manifesto(journal(expira="arquivar", rollup=None)), repo, nivel="erro")
    assert "poda_sem_rollup" not in codigos(manifesto(journal(expira="apagar")), repo, nivel="erro")


def test_writer_without_reader_is_flagged_as_a_warning(mundo):
    repo, _ = mundo
    m = manifesto(journal(leitores=[]))
    assert "escritor_sem_leitor" in codigos(m, repo, nivel="aviso")
    assert "escritor_sem_leitor" not in codigos(manifesto(journal()), repo)


def test_journal_without_writer_is_an_error(mundo):
    repo, _ = mundo
    assert "sem_escritor" in codigos(manifesto(journal(escritores=[])), repo, nivel="erro")


def test_missing_writer_or_reader_file_is_an_error(mundo):
    repo, _ = mundo
    assert "arquivo_inexistente" in codigos(manifesto(journal(escritores=["nao-existe.rs"])), repo, nivel="erro")
    assert "arquivo_inexistente" in codigos(manifesto(journal(leitores=[{"arquivo": "nao.py", "janela_dias": 1}])), repo, nivel="erro")


def test_file_that_does_not_mention_the_journal_is_an_error(mundo):
    repo, _ = mundo
    (repo / "w.rs").write_text("// nothing about it\n", encoding="utf-8")
    assert "arquivo_nao_cita_o_journal" in codigos(manifesto(journal()), repo, nivel="erro")


def test_external_file_is_unverifiable_not_an_error(mundo):
    repo, _ = mundo
    j = journal(escritores=["~/.claude/tools/__inexistente__.sh"])
    m = manifesto(j)
    assert "externo_nao_verificado" in codigos(m, repo, nivel="info")
    assert "arquivo_inexistente" not in codigos(m, repo)


def test_ad_hoc_writer_is_declared_without_a_file_and_is_info_only(mundo):
    repo, _ = mundo
    m = manifesto(journal(escritores=["ad-hoc: redirecionado à mão"]))
    assert "escritor_ad_hoc" in codigos(m, repo, nivel="info")
    assert codigos(m, repo, nivel="erro") == []


def test_reader_without_verified_window_is_reported_and_whole_file_is_default(mundo):
    repo, _ = mundo
    j = journal(leitores=[{"arquivo": "r.py", "janela_dias": "toda"}])
    assert "janela_nao_verificada" in codigos(manifesto(j), repo, nivel="info")


def test_path_escaping_the_base_is_an_error(mundo):
    repo, _ = mundo
    assert "caminho_fora_da_base" in codigos(manifesto(journal(caminho="../fora.jsonl")), repo, nivel="erro")
    assert "caminho_fora_da_base" in codigos(manifesto(journal(caminho="/etc/passwd")), repo, nivel="erro")


def test_census_flags_files_in_the_base_that_no_journal_claims(mundo):
    repo, base = mundo
    (base / "trace.jsonl").write_text("", encoding="utf-8")
    (base / "orfao.jsonl").write_text("", encoding="utf-8")
    (base / "orfao.log").write_text("", encoding="utf-8")
    (base / "dados.json").write_text("", encoding="utf-8")
    p = problemas(manifesto(journal()), repo, base, "aviso")
    sem = sorted(x["msg"] for x in p if x["codigo"] == "sem_manifesto")
    assert len(sem) == 2 and "orfao.jsonl" in sem[0] and "orfao.log" in sem[1]


def test_census_respects_globs_and_only_the_listed_directories(mundo):
    repo, base = mundo
    (base / "logs").mkdir()
    (base / "logs" / "a.log").write_text("", encoding="utf-8")
    (base / "outro").mkdir()
    (base / "outro" / "x.log").write_text("", encoding="utf-8")
    ciclos = journal(id="ciclos", caminho="logs/*.log", tipo="arquivos", escritores=["w.rs"], leitores=[], ts=None, rollup=None, expira="apagar")
    (repo / "w.rs").write_text('"trace.jsonl" "logs/"\n', encoding="utf-8")
    m = manifesto(journal(), ciclos, censo={"diretorios": [".", "logs"], "extensoes": [".jsonl", ".log"]})
    assert not [p for p in problemas(m, repo, base) if p["codigo"] == "sem_manifesto"]


# --- 2. timestamps -------------------------------------------------------------------------------

@pytest.mark.parametrize("valor,unidade,esperado", [
    (1_789_000_000, "epoch_s", "2026-09-10"),
    (1_789_000_000_000, "epoch_ms", "2026-09-10"),
    (1_789_000_000_000_000_000, "epoch_ns", "2026-09-10"),
    ("1789000000.037", "epoch_s", "2026-09-10"),
    ("2026-09-17T23:30:00-03:00", "iso", "2026-09-18"),
    ("2026-09-17T23:30:00Z", "iso", "2026-09-17"),
    ("2026-10-08T12:32:14-0300", "iso", "2026-10-08"),
    ("2026-10-14 00:19:37", "iso", "2026-10-14"),
    ("2026-10-14", "iso", "2026-10-14"),
])
def test_parse_dia_accepts_every_real_format(valor, unidade, esperado):
    assert J.parse_dia(valor, unidade) == esperado


@pytest.mark.parametrize("valor,unidade", [
    (None, "epoch_s"), (True, "epoch_ms"), ("", "iso"), ("amanhã", "iso"), ([], "epoch_s"), ({}, "epoch_s"),
    ("abc", "epoch_s"), (10**30, "epoch_ns"), (float("nan"), "epoch_s"), (-1, "epoch_ms"),
])
def test_parse_dia_returns_none_for_what_it_cannot_date(valor, unidade):
    assert J.parse_dia(valor, unidade) is None


@pytest.mark.parametrize("linha,esperado", [
    ("2026-10-14 00:19:37 psi=avg10=0.00 app-graphical", "2026-10-14"),
    ("[2026-10-18T01:31:07-03:00] cargo clean --target-dir x", "2026-10-18"),
    ("2026-08-23T16:32:51-03:00 [update-touring] ====", "2026-08-23"),
    ("[2026-08-23T19:16:33-03:00] ABORT: cargo/rustc ativo", "2026-08-23"),
    ("[touring-daemon] heavy op: cli-ast-blast", None),
    ("=== daemon-ctl spawn epoch=1787514294 ===", None),
    ("", None),
    ("   Compiling pyo3-ffi v0.29.0", None),
])
def test_dia_do_texto_reads_only_a_leading_date(linha, esperado):
    assert J.dia_do_texto(linha) == esperado


# --- 3. dry-run: plan, and no side effects ---------------------------------------------------------

def escrever_trace(base: Path, linhas: list[bytes], nome="trace.jsonl") -> Path:
    p = base / nome
    p.write_bytes(b"".join(linhas))
    return p


def trace_misto():
    return [
        linha_trace(40, "a", "daemon", 5), linha_trace(40, "a", "standalone", 7), linha_trace(35, "b", "daemon", 3),
        b"{ not json at all\n",
        linha_trace(31, "a", "daemon", 1),      # day 31 before: expired when retention=30 (corte = day 30)
        linha_trace(30, "a", "daemon", 2),      # exactly on the cut: KEPT
        linha_trace(10, "a", "daemon", 4), linha_trace(0, "b", "daemon", 6),
        (json.dumps({"hook": "sem-ts"}) + "\n").encode(),
        b"[1, 2, 3]\n",
    ]


def test_plan_counts_expired_kept_and_undatable_exactly(mundo):
    _, base = mundo
    escrever_trace(base, trace_misto())
    (p,) = J.planejar(manifesto(journal()), base, AGORA)
    assert p["estado"] == "ok"
    assert p["corte"] == dia(30).isoformat()
    assert (p["linhas"], p["expiram"], p["mantem"], p["sem_data"]) == (10, 4, 3, 3)
    assert p["bytes_expiram"] == sum(len(x) for x in trace_misto()[:3] + trace_misto()[4:5])


def test_retention_boundary_a_line_exactly_at_the_cut_is_kept(mundo):
    _, base = mundo
    escrever_trace(base, [linha_trace(30), linha_trace(31)])
    (p,) = J.planejar(manifesto(journal()), base, AGORA)
    assert (p["expiram"], p["mantem"]) == (1, 1)


def test_dry_run_changes_nothing_and_creates_nothing(mundo):
    _, base = mundo
    f = escrever_trace(base, trace_misto())
    antes = (sha(f), sorted(os.listdir(base)))
    J.planejar(manifesto(journal()), base, AGORA)
    assert (sha(f), sorted(os.listdir(base))) == antes


def test_missing_journal_file_is_reported_as_ausente_not_an_error(mundo):
    _, base = mundo
    (p,) = J.planejar(manifesto(journal()), base, AGORA)
    assert p["estado"] == "ausente"
    assert J.aplicar(manifesto(journal()), base, AGORA)[0]["estado"] == "ausente"


def test_journal_with_nothing_datable_is_sem_data_and_never_pruned(mundo):
    _, base = mundo
    f = escrever_trace(base, [b"abc\n", b"def\n"])
    (p,) = J.planejar(manifesto(journal()), base, AGORA)
    assert p["estado"] == "sem_data" and p["expiram"] == 0
    antes = sha(f)
    assert J.aplicar(manifesto(journal()), base, AGORA)[0]["estado"] == "sem_data"
    assert sha(f) == antes


def test_a_reader_that_reads_the_whole_file_blocks_pruning(mundo):
    _, base = mundo
    j = journal(leitores=[{"arquivo": "r.py", "janela_dias": 2}, {"arquivo": "w.rs", "janela_dias": "toda", "verificada": True}])
    f = escrever_trace(base, trace_misto())
    (p,) = J.planejar(manifesto(j), base, AGORA)
    assert p["estado"] == "bloqueado" and p["bloqueio"] == ["w.rs"]
    antes = sha(f)
    r = J.aplicar(manifesto(j), base, AGORA)[0]
    assert r["estado"] == "bloqueado" and sha(f) == antes
    assert not (base / "rollup").exists()


def test_writer_without_reader_is_prunable_but_flagged(mundo):
    _, base = mundo
    escrever_trace(base, trace_misto())
    (p,) = J.planejar(manifesto(journal(leitores=[])), base, AGORA)
    assert p["estado"] == "ok" and p["sem_leitor"] is True


# --- 4. apply: exactly the expired lines, rollup first, archive, equivalence -------------------------

def ler_linhas(p: Path) -> list[bytes]:
    return p.read_bytes().splitlines(keepends=True)


def test_apply_removes_exactly_the_expired_lines_and_keeps_order_and_the_undatable(mundo):
    _, base = mundo
    todas = trace_misto()
    f = escrever_trace(base, todas)
    (r,) = J.aplicar(manifesto(journal()), base, AGORA)
    assert r["estado"] == "podado" and r["podadas"] == 4 and r["mantidas"] == 6
    esperado = [l for i, l in enumerate(todas) if i not in (0, 1, 2, 4)]
    assert ler_linhas(f) == esperado


def test_apply_archives_the_expired_lines_by_month_and_they_read_back_identical(mundo):
    _, base = mundo
    todas = [linha_trace(70), linha_trace(65), linha_trace(40), linha_trace(5)]
    escrever_trace(base, todas)
    J.aplicar(manifesto(journal()), base, AGORA)
    arquivos = sorted((base / "arquivo").glob("trace.jsonl.*.gz"))
    assert [a.name for a in arquivos] == sorted({f"trace.jsonl.{dia(o).strftime('%Y-%m')}.gz" for o in (70, 65, 40)})
    lido = b"".join(gzip.decompress(a.read_bytes()) for a in arquivos)
    assert sorted(lido.splitlines(keepends=True)) == sorted(todas[:3])


def test_expira_apagar_leaves_no_archive(mundo):
    _, base = mundo
    escrever_trace(base, [linha_trace(40), linha_trace(1)])
    J.aplicar(manifesto(journal(expira="apagar")), base, AGORA)
    assert not (base / "arquivo").exists()
    assert (base / "rollup" / "trace.jsonl").exists()


def test_rollup_has_exact_counts_and_sums_per_day_and_group(mundo):
    _, base = mundo
    escrever_trace(base, [
        linha_trace(40, "a", "daemon", 5), linha_trace(40, "a", "daemon", 7), linha_trace(40, "a", "standalone", 1),
        linha_trace(35, "b", "daemon", 3), linha_trace(2, "a", "daemon", 99),
    ])
    J.aplicar(manifesto(journal()), base, AGORA)
    recs = J.ler_rollup(J.caminho_rollup(base, "trace"))
    por = {(r["dia"], r["chave"]["hook"], r["chave"]["route"]): (r["n"], r["soma"]["elapsed_ms"]) for r in recs}
    assert por == {
        (dia(40).isoformat(), "a", "daemon"): (2, 12),
        (dia(40).isoformat(), "a", "standalone"): (1, 1),
        (dia(35).isoformat(), "b", "daemon"): (1, 3),
    }, "the kept day-2 line must NOT be in the rollup: it is still raw"


def test_kpi_from_rollup_equals_kpi_from_raw(mundo):
    """The N6 guard: before pruning, recompute the KPI both ways — they must agree to the bit."""
    _, base = mundo
    linhas = [linha_trace(o, h, r, e) for o, h, r, e in [
        (50, "cli-suggest", "daemon", 10), (50, "cli-suggest", "standalone", 30), (49, "ceg", "daemon", 20),
        (45, "cli-suggest", "daemon", 5), (45, "ceg", "standalone", 15), (41, "ceg", "daemon", 25),
        (3, "cli-suggest", "daemon", 7),
    ]]
    f = escrever_trace(base, linhas)
    ini, fim = dia(50).isoformat(), dia(41).isoformat()

    def do_bruto(arquivo: Path):
        n = daemon = soma = 0
        for raw in arquivo.read_bytes().splitlines():
            r = json.loads(raw)
            d = J.parse_dia(r["ts_ms"], "epoch_ms")
            if ini <= d <= fim:
                n += 1
                daemon += r["route"] == "daemon"
                soma += r["elapsed_ms"]
        return n, daemon, soma

    antes = do_bruto(f)
    J.aplicar(manifesto(journal()), base, AGORA)
    assert do_bruto(f) == (0, 0, 0), "the raw lines of that range are gone"
    tot = J.totais(base, "trace", ini, fim)
    tot_daemon = J.totais(base, "trace", ini, fim, onde={"route": "daemon"})
    assert (tot["n"], tot_daemon["n"], tot["soma"]["elapsed_ms"]) == antes == (6, 4, 105)


def test_totais_filters_by_day_range_inclusive_and_by_group_value(mundo):
    _, base = mundo
    escrever_trace(base, [linha_trace(60, "a", "x", 1), linha_trace(50, "a", "x", 2), linha_trace(40, "b", "y", 4), linha_trace(1)])
    J.aplicar(manifesto(journal()), base, AGORA)
    assert J.totais(base, "trace", dia(60).isoformat(), dia(50).isoformat())["n"] == 2
    assert J.totais(base, "trace", dia(60).isoformat(), dia(51).isoformat())["n"] == 1
    assert J.totais(base, "trace", dia(60).isoformat(), dia(40).isoformat(), onde={"hook": "b"})["soma"]["elapsed_ms"] == 4
    assert J.totais(base, "trace", "2000-01-01", "2000-01-02")["n"] == 0
    assert J.totais(base, "inexistente", "2000-01-01", "2030-01-01") == {"n": 0, "soma": {}}


def test_second_apply_is_a_no_op_and_never_double_counts(mundo):
    _, base = mundo
    f = escrever_trace(base, trace_misto())
    J.aplicar(manifesto(journal()), base, AGORA)
    estado = (sha(f), sha(J.caminho_rollup(base, "trace")))
    r = J.aplicar(manifesto(journal()), base, AGORA)[0]
    assert r["estado"] == "nada" and r["podadas"] == 0
    assert (sha(f), sha(J.caminho_rollup(base, "trace"))) == estado


def test_crash_after_rollup_commit_then_retry_does_not_double_count_or_double_archive(mundo):
    _, base = mundo
    f = escrever_trace(base, trace_misto())
    original = f.read_bytes()

    class Boom(RuntimeError):
        pass

    def estoura(*_a, **_k):
        raise Boom

    with pytest.raises(Boom):
        J.aplicar(manifesto(journal()), base, AGORA, antes_de_trocar_o_bruto=estoura)
    assert f.read_bytes() == original, "a crash before the final swap leaves the raw journal untouched"
    assert not list(base.glob("*.gc-tmp")), "and cleans its temp file"
    J.aplicar(manifesto(journal()), base, AGORA)
    recs = J.ler_rollup(J.caminho_rollup(base, "trace"))
    assert sum(r["n"] for r in recs) == 4, "the batch digest in the rollup meta makes the retry skip the add"
    lido = b"".join(gzip.decompress(a.read_bytes()) for a in (base / "arquivo").glob("*.gz"))
    assert len(lido.splitlines()) == 4, "and the archive is not appended twice"


def test_append_during_apply_survives(mundo):
    _, base = mundo
    f = escrever_trace(base, [linha_trace(40), linha_trace(2)])
    novo = linha_trace(0, "late", "daemon", 123)
    chamadas = []

    def acrescenta_uma_vez():
        chamadas.append(1)
        if len(chamadas) == 1:
            with open(f, "ab") as fh:
                fh.write(novo)

    J.aplicar(manifesto(journal()), base, AGORA, apos_ler_cauda=acrescenta_uma_vez)
    assert ler_linhas(f)[-1] == novo
    assert len(ler_linhas(f)) == 2
    assert len(chamadas) == 2, "the swap re-checks after the tail was re-read"


def test_continuous_appends_abort_instead_of_losing_lines(mundo):
    _, base = mundo
    f = escrever_trace(base, [linha_trace(40), linha_trace(2)])
    antes = f.read_bytes()

    def sempre_acrescenta():
        with open(f, "ab") as fh:
            fh.write(linha_trace(0))

    r = J.aplicar(manifesto(journal()), base, AGORA, apos_ler_cauda=sempre_acrescenta)[0]
    assert r["estado"] == "erro" and "escritor ativo" in r["motivo"]
    assert f.read_bytes().startswith(antes), "nothing was lost; the journal still has every original line"
    assert not list(base.glob("*.gc-tmp"))


def test_an_incomplete_last_line_is_preserved_untouched(mundo):
    _, base = mundo
    f = escrever_trace(base, [linha_trace(40), linha_trace(2)])
    with open(f, "ab") as fh:
        fh.write(b'{"ts_ms": 17')
    J.aplicar(manifesto(journal()), base, AGORA)
    assert f.read_bytes().endswith(b'{"ts_ms": 17')
    assert len(ler_linhas(f)) == 2


def test_file_mode_is_preserved(mundo):
    _, base = mundo
    f = escrever_trace(base, [linha_trace(40), linha_trace(2)])
    os.chmod(f, 0o640)
    J.aplicar(manifesto(journal()), base, AGORA)
    assert (f.stat().st_mode & 0o777) == 0o640


def test_apply_refuses_when_the_manifest_has_errors(mundo):
    repo, base = mundo
    f = escrever_trace(base, trace_misto())
    antes = sha(f)
    m = manifesto(journal(retencao_dias=1))
    with pytest.raises(J.ManifestoInvalido):
        J.aplicar(m, base, AGORA, repo=repo)
    assert sha(f) == antes


def test_journal_held_open_for_writing_by_another_process_is_skipped(mundo):
    _, base = mundo
    f = escrever_trace(base, trace_misto())
    antes = sha(f)
    holder = subprocess.Popen(
        [sys.executable, "-c", f"import sys,time; fh=open({str(f)!r},'ab'); print('aberto', flush=True); time.sleep(60)"],
        stdout=subprocess.PIPE, text=True,
    )
    try:
        assert holder.stdout.readline().strip() == "aberto"
        r = J.aplicar(manifesto(journal()), base, AGORA)[0]
        assert r["estado"] == "pulado" and "aberto para escrita" in r["motivo"] and str(holder.pid) in r["motivo"]
        assert sha(f) == antes
    finally:
        holder.kill()
        holder.wait()
    assert J.aplicar(manifesto(journal()), base, AGORA)[0]["estado"] == "podado"


def test_a_reader_holding_the_file_open_does_not_block(mundo):
    _, base = mundo
    f = escrever_trace(base, trace_misto())
    holder = subprocess.Popen(
        [sys.executable, "-c", f"import time; fh=open({str(f)!r},'rb'); print('aberto', flush=True); time.sleep(60)"],
        stdout=subprocess.PIPE, text=True,
    )
    try:
        assert holder.stdout.readline().strip() == "aberto"
        assert J.aplicar(manifesto(journal()), base, AGORA)[0]["estado"] == "podado"
    finally:
        holder.kill()
        holder.wait()


# --- text logs and whole-file journals -----------------------------------------------------------

def test_text_log_is_pruned_by_its_leading_date_and_undated_lines_stay(mundo):
    _, base = mundo
    f = base / "mem.log"
    f.write_text(f"{dia(60)} 00:00:01 psi=a\n{dia(40)} 00:00:02 psi=b\nsem data\n{dia(1)} 00:00:03 psi=c\n", encoding="utf-8")
    j = journal(id="mem", caminho="mem.log", tipo="texto", ts={"formato": "prefixo-data"}, rollup=None, leitores=[], expira="apagar")
    r = J.aplicar(manifesto(j), base, AGORA)[0]
    assert r["estado"] == "podado" and r["podadas"] == 2
    assert f.read_text(encoding="utf-8") == f"sem data\n{dia(1)} 00:00:03 psi=c\n"


def test_text_log_rollup_counts_lines_per_day_without_groups(mundo):
    _, base = mundo
    f = base / "mem.log"
    f.write_text(f"{dia(60)} a\n{dia(60)} b\n{dia(59)} c\n{dia(1)} d\n", encoding="utf-8")
    j = journal(id="mem", caminho="mem.log", tipo="texto", ts={"formato": "prefixo-data"}, rollup={"agrupar_por": [], "somar": []}, leitores=[])
    J.aplicar(manifesto(j), base, AGORA)
    recs = J.ler_rollup(J.caminho_rollup(base, "mem"))
    assert {r["dia"]: r["n"] for r in recs} == {dia(60).isoformat(): 2, dia(59).isoformat(): 1}


def test_undated_text_log_is_sem_data(mundo):
    _, base = mundo
    (base / "d.log").write_text("[touring-daemon] a\n[touring-daemon] b\n", encoding="utf-8")
    j = journal(id="d", caminho="d.log", tipo="texto", ts={"formato": "nenhum"}, rollup=None, leitores=[], expira="apagar")
    assert J.planejar(manifesto(j), base, AGORA)[0]["estado"] == "sem_data"


def test_file_set_journal_deletes_only_files_older_than_the_retention(mundo):
    _, base = mundo
    (base / "logs").mkdir()
    velho, novo = base / "logs" / "cycle-old.log", base / "logs" / "cycle-new.log"
    ignorado = base / "logs" / "cycle-old.sh"
    for p in (velho, novo, ignorado):
        p.write_text("x\n", encoding="utf-8")
    t_velho = (AGORA - dt.timedelta(days=31)).timestamp()
    t_novo = (AGORA - dt.timedelta(days=29)).timestamp()
    os.utime(velho, (t_velho, t_velho))
    os.utime(ignorado, (t_velho, t_velho))
    os.utime(novo, (t_novo, t_novo))
    j = journal(id="ciclos", caminho="logs/*.log", tipo="arquivos", ts=None, rollup=None, leitores=[], expira="apagar")
    (p,) = J.planejar(manifesto(j), base, AGORA)
    assert p["estado"] == "ok" and p["expiram"] == 1 and velho.exists(), "dry-run deletes nothing"
    (r,) = J.aplicar(manifesto(j), base, AGORA)
    assert r["estado"] == "podado" and r["podadas"] == 1
    assert not velho.exists() and novo.exists() and ignorado.exists()


def test_file_set_boundary_exactly_at_the_retention_is_kept(mundo):
    _, base = mundo
    (base / "logs").mkdir()
    p = base / "logs" / "a.log"
    p.write_text("x\n", encoding="utf-8")
    t = (AGORA - dt.timedelta(days=30)).timestamp()
    os.utime(p, (t, t))
    j = journal(id="ciclos", caminho="logs/*.log", tipo="arquivos", ts=None, rollup=None, leitores=[], expira="apagar")
    assert J.planejar(manifesto(j), base, AGORA)[0]["expiram"] == 0


def test_file_set_blocked_by_a_whole_file_reader(mundo):
    _, base = mundo
    (base / "logs").mkdir()
    p = base / "logs" / "a.log"
    p.write_text("x\n", encoding="utf-8")
    t = (AGORA - dt.timedelta(days=99)).timestamp()
    os.utime(p, (t, t))
    j = journal(id="ciclos", caminho="logs/*.log", tipo="arquivos", ts=None, rollup=None, expira="apagar",
                leitores=[{"arquivo": "r.py", "janela_dias": "toda"}])
    assert J.aplicar(manifesto(j), base, AGORA)[0]["estado"] == "bloqueado" and p.exists()


# --- CLI -----------------------------------------------------------------------------------------

def manifesto_toml(path: Path, base: Path) -> Path:
    path.write_text(f'''
versao = 1
base = "{base}"

[censo]
diretorios = ["."]
extensoes = [".jsonl", ".log"]

[[journal]]
id = "trace"
caminho = "trace.jsonl"
tipo = "jsonl"
escritores = ["w.rs"]
leitores = [ {{ arquivo = "r.py", janela_dias = 2, verificada = true }} ]
retencao_dias = 30
expira = "arquivar"
ts = {{ campo = "ts_ms", unidade = "epoch_ms" }}
rollup = {{ agrupar_por = ["hook", "route"], somar = ["elapsed_ms"] }}
''', encoding="utf-8")
    return path


def test_cli_default_is_dry_run_and_apply_is_explicit(mundo, tmp_path, capsys):
    repo, base = mundo
    f = escrever_trace(base, trace_misto())
    man = manifesto_toml(tmp_path / "j.toml", base)
    args = ["--manifesto", str(man), "--repo", str(repo), "--agora", AGORA.isoformat(), "--json"]
    antes = sha(f)
    assert J.main(args) == 0
    out = json.loads(capsys.readouterr().out)
    assert out["modo"] == "dry-run" and out["plano"][0]["expiram"] == 4 and sha(f) == antes
    assert J.main(args + ["--apply"]) == 0
    out = json.loads(capsys.readouterr().out)
    assert out["modo"] == "apply" and out["resultado"][0]["estado"] == "podado" and sha(f) != antes


def test_cli_exit_2_on_manifest_errors_and_apply_touches_nothing(mundo, tmp_path, capsys):
    repo, base = mundo
    f = escrever_trace(base, trace_misto())
    man = manifesto_toml(tmp_path / "j.toml", base)
    man.write_text(man.read_text().replace("retencao_dias = 30", "retencao_dias = 1"), encoding="utf-8")
    antes = sha(f)
    assert J.main(["--manifesto", str(man), "--repo", str(repo), "--agora", AGORA.isoformat(), "--apply", "--json"]) == 2
    assert sha(f) == antes
    assert "retencao_menor_que_janela" in capsys.readouterr().out


def test_cli_text_output_names_blockers_orphans_and_savings(mundo, tmp_path, capsys):
    repo, base = mundo
    escrever_trace(base, trace_misto())
    (base / "intruso.log").write_text("x\n", encoding="utf-8")
    man = manifesto_toml(tmp_path / "j.toml", base)
    assert J.main(["--manifesto", str(man), "--repo", str(repo), "--agora", AGORA.isoformat()]) == 0
    out = capsys.readouterr().out
    assert "DRY-RUN" in out and "trace" in out and "intruso.log" in out


def test_cli_totais(mundo, tmp_path, capsys):
    repo, base = mundo
    escrever_trace(base, [linha_trace(40, "a", "x", 5), linha_trace(1)])
    man = manifesto_toml(tmp_path / "j.toml", base)
    J.main(["--manifesto", str(man), "--repo", str(repo), "--agora", AGORA.isoformat(), "--apply"])
    capsys.readouterr()
    assert J.main(["--manifesto", str(man), "--totais", "trace", "--de", dia(41).isoformat(), "--ate", dia(39).isoformat()]) == 0
    assert json.loads(capsys.readouterr().out) == {"n": 1, "soma": {"elapsed_ms": 5}}


def test_cli_usage_errors_exit_64(capsys):
    with pytest.raises(SystemExit) as exc:
        J.main(["--bogus"])
    assert exc.value.code == 64
    with pytest.raises(SystemExit) as exc:
        J.main(["--totais", "x"])
    assert exc.value.code == 64


def test_real_dry_run_is_read_only_on_a_copy_of_nothing(tmp_path):
    """The shipped manifest against an EMPTY base: every journal absent, no crash, nothing created."""
    m = J.carregar(REPO / "docs" / "kpi" / "journals.toml")
    base = tmp_path / "vazio"
    base.mkdir()
    plano = J.planejar(m, base, AGORA)
    assert {p["estado"] for p in plano} == {"ausente"}
    assert os.listdir(base) == []


# --- second pass: holes the first mutation run found ---------------------------------------------

def test_naive_timestamps_bucket_by_their_literal_date_whatever_the_local_timezone(monkeypatch):
    for tz in ("America/Sao_Paulo", "Pacific/Kiritimati", "UTC"):
        monkeypatch.setenv("TZ", tz)
        time.tzset()
        assert J.parse_dia("2026-10-14 23:30:00", "iso") == "2026-10-14"
        assert J.parse_dia("2026-10-14T00:30:00", "iso") == "2026-10-14"
    monkeypatch.delenv("TZ")
    time.tzset()


def test_comma_fractions_and_bare_dates_in_text_logs():
    assert J.dia_do_texto("2026-10-14 23:59:59,123 x") == "2026-10-14"
    assert J.dia_do_texto("2026-13-45 00:00:00 x") is None, "a date-shaped prefix that is not a date is undatable"


def test_reader_without_a_window_blocks_pruning(mundo):
    _, base = mundo
    escrever_trace(base, trace_misto())
    j = journal(leitores=[{"arquivo": "r.py"}])
    assert J.planejar(manifesto(j), base, AGORA)[0]["estado"] == "bloqueado"


@pytest.mark.parametrize("janela", [True, False, -1, 1.5, "7", None, "todas"])
def test_reader_window_must_be_toda_or_a_non_negative_int(mundo, janela):
    repo, _ = mundo
    j = journal(leitores=[{"arquivo": "r.py", "janela_dias": janela, "verificada": True}])
    assert "valor_invalido" in codigos(manifesto(j), repo, nivel="erro")


def test_reader_window_zero_and_toda_are_valid(mundo):
    repo, _ = mundo
    for w in (0, 3, "toda"):
        j = journal(leitores=[{"arquivo": "r.py", "janela_dias": w, "verificada": True}])
        assert "valor_invalido" not in codigos(manifesto(j), repo, nivel="erro")


@pytest.mark.parametrize("ts", [None, {}, {"campo": "ts_ms"}, {"unidade": "epoch_ms"}, {"campo": "ts_ms", "unidade": "anos"}])
def test_jsonl_needs_a_timestamp_field_and_a_known_unit(mundo, ts):
    repo, _ = mundo
    assert "campo_ausente" in codigos(manifesto(journal(ts=ts)), repo, nivel="erro")


@pytest.mark.parametrize("ts", [None, {}, {"formato": "regex"}])
def test_text_needs_a_known_timestamp_format(mundo, ts):
    repo, _ = mundo
    j = journal(tipo="texto", ts=ts, rollup=None, leitores=[], expira="apagar")
    assert "campo_ausente" in codigos(manifesto(j), repo, nivel="erro")


@pytest.mark.parametrize("unidade", ["epoch_s", "epoch_ms", "epoch_ns", "iso"])
def test_every_documented_unit_is_accepted(mundo, unidade):
    repo, _ = mundo
    assert "campo_ausente" not in codigos(manifesto(journal(ts={"campo": "t", "unidade": unidade})), repo, nivel="erro")


@pytest.mark.parametrize("valor", [0, -1, True, "30", 1.5, None])
def test_retention_must_be_a_positive_int(mundo, valor):
    repo, _ = mundo
    assert "valor_invalido" in codigos(manifesto(journal(retencao_dias=valor)), repo, nivel="erro")


def test_retention_one_day_is_valid(mundo):
    repo, _ = mundo
    j = journal(retencao_dias=1, leitores=[{"arquivo": "r.py", "janela_dias": 0, "verificada": True}])
    assert "valor_invalido" not in codigos(manifesto(j), repo, nivel="erro")


def test_an_expired_line_that_is_the_last_line_of_the_file_is_pruned(mundo):
    _, base = mundo
    f = escrever_trace(base, [linha_trace(2), linha_trace(40)])
    J.aplicar(manifesto(journal()), base, AGORA)
    assert ler_linhas(f) == [linha_trace(2)]


def test_rollup_group_missing_field_becomes_empty_and_non_numeric_sums_are_ignored(mundo):
    _, base = mundo
    sem_route = (json.dumps({"ts_ms": ms(40), "hook": "a", "elapsed_ms": 4}) + "\n").encode()
    texto_na_soma = (json.dumps({"ts_ms": ms(40), "hook": "a", "route": "x", "elapsed_ms": "oito"}) + "\n").encode()
    bool_na_soma = (json.dumps({"ts_ms": ms(40), "hook": "a", "route": "x", "elapsed_ms": True}) + "\n").encode()
    escrever_trace(base, [sem_route, texto_na_soma, bool_na_soma, linha_trace(1)])
    J.aplicar(manifesto(journal()), base, AGORA)
    por = {(r["chave"]["hook"], r["chave"]["route"]): r for r in J.ler_rollup(J.caminho_rollup(base, "trace"))}
    assert por[("a", "")]["n"] == 1 and por[("a", "")]["soma"] == {"elapsed_ms": 4}
    assert por[("a", "x")]["n"] == 2 and por[("a", "x")]["soma"] == {}, "strings and booleans are not numbers"


def test_a_late_line_for_an_already_rolled_day_adds_to_the_existing_cell(mundo):
    _, base = mundo
    f = escrever_trace(base, [linha_trace(40, "a", "x", 5), linha_trace(1)])
    J.aplicar(manifesto(journal()), base, AGORA)
    with open(f, "ab") as fh:
        fh.write(linha_trace(40, "a", "x", 7))
    J.aplicar(manifesto(journal()), base, AGORA)
    (r,) = J.ler_rollup(J.caminho_rollup(base, "trace"))
    assert (r["n"], r["soma"]["elapsed_ms"]) == (2, 12)
    assert len(J._ler_rollup_completo(J.caminho_rollup(base, "trace"))[0]["batches"]) == 2


def test_the_pruning_process_itself_holding_the_file_open_does_not_skip(mundo):
    _, base = mundo
    f = escrever_trace(base, trace_misto())
    with open(f, "ab"):
        assert J.aplicar(manifesto(journal()), base, AGORA)[0]["estado"] == "podado"


def test_file_set_skips_files_held_open_for_writing(mundo):
    _, base = mundo
    (base / "logs").mkdir()
    velho = base / "logs" / "cycle-old.log"
    velho.write_text("x\n", encoding="utf-8")
    t = (AGORA - dt.timedelta(days=99)).timestamp()
    os.utime(velho, (t, t))
    j = journal(id="ciclos", caminho="logs/*.log", tipo="arquivos", ts=None, rollup=None, leitores=[], expira="apagar")
    holder = subprocess.Popen(
        [sys.executable, "-c", f"import time; fh=open({str(velho)!r},'ab'); print('aberto', flush=True); time.sleep(60)"],
        stdout=subprocess.PIPE, text=True,
    )
    try:
        assert holder.stdout.readline().strip() == "aberto"
        r = J.aplicar(manifesto(j), base, AGORA)[0]
        assert velho.exists() and "cycle-old.log" in r["motivo"]
    finally:
        holder.kill()
        holder.wait()


def test_rollup_ledger_exists_for_arquivar_even_without_rollup_config_and_not_for_apagar(mundo):
    _, base = mundo
    escrever_trace(base, [linha_trace(40), linha_trace(1)])
    J.aplicar(manifesto(journal(rollup=None, expira="arquivar")), base, AGORA)
    assert J.caminho_rollup(base, "trace").exists() and list((base / "arquivo").glob("*.gz"))
    base2 = base / "outra"
    base2.mkdir()
    escrever_trace(base2, [linha_trace(40), linha_trace(1)])
    J.aplicar(manifesto(journal(rollup=None, expira="apagar", leitores=[])), base2, AGORA)
    assert not J.caminho_rollup(base2, "trace").exists()


def test_cli_invalid_agora_exits_64(mundo, tmp_path):
    repo, base = mundo
    man = manifesto_toml(tmp_path / "j.toml", base)
    with pytest.raises(SystemExit) as exc:
        J.main(["--manifesto", str(man), "--repo", str(repo), "--agora", "ontem"])
    assert exc.value.code == 64


def test_cli_verificar_reports_problems_and_never_plans(mundo, tmp_path, capsys):
    repo, base = mundo
    escrever_trace(base, trace_misto())
    man = manifesto_toml(tmp_path / "j.toml", base)
    assert J.main(["--manifesto", str(man), "--repo", str(repo), "--verificar", "--json"]) == 0
    out = json.loads(capsys.readouterr().out)
    assert out["modo"] == "verificar" and out["plano"] == [] and out["resultado"] == []


# --- third pass: survivors of the first mutation run ----------------------------------------------

@pytest.mark.parametrize("valor", [123, 1.5, [], {}, b"2026-10-14"])
def test_parse_dia_iso_rejects_non_strings_without_raising(valor):
    assert J.parse_dia(valor, "iso") is None


def test_census_scans_the_listed_directories_and_only_those(mundo):
    repo, base = mundo
    (base / "logs").mkdir()
    (base / "logs" / "solto.jsonl").write_text("", encoding="utf-8")
    (base / "trace.jsonl").write_text("", encoding="utf-8")
    com = manifesto(journal(), censo={"diretorios": [".", "logs"], "extensoes": [".jsonl"]})
    sem = manifesto(journal(), censo={"diretorios": ["."], "extensoes": [".jsonl"]})
    assert any("logs/solto.jsonl" in p["msg"] for p in problemas(com, repo, base) if p["codigo"] == "sem_manifesto")
    assert not [p for p in problemas(sem, repo, base) if p["codigo"] == "sem_manifesto"]


def test_text_log_declared_formato_nenhum_is_never_dated_even_if_lines_look_dated(mundo):
    _, base = mundo
    f = base / "d.log"
    f.write_text(f"{dia(90)} 00:00:01 antigo\n{dia(80)} 00:00:02 antigo\n", encoding="utf-8")
    j = journal(id="d", caminho="d.log", tipo="texto", ts={"formato": "nenhum"}, rollup=None, leitores=[], expira="apagar")
    assert J.planejar(manifesto(j), base, AGORA)[0]["estado"] == "sem_data"
    antes = sha(f)
    assert J.aplicar(manifesto(j), base, AGORA)[0]["estado"] == "sem_data" and sha(f) == antes


def test_a_complete_json_last_line_without_a_newline_is_treated_as_in_flight_and_kept(mundo):
    _, base = mundo
    f = escrever_trace(base, [linha_trace(40), linha_trace(2)])
    with open(f, "ab") as fh:
        fh.write(linha_trace(40, "ultima").rstrip(b"\n"))
    (p,) = J.planejar(manifesto(journal()), base, AGORA)
    assert (p["linhas"], p["expiram"]) == (2, 1), "the unterminated line is not counted: it may still be being written"
    J.aplicar(manifesto(journal()), base, AGORA)
    assert f.read_bytes() == linha_trace(2) + linha_trace(40, "ultima").rstrip(b"\n")


def test_an_empty_journal_file_is_ok_not_sem_data(mundo):
    _, base = mundo
    (base / "trace.jsonl").write_bytes(b"")
    (p,) = J.planejar(manifesto(journal()), base, AGORA)
    assert p["estado"] == "ok" and p["linhas"] == 0
    assert J.aplicar(manifesto(journal()), base, AGORA)[0]["estado"] == "nada"


def test_rollup_file_is_written_sorted_by_day_then_group(mundo):
    _, base = mundo
    escrever_trace(base, [linha_trace(35, "z", "b"), linha_trace(40, "z", "a"), linha_trace(40, "a", "b"), linha_trace(40, "a", "a"), linha_trace(1)])
    J.aplicar(manifesto(journal()), base, AGORA)
    chaves = [(r["dia"], r["chave"]["hook"], r["chave"]["route"]) for r in J.ler_rollup(J.caminho_rollup(base, "trace"))]
    assert chaves == sorted(chaves) and len(chaves) == 4
    linhas = J.caminho_rollup(base, "trace").read_text(encoding="utf-8").splitlines()
    assert json.loads(linhas[0]).keys() == {"_meta"} and linhas[1:] == sorted(linhas[1:], key=lambda s: (json.loads(s)["dia"], json.dumps(json.loads(s)["chave"], sort_keys=True)))


def test_only_the_most_recent_batch_digests_are_remembered(mundo, monkeypatch):
    _, base = mundo
    monkeypatch.setattr(J, "LOTES_LEMBRADOS", 3)
    f = escrever_trace(base, [linha_trace(1)])
    digests = []
    for i in range(5):
        with open(f, "ab") as fh:
            fh.write(linha_trace(40 + i, "h", "r", i))
        J.aplicar(manifesto(journal()), base, AGORA)
        digests.append(J._ler_rollup_completo(J.caminho_rollup(base, "trace"))[0]["batches"][-1])
    batches = J._ler_rollup_completo(J.caminho_rollup(base, "trace"))[0]["batches"]
    assert batches == digests[-3:], "the ledger keeps a bounded memory, newest last"


def test_archive_leaves_no_temp_files_behind(mundo):
    _, base = mundo
    escrever_trace(base, [linha_trace(70), linha_trace(40), linha_trace(1)])
    J.aplicar(manifesto(journal()), base, AGORA)
    assert [p.name for p in (base / "arquivo").iterdir() if p.name.endswith(".gc-tmp")] == []
    assert len(list((base / "arquivo").iterdir())) == 2


def test_without_rollup_config_the_ledger_holds_digests_only_no_cells(mundo):
    _, base = mundo
    escrever_trace(base, [linha_trace(40), linha_trace(1)])
    J.aplicar(manifesto(journal(rollup=None, expira="arquivar")), base, AGORA)
    meta, recs = J._ler_rollup_completo(J.caminho_rollup(base, "trace"))
    assert recs == [] and len(meta["batches"]) == 1


def test_tail_that_shrinks_between_reads_does_not_leave_stale_bytes(mundo):
    _, base = mundo
    f = escrever_trace(base, [linha_trace(40), linha_trace(2)])
    with open(f, "ab") as fh:
        fh.write(b'{"ts_ms": 1234567')
    chamadas = []

    def encolhe_uma_vez():
        chamadas.append(1)
        if len(chamadas) == 1:
            os.truncate(f, f.stat().st_size - 5)

    J.aplicar(manifesto(journal()), base, AGORA, apos_ler_cauda=encolhe_uma_vez)
    assert f.read_bytes() == linha_trace(2) + b'{"ts_ms": 12'


def test_file_set_apply_keeps_a_file_exactly_at_the_retention(mundo):
    _, base = mundo
    (base / "logs").mkdir()
    p = base / "logs" / "a.log"
    p.write_text("x\n", encoding="utf-8")
    t = (AGORA - dt.timedelta(days=30)).timestamp()
    os.utime(p, (t, t))
    j = journal(id="ciclos", caminho="logs/*.log", tipo="arquivos", ts=None, rollup=None, leitores=[], expira="apagar")
    assert J.aplicar(manifesto(j), base, AGORA)[0]["estado"] == "nada" and p.exists()
    velho = base / "logs" / "b.log"
    velho.write_text("x\n", encoding="utf-8")
    t_velho = (AGORA - dt.timedelta(days=31)).timestamp()
    os.utime(velho, (t_velho, t_velho))
    r = J.aplicar(manifesto(j), base, AGORA)[0]
    assert r["podadas"] == 1 and not velho.exists() and p.exists(), "only the strictly older file goes; the boundary file stays"


def test_cli_exit_1_when_a_journal_fails_during_apply(mundo, tmp_path, monkeypatch, capsys):
    repo, base = mundo
    man = manifesto_toml(tmp_path / "j.toml", base)
    monkeypatch.setattr(J, "aplicar", lambda *a, **k: [{"id": "trace", "estado": "erro", "podadas": 0, "mantidas": 0, "motivo": "escritor ativo"}])
    assert J.main(["--manifesto", str(man), "--repo", str(repo), "--agora", AGORA.isoformat(), "--apply"]) == 1
    assert "escritor ativo" in capsys.readouterr().out


def test_cli_totais_honours_onde(mundo, tmp_path, capsys):
    repo, base = mundo
    escrever_trace(base, [linha_trace(40, "a", "x", 5), linha_trace(40, "b", "x", 9), linha_trace(1)])
    man = manifesto_toml(tmp_path / "j.toml", base)
    J.main(["--manifesto", str(man), "--repo", str(repo), "--agora", AGORA.isoformat(), "--apply"])
    capsys.readouterr()
    assert J.main(["--manifesto", str(man), "--totais", "trace", "--de", dia(41).isoformat(), "--ate", dia(39).isoformat(), "--onde", "hook=b"]) == 0
    assert json.loads(capsys.readouterr().out) == {"n": 1, "soma": {"elapsed_ms": 9}}


def test_cli_naive_agora_is_read_as_utc_not_local(mundo, tmp_path, monkeypatch, capsys):
    repo, base = mundo
    monkeypatch.setenv("TZ", "America/Sao_Paulo")
    time.tzset()
    try:
        f = escrever_trace(base, [linha_trace(30, hora=23)])
        man = manifesto_toml(tmp_path / "j.toml", base)
        # naive 23:30 on day 0: local (BRT) it is already day+1 in UTC, which would move the cut one day later
        naive = f"{dia(0).isoformat()}T23:30:00"
        J.main(["--manifesto", str(man), "--repo", str(repo), "--agora", naive, "--json"])
        assert json.loads(capsys.readouterr().out)["plano"][0]["corte"] == dia(30).isoformat()
        assert f.exists()
    finally:
        monkeypatch.delenv("TZ")
        time.tzset()
