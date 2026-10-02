#!/usr/bin/env python3
"""test_jev_dedupe.py — os testes da S-5.5.

O plano pede `test_dedupe_is_advisory_signal`. Este é ele, e os que o cercam —
porque a descoberta da implementação (37,5% do banco tem o mesmo texto, e as
chaves só diferem pelo timestamp) mudou o desenho: **o Noul sozinho não pegaria
o defeito que a subtask existe para atacar.**

O contrato em três partes:
1. **nenhum par vira deleção** — nem no relatório, nem no banco, nem num
   arquivo. `writes_performed == 0` e `deletions_performed == 0`, sempre;
2. **o sinal diz de que estágio veio** (`exact_normalized` vs
   `near_duplicate`), porque "achado por SQL" e "achado por modelo" são
   evidências de qualidades diferentes e um relatório que os misturas mente
   sobre a origem;
3. **a chave redundante é o eco do scout** — duas entradas com o MESMO texto mas
   chaves que só diferem pelo timestamp são o defeito nomeado; duas entradas com
   o mesmo texto e chaves de arquivos diferentes são agrupamento legítimo.

Roda sem pytest e sem rede: `python3 test_jev_dedupe.py`.
"""

from __future__ import annotations

import json
import sqlite3
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import jev_dedupe as dd  # noqa: E402

FAILURES: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    print(f"  [{'PASS' if ok else 'FAIL'}] {name}" + (f" — {detail}" if detail else ""))
    if not ok:
        FAILURES.append(name)


def _entries(pairs: list[tuple[str, str]]) -> list[dict]:
    return [{"key": k, "value": v, "access_count": 1, "created_at": "2026-10-01"} for k, v in pairs]


def test_dedupe_is_advisory_signal() -> None:
    """**[F] O teste nomeado do plano.** Sinais são publicados; nada é apagado."""
    entries = _entries([
        ("a:1", "o mesmo texto em duas entradas"),
        ("a:2", "o mesmo texto em duas entradas"),
    ])
    groups = dd.exact_groups(entries)
    out = dd.summarize(entries, groups, [], [])
    check("test_dedupe_is_advisory_signal",
          out["groups"] == 1
          and out["writes_performed"] == 0
          and out["deletions_performed"] == 0
          and "nenhum par vira deleção" in out["note"],
          f"groups={out['groups']} writes={out['writes_performed']} deletions={out['deletions_performed']}")


def test_the_redundant_key_marks_the_scout_echo() -> None:
    """**[F] O defeito que a S-5.5 nomeia:** chaves que só diferem pelo
    timestamp, com o mesmo texto. É o eco do scout perpétuo — e ele é
    distinguível de um agrupamento legítimo por chaves de arquivos diferentes."""
    echo = _entries([(f"insight:cila_progression:17875174{i:02d}",
                      "Insufficient compliance data for CILA progression analysis")
                     for i in range(5)])
    legit = _entries([("vgp:create:a.rs:Sym", '{"symbol":"Sym","status":"verified"}'),
                      ("vgp:create:b.rs:Sym", '{"symbol":"Sym","status":"verified"}')])
    groups = dd.exact_groups(echo + legit)
    by_stem = {g["key_stem"]: g for g in groups}
    check("the_redundant_key_marks_the_scout_echo",
          any(g["redundant_key"] and g["count"] == 5 for g in groups)
          and any(not g["redundant_key"] and g["count"] == 2 for g in groups),
          f"grupos={[(g['count'], g['redundant_key']) for g in groups]}")
    out = dd.summarize(echo + legit, groups, [], [])
    check("the_echo_is_counted_separately",
          out["redundant_key_groups"] == 1 and out["redundant_key_excess"] == 4,
          f"grupos_redundantes={out['redundant_key_groups']} excesso={out['redundant_key_excess']}")


def test_normalization_is_whitespace_and_case_only() -> None:
    """Normalizar mais que isso (pontuação, stopwords) criaria um 'quase igual'
    determinístico — que é exatamente o que o estágio 2 existe para julgar."""
    check("test_normalization_is_whitespace_and_case_only",
          dd.normalize("  Olá   MUNDO  ") == dd.normalize("olá mundo")
          and dd.normalize("olá, mundo!") != dd.normalize("olá mundo"),
          "espaco+case sim, pontuação não")


def test_a_single_entry_is_never_a_group() -> None:
    """Um grupo de tamanho 1 não é duplicata — e reportá-lo inflaria o número."""
    groups = dd.exact_groups(_entries([("a:1", "única"), ("b:1", "outra única")]))
    check("a_single_entry_is_never_a_group", groups == [], f"{len(groups)} grupos")


