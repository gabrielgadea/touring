#!/usr/bin/env python3
"""Guarda de conformidade do interruptor de injecao de contexto.

Um interruptor so e confiavel se TODO injetor de contexto o consultar. Esta
guarda enumera os hooks registrados em `~/.claude/settings.json`, separa os que
INJETAM contexto dos que BLOQUEIAM (enforcement), e reprova quando um injetor
nao consulta a politica — a menos que ele passe pelo shim (hooks Rust) ou
esteja na lista explicita de isencoes.

Cobertura (26/10/2026):
  - hooks Rust (`touring-hook <sub>`): cobertos pelo shim, que filtra o canal
    de contexto e preserva o de decisao;
  - hooks Python/shell advisory: precisam chamar `injection_policy.allows(...)`;
  - hooks de enforcement (deny/ask/block): ISENTOS — a decisao nao e contexto.

Executar:  python3 scripts/guard_injection_policy.py [--json]
"""
from __future__ import annotations

import collections
import json
import os
import re
import sys

SETTINGS = os.path.expanduser("~/.claude/settings.json")

INJECT_MARKERS = ("additionalContext", "systemMessage")
ENFORCE_MARKERS = (
    "permissionDecision",
    '"decision": "block"',
    '"decision":"block"',
    "decision: block",
)
#: Uma linha que escreve em stdout. A classificacao usa a JANELA em torno de
#: cada escrita: o marcador tem de estar perto da escrita, senao um comentario
#: ou uma docstring vira falso positivo.
STDOUT_MARKERS = (
    "print(",
    "sys.stdout.write",
    "printf",
    "echo ",
    "jq -cn",
    "cat <<",
)
WINDOW = 15

#: Subcomandos do binario Rust que nunca injetam contexto.
PURE_SUBCOMMANDS = frozenset({
    "setup", "session-stop", "stop-failure", "subagent-start", "subagent-stop",
    "task-created", "task-completed", "task-sync-create", "task-sync-update",
    "task-sync-list", "task-sync-output", "task-sync-get", "task-sync-delete",
    "teammate-idle", "elicitation", "elicitation-result", "permission-request",
    "notification", "file-changed", "instructions-loaded", "config-change",
    "cwd-changed", "exit-plan-mode",
})

#: Injetores que ficam de fora por decisao explicita (com o motivo).
EXEMPT = {
    "loop_outer_effect.py": "enforcement: deny unico na 1a edicao",
    "loop_stop_guard.py": "enforcement: decision block no Stop",
    "agent_model_gate.py": "enforcement: deny PreToolUse Agent",
    "block_git.sh": "enforcement: deny de comando git destrutivo",
    "config_protection.py": "enforcement: permissionDecision ask",
    "touring-quality-block-all.sh": "enforcement: deny das 6 dimensoes P0",
    "touring-process-guard.sh": "enforcement: exit 2 em processo proibido",
    "fable_consent_guard.sh": "enforcement: deny sem consentimento",
    "prose_integrity_gate.py": "enforcement: block (exit 2) no modo block",
    "guard_settings.sh": "repair: restaura settings.json E avisa; cortar o aviso corta o reparo",
    "post_quality_gate.py": "projeto: nao e hook do touring (quality gate do projeto)",
    "gitnexus-hook.cjs": "terceiro: hook do plugin gitnexus",
    "touring pre-task-scout": "roda pelo CLI `touring`, nao pelo shim; 5,6 KB em 21 dias (Wave 2)",
    "injection_policy_gate.py": "enforcement: deny de hook injetor que nao consulta a politica",
}


def uncovered_injector(name, body):
    """Reason string when a hook script with this name and text would be an injector
    the policy does not cover; ``None`` when it is covered (inert, enforcement,
    consults the policy, or exempt). Shared by the guard and the PreToolUse gate."""
    kind = emit_kind(body)
    if kind not in ("injetor", "misto"):
        return None
    if consults_policy_text(body) or name in EXEMPT:
        return None
    if SHELL_FAIL_CLOSED_RE.search(body):
        return "consulta a politica com `if ! python3 … --allows` (cala o hook quando a politica falta)"
    return "injeta contexto sem consultar a politica"


def load_hooks():
    """(evento, matcher, comando) de cada hook registrado."""
    try:
        with open(SETTINGS, encoding="utf-8") as fh:
            data = json.load(fh)
    except (OSError, ValueError) as exc:
        print(f"ERRO: nao foi possivel ler {SETTINGS}: {exc}", file=sys.stderr)
        return []
    out = []
    for event, entries in (data.get("hooks") or {}).items():
        for entry in entries or []:
            matcher = str(entry.get("matcher") or "")
            for hook in entry.get("hooks") or []:
                cmd = str(hook.get("command") or "")
                if cmd:
                    out.append((event, matcher, cmd))
    return out


