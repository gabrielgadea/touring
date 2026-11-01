#!/usr/bin/env python3
"""test_skill_picker.py — os testes nomeados que o gate da S-5.1 exige.

O plano (plan.md, S-5.1) pede:
  - `test_silent_when_below_threshold`
  - `test_manual_override_bypasses_gate`

E os que a forma real do catálogo obriga (medido em 01/10):
  - `test_catalog_deduplicates_by_content` — 102 das 185 entradas são symlink
    para o MESMO `SKILL.md`. Um catálogo por entrada ofereceria ao modelo duas
    cópias da mesma escolha, e o teste é o que impede isso de voltar;
  - `test_shortlist_never_exceeds_three` — a cascata do cookbook é 3, e uma
    Choice de 185 seria um sorteio com latência de 370 ms;
  - `test_gate_silent_without_a_jev_call` — o caso mais comum (a maioria das
    tarefas não é uma skill) tem de calar ANTES de gastar chamada;
  - `test_every_option_carries_a_contrastive_criterion` — o critério de contraste
    é o que separa skills irmãs; sem ele o rótulo é apenas um nome.

Roda sem pytest e sem rede: `python3 test_skill_picker.py`.
"""

from __future__ import annotations

import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import skill_picker as sp
from script_testkit import check, finish


def _skill(key: str, description: str) -> sp.Skill:
    return sp.Skill(key=key, description=description, terms=sp.term_set(f"{key} {description}"))


def test_silent_when_below_threshold() -> None:
    """O gate nomeado do plano: abaixo do corte, silêncio — e SEM chamada."""
    catalog = [_skill("a-skill", "faz relatorios de incidente de seguranca")]
    calls = []

    def spy(*args, **kwargs):
        calls.append(args)
        raise AssertionError("o gate chamou o Jev abaixo do tau")

    out = sp.skill_picker("quero fazer um bolo de chocolate", tau=0.9, catalog=catalog, client=spy)
    check("test_silent_when_below_threshold", out["said"] is False and not calls,
          f"silent_reason={out['silent_reason']} calls={len(calls)}")


def test_manual_override_bypasses_gate() -> None:
    """O override manual ignora o gate por definição — é o usuário falando."""

    argv = ["--query", "qualquer", "--override", "recipe-gmail-send"]
    import contextlib
    import io
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        code = sp.main(argv)
    printed = buf.getvalue().strip()
    check("test_manual_override_bypasses_gate",
          code == 0 and "recipe-gmail-send" in printed,
          f"saida={printed!r}")


def test_gate_silent_without_a_jev_call() -> None:
    """O caso mais comum tem de ser o mais barato: pré-filtre zero = calar."""
    catalog = [_skill("a-skill", "relatorio de incidente")]
    calls = []

    def spy(*args, **kwargs):
        calls.append(1)

    out = sp.skill_picker("assunto sem nenhuma relacao com skills", tau=0.1, catalog=catalog, client=spy)
    check("test_gate_silent_without_a_jev_call",
          out["said"] is False and out["silent_reason"] == "shortlist_empty" and not calls,
          f"{out['silent_reason']}, {len(calls)} chamada(s)")


def test_shortlist_never_exceeds_three() -> None:
    """A cascata do cookbook é top-3; a Choice cresce com o catálogo."""
    catalog = [_skill(f"skill-{i:03}", f"capacidade numero {i} para tarefas especiais") for i in range(200)]
    hits = sp.shortlist("capacidade numero", catalog, limit=3)
    check("test_shortlist_never_exceeds_three", len(hits) == 3,
          f"{len(hits)} de {len(catalog)} no shortlist")


def test_every_option_carries_a_contrastive_criterion() -> None:
    """Sem o campo de contraste, a Choice entre skills irmãs é um sorteio."""
    skill = _skill("taco-cross-audit", "auditoria cruzada de proposito e fusesao")
    option = skill.to_choice_option()
    ok = "NÃO usar" in option and skill.description in option
    check("test_every_option_carries_a_contrastive_criterion", ok, option[:88])


