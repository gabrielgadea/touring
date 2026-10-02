#!/usr/bin/env python3
"""r3_b0 — porte FIEL de `classify_route_outcome` (o baseline B0 do A2).

Fonte da verdade (crates/touring-cli/src/cli_suggester.rs):
  · `classify_route_outcome` :4482-4509
  · `extract_run_body`       :4414-4432
  · `program_shape`          :4441-4469
  · `INSPECTION_VERBS`       :4406-4408

PORTE, não reimplementação: cada regra abaixo cita a linha de origem. Um teste de
paridade com `cli_suggester_tests.rs:3972-3999` é o que prova a fidelidade — e um
teste que reescrevesse a regra provaria que a NOSSA regra funciona.

Nada aqui altera limiar: os critérios pré-registrados (CRITERIOS-ACEITACAO.md) fixam
o que se mede, e este módulo é a medida.
"""
from __future__ import annotations

# `const INSPECTION_VERBS: [&str; 12]` — cli_suggester.rs:4406-4408
INSPECTION_VERBS = frozenset(
    ("grep", "rg", "cat", "head", "tail", "sed", "awk", "ls", "wc", "find", "stat", "jq")
)

# `const BYPASS: [&str; 3]` — cli_suggester.rs:4485-4489
BYPASS = (
    "TOURING_CODE_MODE=native",
    "TOURING_GATE_OK=1",
    "TOURING_CODE_GATES_DISABLED=1",
)

# Separadores de token em Rust: c.is_whitespace() || ";,|&()\"'".split — :4447
_TOKEN_SPLIT = set(";,|&()\"'")
# Separadores de alvo: c.is_whitespace() || "\"'()".split — :4453
_ALVO_SPLIT = set("\"'()")

TRIVIAL = "trivial"
FUSED = "fused"


def extract_run_body(cmd: str) -> str | None:
    """`extract_run_body` — cli_suggester.rs:4414-4432.

    Sem o corpo não há o que classificar, e None mantém o comportamento anterior:
    jamais adivinhar a forma de um programa que não se leu (:4412-4413).
    """
    idx = cmd.find("--code")
    if idx < 0:
        return None
    resto = cmd[idx + len("--code"):]
    resto = resto.lstrip()  # Rust trim_start() — Unicode, não só ASCII
    if not resto:
        return None
    aspa = resto[0]
    if aspa not in ("'", '"'):
        return None
    corpo = resto[1:]
    # A PRIMEIRA aspa igual fecha, não a última (:4423-4431). `corpo.find(aspa)`.
    fim = corpo.find(aspa)
    if fim < 0:
        return None
    return corpo[:fim]


def program_shape(body: str) -> tuple[str, int]:
    """`program_shape` — cli_suggester.rs:4441-4469.

    Devolve (forma, fundido). `Trivial` quando fundido <= 1 (:4464-4465).

    Dois desvios do Rust, ambos por limitação de Python, ambos decididos aqui:
      · `t.trim_start_matches("$(")` remove UM prefixo, não repetidamente: o Rust
        também remove só um (trim_start_matches com string consome uma ocorrência).
      · a lista `alvos` é um BTreeSet no Rust (ordem estável, dedup); aqui é `set`.
        A contagem é a mesma — só a ordem de iteração muda, e nada a consome.
    """
    ops = 0
    alvo_tokens: set[str] = set()
    for tok in _split(body, _TOKEN_SPLIT):
        t = tok
        if t.startswith("$("):
            t = t[2:]
        if t in INSPECTION_VERBS:
            ops += 1
    # `open(` é chamada de função (Python), não token isolado — :4450-4451
    ops += body.count("open(")

    for tok in _split(body, _ALVO_SPLIT):
        if "/" not in tok or len(tok) <= 3 or tok.startswith("-"):
            continue
        # :4458-4461 — dispositivos e descritores não são alvos de inspeção
        t = tok
        while t[:1] in (">", "<", "&", "1", "2"):
            t = t[1:]
        if t.startswith("/dev/"):
            continue
        alvo_tokens.add(tok)

    fundido = max(ops, len(alvo_tokens))
    return (TRIVIAL, fundido) if fundido <= 1 else (FUSED, fundido)


def _split(s: str, seps: set[str]) -> list[str]:
    """split por whitespace ou por qualquer caractere de `seps` — os dois sites Rust."""
    out, cur = [], []
    for ch in s:
        if ch.isspace() or ch in seps:
            if cur:
                out.append("".join(cur))
                cur = []
        else:
            cur.append(ch)
    if cur:
        out.append("".join(cur))
    return out


def classify_route_outcome(cmd: str) -> float | None:
    """`classify_route_outcome` — cli_suggester.rs:4482-4509.

    `Some(1.0)` rodou o programa; `Some(0.5)` rodou uma casca trivial; `Some(0.0)`
    recusou com token de relaxamento; **`None` fez outra coisa — e isso NÃO é
    veredito** (ausência de sinal é desconhecido, jamais zero — :4475-4477).

    A ORDEM importa: a rota é testada ANTES do bypass, porque
    `TOURING_CODE_MODE=native touring run …` RODOU o programa — relaxar o gate no
    caminho não desfaz o fato de a rota ter sido tomada (:4479-4481).
    """
    if "touring run " in cmd or "touring exec " in cmd:
        body = extract_run_body(cmd)
        if body is not None:
            forma, _ = program_shape(body)
            return 0.5 if forma == TRIVIAL else 1.0
        return 1.0  # rota tomada sem corpo legível: `_ => 1.0` (:4502)
    for t in BYPASS:
        if t in cmd:
            return 0.0
    return None


def b0_decision(cmds: list[str], k: int = 3) -> tuple[str, float | None]:
    """B0 da Errata E1.2: `classify_route_outcome` aplicado SEQUENCIALMENTE aos
    próximos K=3 comandos Bash da mesma sessão; o primeiro `Some(v)` decide.

    · `v > 0`  ⇒ `followed`
    · `v == 0` ⇒ `not_followed` (recusa por token de relaxamento)
    · nenhum `Some` nos K ⇒ `not_followed` (E1.2: "sem nenhum Some ⇒ not-followed")

    Devolve (decisao, valor) — o valor para auditoria (0,0 / 0,5 / 1,0), a decisão
    para o estrato do audit sample (E1.3, 120/120).
    """
    for cmd in cmds[:k]:
        v = classify_route_outcome(cmd)
        if v is not None:
            return ("followed" if v > 0 else "not_followed"), v
    return "not_followed", None
