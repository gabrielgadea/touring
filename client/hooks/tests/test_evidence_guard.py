#!/usr/bin/env python3
"""test_evidence_guard.py — a suíte do `evidence_guard`.

Existe por uma razão que precisa ser dita: o guard foi escrito depois de eu
reportar **quatro** scripts com `F3.1 = 0,000` por não terem teste nenhum, e
o meu próprio escorregou para a mesma classe (`F3_1` blocker na primeira
medição). Um guard que fiscaliza cobertura e não tem cobertura é a
incoerência exata que o cross-audit achou no lado do outro.

O contrato em três partes:

1. **Recall sobre erros REAIS** — o corpus é texto que saiu de uma sessão de
   verdade, não casos inventados. Um detector que só se prova contra o que já
   acerta não sabe o que não sabe.
2. **Precisão sobre saídas REAIS limpas** — 0 falsos positivos. Um guard
   ruidoso treina o agente a ignorar a camada seguinte, e aí é pior que nada.
3. **Fail-closed no próprio desconhecimento** — quando não sabe, devolve
   `unmeasured`. Nunca `clean` por ausência de evidência. É o princípio de
   `detector-nao-conclui-por-ausencia` aplicado ao detector que a executa.

Roda sem pytest e sem rede: `python3 test_evidence_guard.py`.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]  # the hooks live one level up
sys.path.insert(0, str(HERE))

import evidence_guard as eg

GUARD = HERE / "evidence_guard.py"
FAILURES: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    print(f"  [{'PASS' if ok else 'FAIL'}] {name}" + (f" — {detail}" if detail else ""))
    if not ok:
        FAILURES.append(name)


# --- 1. RECALL: o corpus são erros que aconteceram ------------------------

def test_the_real_error_corpus_is_caught() -> None:
    """[F] 6 dos 9 erros reais são apanháveis pela SAÍDA. Os 3 restantes são
    o limite estrutural nomeado em §4 do relatório — e este teste fixa essa
    contagem, para que uma regressão ou um ganho seja visível e não sentido."""
    caught = 0
    for case in eg._REAL_ERRORS:
        v = eg.scan_command(case["command"]) if case.get("needs_command") else eg.scan_output(case["output"])
        if v.findings:
            caught += 1
    check("test_the_real_error_corpus_is_caught", caught >= 6,
          f"{caught}/{len(eg._REAL_ERRORS)} apanháveis (piso 6 — o resto é limite estrutural)")


def test_every_detection_names_a_lesson_and_a_corroboration() -> None:
    """[F] Densidade é exigência do acervo: uma detecção sem lição nomeada é
    ruído, e sem corroboração obriga o agente a inventar o segundo sinal."""
    bad = [f["lesson"] for f in
           [x for c in eg._REAL_ERRORS
            for x in (eg.scan_command(c["command"]).findings if c.get("needs_command")
                      else eg.scan_output(c["output"]).findings)]
           if not f.get("lesson") or not f.get("corroborate")]
    check("test_every_detection_names_a_lesson_and_a_corroboration", not bad,
          f"{len(bad)} detecção(ões) incompleta(s)")


# --- 2. PRECISÃO: zero falso positivo em saída limpa ----------------------

_CLEAN = {
    "e2e": '{"overall_score":0.856,"overall_status":"pass","phases":[]}',
    "kpi": '{"decision":{"jev_calls_total":{"by_question":{"kind":1057}}}}',
    "teste": "test result: ok. 17 passed; 0 failed; 0 ignored; 601 filtered out;",
    "qualidade": "  ✓ F4.3: 1.000 F4.3: no deprecated-stdlib imports detected",
    "ls": "FINAL-REPORT.md\nplan.md\nlog.md",
    "sha": "deb107937948021a6ce33caffc10ea14b722f6ae72f69b041401f6c9d75bf3f1",
    "git": "36 arquivos, 21198 linhas adicionadas",
    "find": "deadcode.rs:142: warning: method is never used",
}


def test_clean_output_is_not_flagged() -> None:
    """[F] O teste que MATA guards. Falso positivo > 0,05 em duas semanas e o
    guard é desligado — porque treina o agente a ignorar a camada seguinte."""
    flagged = [k for k, v in _CLEAN.items() if eg.scan_output(v).state != "clean"]
    check("test_clean_output_is_not_flagged", not flagged,
          f"falsos positivos: {flagged or 'nenhum'}")


# --- 3. FAIL-CLOSED: o detector sobre si mesmo ----------------------------

def test_it_never_atests_what_it_does_not_know() -> None:
    """[F] O princípio herdado da lição que este código executa: 'pergunte o
    que ele responde quando não sabe. Se for o valor permissivo, inverta.'"""
    states = {n: eg.scan_output(t).state
              for n, t in (("vazio", ""), ("none", None), ("nao-str", 12345))}
    check("test_it_never_atests_what_it_does_not_know",
          all(s == "unmeasured" for s in states.values()), str(states))