def script_path(command):
    """Extrai o caminho do script de um comando de hook."""
    for token in command.split():
        clean = token.strip("'\"")
        if clean.endswith((".py", ".sh", ".cjs", ".js")):
            return (clean.replace("$HOME", os.path.expanduser("~"))
                        .replace("$CLAUDE_PROJECT_DIR", os.getcwd()))
    return None


def _window_flags(lines):
    """(emite_contexto, emite_decisao) — procura os marcadores perto de uma escrita."""
    inject = False
    enforce = False
    for i, line in enumerate(lines):
        if not any(m in line for m in STDOUT_MARKERS):
            continue
        window = "\n".join(lines[max(0, i - WINDOW): i + WINDOW + 1])
        if any(m in window for m in INJECT_MARKERS):
            inject = True
        if any(m in window for m in ENFORCE_MARKERS):
            enforce = True
    return inject, enforce


def emit_kind(body):
    """'injetor' | 'enforcement' | 'misto' | 'inerte' — public: also used by the
    PreToolUse gate `client/hooks/injection_policy_gate.py` (one predicate, D8)."""
    inject, enforce = _window_flags(body.splitlines())
    if inject and enforce:
        return "misto"
    if inject:
        return "injetor"
    if enforce:
        return "enforcement"
    return "inerte"


def _rust_subcommand(command):
    tail = command.split("touring-hook ", 1)[1].strip()
    return tail.split()[0] if tail else ""


def classify(command):
    """(categoria, caminho) — 'shim', 'shim-puro', 'injetor', 'enforcement', 'misto', 'inerte'."""
    if "touring-hook " in command:
        sub = _rust_subcommand(command)
        return ("shim-puro", None) if sub in PURE_SUBCOMMANDS else ("shim", None)
    path = script_path(command)
    if path is None:
        return "opaco", None
    if not os.path.exists(path):
        return "ausente", path
    try:
        with open(path, encoding="utf-8", errors="ignore") as fh:
            body = fh.read()
    except OSError:
        return "ausente", path
    return emit_kind(body), path


def _consults_policy(path):
    """O hook consulta a política **de verdade**?

    A verificação anterior procurava o NOME do módulo — e o hook de shell
    menciona o módulo no comentário sem chamá-lo (chama a CLI `--allows`).
    Exigimos uma CHAMADA: `allows(` (Python) ou `--allows` (shell).
    """
    if not path or not os.path.exists(path):
        return False
    try:
        with open(path, encoding="utf-8", errors="ignore") as fh:
            body = fh.read()
    except OSError:
        return False
    return consults_policy_text(body)


def consults_policy_text(body):
    """The same check over a script's TEXT (the gate judges content before it is written)."""
    if SHELL_FAIL_CLOSED_RE.search(body):
        # `if ! python3 … --allows` silences the hook whenever the policy
        # file is missing or crashes (rc 2), not only when it denies (rc 1):
        # 27 client/hooks tests went red this way (27/10/2026).
        return False
    return "allows(" in body or "--allows" in body


#: Shell guard that treats ANY non-zero rc of the policy CLI as "denied".
SHELL_FAIL_CLOSED_RE = re.compile(r"if\s+!\s+python3[^\n]*--allows")


def _reason_for(category, name, path):
    """(coberto, motivo) para um classificado."""
    if category.startswith("shim"):
        return True, "coberto pelo shim (filtra contexto, preserva decisao)"
    if category == "inerte":
        return True, "inerte: nao escreve contexto em stdout"
    if category == "enforcement":
        return True, "enforcement: decisao preservada por construcao"
    # Um hook "misto" (injeta E bloqueia) NAO está coberto por bloquear:
    # o contexto que ele injeta ainda precisa da política.
    if _consults_policy(path):
        return True, "consulta injection_policy"
    if name in EXEMPT:
        return True, "isento: " + EXEMPT[name]
    if category in ("injetor", "misto"):
        return False, "NAO consulta a politica"
    return False, "categoria " + category


def row_for(event, matcher, command):
    """(evento, matcher, nome, categoria, coberto, motivo)."""
    category, path = classify(command)
    if path:
        name = os.path.basename(path)
    elif category == "opaco":
        # Sem caminho: o comando E o próprio nome (ex.: `touring pre-task-scout`).
        name = command.split()[0] + " " + command.split()[-1] if command.split() else command[:60]
    else:
        name = command[:60]
    covered, reason = _reason_for(category, name, path)
    return event, matcher, name, category, covered, reason


