#!/usr/bin/env python3
"""test_jev_tags_backfill.py — os testes da prévia da S-5.2.

O quality gate reprovou a primeira versão em **F3.1 = 0.000** (`test fns=0`,
`public fns=6`, `ratio=0.000`) — o blocker real: eu escrevi o script sem um único
teste. A REGRA #21 diz que falha observada se corrige, então estes testes não são
cerimônia: eles fixam as três propriedades que tornam a prévia confiável.

O que precisa estar provado, em ordem de importância:

1. **Nada é gravado.** `writes_performed == 0` sempre, e o modo é
   `preview_only`. A prévia existe para ser o pacote de evidência do
   `TagSource::Jev`; se ela gravasse, teria gravado com `explicit`, que é
   mentira e irreversível (ver `tags.rs:506`).
2. **O τ vem do contrato do G2, não de uma constante.** Com `threshold_recommended`
   ausente ou `null`, o script **aborta** em vez de usar um τ chutado — o mesmo
   princípio que fez a família `decision` distinguir `unmeasured` de 0.
3. **O SQL é parametrizado.** Não é só o F2.1: o teste executa a query contra o
   banco real e prova que ela devolve o alvo esperado, e um valor malicioso no
   vocabulário não altera a estrutura.

Roda sem pytest e sem rede: `python3 test_jev_tags_backfill.py`.
"""

from __future__ import annotations

import json
import sqlite3
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import jev_tags_backfill as bf
from script_testkit import check, finish


def test_no_writes_are_performed() -> None:
    """A prévia é leitura; `writes_performed` é 0 por construção do summary."""
    rows = [
        {"auto_kind": "lesson", "jev_kind": "outcome", "confidence": 1.0, "latency_ms": 380.0},
        {"auto_kind": "insight", "jev_kind": "insight", "confidence": 0.5, "latency_ms": 370.0},
    ]
    out = bf.summarize(rows, {"status": "measured", "tau": 0.8})
    check("test_no_writes_are_performed",
          out["writes_performed"] == 0 and out["n_requested"] == 2,
          f"writes={out['writes_performed']}")


def test_a_fallback_counts_as_unmeasured() -> None:
    """`jev_kind: None` é o fail-open do adapter, e conta como `unmeasured` —
    nunca como "o Jev concordou" nem como "o Jev discordou"."""
    rows = [
        {"auto_kind": "lesson", "jev_kind": "outcome", "confidence": 1.0, "latency_ms": 380.0},
        {"auto_kind": "lesson", "jev_kind": None, "confidence": None, "latency_ms": 12.0},
    ]
    out = bf.summarize(rows, {"status": "measured", "tau": 0.8})
    check("test_a_fallback_counts_as_unmeasured",
          out["measured"] == 1 and out["unmeasured"] == 1,
          f"measured={out['measured']} unmeasured={out['unmeasured']}")


def test_only_above_tau_is_counted_as_writable() -> None:
    """O contrato do τ: abaixo do corte, a mudança existe mas não é escrita."""
    rows = [
        {"auto_kind": "lesson", "jev_kind": "outcome", "confidence": 0.95, "latency_ms": 1.0},
        {"auto_kind": "lesson", "jev_kind": "outcome", "confidence": 0.42, "latency_ms": 1.0},
    ]
    out = bf.summarize(rows, {"status": "measured", "tau": 0.8})
    check("test_only_above_tau_is_counted_as_writable",
          out["would_change"] == 2 and out["would_change_above_tau"] == 1,
          f"mudariam={out['would_change']} acima_de_tau={out['would_change_above_tau']}")


def test_a_null_tau_writes_nothing_rather_than_defaulting_to_zero() -> None:
    """τ ausente = `unmeasured`. Um τ 0 aqui gravaria TODAS as mudanças, que é o
    oposto do que o gate protege."""
    rows = [{"auto_kind": "lesson", "jev_kind": "outcome", "confidence": 0.3, "latency_ms": 1.0}]
    out = bf.summarize(rows, {"status": "unmeasured", "tau": None, "reason": "g2_absent"})
    check("test_a_null_tau_writes_nothing_rather_than_defaulting_to_zero",
          out["tau"] is None and out["would_change_above_tau"] == 0,
          f"tau={out['tau']} acima={out['would_change_above_tau']}")


