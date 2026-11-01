#!/usr/bin/env python3
"""Testes do porte B0 — paridade com `cli_suggester_tests.rs:3972-3999` e com as
regras de `classify_route_outcome` (cli_suggester.rs:4482-4509).

Cada caso aqui tem a linha Rust que o origina. Um teste que reescrevesse a regra
provaria que a NOSSA regra funciona; o valor deste arquivo é provar que é A DELE.
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from r3_b0 import (  # noqa: E402
    BYPASS,
    INSPECTION_VERBS,
    b0_decision,
    classify_route_outcome,
    extract_run_body,
    program_shape,
)


def test_b0_matches_the_rust_reference_table() -> None:
    """Paridade com `cli_suggester_tests.rs:3972-3983` — lida pelo que a linha PROVA.

    O teste Rust faz três afirmações, e a terceira NÃO é sobre o classificador:
      · `:3972` `""` → `None` — um `Read` não carrega comando, nada a classificar
      · `:3974-3981` `"touring run --lang bash --code 'ls'"` → `Some(0,5)`,
        "programa de um alvo: rota tomada, economia nula"
      · `:3983` `"touring run --code 'x'"` → `None` — **a oferta JÁ FOI CONSUMIDA**
        em `:3974`; o `None` é do `claim_route_reward` sem oferta pendente
        (`cli_suggester.rs:4886`), não do classificador. O classificador devolve
        `Some(0,5)` para as DUAS commands — é a ausência da oferta que zera.
    """
    assert classify_route_outcome("") is None, "cmd vazio não é veredito"
    # `ls` = 1 verbo, 0 alvos → fundido=1 → Trivial → 0,5 (:4464-4465, :4501)
    assert classify_route_outcome("touring run --lang bash --code 'ls'") == 0.5, (
        "programa de uma operação é casca: 0,5 (cli_suggester.rs:4497-4501)"
    )
    # A 3ª linha do Rust NÃO é um caso do classificador: prove que o classificador
    # devolve Some para ela, e que o None da linha 3983 é a oferta consumida.
    assert classify_route_outcome("touring run --code 'x'") == 0.5, (
        "`touring run ` casa (o espaço está depois de run, :4490) e o corpo `x` é "
        "trivial — o None de cli_suggester_tests.rs:3983 vem da oferta já consumida"
    )


def test_bypass_by_env_scores_zero_not_none() -> None:
    """`Some(0.0)` recusou a rota com token de relaxamento — :4505-4507.

    E a ORDEM importa: a rota é testada ANTES do bypass, porque
    `TOURING_CODE_MODE=native touring run …` RODOU o programa (:4479-4481).
    O valor então depende da FORMA do programa: `a b` são 2 tokens-alvo, logo
    fundido=2 → Fused → 1,0.
    """
    for t in BYPASS:
        assert classify_route_outcome(f"{t} ls -la") == 0.0, f"bypass {t} = 0,0"
    # `a b` não funde (0 alvos, `b` sem `/`) → Trivial → 0,5: o Rust daria 0,5
    assert classify_route_outcome("TOURING_CODE_MODE=native touring run --code 'a b'") == 0.5, (
        "o bypass não vence a rota (cli_suggester.rs:4479-4481); o valor é o da forma"
    )
    assert classify_route_outcome(
        "TOURING_CODE_MODE=native touring run --code 'cat /a/x.rs /b/y.rs'"
    ) == 1.0, "relaxar o gate no caminho não desfaz a rota tomada (rota = Some, forma fundida = 1,0)"


def test_program_shape_counts_words_not_substrings() -> None:
    """O teste que o Rust já tem: `body.matches("rg")` contaria 1 dentro de
    "Cargo.toml" e `cat /a/Cargo.toml` — um arquivo, uma operação — viraria fusão
    de duas (cli_suggester.rs:4442-4445)."""
    forma, fundido = program_shape("cat /a/Cargo.toml")
    assert forma == "trivial", f"'cat /a/Cargo.toml' é 1 alvo (obrigatório trivial), veio {forma}/{fundido}"
    forma, fundido = program_shape("cat /a/x.rs && cat /b/y.rs")
    assert forma == "fused", f"2 arquivos é fusão, veio {forma}/{fundido}"


def test_target_needs_more_than_three_characters() -> None:
    """`t.len() > 3` (:4454) — o mutante que troca por `> 2` muda o que é alvo.

    `/a` tem 2 chars e não é alvo de inspeção; `/ab` tem 3 e também não. O corte
    existe para que fragmento de caminho não conte como arquivo tocado.
    """
    assert program_shape("cat /ab")[0] == "trivial", "'/ab' tem 3 chars: não é alvo (len > 3)"
    assert program_shape("cat /abc")[0] == "trivial", "'/abc' tem 4 chars: 1 alvo, ainda trivial"
    forma, fundido = program_shape("cat /ab /abcd")
    assert fundido == 1 and forma == "trivial", (
        f"só '/abcd' (4 chars) é alvo → fundido=1 → TRIVIAL, veio {forma}/{fundido}"
    )
    # o par que o mutante len>2 estragaria: '/ab' (3 chars) contaria como alvo
    forma_mut, fundido_mut = program_shape("cat /ab /efgh")
    assert fundido_mut == 1 and forma_mut == "trivial", (
        f"'/ab' e '/efgh' têm 3 e 4 chars: só '/efgh' conta → fundido=1, veio {forma_mut}/{fundido_mut}"
    )


def test_fused_threshold_is_one_not_two() -> None:
    """`if fundido <= 1` (:4464) — o mutante que vira `<= 2` promote um programa de
    2 alvos a "trivial" (0,5) quando o Rust daria fusão (1,0)."""
    forma, fundido = program_shape("cat /abcd /efgh")
    assert fundido == 2 and forma == "fused", f"2 alvos é fusão, veio {forma}/{fundido}"


def test_route_without_readable_body_still_counts_as_taken() -> None:
    """`_ => 1.0` (:4502): rota tomada com corpo ilegível credita 1,0 — o canal
    está certo, e sem corpo não há forma a arbitrar. O mutante que devolve 0,5
    inventaria economia onde o Rust não mediu nenhuma."""
    assert classify_route_outcome("touring run --lang bash") == 1.0, (
        "rota sem --code: `extract_run_body` devolve None e o `_` credita 1,0 (:4502)"
    )
    assert classify_route_outcome("touring run --code sem-aspas") == 1.0, (
        "corpo sem aspa de abertura também é None → 1,0 (:4419-4421, :4502)"
    )


def test_bypass_value_is_zero_and_not_a_route() -> None:
    """`Some(0.0)` (:4506) é a RECUSA — e B0 só a distingue porque compara `v > 0`.
    O mutante que vira 1,0 transforma toda recusa em adesão."""
    assert classify_route_outcome("TOURING_GATE_OK=1 ls") == 0.0
    decisao, valor = b0_decision(["TOURING_GATE_OK=1 ls", "TOURING_GATE_OK=1 wc"], k=3)
    assert decisao == "not_followed" and valor == 0.0, (
        f"bypass é not_followed com 0,0 (v>0 falso), veio {decisao}/{valor}"
    )


def test_dev_null_is_plumbing_not_a_target() -> None:
    """`2>/dev/null` contava como segundo alvo e fazia um programa de um arquivo
    parecer que tocava dois — medido ao vivo em 26/08 (:4455-4461)."""
    forma, _ = program_shape("cat /a/x.rs 2>/dev/null")
    assert forma == "trivial", f"/dev/ é plumbing, não alvo — veio {forma}"


def test_extract_run_body_closes_on_the_first_quote() -> None:
    """`rfind` capturava além do corpo quando o comando externo trazia mais aspas
    depois — medido ao vivo em 26/08 (:4423-4431)."""
    cmd = "touring run --lang bash --code 'cat X' 2>/dev/null | python3 -c \"print('1')\""
    assert extract_run_body(cmd) == "cat X", (
        f"a PRIMEIRA aspa fecha o corpo (cli_suggester.rs:4430), veio {extract_run_body(cmd)!r}"
    )


def test_window_is_cut_at_the_next_offer() -> None:
    """E1.4: a janela são os PRÓXIMOS K=3 comandos, e o 1º `Some` decide.

    O programa de fusão precisa de 2 **alvos** (tokens com `/`, >3 chars) ou 2
    verbos — `cat a b` não funde nada: sem `/` não há alvo, e `cat` sozinho dá
    fundido=1. Por isso o corpo é `cat /a/x.rs /b/y.rs`.
    """
    fusivel = "touring run --lang bash --code 'cat /a/x.rs /b/y.rs'"

    # O 4º comando está FORA da janela: um Some nele não pode virar veredito.
    cmds = ["ls", "grep x f", "wc -l g", fusivel]
    decisao, valor = b0_decision(cmds, k=3)
    assert decisao == "not_followed" and valor is None, (
        f"o Some está no 4º, além da janela K=3 — não pode decidir, veio {decisao}/{valor}"
    )

    # Dentro da janela, o Some decide — e o valor é o da FORMA (fundido = 1,0).
    decisao, valor = b0_decision(["ls", fusivel, "grep x f", "wc -l g"], k=3)
    assert decisao == "followed" and valor == 1.0, (
        f"o 1º Some é o 2º comando: decide com 1,0 (fundido), veio {decisao}/{valor}"
    )


def test_offer_without_followup_is_not_followed() -> None:
    """E1.2: sem nenhum `Some` nos K comandos ⇒ `not_followed`."""
    assert b0_decision([], k=3) == ("not_followed", None)
    assert b0_decision(["git status", "ls -la", "wc -l f"], k=3) == ("not_followed", None)


def test_inspection_verbs_is_the_twelve_of_the_rust() -> None:
    """A lista é a de `cli_suggester.rs:4406-4408` — 12 verbos, nem um a mais."""
    assert len(INSPECTION_VERBS) == 12, f"a lista tem {len(INSPECTION_VERBS)}, o Rust tem 12"
    assert "rg" in INSPECTION_VERBS and "jq" in INSPECTION_VERBS


def test_no_command_is_ever_inferred_from_absence() -> None:
    """Ausência de sinal é desconhecido, jamais zero (:4475-4477)."""
    assert classify_route_outcome("git commit -m 'oi'") is None
    assert classify_route_outcome("python3 -c 'print(1)'") is None
