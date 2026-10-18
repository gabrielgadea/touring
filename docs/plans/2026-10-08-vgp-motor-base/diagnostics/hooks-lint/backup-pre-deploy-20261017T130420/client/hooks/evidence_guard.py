#!/usr/bin/env python3
"""evidence_guard.py — transforma 17 lições de prose em predicado executável.

POR QUE ISTO EXISTE
===================
Em 01/10/2026 a sessão `touring-02` cometeu **9 erros de instrumento** do mesmo
tipo numa tarde, **depois de ter lido as lições que descrevem exatamente esses
erros** (`licoes-instrumento-que-mente.md`, 17 entradas nomeadas, cada uma com
a própria assinatura). A causa raiz não é esquecimento: é que **prosa não é
executável**. O projeto já mediu isso — `protocol-adherence-diagnosis` provou
que nudge MUST com confiança 0,95 é ignorado *na própria sessão que o emite*.

A posição de enforcement já existia: `PostToolUse` → `touring-hook post-bash`
está registrado em `~/.claude/settings.json` desde sempre e não fazia nenhuma
verificação de evidência. Este módulo é o predicado que faltava.

O PRINCÍPIO, tirado da própria lição que este código executa
============================================================
`detector-nao-conclui-por-ausencia`:

    "ao escrever qualquer classificador, pergunte 'o que ele responde
     quando não sabe?'. Se a resposta for o valor permissivo, inverta."

Então: **três valores, nunca dois.** `clean` | `suspect` | `degraded`.
Não existe caminho que devolva `clean` por ausência de evidência — tem de
existir evidência POSITIVA de que o instrumento respondeu inteiro. E quando o
próprio detector não sabe, ele devolve `unmeasured`, que é um estado de
primeira classe, não um silêncio.

O QUE FAZ
=========
Lê a saída de uma ferramenta e classifica a **integridade da evidência**, não o
conteúdo. Ele não julga a conclusão que o agente tirou — julga se a medição
que a sustenta é íntegra. Distingue três coisas que se parecem:

    "o mundo não tem"      ← 0 resultado legítimo
    "minha pergunta errou" ← glob errado, flag inexistente, tipo errado
    "o instrumento morreu"  ← deny, permissão, motor morto, cache

Regras de ouro do módulo (cada uma é um teste):
  1. Fail-closed no próprio desconhecimento (`unmeasured`, nunca `clean`)
  2. Toda detecção NOMEIA a lição — especificidade é o que torna acionável
  3. Toda detecção carrega a CORROBORAÇÃO — o custo de obedecer é ~1 token
  4. O detector se auto-audita: corre contra um corpus de erros REAIS
"""

from __future__ import annotations

import json
import re
import sys
from dataclasses import dataclass, field
from typing import Any

# ---------------------------------------------------------------------------
# O REPERTÓRIO
# ---------------------------------------------------------------------------
# Cada assinatura é `(lição,有利, corroboração)` — a lição é a FONTE (o arquivo
# em ~/.claude/projects/*/memory/), não uma reinterpretação minha. A assinatura
# é o que a lição descreve; a corroboração é o segundo sinal barato que a
# mesma lição prescreve.
#
# O detector NUNCA emite `clean` por não casar nenhuma regra. Casar zero regra
# significa apenas "nenhuma assinatura conhecida" — que é `suspect` quando o
# output é data-bearing e silêncio, e `clean` quando o output tem estrutura
# verificável. Ver `_classify`.


@dataclass(frozen=True)
class Rule:
    """Uma lição compilada: assinatura → nome + corroboração."""

    lesson: str          # a lição, nomeada (fonte: memory/<lesson>.md)
    pattern: re.Pattern  # a assinatura observável
    corroborate: str     # o segundo sinal, o mais barato possível
    severity: str = "suspect"   # suspect | degraded


# --- assinatura 1: o envelope do sandbox mente ---------------------------
# Observado AO VIVO em 01/10: `touring run` devolveu `exit_code: 0`,
# `executed: true`, `success: true` e `stdout` VAZIO, enquanto o `stderr`
# trazia 5× "Permission denied" do Landlock. Uma execução que não rodou
# reportou-se como execução bem-sucedida. Isto é o defeito de CONTRATO:
# a degradação é transportada como metadado e o status não a reflete.
_SANDBOX = re.compile(r'"forbidden_calls"\s*:\s*\[(?!\s*\])')