def test_the_three_valued_contract_exists() -> None:
    """`clean` exige evidência positiva; nunca sai por ausência de match."""
    v = eg.scan_output("conteudo qualquer sem assinatura conhecida")
    check("test_the_three_valued_contract_exists",
          v.state in ("clean", "suspect", "degraded") and v.state != "unmeasured",
          f"sem assinatura -> {v.state}")


# --- 4. O CAMINHO REAL DO HOOK --------------------------------------------

def test_the_hook_path_keeps_the_whole_envelope() -> None:
    """[F] Regressão do erro nº 11: `_extract_text` descia até `stdout` e
    descartava as flags IRMÃS do envelope — onde a degradação é declarada.
    O self-test continuava verde porque o corpus não tinha o formato de fio.
    Este teste usa o payload que o hook recebe DE VERDADE."""
    payload = {"tool_input": {"command": "touring run --code ..."},
               "tool_response": {"exit_code": 0, "stdout": '{"ok":1}',
                                 "forbidden_calls": ["subprocess.run (subprocess)"]}}
    v = eg.scan_output(eg._extract_text(payload))
    check("test_the_hook_path_keeps_the_whole_envelope",
          v.state == "degraded" and any("forbidden_calls" in f["matched"] for f in v.findings),
          f"estado={v.state}, {len(v.findings)} detecção(ões)")


def test_the_hook_fails_open_but_the_detector_fails_closed() -> None:
    """[F] As duas propriedades são diferentes e ambas deliberadas: o gancho
    NUNCA trava a sessão; o detector NUNCA atesta o que não sabe."""
    p = subprocess.run([sys.executable, str(GUARD), "--stdin"],
                       input="nao-e-json", capture_output=True, text=True, timeout=10, check=False)
    check("test_the_hook_fails_open_but_the_detector_fails_closed",
          p.returncode == 0, f"exit={p.returncode} (0 = fail-open no gancho)")


def test_clean_payload_injects_nothing() -> None:
    """Se um payload limpo produzisse texto, todo Bash da sessão pagaria um
    ruído — e o guard seria desligado na primeira semana."""
    payload = json.dumps({"tool_input": {"command": "git status"},
                          "tool_response": {"stdout": "nothing to commit"}})
    p = subprocess.run([sys.executable, str(GUARD), "--stdin"],
                       input=payload, capture_output=True, text=True, timeout=10, check=False)
    check("test_clean_payload_injects_nothing", p.stdout.strip() == "",
          f"{len(p.stdout)} bytes injetados (esperado 0)")


def main() -> int:
    print(f"guard: {GUARD}\nrepertório: {len(eg.RULES)} assinaturas compiladas de lições\n")
    for test in (
        test_the_real_error_corpus_is_caught,
        test_every_detection_names_a_lesson_and_a_corroboration,
        test_clean_output_is_not_flagged,
        test_it_never_atests_what_it_does_not_know,
        test_the_three_valued_contract_exists,
        test_the_hook_path_keeps_the_whole_envelope,
        test_the_hook_fails_open_but_the_detector_fails_closed,
        test_clean_payload_injects_nothing,
    ):
        test()
    print(f"\n{len(FAILURES)} falha(s): {FAILURES}" if FAILURES
          else "\nTODOS OS TESTES PASSARAM")
    return 1 if FAILURES else 0


if __name__ == "__main__":
    sys.exit(main())