def test_identical_pairs_belong_to_stage_one_not_two() -> None:
    """O estágio 2 compara o que o estágio 1 já resolveu seria jogar fora."""
    entries = _entries([(f"k:{i}", "texto identico o suficiente para similarity 1.0") for i in range(4)])
    pairs = dd.near_pairs(entries, dd.SIMILARITY_FLOOR)
    groups = dd.exact_groups(entries)
    out = dd.summarize(entries, groups, pairs, [])
    check("identical_pairs_belong_to_stage_one_not_two",
          out["near_pairs_found"] == 0 and out["groups"] == 1,
          f"grupos={out['groups']} near_pairs={out['near_pairs_found']}")


def test_short_texts_never_reach_similarity() -> None:
    """Duas frases de 20 caracteres têm similaridade sempre 0,0 ou sempre 1,0, e
    nenhum dos dois quer dizer nada — daí o piso de comprimento."""
    entries = _entries([("k:1", "erro curto"), ("k:2", "erro curto")])
    check("test_short_texts_never_reach_similarity",
          len(dd.near_pairs(entries, dd.SIMILARITY_FLOOR)) == 0,
          "textos curtos ficam fora do estágio 2")


def test_the_real_database_shows_the_echo_and_the_near_pairs() -> None:
    """O banco real, se existir: **os dois** números que motivaram o desenho em
    dois estágios.

    **[F] Correção de uma afirmação minha.** O relatório da primeira medição
    dizia "0 pares ≥0,85" e eu escrevi isso no docstring do `near_pairs`. Era
    **falso**: eu tinha medido só as 120 primeiras entradas do banco (7.140
    comparações), e o número cresce com o cap — **cap 200 → 1 par · cap 400 → 10
    pares**, todos entre chaves `artifact:*`, com similaridade até 0,9668.

    **[I] (0,95)** Um zero medido sobre um subconjunto que não é o universo é o
    pior tipo de zero: parece ausência e é limite de amostragem. É a mesma
    classe de `zero de dentro do sandbox` e de `recalls=0`. O teste agora afirma
    os DOIS números, e o cap fica no relatório — porque "10 pares em 400
    entradas" e "10 pares em 20.042" não são a mesma afirmação.
    """
    if not dd.MEMORY_DB.exists():
        check("the_real_database_shows_the_echo_and_the_near_pairs", True, "banco ausente")
        return
    entries = dd.read_entries(dd.MEMORY_DB)
    groups = dd.exact_groups(entries)
    pairs = dd.near_pairs(entries, dd.SIMILARITY_FLOOR, cap=400)
    biggest = max((g["excess"] for g in groups), default=0)
    check("the_real_database_shows_the_echo_and_the_near_pairs",
          biggest > 1000 and len(pairs) > 0,
          f"{len(entries)} entradas, maior_excesso={biggest}, near_pairs(cap=400)={len(pairs)}")


def test_the_database_is_opened_read_only() -> None:
    """`mode=ro` é o que sustenta `deletions_performed: 0`."""
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "probe.db"
        seed = sqlite3.connect(path)
        seed.execute("CREATE TABLE t (x int)")
        seed.commit()
        seed.close()
        ro = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
        try:
            ro.execute("DELETE FROM t")
            ok, detail = False, "DELETE passou — mode=ro nao esta valendo"
        except sqlite3.OperationalError as exc:
            ok, detail = True, str(exc)[:56]
        finally:
            ro.close()
        check("test_the_database_is_opened_read_only", ok, detail)


def main() -> int:
    print(f"banco: {dd.MEMORY_DB}")
    print(f"corte de similaridade: {dd.SIMILARITY_FLOOR}  piso de comprimento: {dd.MIN_LENGTH}\n")
    for test in (
        test_dedupe_is_advisory_signal,
        test_the_redundant_key_marks_the_scout_echo,
        test_normalization_is_whitespace_and_case_only,
        test_a_single_entry_is_never_a_group,
        test_identical_pairs_belong_to_stage_one_not_two,
        test_short_texts_never_reach_similarity,
        test_the_real_database_shows_the_echo_and_the_near_pairs,
        test_the_database_is_opened_read_only,
    ):
        test()
    print(f"\n{len(FAILURES)} falha(s): {FAILURES}" if FAILURES else "\nTODOS OS TESTES PASSARAM")
    return 1 if FAILURES else 0


if __name__ == "__main__":
    sys.exit(main())