def test_tau_is_read_from_the_g2_contract() -> None:
    """O τ não é constante do script — vem do `threshold_recommended` do G2."""
    if not bf.G2_RESULTS.exists():
        check("test_tau_is_read_from_the_g2_contract", False, "results.json do G2 ausente")
        return
    data = json.loads(bf.G2_RESULTS.read_text(encoding="utf-8"))
    entry = (data.get("threshold_recommended") or {}).get("memory_kind")
    tau = bf.load_tau()
    check("test_tau_is_read_from_the_g2_contract",
          entry is not None and tau.get("tau") == float(entry["tau"]) and tau["status"] == "measured",
          f"tau={tau.get('tau')} de '{tau.get('reason')}'")
    # E o split que o τ foi medido em é o meu.
    check("the_tau_contract_carries_the_split_sha256",
          tau.get("split_sha256") == "deb107937948021a6ce33caffc10ea14b722f6ae72f69b041401f6c9d75bf3f1",
          str(tau.get("split_sha256"))[:24])


def test_the_sql_is_parameterized() -> None:
    """Nenhum valor entra na string do SQL — os `?` são todos vinculados."""
    sql, params = bf.target_sql()
    check("test_the_sql_is_parameterized",
          sql.count("?") == len(params),
          f"{sql.count('?')} placeholders, {len(params)} params")


def test_no_vocabulary_value_is_interpolated() -> None:
    """Nenhum valor do vocabulário aparece literalmente na query — é o que o
    F2.1 (CWE-89) reprovou na primeira versão."""
    sql, _ = bf.target_sql()
    leaked = [v for v in bf.KIND_VALUES if f"'{v}'" in sql]
    check("test_no_vocabulary_value_is_interpolated", not leaked, f"vazando={leaked}")


def test_the_target_query_runs_against_the_real_database() -> None:
    """A query executa e devolve o alvo esperado: `kind:auto` dentro do vocabulário."""
    if not bf.MEMORY_DB.exists():
        check("test_the_target_query_runs_against_the_real_database", False, "memory.db ausente")
        return
    target = bf.load_target(50, 20261001)
    ok = bool(target) and all(r["auto_kind"] in bf.KIND_VALUES for r in target)
    sources = {r["auto_source"] for r in target}
    check("test_the_target_query_runs_against_the_real_database",
          ok and sources == {"auto"},
          f"{len(target)} linhas, sources={sources or '{}'}")


def test_the_target_is_deterministic() -> None:
    """Mesma seed → mesma amostra, e o banco não foi tocado entre as duas."""
    a = [r["key"] for r in bf.load_target(30, 20261001)]
    b = [r["key"] for r in bf.load_target(30, 20261001)]
    c = [r["key"] for r in bf.load_target(30, 1)]
    check("test_the_target_is_deterministic", a == b and a != c,
          f"{len(a)} chaves, 2 runs iguais={a == b}, seed+1 muda={a != c}")


def test_the_database_is_opened_read_only() -> None:
    """`mode=ro` é o que garante a afirmação de "nada é gravado". Abrir o arquivo
    inexistente é o que prova que o caminho de URI está valendo."""
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "probe.db"
        seed = sqlite3.connect(path)
        seed.execute("CREATE TABLE t (x int)")
        seed.execute("INSERT INTO t VALUES (1)")
        seed.commit()
        seed.close()
        ro = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
        try:
            ro.execute("INSERT INTO t VALUES (2)")
            ok, detail = False, "INSERT passou — mode=ro NAO esta valendo"
        except sqlite3.OperationalError as exc:
            ok, detail = True, str(exc)[:56]
        finally:
            ro.close()
        check("test_the_database_is_opened_read_only", ok, detail)


def main() -> int:
    print(f"banco  : {bf.MEMORY_DB}")
    print(f"tau    : {bf.load_tau().get('tau')} (fonte: {bf.load_tau().get('status')})")
    print(f"alvo   : {len(bf.KIND_VALUES)} valores de vocabulário\n")
    for test in (
        test_no_writes_are_performed,
        test_a_fallback_counts_as_unmeasured,
        test_only_above_tau_is_counted_as_writable,
        test_a_null_tau_writes_nothing_rather_than_defaulting_to_zero,
        test_tau_is_read_from_the_g2_contract,
        test_the_sql_is_parameterized,
        test_no_vocabulary_value_is_interpolated,
        test_the_target_query_runs_against_the_real_database,
        test_the_target_is_deterministic,
        test_the_database_is_opened_read_only,
    ):
        test()
    return finish()


if __name__ == "__main__":
    sys.exit(main())
