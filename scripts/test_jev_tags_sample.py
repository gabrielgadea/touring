#!/usr/bin/env python3
"""test_jev_tags_sample.py — os testes nomeados que o gate da S-1.1 exige.

O plano (plan.md, S-1.1) pede:
  - `test_sample_is_deterministic` — mesma seed → mesma amostra;
  - `test_report_has_calibration_curve`;
  - `test_no_writes_to_memory_db`.

E os que a medição precisa para não mentir (dois saíram de defeitos reais
encontrados na construção):
  - `test_vocabulary_matches_production` — a lista de kinds NÃO pode divergir do
    `Facet::Kind::seed_values()` sem falhar alto. Uma lista desatualizada faria a
    medição comparar o Jev contra um vocabulário que já não existe;
  - `test_reference_is_achievable` — pelo menos uma linha da amostra tem que ter
    rótulo do G1. Sem isso, `accuracy_vs_g1_teacher` sai `None` e o relatório
    parece ter medido o professor quando mediu o vazio.

O terceiro teste da lista original (`test_no_writes_to_memory_db`) é verificado
por HASH do arquivo antes e depois de uma chamada real, num banco temporário
— o que prova a afirmação em vez de declará-la.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import sqlite3
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import jev_tags_sample as s  # noqa: E402

TAGS_RS = HERE.parent / "crates" / "touring-intelligence" / "src" / "rl" / "memory" / "tags.rs"
FAILURES: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    print(f"  [{'PASS' if ok else 'FAIL'}] {name}" + (f" — {detail}" if detail else ""))
    if not ok:
        FAILURES.append(name)


def test_sample_is_deterministic() -> None:
    """Mesma seed → mesma amostra, e seed diferente → amostra diferente."""
    memories = s.read_sqlite_ro(s.MEMORY_DB)
    a = [m["key"] for m in s.sample(memories, 100, s.DEFAULT_SEED)]
    b = [m["key"] for m in s.sample(memories, 100, s.DEFAULT_SEED)]
    c = [m["key"] for m in s.sample(memories, 100, s.DEFAULT_SEED + 1)]
    check(
        "test_sample_is_deterministic",
        a == b and a != c and len(a) == 100,
        f"100 chaves, 2 runs iguais={a == b}, seed+1 muda={a != c}",
    )


def test_vocabulary_matches_production() -> None:
    """A lista de kinds tem que ser a de `Facet::Kind::seed_values()`.

    Parseia o Rust em vez de confiar na cópia: a lista está no script porque o
    runtime precisa dela, mas a verdade é o código de produção. Se alguém
    acrescentar um kind lá, este teste avisa em vez de a medição correr contra
    um vocabulário velho.
    """
    if not TAGS_RS.exists():
        check("test_vocabulary_matches_production", False, f"fonte ausente: {TAGS_RS}")
        return
    text = TAGS_RS.read_text(encoding="utf-8")
    match = re.search(r"Facet::Kind\s*=>\s*&\[(.*?)\]", text, re.S)
    if not match:
        check("test_vocabulary_matches_production", False, "Facet::Kind => &[...] não localizado")
        return
    from_rust = set(re.findall(r'"([a-z0-9_-]+)"', match.group(1)))
    from_script = set(s.KIND_VALUES)
    missing = from_rust - from_script
    extra = from_script - from_rust
    check(
        "test_vocabulary_matches_production",
        not missing and not extra,
        f"rust={len(from_rust)} script={len(from_script)}"
        + (f" FALTANDO={sorted(missing)}" if missing else "")
        + (f" SOBRANDO={sorted(extra)}" if extra else ""),
    )


def test_report_has_calibration_curve() -> None:
    """A curva de calibração existe, tem as 5 faixas, e `None` onde não há dado —
    nunca 0, porque 0 é um valor medido e faixa vazia não é medida.

    Casos escolhidos para exercitar os três caminhos: acerto com confiança
    alta (0,9), acerto com confiança BAIXA (0,1 — o caso interessante: é
    confiança mal calibrada, e o ECE existe para medir isso), e uma linha sem
    resposta, que não entra em nenhuma faixa e conta como `unmeasured`.

    A primeira versão deste teste montou "gotcha" predito contra `g1_label:
    "gotcha"` e esperava acerto 0,0 na faixa baixa — ou seja, esperava um ERRO
    onde o código estava certo. O código estava certo; a asserção é que não
    estava. Recordado porque a forma do engano (confiar no que o código devolve
    e ajustar a expectativa) é exatamente o que a lição `instrumento-que-lie`
    descreve.
    """
    rows = [
        {"answer": "lesson", "confidence": 0.9, "current_tag": "lesson", "g1_label": "lesson", "kind_tags_all": []},
        {"answer": "gotcha", "confidence": 0.1, "current_tag": "lesson", "g1_label": "gotcha", "kind_tags_all": []},
        {"answer": None, "confidence": None, "current_tag": "text", "g1_label": "text", "kind_tags_all": []},
    ]
    out = s.evaluate(rows, {})
    g1_curve = out.get("calibration_curve_vs_g1", [])
    prod_curve = out.get("calibration_curve_vs_production_tag", [])
    ok = (
        len(g1_curve) == 5 and len(prod_curve) == 5
        and g1_curve[0]["n"] == 1 and g1_curve[0]["accuracy"] == 1.0   # 0,1 acertou — confiança BAIXA
        and g1_curve[4]["n"] == 1 and g1_curve[4]["accuracy"] == 1.0   # 0,9 acertou — confiança ALTA
        and g1_curve[1]["n"] == 0 and g1_curve[1]["accuracy"] is None # faixa vazia → None, não 0
        and out["unmeasured"] == 1 and out["measured"] == 2            # falha ≠ 0
        and out["accuracy_vs_g1_teacher"] == 1.0                       # 2 de 2 contra o G1
        and out["accuracy_vs_production_tag"] == 0.5                  # só 1 de 2 (a 2ª divergiu)
        # 0,1 acertou (|1,0 − 0,1| = 0,9) e 0,9 acertou (|1,0 − 0,9| = 0,1),
        # cada um meio da amostra → ECE = ½·0,9 + ½·0,1 = 0,5. Valor grande DE
        # PROPÓSITO: prova que a curva capta erro de calibração real.
        and out["ece_vs_g1"] == 0.5
        # Contra a produção o ECE sai 0,1 — MUITO menor que os 0,5 do G1, e o
        # motivo é o dado: só 2 das 3 linhas têm `current_tag` e confiança ao
        # mesmo tempo, e essas duas concordam. Duas referências, dois ECES
        # muito diferentes — é o que faltava quando a curva vivia só no G1.
        # (Meu cálculo à mão dizia 0,45 e estava errado: a 3ª linha não entra
        # nesta curva porque não tem confiança. O valor vem do código.)
        and out["ece_vs_production_tag"] == 0.1
        # A contagem de confiança é publicada como campo próprio: a confusão que
        # eu cometi foi ler `g1_no_reference` como "respostas sem confidence".
        and out["responses_with_confidence"] == 2
    )
    check("test_report_has_calibration_curve", ok,
          f"ece_g1={out['ece_vs_g1']} ece_prod={out['ece_vs_production_tag']} conf={out['responses_with_confidence']}/3")


def test_reference_is_achievable() -> None:
    """A amostra tem que alcançar o corpus do G1.

    Defeito real de 01/10: com o `memory.db` GLOBAL, as 100 linhas vieram sem
    nenhum rótulo do professor (`g1_id` nunca casou) — e `accuracy_vs_g1_teacher`
    teria saído `None`, lido como "medido" em vez de "sem referência". O corpus
    G1 tem 483 das 486 chaves em comum com o banco do PROJETO.
    """
    memories = s.read_sqlite_ro(s.MEMORY_DB)
    g1 = s.load_g1_labels()
    g1_by_key = s.load_g1_key_index()
    if not g1:
        check("test_reference_is_achievable", False, "corpus do G1 ausente — nada a comparar")
        return
    chosen = s.sample(memories, 100, s.DEFAULT_SEED)
    hits = sum(1 for m in chosen if g1_by_key.get(str(m.get("key"))) in g1)
    check("test_reference_is_achievable", hits > 0, f"{hits}/100 linhas com rótulo do G1")


def test_no_writes_to_memory_db() -> None:
    """Prova por hash: o arquivo do banco tem o MESMO sha256 antes e depois de
    uma chamada real do adapter (com chave ausente, para não gastar API)."""
    if not s.MEMORY_DB.exists():
        check("test_no_writes_to_memory_db", False, f"banco ausente: {s.MEMORY_DB}")
        return
    before = hashlib.sha256(s.MEMORY_DB.read_bytes()).hexdigest()
    mtime_before = s.MEMORY_DB.stat().st_mtime_ns
    memories = s.read_sqlite_ro(s.MEMORY_DB)
    s.ask(memories[0], s.KIND_VALUES)  # fail-open sem chave; e mesmo com chave, só lê
    after = hashlib.sha256(s.MEMORY_DB.read_bytes()).hexdigest()
    mtime_after = s.MEMORY_DB.stat().st_mtime_ns
    check(
        "test_no_writes_to_memory_db",
        before == after and mtime_before == mtime_after,
        f"sha256 {before[:12]} inalterado" if before == after else "CONTEUDO MUDOU",
    )


def test_readonly_uri_is_enforced() -> None:
    """O modo read-only é o que garante a afirmação acima: `sqlite3.connect()`
    sem `mode=ro` traz `CREATE` implícito. Este teste prova que o caminho de URI
    realmente falha numa escrita.

    O banco é criado PRIMEIRO com escrita normal — abrir `mode=ro` um arquivo
    inexistente falha com "unable to open database file", que é o que a
    primeira versão deste teste fez (o erro estava no teste, não no código).
    """
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "probe.db"
        seed = sqlite3.connect(path)
        seed.execute("CREATE TABLE t (x int)")
        seed.execute("INSERT INTO t VALUES (7)")
        seed.commit()
        seed.close()
        ro = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
        try:
            ro.execute("INSERT INTO t VALUES (1)")
            ok, detail = False, "INSERT passou — mode=ro NÃO está valendo"
        except sqlite3.OperationalError as exc:
            ok, detail = True, str(exc)[:70]
        finally:
            ro.close()
        check("test_readonly_uri_is_enforced", ok, detail)


def test_current_tag_prefers_higher_trust_source() -> None:
    """Medido no banco real: `gotcha:omarchy-update-sudo-v-prompt` tem
    `kind:lesson` (auto) e `kind:gotcha` (explicit). A referência tem de ser a
    de fonte mais confiável, senão a taxa de acerto é medida contra a tag que
    ninguém escolheu."""
    memory = {"kind_tags": [
        {"value": "lesson", "source": "auto"},
        {"value": "gotcha", "source": "explicit"},
    ]}
    check("test_current_tag_prefers_higher_trust_source", s.current_tag(memory) == "gotcha",
          f"resolveu para {s.current_tag(memory)!r}")


def main() -> int:
    print(f"banco: {s.MEMORY_DB}")
    print(f"corpus G1: {s.G1_CORPUS}\n")
    for test in (
        test_sample_is_deterministic,
        test_vocabulary_matches_production,
        test_report_has_calibration_curve,
        test_reference_is_achievable,
        test_current_tag_prefers_higher_trust_source,
        test_readonly_uri_is_enforced,
        test_no_writes_to_memory_db,
    ):
        test()
    print(f"\n{len(FAILURES)} falha(s): {FAILURES}" if FAILURES else "\nTODOS OS TESTES PASSARAM")
    return 1 if FAILURES else 0


if __name__ == "__main__":
    sys.exit(main())