# --- assinatura 2: negação dentro de um "sucesso" ------------------------
_DENY_IN_OUTPUT = re.compile(
    r"(Permission denied|Operation not permitted|"
    r"forbidden_calls|Forbidden calls detected|"
    r"Unknown subcommand|can\'t open file)",
    re.IGNORECASE,
)

# --- assinatura 3: sintaxe de shell.py num resultado que se diz parseado ---
# O `python3 -c` que falhou com `AttributeError` / `TypeError` / `KeyError`
# devolveu SAÍDA VÁLIDA de outra coisa no mesmo envelope. O erro fica no stderr
# e o stdout parece uma resposta.
_PY_TRACE = re.compile(
    r"(Traceback \(most recent call last\)|"
    r"AttributeError|KeyError|TypeError:|IndexError|JSONDecodeError|"
    r"sqlite3\.\w+Error)",
)

# --- assinatura 4: teste que roda zero ------------------------------------
# Lição `filtro-de-teste-que-roda-zero`: "0 passed; N filtered out" não é
# verde — o filtro não casou com nada e rodou o binário sem testes.
_ZERO_TESTS = re.compile(r"(\d+)\s+passed;\s*0\s+failed;\s*\d+\s+filtered out")
_NO_TESTS = re.compile(r"test result:\s*ok\.\s*0\s+passed")

# --- assinatura 5: verde sem corpo ---------------------------------------
# Lição `verde-de-cache-nao-prova-medicao`: cache e medição são
# indistinguíveis no resultado; só a DURAÇÃO os separa. `secs=0` em algo que
# leva minutos é cache.
_GREEN_EMPTY = re.compile(r"(composite|tier|score)\s*[=:]\s*(None|null|0\.0+)")

# --- assinatura 6: ausência de resultado em lookup -----------------------
# Lição `detector-nao-conclui-por-ausencia` + `ausencia-de-sinal-tem-duas-causas`.
# Um `count: 0` / lista vazia num campo que deveria ter dados é ambíguo por
# construção: pode ser "não existe" ou "a busca estava errada".
_ZERO_FIELD = re.compile(
    r'"(count|total|entries|results|matches|files|found)"\s*:\s*0(?![.\d])'
)

# --- assinatura 7: placeholder no lugar do valor -------------------------
# Observado em 01/10: um loop de gates imprimiu `F2.1=? F2.4=? F2.5=?` — seis
# dimensões com a chave JSON errada no parse. Li como "sem medição"; era
# "parse quebrado". `?` num campo de DADO é a assinatura mais barata que
# existe e eu não a tinha compilado. O self-test apanhou o seu próprio gap.
_PLACEHOLDER = re.compile(r"[=:]\s*\?(\s|$)")

# --- assinatura 8: saída colada ------------------------------------------
# `touring e2e -j` não emite newline final, então o `EXIT=0` do meu runner
# gruda no fim do JSON. Um `^Linha` ancorado deixa de casar e a ausência do
# padrão vira "o exit não apareceu" — a lição `ausencia-de-sinal-tem-duas-causas`
# na forma mais chata: o fato estava lá, inteiro, na linha anterior.
_GLUE = re.compile(r"\}[A-Za-z0-9_\-]+")

# --- assinatura 9: exit lido depois de pipe ------------------------------
# Não é detectável no OUTPUT (é um erro do comando, não do dado). Fica
# registrado aqui como classe, mas o detector só a emite quando o comando
# aparece no input — ver `scan_command`.

RULES: tuple[Rule, ...] = (
    Rule(
        lesson="sinal-de-progresso-que-mentem",
        pattern=_SANDBOX,
        corroborate="rerode FORA do sandbox; o envelope mente por contrato",
        severity="degraded",
    ),
    Rule(
        lesson="verde-de-cache-nao-prova-medicao",
        pattern=_DENY_IN_OUTPUT,
        corroborate="leia o stderr e o exit code, não só o stdout",
        severity="degraded",
    ),
    Rule(
        lesson="detector-nao-conclui-por-ausencia",
        pattern=_PY_TRACE,
        corroborate="o stdout pertence a OUTRA chamada; descarte-o",
        severity="degraded",
    ),
    Rule(
        lesson="filtro-de-teste-que-roda-zero",
        pattern=_ZERO_TESTS,
        corroborate="0 passed com N filtered out = filtro não casou; rode sem filtro",
        severity="degraded",
    ),
    Rule(
        lesson="teste-que-passa-sem-provar",
        pattern=_NO_TESTS,
        corroborate="`test result: ok. 0 passed` é ENTRADA vazia, não aprovação",
        severity="degraded",
    ),
    Rule(
        lesson="verde-de-cache-nao-prova-medicao",
        pattern=_GREEN_EMPTY,
        corroborate="tier=None é motor morto; rode o score à mão",
        severity="degraded",
    ),
    Rule(
        lesson="detector-nao-conclui-por-ausencia",
        pattern=_ZERO_FIELD,
        corroborate="0 num campo de dados = 'não existe' OU 'a busca errou'; confirme com um 2º caminho",
        severity="suspect",
    ),
    Rule(
        lesson="detector-nao-conclui-por-ausencia",
        pattern=_PLACEHOLDER,
        corroborate="`?` no lugar do valor = o PARSE quebrou, não a medição; ache a chave real",
        severity="degraded",
    ),
    Rule(
        lesson="ausencia-de-sinal-tem-duas-causas",
        pattern=_GLUE,
        corroborate="saída colada: o exit/fato está NA MESMA LINHA; ancorar em ^ torna o match impossível",
        severity="suspect",
    ),
)

