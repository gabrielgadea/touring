#!/usr/bin/env python3
"""Testes do interruptor de injeção de contexto (`TOURING_HOOK_INJECTION`).

Cobre a política (níveis, classificação de família) e o filtro (o que sai,
o que fica), mais uma prova de ponta a ponta do shim com um binário falso que
emite contexto E decisão.

Executar:  python3 scripts/test_injection_policy.py
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
HOOKS = os.path.join(REPO, "client", "hooks")
SHIM = os.path.join(REPO, "scripts", "hooks", "touring-hook-shim.sh")

sys.path.insert(0, HOOKS)
import injection_policy as ip  # noqa: E402

FAILURES = []


def check(name, cond, detail=""):
    if cond:
        print("  PASS  %s" % name)
    else:
        print("  FAIL  %s  %s" % (name, detail))
        FAILURES.append(name)


def ctx(payload):
    """Resposta de hook no formato do protocolo: contexto + decisão."""
    return json.dumps(
        {
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "additionalContext": payload,
                "permissionDecision": "deny",
                "permissionDecisionReason": "motivo que precisa sobreviver",
            }
        }
    )


SUGGEST = "[TOURING SUGGEST · pre-edit-triage-rust · conf=0.89]\nMUST touring ast meta X"
GATE = "[CODE MODE · rajada] aviso — rode o lote de uma vez: touring run --lang bash --code '...'"
FACT = "HOTSPOT: `x` has 6 callers | callers: [a, b, c]"
PLUGIN = "🌅 ATZILUTH — Painel de Emanação"


def test_levels():
    print("\n[níveis]")
    old = os.environ.pop(ip.ENV_VAR, None)
    check("default é full", ip.policy() == "full")
    os.environ[ip.ENV_VAR] = "off"
    check("env off", ip.policy() == "off")
    os.environ[ip.ENV_VAR] = "OFF"
    check("env é case-insensitive", ip.policy() == "off")
    os.environ[ip.ENV_VAR] = "lixo"
    check("valor inválido cai em full", ip.policy() == "full")
    if old is None:
        os.environ.pop(ip.ENV_VAR, None)
    else:
        os.environ[ip.ENV_VAR] = old


def test_classify():
    print("\n[classificação de família]")
    check("touring-suggest", ip.classify(SUGGEST) == "touring-suggest")
    check("code-mode-deny", ip.classify(GATE) == "code-mode-deny")
    check("raw-stdout (desconhecido)", ip.classify("blablabla") == "raw-stdout")
    # O dashboard é `session-start` — o nome canônico da régua (S-3).
    check("session-start (dashboard)", ip.classify(PLUGIN) == "session-start")
    check("vazio", ip.classify("") == "empty")


def test_allows():
    print("\n[allows por nível]")
    os.environ[ip.ENV_VAR] = "full"
    check("full libera tudo", ip.allows("touring-suggest") and ip.allows("code-mode-deny"))
    os.environ[ip.ENV_VAR] = "no-advisory"
    check("no-advisory corta o sermão", not ip.allows("touring-suggest"))
    check("no-advisory mantém o gate", ip.allows("code-mode-deny"))
    check("no-advisory mantém o fato", ip.allows("pre-edit"))
    os.environ[ip.ENV_VAR] = "off"
    check("off corta tudo", not ip.allows("touring-suggest") and not ip.allows("code-mode-deny"))
    os.environ.pop(ip.ENV_VAR, None)


def test_filter():
    print("\n[filtro]")
    os.environ[ip.ENV_VAR] = "full"
    original = ctx(SUGGEST)
    check("full não mexe", ip.filter_output("full", original) == original)

    out = json.loads(ip.filter_output("no-advisory", ctx(SUGGEST)))
    hso = out.get("hookSpecificOutput", {})
    check("no-advisory remove o contexto do sermão", "additionalContext" not in hso)
    check("no-advisory preserva a decisão", hso.get("permissionDecision") == "deny")
    check(
        "no-advisory preserva o motivo",
        hso.get("permissionDecisionReason") == "motivo que precisa sobreviver",
    )

    out = json.loads(ip.filter_output("no-advisory", ctx(GATE)))
    check(
        "no-advisory preserva o payload do gate",
        out.get("hookSpecificOutput", {}).get("additionalContext") == GATE,
    )

    out = json.loads(ip.filter_output("off", ctx(GATE)))
    hso = out.get("hookSpecificOutput", {})
    check("off remove o contexto do gate", "additionalContext" not in hso)
    check("off preserva a decisão", hso.get("permissionDecision") == "deny")

    out = json.loads(ip.filter_output("off", ctx(FACT)))
    check("off remove o fato", "additionalContext" not in out.get("hookSpecificOutput", {}))

    check("off limpa stdout cru", ip.filter_output("off", "log de debug\n") == "")
    check(
        "no-advisory CONSERVA stdout cru desconhecido (conservador)",
        ip.filter_output("no-advisory", "log de debug\n") == "log de debug\n",
    )
    check(
        "no-advisory limpa stdout cru de depuracao (marcado)",
        ip.filter_output("no-advisory", "[EVENT_READER] Reading hook event\n") == "",
    )
    check(
        "no-advisory preserva stdout cru de família conhecida",
        ip.filter_output("no-advisory", GATE + "\n") == GATE + "\n",
    )

    out = json.loads(ip.filter_output("off", json.dumps({"decision": "block", "reason": "r"})))
    check("off preserva decision/reason", out.get("decision") == "block" and out.get("reason") == "r")

    # Regressão (revisão de segurança, 26/10/2026): `systemMessage` é MENSAGEM,
    # não decisão. Sob `off` tem de sumir; a decisão real permanece.
    out = json.loads(ip.filter_output("off", json.dumps({"systemMessage": "texto"})))
    check("off remove systemMessage", "systemMessage" not in out)
    out = json.loads(ip.filter_output("off", json.dumps({
        "systemMessage": "texto",
        "hookSpecificOutput": {"permissionDecision": "deny", "permissionDecisionReason": "r"},
    })))
    check("off remove systemMessage e preserva a decisao",
          "systemMessage" not in out
          and out.get("hookSpecificOutput", {}).get("permissionDecision") == "deny"
          and out.get("hookSpecificOutput", {}).get("permissionDecisionReason") == "r")
    out = json.loads(ip.filter_output("no-advisory", json.dumps({"systemMessage": SUGGEST})))
    check("no-advisory remove systemMessage de sermao", "systemMessage" not in out)
    out = json.loads(ip.filter_output("no-advisory", json.dumps({"systemMessage": FACT})))
    check("no-advisory preserva systemMessage de fato", "systemMessage" in out)
    check("systemMessage nao e campo de decisao", "systemMessage" not in ip.DECISION_FIELDS)

    # Colisão de nome resolvida (S-3): a taxonomia é CANÔNICA e a régua manda.
    # `session-start` = os DASHBOARDS (Advisory, o nome da régua);
    # `code-mode-contract` = o CONTRATO do code mode (Fact);
    # `session-contract` = o bloco "MUST touring doctor -j" (Fact).
    # TRÊS nomes, TRÊS coisas — nenhum colide.
    check("contrato do code mode é familia de fato",
          "code-mode-contract" in ip.FACT_FAMILIES
          and "code-mode-contract" not in ip.ADVISORY_FAMILIES)
    check("dashboards sao familia advisory (session-start)",
          "session-start" in ip.ADVISORY_FAMILIES
          and "session-start" not in ip.FACT_FAMILIES)
    check("os tres nomes de sessao sao distintos",
          "session-start" != "code-mode-contract" != "session-contract"
          and len({"session-start", "code-mode-contract", "session-contract"}) == 3)
    check("contrato preservado em no-advisory",
          ip.allows_payload("[CODE MODE — contrato da sessao] ...", "no-advisory"))
    check("dashboard cortado em no-advisory",
          not ip.allows_payload("ATZILUTH — Painel de Emanacao ...", "no-advisory"))

    # Falha silenciosa de 26/10/2026: um nome de familia OBSOLETO caia em
    # "permitir" (porque nao estava em ADVISORY_FAMILIES) e o hook seguia
    # injetando — sem nada acusar. Agora o nome desconhecido NAO injeta, e a
    # guarda valida os nomes em CI.
    check("familia desconhecida nao injeta",
          not ip.allows("familia-que-nao-existe", "no-advisory")
          and not ip.allows("familia-que-nao-existe", "full"))
    check("familia desconhecida nao injeta em full tambem",
          not ip.allows("session-start-contract", "full"))

    # ASSIMETRIA DECLARADA (revisao de seguranca, 26/10/2026):
    # `off` e incondicional; `no-advisory` deixa passar o que nao reconhece.
    check("off e incondicional (payload sem familia tambem sai)",
          not ip.allows_payload("texto sem nenhum marcador", "off"))
    check("no-advisory deixa passar o que nao reconhece (decisao medida)",
          ip.allows_payload("texto sem nenhum marcador", "no-advisory"))
    check("no-advisory corta o log de depuracao (marcador proprio)",
          not ip.allows_payload("[EVENT_READER] Reading hook event", "no-advisory"))
    check("no-advisory preserva o enriquecimento (marcador de fato)",
          ip.allows_payload("HOTSPOT: `x` has 6 callers | callers: [a]", "no-advisory"))

    # Payload MISTO: `verdict.rs:37` (`context_with`) compoe os avisos primeiro e
    # o contexto do veredito depois, num texto so. O fato que veio de carona
    # nao pode ser descartado junto com o sermao.
    MIXED = SUGGEST + "\n" + FACT
    out = json.loads(ip.filter_output("no-advisory", ctx(MIXED)))
    check(
        "no-advisory preserva payload misto (fato de carona)",
        out.get("hookSpecificOutput", {}).get("additionalContext") == MIXED,
    )
    out = json.loads(ip.filter_output("off", ctx(MIXED)))
    check("off remove payload misto", "additionalContext" not in out.get("hookSpecificOutput", {}))

    # Um payload que so tem sermao e sermao: sai.
    PURE_ADVICE = SUGGEST + "\n" + "[delegation-nudge] 3 leituras — delegue"
    out = json.loads(ip.filter_output("no-advisory", ctx(PURE_ADVICE)))
    check("no-advisory remove sermao puro", "additionalContext" not in out.get("hookSpecificOutput", {}))

    # Varias familias detectadas no mesmo payload.
    check(
        "families() ve varias familias",
        ip.families(MIXED) == {"touring-suggest", "pre-edit"},
        str(ip.families(MIXED)),
    )
    check(
        "payload desconhecido e conservado em no-advisory",
        ip.allows_payload("texto sem marcador reconhecivel", "no-advisory"),
    )
    check(
        "log de depuracao cru e suprimido em no-advisory",
        not ip.allows_payload("[EVENT_READER] Reading hook event", "no-advisory"),
    )
    check(
        "enriquecimento e preservado em no-advisory",
        ip.allows_payload(FACT, "no-advisory"),
    )


def test_shim_end_to_end():
    print("\n[shim de ponta a ponta]")
    if not os.path.exists(SHIM):
        check("shim existe", False, SHIM)
        return
    with tempfile.TemporaryDirectory() as tmp:
        fake = os.path.join(tmp, "fake-hook")
        with open(fake, "w", encoding="utf-8") as fh:
            fh.write(
                "#!/usr/bin/env bash\n"
                "cat <<'JSON'\n"
                + ctx(SUGGEST)
                + "\nJSON\n"
                "exit 0\n"
            )
        os.chmod(fake, 0o755)

        def run(policy):
            env = dict(os.environ)
            env["TOURING_HOOK_SHIM_FORCE_BIN"] = fake
            if policy is None:
                env.pop(ip.ENV_VAR, None)
            else:
                env[ip.ENV_VAR] = policy
            proc = subprocess.run(
                ["bash", SHIM, "pre-edit"],
                capture_output=True,
                text=True,
                env=env,
                timeout=30,
            )
            return proc

        proc = run(None)
        check("full: contexto passa", "additionalContext" in proc.stdout)
        check("full: exit 0", proc.returncode == 0)

        proc = run("no-advisory")
        check("no-advisory: contexto do sermão sai", "additionalContext" not in proc.stdout)
        check("no-advisory: decisão permanece", "permissionDecision" in proc.stdout)
        check("no-advisory: motivo permanece", "motivo que precisa sobreviver" in proc.stdout)
        check("no-advisory: exit 0", proc.returncode == 0)

        proc = run("off")
        check("off: contexto sai", "additionalContext" not in proc.stdout)
        check("off: decisão permanece", "permissionDecision" in proc.stdout)

        # exit code do hook precisa atravessar a política
        failing = os.path.join(tmp, "failing-hook")
        with open(failing, "w", encoding="utf-8") as fh:
            fh.write("#!/usr/bin/env bash\necho '{\"decision\":\"block\",\"reason\":\"r\"}'\nexit 2\n")
        os.chmod(failing, 0o755)
        env = dict(os.environ)
        env["TOURING_HOOK_SHIM_FORCE_BIN"] = failing
        env[ip.ENV_VAR] = "off"
        proc = subprocess.run(
            ["bash", SHIM, "pre-edit"], capture_output=True, text=True, env=env, timeout=30
        )
        check("off: exit code do hook atravessa", proc.returncode == 2, str(proc.returncode))
        check("off: decisão do hook permanece", "block" in proc.stdout)


def test_round34_families():
    """Famílias descobertas na rodada 3/4 (26/10/2026) e unificadas na S-3.

    O furo que estas asserções cobrem: a taxonomia casava por CABEÇALHO, e o
    caminho de "orçamento de turno esgotado" emite a mesma diretiva sem
    cabeçalho. O payload caía em "desconhecido" e — pela assimetria
    conservadora de `no-advisory` — PASSAVA. Medido: `no-advisory` cortava
    13,6% do contexto do Touring; com a taxonomia por conteúdo, 19,9% — e
    o que sobra é o canal de FATO, que é o que tem 52% de conformidade.

    S-3: a taxonomia passou a ser CANÔNICA (a régua manda) e o ESPELHO ficou
    aqui — mesmos nomes, mesma ordem. Onde os nomes divergiam, o da régua
    venceu: `similar-fail` → `past-lessons`, `compact-warning` →
    `context-monitor`, `session-start-contract` → `code-mode-contract`, e os
    dashboards saíram de `plugin-hook` para `session-start`.
    """
    print("\n[rodada 3/4 + S-3: famílias canônicas]")
    headless = "  MUST    touring ast meta /tmp/x.rs --depth summary -j"
    check("sermão sem cabeçalho é touring-suggest",
          "touring-suggest" in ip.families(headless))
    check("sermão sem cabeçalho é cortado em no-advisory",
          not ip.allows_payload(headless, "no-advisory"))
    check("sermão sem cabeçalho é preservado em full",
          ip.allows_payload(headless, "full"))

    elided = ("  MUST    touring run --lang bash --code 'ls -la'\n"
              "  (orçamento do turno esgotado — só as diretivas)")
    check("o payload do orçamento esgotado é touring-suggest",
          "touring-suggest" in ip.families(elided))
    check("o payload do orçamento esgotado é cortado em no-advisory",
          not ip.allows_payload(elided, "no-advisory"))

    similar = "Similar command failed recently (similarity=68%)"
    check("past-lessons é família conhecida", "past-lessons" in ip.families(similar))
    check("past-lessons é sermão (alerta sem alvo)", "past-lessons" in ip.ADVISORY_FAMILIES)
    check("past-lessons é cortado em no-advisory",
          not ip.allows_payload(similar, "no-advisory"))

    check("generic-noise é família conhecida", "generic-noise" in ip.families("[generic]"))
    check("generic-noise é cortado em no-advisory",
          not ip.allows_payload("[generic]", "no-advisory"))

    contract = "[CODE MODE — contrato desta sessão · apresentação `code`]"
    check("code-mode-contract é família conhecida",
          "code-mode-contract" in ip.families(contract))
    check("code-mode-contract é fato (preservado)",
          ip.allows_payload(contract, "no-advisory"))

    session = "  MUST    touring doctor -j"
    check("session-contract é família conhecida",
          "session-contract" in ip.families(session))
    check("session-contract é fato (preservado)",
          ip.allows_payload(session, "no-advisory"))

    gaps = "[COMPACTAÇÃO · lacunas do resumo] o resumo não cita o que segue"
    check("compaction-gaps é família conhecida",
          "compaction-gaps" in ip.families(gaps))
    check("compaction-gaps é fato (preservado)",
          ip.allows_payload(gaps, "no-advisory"))

    gate = "[G3 edit-sem-read] Edit de /tmp/x.rs"
    check("gate [G<n>] é code-mode-deny", "code-mode-deny" in ip.families(gate))
    check("gate [G<n>] é preservado", ip.allows_payload(gate, "no-advisory"))

    echo = '⚡ last fail: python3 — sqlite3.OperationalError: no such column'
    check("failure-echo é família conhecida", "failure-echo" in ip.families(echo))
    check("failure-echo é fato (preservado)", ip.allows_payload(echo, "no-advisory"))

    pipefail = ("`python3` exited 0, but its output shows errors after a pipe "
                "without `set -o pipefail` — the real exit code may be masked.")
    check("pipefail-evidence é família conhecida",
          "pipefail-evidence" in ip.families(pipefail))
    check("pipefail-evidence é fato (preservado)",
          ip.allows_payload(pipefail, "no-advisory"))

    warning = "Contexto em 63% — a autocompactação dispara aos 65%."
    check("context-monitor é família conhecida",
          "context-monitor" in ip.families(warning))
    check("context-monitor é fato (preservado)",
          ip.allows_payload(warning, "no-advisory"))

    # Três nomes com "session/contract" são três coisas — a lição do repo
    # ("predicado escrito duas vezes nunca se encontra") já cobrou o preço.
    check("os três nomes de sessão são distintos",
          "session-start" in ip.ALL_FAMILIES
          and "code-mode-contract" in ip.ALL_FAMILIES
          and "session-contract" in ip.ALL_FAMILIES
          and len({"session-start", "code-mode-contract", "session-contract"}) == 3)
    check("nenhum nome canônico é obsoleto (todos em ALL_FAMILIES)",
          all(f in ip.ALL_FAMILIES for f in (
              "past-lessons", "generic-noise", "code-mode-contract",
              "session-contract", "compaction-gaps", "context-monitor",
              "pipefail-evidence", "delegation-nudge", "failure-echo",
              "quality-gate", "session-start")))
    check("as novas famílias de sermão são Advisory",
          {"past-lessons", "generic-noise", "delegation-nudge", "session-start"}
          <= ip.ADVISORY_FAMILIES)
    check("os nomes antigos da rodada 3/4 saíram do espelho",
          not ({"similar-fail", "session-start-contract", "compact-warning"}
               & ip.ALL_FAMILIES),
          "um nome obsoleto em ALL_FAMILIES faria o call site injetar sem lote")


def main():
    test_levels()
    test_classify()
    test_allows()
    test_filter()
    test_round34_families()
    test_shim_end_to_end()
    print()
    if FAILURES:
        print("FALHOU: %d" % len(FAILURES))
        for f in FAILURES:
            print("  - %s" % f)
        return 1
    print("TODOS OS TESTES PASSARAM")
    return 0


if __name__ == "__main__":
    sys.exit(main())
