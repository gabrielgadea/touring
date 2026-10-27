#!/usr/bin/env python3
"""Política de injeção de contexto dos hooks — fonte única da verdade.

RESUMO
------
O corpus de 1.028 sessões (21 dias, 440.867 invocações de hook) mostrou que
a injeção de contexto responde por 27,8% dos bytes da janela do modelo e
que o perfil dela é misto: parte carrega FATOS sobre o código (blast radius,
file context, syntax pre-check) e parte carrega SERMÃO (nudges, eco de
pós-tool, log de depuração). Desligar tudo junto perderia os fatos; deixar
como está paga o sermão.

Este módulo define o interruptor único `TOURING_HOOK_INJECTION`:

    full         (padrão) — nada é suprimido; comportamento idêntico ao atual
    no-advisory  — suprime só as famílias de sermão; mantém fatos e gates
    off          — suprime todo o contexto injetado; mantém TODAS as decisões

CONTRATO DE ENFORCEMENT (o que este módulo NÃO toca)
-----------------------------------------------------
A injeção de contexto e o enforcement são canais separados do protocolo de
hook. As decisões vivem em `permissionDecision`, `permissionDecisionReason`,
`decision`, `reason` e no exit code; o contexto vive em
`hookSpecificOutput.additionalContext`, `systemMessage` (raiz) e
`hookSpecificOutput.systemMessage`.
Este módulo remove SOMENTE os campos de contexto. Um hook que bloqueia
continua bloqueando, com o mesmo código de saída e a mesma razão.

ASSIMETRIA DECLARADA — o que cada nível garante
-----------------------------------------------
`off`      → remove TUDO, incondicionalmente. Mesmo um payload que nenhuma
              família reconhece sai. **Este nível não tem fail-open.**
`no-advisory` → remove só as famílias comprovadamente de sermão. Um payload
              **não reconhecido passa** — e isso é uma decisão medida, não um
              descuido: o canal de enriquecimento (blast radius, file context,
              syntax pre-check) é o de MAIOR volume e de MAIOR impacto
              (59,9% — §4.6 da análise consolidada) e **não tem marcador
              confiável**; classificá-lo por substring descartaria
              conhecimento do repositório. O log de depuração — que é ruído —
              tem marcador próprio (`debug-stdout`) e é cortado. O preço da
              assimetria: um payload de sermão com marcador fora da taxonomia
              passa. A mitigação é a guarda, que valida os NOMES de família
              usados nos call sites (`scripts/guard_injection_policy.py`).
`full`     → não remove nada.

MEDIÇÃO (não narrativa)
-----------------------
- bytes injetados no corpus: 61.499.972 (21 dias)
- byte-idênticos: 25,1% (entropia condicional zero)
- stdout de depuração cru: 5.316.441 B (9,0%) — família `raw-stdout`
- follow-ratio do canal de sermão: 0,094 sobre 6.000 MUSTs emitidos
- injeção / texto do usuário: 1,30x

Uso
---
    import injection_policy
    if not injection_policy.allows("plugin-hook"):
        sys.exit(0)

    # como filtro de resposta de hook (usado pelo shim):
    python3 injection_policy.py --filter no-advisory < resposta.json
"""
from __future__ import annotations

import json
import os
import re
import sys

ENV_VAR = "TOURING_HOOK_INJECTION"
LEVELS = ("full", "no-advisory", "off")
DEFAULT = "full"

#: Famílias que são SERMÃO: suprimidas em `no-advisory`.
#:
#: `past-lessons` (o aviso de "Similar command failed recently", que **não nomeia
#: o comando que falhou** — logo não tem alvo e o modelo não pode obedecê-lo) e
#: `generic-noise` (o payload literalmente `[generic]`) entraram na rodada 3/4:
#: o corpus mediu **1.676 blocos** e **1.499 blocos** respectivamente. Antes,
#: ambos caíam em "desconhecido" e passavam: o interruptor prometia cortar o
#: sermão e não cortava.
ADVISORY_FAMILIES = frozenset(
    {
        "touring-suggest",
        "prompt-enhance",
        "plugin-hook",
        "post-tool-echo",
        "debug-stdout",
        "past-lessons",
        "generic-noise",
        "delegation-nudge",
        "session-start",
        "pre-bash",
        "prose-integrity",
    }
)