#: Nomes de família passados a `allows(...)` nos hooks.
ALLOWS_RE = re.compile(r"""allows\(\s*["']([A-Za-z0-9_\-]+)["']""")

#: O módulo da política (fonte única das famílias).
POLICY_MODULE = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "client", "hooks", "injection_policy.py",
)


def _known_families():
    """As famílias que a política reconhece (ADVISORY ∪ FACT)."""
    try:
        import importlib.util

        spec = importlib.util.spec_from_file_location("injection_policy", POLICY_MODULE)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return set(module.ALL_FAMILIES)
    except Exception as exc:  # noqa: BLE001 — a guarda reporta, não morre calada
        print(f"ERRO ao carregar a politica ({type(exc).__name__}): {exc}")
        return None


def _families_used(paths):
    """Nomes de família passados a `allows(...)`, por hook."""
    used = collections.defaultdict(list)
    for path in sorted({p for p in paths if p}):
        if not os.path.exists(path):
            continue
        try:
            with open(path, encoding="utf-8", errors="ignore") as fh:
                body = fh.read()
        except OSError:
            continue
        for name in ALLOWS_RE.findall(body):
            used[name].append(path)
    return used


def _unknown_families(paths):
    """Famílias usadas nos call sites que a política NÃO reconhece.

    Um nome obsoleto não quebra o hook: `allows()` devolve False e o hook
    simplesmente deixa de injetar — ou, no caso inverso, continuava injetando
    quando o nome ficou inválido depois de um rename. O sintoma é **silencioso**
    (26/10/2026: os dashboards seguiam sendo injetados sob `no-advisory` e a
    guarda passava verde, porque ela só checava que o hook CHAMA `allows`).
    Aqui o nome é validado.
    """
    known = _known_families()
    if known is None:
        return {}
    used = _families_used(paths)
    return {name: places for name, places in used.items() if name not in known}


def _script_paths():
    """Os caminhos de script de todos os hooks registrados."""
    paths = []
    for event, matcher, command in load_hooks():
        _cat, path = classify(command)
        if path:
            paths.append(path)
    return paths


def _report(rows, gaps, unknown):
    """Imprime o relatório da guarda. Devolve o código de saída."""
    print("=== guarda de conformidade da politica de injecao ===")
    print(f"{'evento':<20} {'hook':<34} {'categoria':<12} coberto")
    for event, matcher, name, category, covered, reason in rows:
        print(f"{event[:20]:<20} {name[:34]:<34} {category:<12} {'SIM' if covered else 'NAO'}  {reason[:58]}")
    print()
    print(f"total de registros : {len(rows)}")
    print(f"cobertos           : {sum(1 for r in rows if r[4])}")
    print(f"lacunas            : {len(gaps)}")
    print(f"nomes de familia obsoletos: {len(unknown)}")
    if unknown:
        print()
        print("NOMES INVALIDOS (um nome que a politica nao reconhece NAO injeta —")
        print("mas o hook nao avisa, entao o corte some em silencio):")
        for name, places in sorted(unknown.items()):
            print(f"  {name:<26} {', '.join(os.path.basename(p) for p in places)}")
    if gaps:
        print()
        print("LACUNAS (um injetor que nao consulta a politica nao e coberto):")
        for event, matcher, name, category, _c, _r in gaps:
            print(f"  {event:<18} {name:<32} {category}")
        return 1
    if unknown:
        return 1
    print("OK — todo injetor de contexto consulta a politica, e todo nome de familia existe.")
    return 0


def main(argv):
    as_json = "--json" in argv
    rows = []
    gaps = []
    for event, matcher, command in load_hooks():
        row = row_for(event, matcher, command)
        rows.append(row)
        if not row[4]:
            gaps.append(row)
    unknown = _unknown_families(_script_paths())
    if not rows:
        # No hook read means nothing was checked — not "0 gaps" (27/10/2026: inside
        # the `touring run` sandbox settings.json is unreadable and this printed
        # 0/0 with exit 0).
        print(f"ERRO: nenhum hook lido de {SETTINGS} — nada foi verificado", file=sys.stderr)
        return 2

    if as_json:
        print(json.dumps({
            "total": len(rows),
            "covered": sum(1 for r in rows if r[4]),
            "gaps": [{"event": g[0], "matcher": g[1], "name": g[2], "category": g[3]} for g in gaps],
            "unknown_families": {k: [os.path.basename(p) for p in v] for k, v in unknown.items()},
        }, ensure_ascii=False, indent=1))
        return 1 if (gaps or unknown) else 0
    return _report(rows, gaps, unknown)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