def test_catalog_deduplicates_by_content() -> None:
    """102 das 185 entradas reais são symlink para o MESMO conteúdo."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        body = "---\nname: teste\ndescription: conteudo unico para dedup\n---\ncorpo\n"
        (root / "skill-original").mkdir()
        (root / "skill-original" / "SKILL.md").write_text(body, encoding="utf-8")
        (root / "skill-espelho").mkdir()
        (root / "skill-espelho" / "SKILL.md").write_text(body, encoding="utf-8")
        (root / "skill-outro").mkdir()
        (root / "skill-outro" / "SKILL.md").write_text(
            "---\nname: outro\ndescription: conteudo diferente\n---\n", encoding="utf-8")
        # uma entrada que NÃO é skill
        (root / ".pytest_cache").mkdir()

        catalog = sp.build_catalog(root)
        check("test_catalog_deduplicates_by_content", len(catalog) == 2,
              f"{len(catalog)} opções de 4 entradas")
        # O canônico é o PRIMEIRO nome em ordem alfabética entre os que
        # compartilham conteúdo — aqui `skill-espelho`, não `skill-original`.
        # A primeira versão deste teste exigia `skill-original` e falhou: o
        # código estava certo (a dedup happened, o alias viajou) e a asserção é
        # que assumia um "canônico" que o código não promete. O que importa não
        # é qual nome sobrevive — é que o outro foi preservado como alias, para
        # que a resposta possa ser apresentada com o nome que o usuário usou.
        deduped = next((s for s in catalog if {"skill-original", "skill-espelho"} <= {s.key, *s.aliases}), None)
        check("the_alias_travels_with_the_option",
              deduped is not None and len(deduped.aliases) == 1,
              f"canônico={deduped.key if deduped else None} aliases={deduped.aliases if deduped else None}")


def test_the_real_catalog_has_no_duplicated_content() -> None:
    """O catálogo real, se ainda existir: nenhuma opção repetida."""
    catalog = sp.build_catalog()
    if not catalog:
        check("the_real_catalog_has_no_duplicated_content", True, "sem skills em disco")
        return
    keys = [s.key for s in catalog]
    aliases = [a for s in catalog for a in s.aliases]
    overlap = set(keys) & set(aliases)
    check("the_real_catalog_has_no_duplicated_content", not overlap,
          f"{len(catalog)} opções, {len(aliases)} aliases, sobreposição={len(overlap)}")


def test_term_set_drops_stopwords() -> None:
    """Sem stopwords, toda consulta genérica casa com toda skill."""
    terms = sp.term_set("como criar um relatório de incidente de segurança em python")
    ok = "criar" not in terms and "como" not in terms and "relatório" in terms or "relatorio" in terms
    check("test_term_set_drops_stopwords", ok, f"{sorted(terms)[:8]}")


def test_the_judge_path_runs_end_to_end() -> None:
    """**[F] Este é o teste que faltava, e o motivo é um bug real.**

    `touring-a7` registrou o picker no `settings.json` (decisão 4 do Gabriel) e o
    hook encontrou: `TypeError: 'function' object is not iterable` em
    `skill_picker.py:285`. A causa era `shortlist` — a **FUNÇÃO** do módulo, não a
    lista: eu tinha renomeado a variável local para `candidates` justamente
    porque o mesmo sombreamento já me dera `UnboundLocalError`, mas escapei nos
    DOIS usos abaixo.

    **[I] (0,90) Por que os 9 testes passavam:** nenhum exercitava o caminho
    `--judge`. O `try` do import cobre só a importação do adapter; o `TypeError`
    nasce **depois**, na linha da chamada, e escapa como exceção — não vira
    `silent_reason`. Um gate com 9/9 verde e o caminho principal nunca executado
    é o pior estado possível para um componente que vai viver num hook.

    **A camada que se exercita é o `client` do SDK, não o `system_one`.** O
    `client` injetado é repassado ao adapter, que o chama por dentro — injetar um
    spy em `system_one` testaria a camada de cima e o `TypeError` nem apareceria.
    A primeira versão deste teste fez exatamente isso e passou com o bug ainda
    presente.
    """
    catalog = [
        _skill("skill-incidente", "relatorio de incidente de seguranca em producao"),
        _skill("skill-deploy", "publicar uma versao em producao com rollback"),
        _skill("skill-refactor", "refatorar codigo preservando comportamento"),
    ]

    class FakeClient:
        """O mínimo que `TypeSafeClient` precisa: `system_one(state, questions, model)`."""

        def __init__(self):
            self.seen = None

        def system_one(self, state, questions, model=None):
            self.seen = (state, questions)
            raise RuntimeError("fake: cortando antes da rede")

    fake = FakeClient()
    out = sp.skill_picker("relatorio de incidente de seguranca em producao",
                          tau=0.3, catalog=catalog, client=fake)
    reached = fake.seen is not None
    # O `RuntimeError` é pego pelo `_SDK_ERRORS`/fail-open do adapter → `None`
    # → `silent_reason: jev_fail_open`. O que se prova é que CHEGOU à chamada.
    candidates_ok = reached and isinstance(fake.seen[0]["candidates"], list)
    check("test_the_judge_path_runs_end_to_end",
          reached and candidates_ok and out["silent_reason"] == "jev_fail_open",
          f"chegou_a_chamada={reached} candidates_e_lista={candidates_ok} "
          f"silent_reason={out['silent_reason']}")


def test_the_state_and_criteria_carry_lists_not_functions() -> None:
    """A forma exata que o bug violou: `state["candidates"]` e `criteria` têm de
    ser LISTAS de strings. Uma função no lugar reprova o adapter com
    `TypeError` — e o `TypeError` escapa do `try`."""
    catalog = [
        _skill("skill-incidente", "relatorio de incidente de seguranca"),
        _skill("skill-deploy", "publicar versao com rollback"),
    ]
    candidates = sp.shortlist("relatorio de incidente de seguranca", catalog, limit=3)
    state = {"task": "x", "candidates": [s.key for s, _ in candidates]}
    criteria = sp.build_question(candidates)["which_skill"]["criteria"]

    ok_state = isinstance(state["candidates"], list) and all(isinstance(k, str) for k in state["candidates"])
    ok_crit = isinstance(criteria, dict) and all(isinstance(v, str) for v in criteria.values())
    ok_none = "none" in criteria  # a válvula de escape do cookbook
    check("test_the_state_and_criteria_carry_lists_not_functions",
          ok_state and ok_crit and ok_none,
          f"candidates={type(state['candidates']).__name__} criteria={len(criteria)} com_none={ok_none}")


def main() -> int:
    print(f"catálogo real: {len(sp.build_catalog())} opções de conteúdo\n")
    for test in (
        test_term_set_drops_stopwords,
        test_silent_when_below_threshold,
        test_manual_override_bypasses_gate,
        test_gate_silent_without_a_jev_call,
        test_the_judge_path_runs_end_to_end,
        test_the_state_and_criteria_carry_lists_not_functions,
        test_shortlist_never_exceeds_three,
        test_every_option_carries_a_contrastive_criterion,
        test_catalog_deduplicates_by_content,
        test_the_real_catalog_has_no_duplicated_content,
    ):
        test()
    return finish()


if __name__ == "__main__":
    sys.exit(main())