#: Famílias que carregam FATO sobre o código ou PAYLOAD DE GATE.
#: Nenhuma é suprimida em `no-advisory`.
#:
#: TRÊS NOMES COM "session/contract", e são TRÊS COISAS:
#:   `session-start`       — os DASHBOARDS da abertura (o nome da régua);
#:   `code-mode-contract` — o CONTRATO do code mode (`CODE MODE — contrato`);
#:   `session-contract`    — o bloco "MUST touring doctor -j".
#: Nenhum colide: cada nome designa uma coisa só.
FACT_FAMILIES = frozenset(
    {
        "pre-edit",
        "pre-write",
        "pre-read",
        "post-edit",
        "post-write",
        "pre-edit-prevention",
        "code-mode-contract",
        "session-contract",
        "compaction-gaps",
        "context-monitor",
        "pipefail-evidence",
        "evidence-guard",
        "code-mode-deny",
        "loop-outer",
        "failure-echo",
        "quality-gate",
    }
)

#: NOMES DE FAMÍLIA = o CONTRATO entre a régua e a política.
#:
#: A REGUA é a canônica (`crates/touring-cli/src/cli/context_budget.rs`) e esta
#: política é o ESPELHO: as mesmas linhas, a mesma ordem, os mesmos nomes. As
#: duas escreviam o mesmo predicado em lugares diferentes e divergiam em nove
#: famílias — a lição do repo ("predicado escrito duas vezes nunca se encontra")
#: cobrando o preço de novo. O teste `the_python_policy_mirrors_this_taxonomy`
#: (na régua) é o juiz: quando os dois divergem, o build reprova.
#:
#: Onde os nomes divergiam, o da régua venceu (a régua publica `by_family` no
#: KPI e mede em produção): `similar-fail` → `past-lessons`, `compact-warning` →
#: `context-monitor`, `session-start-contract` → `code-mode-contract`, e os
#: dashboards saíram de `plugin-hook` para `session-start`.
#:
#: Marcadores de família, na ordem de teste.
#:
#: Cada família TEM marcador explícito. Um payload sem marcador algum é
#: DESCONHECIDO, e o desconhecido é tratado de forma conservadora — nunca como
#: sermão: só se perde contexto por causa de um aviso que veio de carona, nunca
#: um fato.
FAMILY_MARKERS = (
    ("code-mode-contract", ("CODE MODE — contrato",)),
    ("code-mode-deny", (
        "CODE MODE",
        "G7 re-inspe",
        "G10 write",
        "G1 rajada",
        "G11 ",
        "G3 edit",
    )),
    ("touring-suggest", ("TOURING SUGGEST",)),
    ("loop-outer", ("LOOP RESUME", "work-outer", "explore-ledger", "OUTER ·")),
    ("session-start", (
        "Touring Knowledge",
        "ATZILUTH",
        "SCOUT PERP",
        "TOURING (slim)",
        "Painel",
        "Session intelligence",
        "Touring: collecting session intelligence",
        "Touring: initializing cognitive engine",
    )),
    ("prompt-enhance", ("PROMPT ENHANCEMENT",)),
    ("past-lessons", ("lições de erros passados", "Similar command failed")),
    ("post-tool-echo", ("temporal: reliability", "last fail:", "post-tool-batch")),
    ("evidence-guard", ("EVIDENCE GUARD",)),
    # stdout cru de depuração: o bug que injeta 5,3 MB (9% da injeção) no modelo.
    ("debug-stdout", ("[EVENT_READER]", "[POST_QUALITY_GATE", "Hook execution started")),
    ("plugin-hook", ("[SUGESTÃO DE SKILL",)),
    ("pre-write", ("code-workflow", "rust-workflow")),
    ("pre-edit", ("HOTSPOT:", "wiring_predict:", "callers:", "related docs:")),
    ("pre-read", ("defs[", "📌 defs", "🪝 defs")),
    ("post-edit", ("post-edit verification:", "post-read:")),
    ("post-write", ("post-write:",)),
    ("pre-edit-prevention", ("Pre-edit prevention",)),
    ("delegation-nudge", ("[delegation-nudge]",)),
)