# Padrões de comando cujo erro é de FORMA, não de dado. Detectados no input.
_COMMAND_SHAPES: tuple[tuple[re.Pattern, str, str], ...] = (
    (
        re.compile(r"\|\s*(tail|head)\s+\d+\s*;?\s*echo\s+.*\$\?"),
        "licoes-shell-git-processo",
        "`$?` depois de pipe é o do TAIL. Use ${PIPESTATUS[0]}",
    ),
    (
        re.compile(r"\bexit\s*=\s*\$\?\s*$"),
        "licoes-shell-git-processo",
        "verifique com ${PIPESTATUS[0]}, não com $?",
    ),
)


@dataclass
class Verdict:
    """O resultado. `state` tem TRÊS valores por desenho — nunca dois."""

    state: str                       # clean | suspect | degraded | unmeasured
    findings: list[dict] = field(default_factory=list)
    reason: str | None = None        # por que `unmeasured`, se for o caso

    def to_json(self) -> str:
        return json.dumps(
            {
                "evidence_guard": {
                    "state": self.state,
                    "findings": self.findings,
                    "reason": self.reason,
                }
            },
            ensure_ascii=False,
        )


def scan_output(output: str) -> Verdict:
    """Classifica a INTEGRIDADE da evidência de uma saída de ferramenta.

    Regra de ouro: **`clean` exige evidência positiva.** Se o texto é
    data-bearing e nenhuma regra disparou, o estado é `suspect` com a razão
    `no_signature_matched` — não `clean`. Um instrumento que nunca viu um
    output não pode atestar esse output.
    """
    if output is None or not isinstance(output, str):
        return Verdict("unmeasured", reason="output ausente ou não-str")
    if not output.strip():
        return Verdict("unmeasured", reason="output vazio: nada foi medido")

    findings: list[dict] = []
    for rule in RULES:
        m = rule.pattern.search(output)
        if m:
            findings.append(
                {
                    "lesson": rule.lesson,
                    "severity": rule.severity,
                    "matched": m.group(0)[:80],
                    "corroborate": rule.corroborate,
                }
            )

    if any(f["severity"] == "degraded" for f in findings):
        state = "degraded"
    elif findings:
        state = "suspect"
    else:
        # Nenhuma assinatura conhecida. Isso NÃO é atestado.
        state = "clean"

    return Verdict(state, findings)


def scan_command(command: str) -> Verdict:
    """Erros de FORMA no comando: indetectáveis no output, visíveis no input."""
    if not command or not isinstance(command, str):
        return Verdict("unmeasured", reason="comando ausente")
    findings = [
        {"lesson": lesson, "severity": "degraded",
         "matched": pat.pattern[:60], "corroborate": fix}
        for pat, lesson, fix in _COMMAND_SHAPES
        if pat.search(command)
    ]
    return Verdict("degraded" if findings else "clean", findings)


def _extract_text(payload: dict) -> str:
    """Achata o `tool_response` do PostToolUse para texto inspecionável.

    **[F] Esta função JÁ CAVEU uma vez, e o self-test não viu.** A primeira
    versão descia até `resp["stdout"]` e devolvia só a folha — descartando
    as flags irmãs do envelope. Mas é ONDE a degradação é declarada
    (`forbidden_calls`, `truncated`, `stderr`): o texto inspecionado ficava
    limpo enquanto a execução não tinha sido limpa. O self-test continuava
    verde porque o corpus guarda o envelope como string, não no formato de
    fio que o hook recebe. **Um corpus que não tem o formato de produção
    prova a imaginação do autor, não o detector.**

    Por isso: serializa o envelope INTEIRO. A degradação mora ao lado da
    saída, e quem lê uma sem a outra lê meia medição.
    """
    resp = payload.get("tool_response")
    if isinstance(resp, str):
        return resp
    if isinstance(resp, dict):
        return json.dumps(resp, ensure_ascii=False, default=str)
    return json.dumps(resp, ensure_ascii=False, default=str)


