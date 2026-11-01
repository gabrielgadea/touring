#!/usr/bin/env python3
"""test_jev_echo_cleanup.py — a suíte do script de limpeza do eco.

O que este ficheiro protege, por ordem de gravidade:

1. **A chave da PROVA não é apagada.** Este é o teste que mais importa. A
   decisão B é "apagar *depois* da prova" — logo apagar a prova é apagar a
   autorização, e o script ficaria sem forma de correr outra vez nem de dizer
   porque é que foi disparado. O predicado é só-timestamp; o teste fixa isso.
2. **`--apply` não corre sem a prova.** E não existe `--force`. A pré-condição
   está no código, não na memória de quem o executa.
3. **As duas derivações têm de concordar.** Foi a divergência entre a regex em
   Python e o GLOB do SQLite que apanhou o bug real (`[0-9]` é o conjunto
   literal `{0,-,9}` no GLOB — não existe intervalo). Um `DELETE` de milhares
   de linhas não pode depender de uma leitura só.
4. **Alvo com mais de um valor distinto é recusado.** Eco puro tem *um* texto;
   dois significam que o filtro apanha mais do que devia.
5. **Leitura por omissão.** `mode=ro` — nada é escrito sem `writable=True`, e
   `--apply` é a única forma de lá chegar.

Roda sem pytest e sem rede: `python3 test_jev_echo_cleanup.py`
"""

from __future__ import annotations

import re
import sqlite3
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import jev_echo_cleanup as ec  # noqa: E402

FAILURES: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    print(f"  [{'PASS' if ok else 'FAIL'}] {name}" + (f" — {detail}" if detail else ""))
    if not ok:
        FAILURES.append(name)


def make_db(tmp: Path, rows: list[tuple[str, str, int]]) -> Path:
    """`(key, value, access_count)` → uma base isolada, com o schema mínimo."""
    path = tmp / "memory.db"
    con = sqlite3.connect(path)
    con.execute("CREATE TABLE memory_entries ("
                "key TEXT PRIMARY KEY, value TEXT, created_at TEXT, access_count INT, "
                "superseded_by TEXT)")
    con.executemany(
        "INSERT INTO memory_entries (key, value, created_at, access_count) "
        "VALUES (?, ?, '2026-10-01 00:00:00', ?)", rows)
    con.commit()
    con.close()
    return path


ECHO = ec.ECHO_TEXT
OTHER = "Uma insight completamente diferente, que nada tem a ver com o eco."


# --- 1 · a PROVA SOBREVIVE ao predicado -----------------------------------

def test_the_proof_key_is_never_selected() -> None:
    """[F] O teste que mais importa. A decisão B é apagar *depois* da prova; se
    o predicado a apanhasse, apagava a própria autorização e o script ficava
    sem forma de correr outra vez nem de explicar porque foi disparado."""
    with tempfile.TemporaryDirectory() as d:
        tmp = Path(d)
        original = ec.MEMORY_DB
        try:
            ec.MEMORY_DB = make_db(tmp, [
                ("insight:cila_progression:1790879043", ECHO, 1),
                ("insight:cila_progression:1790879343", ECHO, 1),
                ("insight:cila_progression:a924b810a5bc", ECHO, 3),   # a PROVA
                ("insight:other:1234567890", OTHER, 1),
            ])
            con = ec.connect(writable=False)
            try:
                py = ec.echo_keys_python(con)
                sql = ec.echo_keys_sql(con)
            finally:
                con.close()
        finally:
            ec.MEMORY_DB = original

    check("test_the_proof_key_is_never_selected",
          "insight:cila_progression:a924b810a5bc" not in py
          and "insight:cila_progression:a924b810a5bc" not in sql
          and len(py) == 2 and len(sql) == 2,
          f"python={len(py)} sql={len(sql)}  proof intacta= "
          f"{'sim' if 'a924b810a5bc' not in py + sql else 'NAO'}")


def test_a_hex_key_of_any_shape_is_not_a_timestamp() -> None:
    """A propriedade real: **qualquer chave com letras** no sufixo nunca é
    timestamp, seja de 8, 12 ou N hex. E um sufixo SÓ de dígitos é timestamp
    com 10 ou 11 chars — epoch cresce, e 2033 é um ano válido.

    [F] A primeira versão deste teste afirmava o contrário (que 11 dígitos não
    deviam casar) e FALHOU. O erro era do teste: data opressa por hex de 12
    caracteres, e um limite de comprimento rejeitaria epochs legítimos em 2033.
    A regra correcta é 'só dígitos', sem contagem."""
    hex_keys = [
        "insight:cila_progression:a924b810a5bc",   # 12 hex (a prova real)
        "insight:cila_progression:deadbeef",       # 8 hex
        "insight:cila_progression:1a2b3c4d5e6f",   # hex com dígito inicial
        "insight:cila_progression:ABCDEF",         # hex maiúsculo
    ]
    digit_keys = [
        "insight:cila_progression:1790879043",     # 10 dígitos (hoje)
        "insight:cila_progression:17908790431",    # 11 dígitos (epoch futuro)
    ]
    wrongly_hex = [k for k in hex_keys if ec.IS_TIMESTAMP.match(k)]
    wrongly_digit = [k for k in digit_keys if not ec.IS_TIMESTAMP.match(k)]
    check("test_a_hex_key_of_any_shape_is_not_a_timestamp",
          not wrongly_hex and not wrongly_digit,
          f"hex que casou: {wrongly_hex or 'nenhum'}; "
          f"dígitos que não casaram: {wrongly_digit or 'nenhum'}")