#: Famílias reconhecidas por PADRÃO (regex) — as que não têm cabeçalho
#: próprio, ou cuja forma varia.
#:
#: A rodada 3/4 achou o furo grande: o caminho de **orçamento de turno
#: esgotado** emite a mesma diretiva SEM o cabeçalho `[TOURING SUGGEST …]`.
#: Como a taxonomia casava por cabeçalho, esses payloads eram "desconhecidos"
#: e — pela assimetria conservadora de `no-advisory` — **passavam**. Medido:
#: `no-advisory` cortava **13,6%** do contexto do Touring, não os ~40% que o
#: desenho prometeia.
#:
#: O padrão de sermão sem cabeçalho é uma LINHA que começa por diretiva.
#: Quando a mesma linha é o contrato do code mode ou o contrato de sessão, os
#: padrões específicos abaixo também casam — e como as duas famílias são de
#: FATO, o filtro as preserva (a política decide pelo CONJUNTO).
FAMILY_PATTERNS = (
    ("code-mode-deny", re.compile(r"\[G\d+\s")),
    ("session-contract", re.compile(r"MUST\s+touring doctor -j")),
    ("pipefail-evidence",
     re.compile(r"exited 0, but its output shows errors after a pipe")),
    ("generic-noise", re.compile(r"^\[generic\]")),
    ("context-monitor", re.compile(r"Contexto em \d+%")),
    ("compaction-gaps", re.compile(r"\[COMPACTA[ÇC][ÃA]O")),
    ("delegation-nudge", re.compile(r"\[M3 delega")),
    ("failure-echo", re.compile(r"⚡ last fail:|failed \(exit \d+\):")),
    ("quality-gate", re.compile(r"\[E\d+\s")),
    ("touring-suggest", re.compile(r"(?m)^\s*(?:MUST|SHOULD|MAY)\s+\S")),
)

#: Ordem de teste das famílias: cabeçalalho primeiro, padrões depois.
FAMILY_ORDER = (tuple(name for name, _ in FAMILY_MARKERS)
                + tuple(name for name, _ in FAMILY_PATTERNS))

#: Campos que carregam DECISÃO — nunca removidos por esta política.
#:
#: `systemMessage` NÃO está aqui: ele é uma mensagem, não um veredito. Sob
#: `off` ele é removido como contexto, e sob `no-advisory` segue o regime das
#: demais famílias (só sai se for exclusivamente sermão). A revisão de
#: segurança de 26/10/2026 apontou a confusão, e ela estava certa: tratar
#: `systemMessage` como decisão fazia o nível `off` NÃO desligar tudo.
DECISION_FIELDS = (
    "permissionDecision",
    "permissionDecisionReason",
    "decision",
    "reason",
    "continue",
    "stopReason",
    "suppressOutput",
)

#: Campos que carregam CONTEXTO — sujeitos à política.
CONTEXT_FIELDS = ("additionalContext", "systemMessage")


def policy() -> str:
    """Nível de injeção em vigor. Fonte: env var; default `full`."""
    raw = (os.environ.get(ENV_VAR) or "").strip().lower()
    if raw in LEVELS:
        return raw
    return DEFAULT


def classify(payload: str) -> str:
    """Família primária de um payload de injeção, pela taxonomia da régua."""
    if not payload:
        return "empty"
    found = families(payload)
    for family in FAMILY_ORDER:
        if family in found:
            return family
    return "raw-stdout"


def families(payload: str) -> set:
    """TODAS as famílias presentes num payload.

    Um payload pode misturar advisory e fato: `verdict.rs:37` (`context_with`)
    compoe os avisos primeiro e o contexto do veredito depois, num texto só.
    Julgar o payload inteiro pela família primária descartaria o fato que veio
    de carona. Por isso o filtro decide pelo CONJUNTO — e `no-advisory` só
    suprime o que é **exclusivamente** sermão.
    """
    if not payload:
        return set()
    found = set()
    for family, markers in FAMILY_MARKERS:
        for marker in markers:
            if marker in payload:
                found.add(family)
                break
    for family, pattern in FAMILY_PATTERNS:
        if pattern.search(payload):
            found.add(family)
    return found


#: Todas as famílias conhecidas — a união das duas listas.
ALL_FAMILIES = ADVISORY_FAMILIES | FACT_FAMILIES