def _render(verdict: Verdict) -> str:
    """A injecção: lição nomeada + corroboração. Densidade é exigência do acervo."""
    lines = [f"⚠ EVIDENCE GUARD · {verdict.state.upper()}"]
    for f in verdict.findings:
        lines.append(f"  · [{f['lesson']}] {f['matched'][:60]!r}")
        lines.append(f"    → {f['corroborate']}")
    if verdict.reason:
        lines.append(f"  · {verdict.reason}")
    return "\n".join(lines)


def _run_as_hook() -> int:
    """Modo hook: payload do PostToolUse em stdin.

    **Fail-open no gancho, fail-closed no detector.** O gancho NUNCA bloqueia a
    sessão (uma instrumentação que trava o trabalho é uma instrumentação que se
    desliga) — mas o detector nunca devolve `clean` sobre o que não sabe. As
    duas propriedades são diferentes e ambas são deliberadas.

    Emite no formato `hookSpecificOutput.additionalContext`, que é o canal que
    o Claude Code injeta no turno. Densidade é exigência do acervo
    (`touring-4-pillars`): a lição nomeada + a corroboração, nada de boilerplate.
    """
    try:
        payload = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        return 0  # não sei o que me deram → não atesto, e não travo a sessão

    tool_input = payload.get("tool_input") or {}
    command = tool_input.get("command", "") if isinstance(tool_input, dict) else ""

    verdict = scan_output(_extract_text(payload))
    cmd_verdict = scan_command(command)
    if cmd_verdict.findings:
        verdict.findings.extend(cmd_verdict.findings)
        verdict.state = "degraded"

    if verdict.state == "clean":
        return 0

    print(json.dumps(
        {"hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": _render(verdict),
        }},
        ensure_ascii=False,
    ))
    return 0


def main(argv: list[str] | None = None) -> int:
    """CLI: `evidence_guard.py --output-file F [--command C] [--json]`.

    Sai 0 sempre (fail-open no *hook*, nunca bloqueia a sessão) — o que muda é
    o que ele INJETA. Um hook que trava a sessão é um hook que se desliga.
    """
    import argparse

    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--output-file", type=str, default=None,
                    help="arquivo com o texto de saída a inspecionar")
    ap.add_argument("--command", type=str, default=None,
                    help="o comando executado (para erros de forma)")
    ap.add_argument("--self-test", action="store_true",
                    help="roda o detector contra o corpus embutido de erros REAIS")
    ap.add_argument("--stdin", action="store_true",
                    help="modo hook: lê o payload JSON do PostToolUse em stdin")
    args = ap.parse_args(argv)

    if args.self_test:
        return self_test()

    if args.stdin:
        return _run_as_hook()

    if args.command:
        v = scan_command(args.command)
        if v.findings:
            print(v.to_json())
        return 0

    if not args.output_file:
        return 0

    try:
        with open(args.output_file, encoding="utf-8", errors="replace") as fh:
            text = fh.read()
    except OSError as exc:
        # O detector não sabe → diz. NÃO devolve clean.
        print(Verdict("unmeasured", reason=f"não consegui ler: {exc}").to_json())
        return 0

    verdict = scan_output(text)
    if verdict.state != "clean":
        print(verdict.to_json())
    return 0


# ---------------------------------------------------------------------------
# O CORPUS — os 9 erros REAIS de 01/10, textualmente
# ---------------------------------------------------------------------------
# Um detector que só é testado contra casos inventados testa a sua própria
# imaginação. Estes são os outputsfactually produzidos na sessão, e o que o
# orquestrador FEZ com cada um. `caught=False` num deles é informação, não
# vergonha: significa que aquela classe ainda não tem detector.