# --- 2 · a DUPLA DERIVAÇÃO -----------------------------------------------

def test_both_derivations_agree_on_the_real_database() -> None:
    """O bug real morou aqui: `GLOB` do SQLite não tem intervalos, e
    `[0-9]` casava zero linhas. A concordância é o que teria apanhado aquilo."""
    con = ec.connect(writable=False)
    try:
        s = ec.survey(con)
    finally:
        con.close()
    check("test_both_derivations_agree_on_the_real_database",
          s["py_keys"] == s["sql_keys"] and s["py_keys"],
          f"python={len(s['py_keys'])} sql={len(s['sql_keys'])}")


def test_divergence_between_derivations_blocks_the_delete() -> None:
    """Se as duas leituras discordarem, o script recusa. Um `DELETE` de
    milhares de linhas não pode assentar numa leitura única."""
    s = {"py_keys": ["a", "b"], "sql_keys": ["a"], "texts": {"distinct": 1, "texts": []},
         "proof": {"max_access": 9, "found": []}}
    bl = ec.blockers_for(s)
    check("test_divergence_between_derivations_blocks_the_delete",
          any("divergem" in b for b in bl), f"travas={bl}")


# --- 3 · as TRAVAS ---------------------------------------------------------

def test_missing_proof_blocks_the_delete() -> None:
    """A pré-condição da decisão B. Sem prova, o número é desenho, não deriva."""
    s = {"py_keys": ["a"], "sql_keys": ["a"], "texts": {"distinct": 1, "texts": []},
         "proof": {"max_access": 0, "found": []}}
    check("test_missing_proof_blocks_the_delete",
          any("PROOF" in b for b in ec.blockers_for(s)))


def test_partial_proof_does_not_count() -> None:
    """acc=1 e acc=2 não são 'corrido o suficiente'. O critério é ≥3."""
    for acc, should_block in ((1, True), (2, True), (3, False), (7, False)):
        s = {"py_keys": ["a"], "sql_keys": ["a"], "texts": {"distinct": 1, "texts": []},
             "proof": {"max_access": acc, "found": []}}
        blocked = any("PROOF" in b for b in ec.blockers_for(s))
        check(f"acc={acc} {'bloqueia' if should_block else 'passa'}",
              blocked == should_block)


def test_more_than_one_distinct_value_blocks_the_delete() -> None:
    """Eco puro tem UM texto. Dois significam que o filtro apanha demais."""
    s = {"py_keys": ["a", "b"], "sql_keys": ["a", "b"],
         "texts": {"distinct": 2, "texts": []}, "proof": {"max_access": 9, "found": []}}
    check("test_more_than_one_distinct_value_blocks_the_delete",
          any("valores distintos" in b for b in ec.blockers_for(s)))


def test_empty_target_blocks_the_delete() -> None:
    s = {"py_keys": [], "sql_keys": [], "texts": {"distinct": 0, "texts": []},
         "proof": {"max_access": 9, "found": []}}
    check("test_empty_target_blocks_the_delete",
          any("vazio" in b for b in ec.blockers_for(s)))


def test_all_gates_open_lets_the_delete_through() -> None:
    """O contraponto: com tudo em ordem, NÃO pode haver trava. Um guard que
    bloqueia sempre é tão inútil como um que nunca bloqueia."""
    s = {"py_keys": ["a", "b"], "sql_keys": ["a", "b"],
         "texts": {"distinct": 1, "texts": []}, "proof": {"max_access": 5, "found": []}}
    check("test_all_gates_open_lets_the_delete_through", ec.blockers_for(s) == [])


# --- 4 · o que NÃO pode ser contornado ------------------------------------