def allows(family: str, level: str | None = None) -> bool:
    """O contexto desta família deve chegar ao modelo?

    `level` explícito vence o ambiente; sem ele, resolve do ambiente.

    **Família DESCONHECIDA devolve False (não injeta).** Isso é deliberado e
    é a correção de uma falha silenciosa encontrada em 26/10/2026: o rename de
    `session-start` para `session-start-contract` deixou call sites com um
    nome inexistente, e um nome inexistente caía em "permitir" — os
    dashboards seguiam sendo injetados e **nada acusava**. Falhar fechado na
    injeção (e aberto na sessão, porque isto aqui não levanta exceção) troca
    "injeta por engano" por "não injeta"; e `scripts/guard_injection_policy.py`
    valida os nomes usados nos call sites, de modo que o erro aparece em CI,
    antes do runtime. O rename de 26/10/2026 (S-3) levou o contrato de
    `session-start-contract` para `code-mode-contract` — o nome canônico da
    régua — e o guard é o que acusa o call site se ele ficar para trás.
    """
    if family not in ALL_FAMILIES:
        return False
    lvl = level if level is not None else policy()
    if lvl == "full":
        return True
    if lvl == "no-advisory":
        return family not in ADVISORY_FAMILIES
    return False  # off


def allows_payload(payload: str, level: str | None = None) -> bool:
    """Este payload inteiro deve chegar ao modelo?

    Conservative por construcao: em `no-advisory` o payload so e suprimido
    quando NAO contem nenhuma familia de fato/gate. Um payload que misture
    sermao e fato e preservado inteiro — nunca se perde conhecimento do
    repositorio por causa de um aviso que veio de carona.
    """
    lvl = level if level is not None else policy()
    if lvl == "full":
        return True
    if lvl == "off":
        return False
    found = families(payload)
    if not found:
        # DESCONHECIDO: nao se descarta. Preferimos deixar passar um payload
        # que nao reconhecemos a descartar um fato do repositorio. O log de
        # depuracao cru tem marcador proprio (`debug-stdout`) e e coberto.
        return True
    return any(family not in ADVISORY_FAMILIES for family in found)


def _strip_context(obj, level):
    """Remove campos de contexto de um objeto JSON, preservando decisões.

    Devolve (objeto_alterado, bytes_removidos, familia_detectada).
    """
    removed = 0
    family = None
    targets = []
    hso = obj.get("hookSpecificOutput")
    for holder in (hso, obj):
        if not isinstance(holder, dict):
            continue
        for field in CONTEXT_FIELDS:
            if isinstance(holder.get(field), str):
                targets.append((holder, field))
    for holder, key in targets:
        text = holder.get(key) or ""
        if family is None:
            family = classify(text)
        if allows_payload(text, level):
            continue
        holder.pop(key, None)
        removed += len(text)
    if isinstance(hso, dict):
        # `hookSpecificOutput` vazio não deve sobrar no envelope.
        if not hso and "hookSpecificOutput" in obj:
            obj.pop("hookSpecificOutput", None)
    return obj, removed, family


def filter_output(level: str, text: str) -> str:
    """Filtra a resposta de um hook conforme a política.

    Texto que não é JSON (stdout cru) é tratado como família `raw-stdout`:
    o corpus mediu 5,3 MB (9,0% de toda a injeção) de log de depuração cru
    chegando ao modelo por esse caminho, e 1% de unicidade — é ruído, não
    contexto. Em `no-advisory` e em `off` ele é suprimido.

    A decisão do hook (permissionDecision, decision, exit code) nunca é
    tocada: quem bloqueia continua bloqueando.
    """
    if level not in LEVELS or level == "full":
        return text
    stripped = text.strip()
    if not stripped:
        return text
    if stripped[0] not in "{[":
        # stdout cru: decide pelo conjunto de familias do texto.
        if allows_payload(stripped, level):
            return text
        return ""
    try:
        obj = json.loads(stripped)
    except (ValueError, TypeError):
        if allows_payload(stripped, level):
            return text
        return ""
    if not isinstance(obj, dict):
        return text
    obj, _removed, _fam = _strip_context(obj, level)
    return json.dumps(obj, ensure_ascii=False, separators=(",", ":"))


def _main(argv):
    if len(argv) >= 3 and argv[1] == "--filter":
        level = argv[2]
        sys.stdout.write(filter_output(level, sys.stdin.read()))
        return 0
    if len(argv) >= 2 and argv[1] == "--policy":
        sys.stdout.write(policy() + "\n")
        return 0
    if len(argv) >= 3 and argv[1] == "--allows":
        # Usado pelos hooks em shell: exit 0 = pode injetar; exit 1 = suprimido.
        return 0 if allows(argv[2]) else 1
    sys.stderr.write(
        "uso: injection_policy.py [--filter <full|no-advisory|off>] [--policy] [--allows <familia>]\n"
    )
    return 2


if __name__ == "__main__":
    sys.exit(_main(sys.argv))