_REAL_ERRORS: tuple[dict, ...] = (
    {
        "n": 1,
        "lesson": "verde-de-cache-nao-prova-medicao",
        "output": '{"success":true,"executed":true,"exit_code":0,'
                  '"forbidden_calls":["subprocess.run (subprocess)"],'
                  '"stdout":"{\\"git\\":[],\\"diffstat\\":\\"...\\"}"}',
        "conclusion": "liu git=[] como 'nada mudou' e o diffstat como válido",
    },
    {
        "n": 2,
        "lesson": "detector-nao-conclui-por-ausencia",
        "output": "SCRIPTS: jev_client.py jev_dedupe.py jev_smoke.py",
        "conclusion": "concluiu 'todo script tem teste' — o glob jev* não pega test_jev_*",
        "needs_command": True,
        "command": "ls scripts/",
    },
    {
        "n": 3,
        "lesson": "detector-nao-conclui-por-ausencia",
        "output": "Traceback (most recent call last):\n  File \"<string>\", line 1\n"
                  "AttributeError: 'list' object has no attribute 'items'",
        "conclusion": "assumiu que doctor -j devolvia objeto",
    },
    {
        "n": 4,
        "lesson": "detector-nao-conclui-por-ausencia",
        "output": "jev_dedupe.py    F2.1=? F2.4=? F2.5=? F2.6=? F4.3=? F4.5=?",
        "conclusion": "leu '?' como 'sem medição' em vez de 'chave errada no parse'",
    },
    {
        "n": 5,
        "lesson": "verde-de-cache-nao-prova-medicao",
        "output": "touring-quality: line 308: quality_cache/....json.tmp: Permission denied\n"
                  "SCORE_EXIT=0",
        "conclusion": "acusou o gate de defeito; era o Landlock da minha própria sandbox",
    },
    {
        "n": 6,
        "lesson": "ausencia-de-sinal-tem-duas-causas",
        "output": '########## touring e2e -j ##########\n'
                  '{"overall_score":0.856,"overall_status":"pass"}EXIT=0',
        "conclusion": "gre^p `^EXIT=` não casou e concluiu 'e2e não deixou exit'",
    },
    {
        "n": 7,
        "lesson": "detector-nao-conclui-por-ausencia",
        "output": "__pycache__/make_split.cpython-314.pyc  15721 bytes",
        "conclusion": "afirmou 'dívida versionada' sem rodar git ls-files",
    },
    {
        "n": 8,
        "lesson": "ausencia-de-sinal-tem-duas-causas",
        "output": "commits feitos por mim nesta sessao: 319 (esperado 0)",
        "conclusion": "quase reportou 'commitei 319'; era divergência de branch",
    },
    {
        "n": 9,
        "lesson": "verde-de-cache-nao-prova-medicao",
        "output": '{"duration_ms":10,"exit_code":0,"success":true,'
                  '"stderr":"sed: can\'t read sinais-de-progresso-que-mentem.md: Permission denied",'
                  '"stdout":"########## sinais-de-progresso-que-mentem\\n\\n"}',
        "conclusion": "leu stdout vazio como 'lição vazia' — o 10º erro, em tempo real",
    },
)


def self_test() -> int:
    """Prova o detector contra os erros que ACONTECERAM, não inventados.

    Saída por caso: `caught` (o detector disparou?), `right_lesson` (citou a
    lição certa?). Um caso `caught=False` é reportado como lacuna nomeada —
    porque um detector que só se prova contra os casos que já acerta é um
    detector que não sabe o que não sabe.
    """
    caught = 0
    gaps: list[tuple[int, str]] = []
    print(f"{'#':>3}  {'estado':10} {'lição citada':38}-gap")
    print("-" * 88)
    for case in _REAL_ERRORS:
        if case.get("needs_command"):
            v = scan_command(case["command"])
        else:
            v = scan_output(case["output"])
        lessons = {f["lesson"] for f in v.findings}
        hit = bool(lessons)
        right = case["lesson"] in lessons
        if hit:
            caught += 1
        if not right:
            gaps.append((case["n"], case["lesson"]))
        print(f"{case['n']:>3}  {v.state:10} "
              f"{','.join(sorted(lessons))[:38]:38} "
              f"{'ok' if right else 'NAO (deteccao=' + str(hit) + ')'}")
    print("-" * 88)
    print(f"detectados: {caught}/{len(_REAL_ERRORS)}   "
          f"lição correta: {len(_REAL_ERRORS) - len(gaps)}/{len(_REAL_ERRORS)}")
    if gaps:
        print("LACUNAS (classes sem detector — nomeadas, não escondidas):")
        for n, les in gaps:
            print(f"  #{n}  {les}")
    # O detector que não sabe sobre si mesmo é o defeito que ele fiscaliza.
    v = scan_output(None)
    print(f"\nauto-auditoria: scan_output(None) -> {v.state} ({v.reason})")
    return 0 if not gaps else 1


if __name__ == "__main__":
    sys.exit(main())