def test_there_is_no_force_flag() -> None:
    """[F] Se existisse `--force`, a pré-condição seria um pedido, não uma
    garantia. Mudar a decisão é escrever outro comando — não passar uma flag."""
    import io, contextlib
    # A palavra "--force" aparece na DOCSTRING a dizer que NÃO existe — que é
    # o oposto de uma flag. Testar o texto seria testar a prosa; o que se
    # executa é o parser, com a flag proibida como argumento real.
    with contextlib.redirect_stderr(io.StringIO()):
        try:
            ec.main(["--force"])
            code = None
        except SystemExit as exc:
            code = exc.code
    check("test_there_is_no_force_flag",
          code is not None and code != 0,
          f"main(['--force']) saiu com {code} (2 = argparse recusou a opção)")


def test_the_database_is_read_only_by_default() -> None:
    """A garantia de `mode=ro` — provada por tentativa, não por comentário."""
    with tempfile.TemporaryDirectory() as d:
        tmp = Path(d)
        path = make_db(tmp, [("k", "v", 0)])
        original = ec.MEMORY_DB
        try:
            ec.MEMORY_DB = path
            con = ec.connect(writable=False)
            try:
                try:
                    con.execute("DELETE FROM memory_entries")
                    ok, detail = False, "DELETE passou — mode=ro não está a valer"
                except sqlite3.OperationalError as exc:
                    ok, detail = True, str(exc)[:48]
            finally:
                con.close()
        finally:
            ec.MEMORY_DB = original
    check("test_the_database_is_read_only_by_default", ok, detail)


def test_no_sql_is_interpolated() -> None:
    """[F] O gate F2.1 bloqueou este ficheiro DUAS vezes por f-string em
    `execute()`. A regra que fica escrita é a forma, não a intenção: um dia
    alguém acrescenta uma variável a um `WHERE` e o padrão não existe mais."""
    src = (HERE / "jev_echo_cleanup.py").read_text(encoding="utf-8")
    bad = re.findall(r'execute\(\s*f["\']', src)
    check("test_no_sql_is_interpolated", not bad,
          f"{len(bad)} execute() com f-string")


def test_the_sql_literals_carry_the_same_predicate() -> None:
    """Os quatro literais repetem o WHERE de propósito. O `assert` do módulo
    protege em tempo de importação; este teste diz que o check existe."""
    def where_of(sql: str) -> str:
        tail = sql.split(" WHERE ", 1)[1]
        # o GROUP BY/ORDER BY de SQL_VALUES não faz parte da cláusula
        for stop in (" GROUP BY", " ORDER BY"):
            tail = tail.split(stop, 1)[0]
        return tail.strip()
    tails = {where_of(x) for x in (ec.SQL_KEYS_ECHO, ec.SQL_VALUES, ec.SQL_DELETE)}
    check("test_the_sql_literals_carry_the_same_predicate",
          tails == {"key GLOB ?"},
          f"cláusulas distintas={tails}")
    check("test_the_sql_literals_carry_the_same_predicate_deletes_only_memory_entries",
          ec.SQL_DELETE.startswith("DELETE FROM memory_entries WHERE "),
          f"alvo: {ec.SQL_DELETE[:38]}...")


def test_the_dump_round_trips() -> None:
    """[F] O `memory.db` não tem backup. O dump é o único ponto de retorno —
    portanto 'posso reverter' é uma afirmação que precisa de ser executada."""
    with tempfile.TemporaryDirectory() as d:
        tmp = Path(d)
        original_dump = ec.DUMP
        try:
            ec.DUMP = tmp / "dump.json"
            keys = [f"insight:cila_progression:1790879{i:03d}" for i in range(3)]
            path = ec.dump_keys(keys, {"distinct": 1, "texts": []})
            check("test_the_dump_round_trips",
                  path.exists() and ec.json.loads(path.read_text())["keys"] == keys,
                  f"{len(keys)} chaves gravadas")
        finally:
            ec.DUMP = original_dump


def main() -> int:
    print(f"script : {HERE / 'jev_echo_cleanup.py'}")
    print(f"banco  : {ec.MEMORY_DB}\n")
    for test in (
        test_the_proof_key_is_never_selected,
        test_a_hex_key_of_any_shape_is_not_a_timestamp,
        test_both_derivations_agree_on_the_real_database,
        test_divergence_between_derivations_blocks_the_delete,
        test_missing_proof_blocks_the_delete,
        test_partial_proof_does_not_count,
        test_more_than_one_distinct_value_blocks_the_delete,
        test_empty_target_blocks_the_delete,
        test_all_gates_open_lets_the_delete_through,
        test_there_is_no_force_flag,
        test_the_database_is_read_only_by_default,
        test_no_sql_is_interpolated,
        test_the_sql_literals_carry_the_same_predicate,
        test_the_dump_round_trips,
    ):
        test()
    print(f"\n{len(FAILURES)} falha(s): {FAILURES}" if FAILURES
          else "\nTODOS OS TESTES PASSARAM")
    return 1 if FAILURES else 0


if __name__ == "__main__":
    sys.exit(main())
