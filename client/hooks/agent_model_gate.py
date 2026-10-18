#!/usr/bin/env python3
"""agent_model_gate.py — PreToolUse(Agent): subagente sem `model`, ou com modelo que não serve à tarefa, é recusado.

POR QUE ISTO EXISTE
===================
A regra "cada parte da tarefa desce ao executor mais barato que ainda passa no juiz" estava
escrita no CLAUDE.md dos dois projetos (08/10/2026, diretriz de Gabriel) e a medição de 14 dias
de transcripts mostrou o que a prosa produz: em sessões Opus, **102 de 171 chamadas `Agent`
(60%) saíram sem `model`** e herdaram o Opus — o gargalo não era o custo de decidir, era a
decisão não tomada. Enforcement mora no executor, não no anúncio (D8): este hook transforma
"escolha o modelo" em afordância — a chamada sem decisão volta com a matriz, e reemiti-la com
`model` custa uma linha.

O QUE PASSA SEM `model` (a decisão já foi tomada em outro lugar)
=================================================================
`EXEMPTIONS`, na ordem em que o predicado as testa — o corpo canônico
(`docs/economia-de-creditos-e-contexto.md` §1) as lista na MESMA ordem, e um teste exige:
kill switch humano · sessão irmã Kimi/MiniMax (pelo `ANTHROPIC_BASE_URL` que o hook herda;
gateway desconhecido NÃO isenta) · fork · `model:` declarado na definição do agente, inclusive
`inherit` escrito (o autor decidiu herdar) · built-in com default próprio ·
`CLAUDE_CODE_SUBAGENT_MODEL`. `model: "opus"` explícito também passa.

O env não passa em silêncio: a decisão tomada por ele é a que MAIS apaga as regras de adequação (sem
`model` na chamada e sem `model:` na definição, o env vence e o gate o aceita), então o log a nomeia —
`why: "env:CLAUDE_CODE_SUBAGENT_MODEL=<valor>"` (`WHY_ENV_PREFIX`) — e `scripts/model_routing_adherence.py`
conta quantas chamadas foram decididas assim (`gate.decididas_por_env`). O comportamento é mantido: as sessões
irmãs Kimi/MiniMax dependem do env.

Onde a definição é procurada (code.claude.com/docs/en/sub-agents, lida em 08/10/2026):
projeto — subindo do cwd até a raiz do repositório, a mais próxima vence — > usuário
(`~/.claude/agents`) > plugin (`installPath` do `installed_plugins.json` e `plugins/synced`;
subpasta de `agents/` entra no identificador: `plug:review:security`). Definições de managed
settings, `--agents` e `--add-dir` não são visíveis daqui: caem no deny, cujo remédio funciona.

FAIL-OPEN, NUNCA MUDO
=====================
Erro do próprio gate deixa a chamada passar (um bug aqui não pode travar o trabalho), mas
cada chamada `Agent` — permitida, recusada ou com erro — vira uma linha em
`~/.claude/touring/agent_model_gate.jsonl` com o motivo, sem prompt nem description. Entrada e
saída são bytes/ASCII: nenhuma configuração de encoding derruba o veredito.

AJUSTE MODELO × TAREFA (17/10/2026)
===================================
Presença não é adequação: a matriz (§1) manda busca para haiku e implementação para sonnet, e um
`model: "opus"` explícito num Explore passava como "decisão tomada". Depois das isenções de sessão
(kill switch, irmã) e do `fork`, o gate julga o modelo que a CHAMADA nomeia (o parâmetro vence a
definição e o env na precedência do Claude Code, então nenhum dos dois o protege), pelo
`subagent_type`, deterministicamente e na ordem abaixo — a razão do deny ENSINA a correção:

* `SEARCH_TYPES` com sonnet/opus/fable → `fit:busca-exige-haiku`;
* fable em qualquer tipo → `fable:sem-consentimento`, salvo consentimento vigente (`fable:consentido`);
* `IMPLEMENT_TYPES` com opus → `fit:opus-fica-na-sessao-principal`.

Tipos fora das duas classes só passam pela regra de presença e pelo Fable. Omitir `model` não escapa:
o `model:` que a DEFINIÇÃO fixa no frontmatter é o que rodaria (chamada > definição > env), e é julgado
pela mesma `_fit_decision` — a razão ensina a passar `model: "haiku"`, que vence a definição. `inherit` e
definição sem `model` não têm regra de ajuste; o default de built-in (`statusline-setup` = sonnet) não é
frontmatter e segue passando; o env `CLAUDE_CODE_SUBAGENT_MODEL` é decisão tomada em outro lugar e não é
julgado. Cada veredito de ajuste grava no log `judged` (o modelo visto) e `origin` (`chamada`|`definição`).

CONSENTIMENTO DO FABLE
======================
Fable custa 2,5x o Opus e consome a franquia semanal mais rápido: ligá-lo é decisão do Gabriel, com
prazo. `~/.claude/touring/fable_consent.json` = `{"valido_ate": "<ISO-8601 com fuso>", "criado_em",
"motivo"}`; só vale com fuso, no futuro e a no máximo 24 h de agora (o teto é do LEITOR, não só do
escritor: um arquivo à mão com ano 9999 não vira consentimento eterno). Ausente, expirado, ilegível,
sem fuso ou acima do teto = sem consentimento (sinal ausente nunca libera).

    ! python3 ~/.claude/hooks/agent_model_gate.py --consentir-fable 2h [--motivo "..."]
    ! python3 ~/.claude/hooks/agent_model_gate.py --revogar-fable

LIMITAÇÃO CONHECIDA: o Bash do próprio agente também poderia rodar esse comando (o `!` do usuário não
passa por PreToolUse; o Bash do modelo passa). `fable_consent_guard.sh` (PreToolUse em Bash e em
Write|Edit|MultiEdit|NotebookEdit) nega ao modelo `--consentir-fable` e a escrita de `fable_consent.json`;
`--revogar-fable` passa. Resíduo: o guard é textual — indireção em tempo de execução e ferramenta MCP de
execução não são vistas. Fechamento de substrato (não implementado): `sandbox.filesystem.denyWrite` do
Claude Code nesse caminho; e a CLI exigir `/dev/tty` (premissa por verificar para o `!` do TUI).

Registro: `client/claude/hooks.settings.json` (PreToolUse, matcher `Agent`) →
`scripts/install_claude_hooks.py --apply`. Testes: `client/hooks/tests/test_agent_model_gate.py`.
Kill switch (decisão humana): `AGENT_MODEL_GATE_DISABLED=1` no env da sessão.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from collections.abc import Iterator, Mapping, Sequence
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, NamedTuple

sys.path.insert(0, str(Path(__file__).resolve().parent))
# One predicate for every hook (D8).
from backend_provider import SISTER_PROVIDERS, model_family, provider_of

#: Task classes judged on fit (docs/economia-de-creditos-e-contexto.md §1); every other type is not.
SEARCH_TYPES = frozenset({"Explore", "touring-scouter", "claude-code-guide", "statusline-setup"})
IMPLEMENT_TYPES = frozenset({"touring-engineer", "general-purpose", "touring-architect", "touring-auditor",
                             "touring-scriber", "Plan"})
WHY_SEARCH = "fit:busca-exige-haiku"
WHY_OPUS = "fit:opus-fica-na-sessao-principal"
WHY_NO_CONSENT = "fable:sem-consentimento"
WHY_CONSENT = "fable:consentido"
#: A call decided by the env var alone logs `<prefix><value>`; scripts/model_routing_adherence.py counts these.
WHY_ENV_PREFIX = "env:CLAUDE_CODE_SUBAGENT_MODEL="
CONSENT_NAME = "fable_consent.json"
CONSENT_MAX = timedelta(hours=24)
CONSENT_CMD = "! python3 ~/.claude/hooks/agent_model_gate.py --consentir-fable 2h"
DOC_REF = "~/projects/touring/docs/economia-de-creditos-e-contexto.md §1"
#: The Agent tool's `model` enum, each with the work it is the cheapest fit for.
MATRIX = (
    ("haiku", "buscar, extrair, classificar, recuperar documentação, varredura mecânica"),
    ("sonnet", "implementar com spec clara, testes, revisão, pesquisa web com julgamento"),
    ("opus", ("decidir, sintetizar, julgar, arquitetura: é a sessão principal — "
              f"`model: \"opus\"` é recusado em {', '.join(sorted(SEARCH_TYPES | IMPLEMENT_TYPES))}")),
    ("fable", "só com consentimento vigente do Gabriel (ambiguidade longa em que o Opus em effort alto já falhou)"),
)
#: (keyword the canonical doc's bullet must contain, phrase shown to the model), in predicate order.
EXEMPTIONS = (
    ("AGENT_MODEL_GATE_DISABLED", "AGENT_MODEL_GATE_DISABLED=1 (só humano)"),
    ("sessão irmã", "sessão irmã Kimi/MiniMax"),
    ("fork", "fork"),
    ("frontmatter", "agente com `model:` adequado no frontmatter (inclusive `inherit` escrito)"),
    ("claude-code-guide", "built-in com default próprio (claude-code-guide, statusline-setup)"),
    ("CLAUDE_CODE_SUBAGENT_MODEL", "CLAUDE_CODE_SUBAGENT_MODEL definido"),
)
#: Built-ins whose default is not the session model (code.claude.com/docs/en/sub-agents, 08/10/2026).
#: general-purpose, Explore and Plan inherit it, so they are judged like any undecided call.
BUILTIN_DEFAULTS = {"claude-code-guide": "haiku", "statusline-setup": "sonnet"}
FRONTMATTER = re.compile(r"\A\ufeff?---\r?\n(.*?)\r?\n---", re.DOTALL)
INLINE_COMMENT = re.compile(r"\s+#.*$")
SESSION_MODEL = re.compile(rb'"model":"(claude-[^"]+)"')
TRANSCRIPT_TAIL_BYTES = 262_144


def frontmatter(path: Path) -> dict[str, str]:
    """Top-level `key: value` pairs of a Markdown frontmatter (BOM, CRLF and `# comments` tolerated)."""
    try:
        match = FRONTMATTER.match(path.read_text(encoding="utf-8", errors="replace"))
    except OSError:
        return {}
    fields: dict[str, str] = {}
    for line in (match.group(1) if match else "").splitlines():
        key, sep, value = line.partition(":")
        if sep and not line.startswith((" ", "\t", "#")):
            fields[key.strip()] = INLINE_COMMENT.sub("", value).strip().strip("\"'")
    return fields


def project_agent_dirs(cwd: Path | None, project_dir: Path | None, home: Path) -> list[Path]:
    """Every `.claude/agents` from cwd up to the repository root, closest first (docs: closest wins)."""
    dirs: list[Path] = []
    start = cwd or project_dir
    for directory in (start, *start.parents) if start else ():
        if directory == home:  # ~/.claude/agents is the user scope, never a project one
            break
        dirs.append(directory / ".claude" / "agents")
        if (directory / ".git").exists():
            break
    if project_dir is not None and project_dir / ".claude" / "agents" not in dirs:
        dirs.append(project_dir / ".claude" / "agents")
    return dirs


def plugin_agent_dirs(plugin: str, home: Path) -> list[Path]:
    """`agents/` of the INSTALLED copy (installed_plugins.json installPath) and of synced plugins.

    Never the cache by mtime: it keeps stale versions side by side with the installed one.
    """
    root = home / ".claude" / "plugins"
    try:
        installed = json.loads((root / "installed_plugins.json").read_text(encoding="utf-8")).get("plugins", {})
    except (OSError, ValueError, AttributeError):
        installed = {}
    dirs = [Path(entry["installPath"]) / "agents"
            for key, entries in installed.items() if key.split("@", 1)[0] == plugin
            for entry in (entries if isinstance(entries, list) else [entries])
            if isinstance(entry, dict) and entry.get("installPath")]
    return dirs + sorted(root.glob(f"synced/*/{plugin}/agents"))


def _definition_dirs(subagent_type: str, project_dirs: Sequence[Path], home: Path) -> Iterator[tuple[Path, str]]:
    """(directory, name to match) in Claude Code's precedence: project > user > plugin."""
    plugin, *subpath, name = subagent_type.split(":") if ":" in subagent_type else ("", subagent_type)
    if plugin:
        for agents in plugin_agent_dirs(plugin, home):
            yield agents.joinpath(*subpath), name
        return
    for directory in project_dirs:
        yield directory, name
    yield home / ".claude" / "agents", name


def definition_model(subagent_type: str, project_dirs: Sequence[Path], home: Path) -> str | None:
    """`model:` of the winning definition: None = no definition found, "" = found without one."""
    is_plugin = ":" in subagent_type
    for directory, name in _definition_dirs(subagent_type, project_dirs, home):
        if not directory.is_dir():
            continue
        for path in sorted(directory.rglob("*.md")):
            fields = frontmatter(path)
            # A plugin file registers by its path (agents/review/security.md → plug:review:security).
            if fields.get("name") == name or (is_plugin and path.stem == name):
                return fields.get("model", "")
    return None


def _session_decision(env: Mapping[str, str]) -> str | None:
    """The whole session is out of scope: the human switch, or a named sister backend."""
    if env.get("AGENT_MODEL_GATE_DISABLED") == "1":
        return "disabled"
    provider = provider_of(env.get("ANTHROPIC_BASE_URL"))
    return f"irmã:{provider}" if provider in SISTER_PROVIDERS else None


def _definition_decision(subagent_type: str, pinned: str | None) -> str | None:
    """The agent itself fixes the model: a declared `model:` (even `inherit`) or a built-in default.

    `pinned` is `definition_model`'s answer: None = no definition found, "" = found without a `model:`.
    """
    if pinned:
        return f"frontmatter:{pinned.lower() if pinned.lower() == 'inherit' else pinned}"
    if pinned is None and subagent_type in BUILTIN_DEFAULTS:
        return f"builtin:{BUILTIN_DEFAULTS[subagent_type]}"
    return None


class Verdict(NamedTuple):
    """What the gate decided: `why` names the decision, `consent` the Fable consent state when it mattered.

    A fit verdict also carries `judged` (the model it looked at) and `origin` (`chamada` = the call
    named it, `definição` = the call omitted `model` and the definition's frontmatter pins it).
    """

    allowed: bool
    why: str
    consent: str | None = None
    judged: str | None = None
    origin: str = "chamada"


def fable_consent(home: Path, now: datetime | None = None) -> tuple[bool, str]:
    """(live, state) of `~/.claude/touring/fable_consent.json`; state = consentido|ausente|expirado|ilegível|acima-do-teto.

    Fail-closed on every doubt: a missing, unreadable, zone-less or over-long consent never counts.
    """
    now = now or datetime.now(timezone.utc)
    try:
        raw = json.loads((home / ".claude" / "touring" / CONSENT_NAME).read_text(encoding="utf-8"))
        valid_until = datetime.fromisoformat(raw["valido_ate"])
    except FileNotFoundError:
        return False, "ausente"
    except (OSError, ValueError, KeyError, TypeError):
        return False, "ilegível"
    if valid_until.tzinfo is None:
        return False, "ilegível"
    if valid_until <= now:
        return False, "expirado"
    if valid_until - now > CONSENT_MAX:
        return False, "acima-do-teto"
    return True, "consentido"


def _fit_decision(subagent_type: str, model: object, home: Path, now: datetime | None,
                  origin: str = "chamada") -> Verdict | None:
    """Judge a model (named by the call, or pinned by the definition) against the task class.

    None = no fit rule objects (or applies).
    """
    family = model_family(model)
    if family is None:
        return None
    judged = str(model).strip()
    if subagent_type in SEARCH_TYPES and family != "haiku":
        return Verdict(False, WHY_SEARCH, None, judged, origin)
    if family == "fable":
        live, state = fable_consent(home, now)
        return Verdict(live, WHY_CONSENT if live else WHY_NO_CONSENT, state, judged, origin)
    if subagent_type in IMPLEMENT_TYPES and family == "opus":
        return Verdict(False, WHY_OPUS, None, judged, origin)
    return None


def _explicit_why(model: object) -> str | None:
    return f"explicit:{model.strip()}" if isinstance(model, str) and model.strip() else None


def _omitted_model_verdict(subagent_type: str, project_dirs: Sequence[Path], home: Path, env: Mapping[str, str],
                           now: datetime | None) -> Verdict:
    """The call omitted `model`: the definition's pin is what would run, so it is judged like a named model.

    Claude Code resolves call > definition > env. A pin that passes the fit (or `inherit`, or no pin) falls
    through to the presence rules; the env var is a decision made elsewhere and is not judged.
    """
    pinned = definition_model(subagent_type, project_dirs, home)
    if pinned and (fit := _fit_decision(subagent_type, pinned, home, now, "definição")):
        return fit
    env_model = env.get("CLAUDE_CODE_SUBAGENT_MODEL", "").strip()
    why = _definition_decision(subagent_type, pinned) or (f"{WHY_ENV_PREFIX}{env_model}" if env_model else None)
    return Verdict(True, why) if why else Verdict(False, "deny")


def decide(tool_input: Mapping[str, Any], env: Mapping[str, str], project_dirs: Sequence[Path],
           home: Path, now: datetime | None = None) -> Verdict:
    """The verdict. Pure but for reading the definitions and the consent: every input is an argument.

    Order: session-wide exemptions (kill switch, sister session) > fork (runs in the session model, no
    rule applies) > the model the call names, judged for fit (it overrides definition and env, so
    neither shields it) > for an omitted `model`, the definition's pin judged for fit, then the
    presence rules in Claude Code's resolution order (definition > env).
    """
    subagent_type = str(tool_input.get("subagent_type") or "general-purpose")
    explicit = _explicit_why(tool_input.get("model"))
    session = _session_decision(env)
    if session or subagent_type == "fork":
        return Verdict(True, session or "fork")
    if explicit:
        return _fit_decision(subagent_type, tool_input.get("model"), home, now) or Verdict(True, explicit)
    return _omitted_model_verdict(subagent_type, project_dirs, home, env, now)


def session_model(transcript_path: str | None) -> str | None:
    """Model of the last assistant turn, from the transcript TAIL only (the hook input carries none)."""
    if not transcript_path:
        return None
    try:
        with open(transcript_path, "rb") as fh:
            fh.seek(0, os.SEEK_END)
            fh.seek(max(0, fh.tell() - TRANSCRIPT_TAIL_BYTES))
            found = SESSION_MODEL.findall(fh.read())
    except OSError:
        return None
    return found[-1].decode("ascii", errors="replace") if found else None


#: First line of a fit deny, by (why, origin). A definition's pin is named as such: the call omitted `model`.
_FIT_HEAD = {
    (WHY_SEARCH, "chamada"): '[agent-model-gate] Busca/extração com `model: "{named}"` ({call}): a matriz §1 manda busca → haiku.',
    (WHY_SEARCH, "definição"): ("[agent-model-gate] A definição de {sub} fixa `model: {named}` e a chamada ({call}) omitiu "
                                "`model` — o parâmetro `model` da chamada vence a definição, e a matriz §1 manda busca → haiku."),
    (WHY_OPUS, "chamada"): ('[agent-model-gate] `model: "{named}"` para {call}: decidir/sintetizar fica na sessão principal, '
                            "que já é o Opus — delegar a outro Opus só soma contexto e franquia."),
    (WHY_OPUS, "definição"): ("[agent-model-gate] A definição de {sub} fixa `model: {named}` e a chamada ({call}) omitiu "
                              "`model` — o parâmetro `model` da chamada vence a definição, e decidir/sintetizar fica na sessão "
                              "principal (delegar a outro Opus só soma contexto e franquia)."),
}
_FIT_FABLE = ("custa 2,5× o Opus 5.5 por token, consome a franquia semanal mais rápido e só serve à ambiguidade longa em "
              "que o Opus em effort alto já falhou — ligá-lo é decisão do Gabriel, com prazo.")


def _fit_reason(why: str, call: str, named: str, consent: str | None, origin: str = "chamada",
                sub: str = "general-purpose") -> list[str]:
    """The fit deny: what was refused, why (matrix §1), and the exact way back."""
    head = _FIT_HEAD.get((why, origin), "").format(named=named, call=call, sub=sub)
    if why == WHY_SEARCH:
        return [
            head,
            ('Reemita a MESMA chamada com `model: "haiku"` (busca/extração → haiku). Se a tarefa exige raciocínio, '
             'faça a síntese na sessão principal ou use touring-engineer/general-purpose com `model: "sonnet"`.'),
            f"Tipos julgados como busca: {' · '.join(sorted(SEARCH_TYPES))}.",
        ]
    if why == WHY_OPUS:
        return [
            head,
            ('Reemita a MESMA chamada com `model: "sonnet"` (implementação com spec, testes, revisão) ou '
             '`model: "haiku"` (busca/extração), ou faça você mesmo na sessão principal.'),
            f"Tipos julgados como implementação: {' · '.join(sorted(IMPLEMENT_TYPES))}.",
        ]
    state = f"consentimento {consent or 'ausente'}"
    who = (f"A definição de {sub} fixa `model: {named}` (Fable 5.1; {call}) sem consentimento vigente ({state}): "
           if origin == "definição" else f"Fable 5.1 ({call}) sem consentimento vigente ({state}): ")
    return [
        f"[agent-model-gate] {who}{_FIT_FABLE}",
        f"PEÇA ao Gabriel o consentimento; ele roda no prompt: {CONSENT_CMD}",
        ('Sem o sim dele: continue na sessão principal, ou delegue a `model: "sonnet"` (implementar) / '
         '`model: "haiku"` (buscar).'),
    ]


def _omitted_reason(call: str, model: str | None, quota: str | None) -> list[str]:
    """The omitted-`model` deny: the matrix and the exemptions that would have let the call through."""
    return [
        (f"[agent-model-gate] Agent sem `model` ({call}) herdaria {model or 'o modelo desta sessão'}: "
         "a decisão de modelo não foi tomada. Reemita a MESMA chamada com `model`:"),
        *(f"  {alias:<6} — {use}" for alias, use in MATRIX),
        *([f"{quota}."] if quota else []),
        "Passam sem `model`: " + " · ".join(phrase for _, phrase in EXEMPTIONS) + ".",
    ]


def deny_reason(tool_input: Mapping[str, Any], model: str | None, quota: str | None = None, why: str = "deny",
                verdict: Verdict | None = None) -> str:
    """The matrix plus the real call — dense, specific, and the way back named.

    `why` picks the text (omitted `model`, a fit rule, or the Fable consent; `verdict` carries the judged model,
    its origin and the consent state). `quota` is the recorded
    franchise line (rate_limits_snapshot.quota_line); absent, nothing is claimed.
    """
    subagent_type = tool_input.get("subagent_type") or "general-purpose"
    description = str(tool_input.get("description") or "")[:80]
    call = f'subagent_type={subagent_type}, "{description}"' if description else f"subagent_type={subagent_type}"
    if why == "deny":
        lines = _omitted_reason(call, model, quota)
    else:
        judged = verdict.judged if verdict and verdict.judged else str(tool_input.get("model") or "").strip()
        origin, consent = (verdict.origin, verdict.consent) if verdict else ("chamada", None)
        lines = [*_fit_reason(why, call, judged, consent, origin, str(subagent_type)), *([f"{quota}."] if quota else [])]
    return "\n".join([*lines, f"Matriz e fatos datados: {DOC_REF}."])


def recorded_quota(home: Path) -> str | None:
    """The Anthropic franchise line the status line recorded; None (never a guess) when unavailable."""
    try:
        from rate_limits_snapshot import quota_line  # only a deny pays for this import
        return quota_line(home)
    except Exception:  # noqa: BLE001 — the quota is context for the deny, never a reason to lose it
        return None


def log(home: Path | None, record: dict[str, Any]) -> None:
    """Append one verdict. Never raises: a log failure goes to stderr and never changes the verdict."""
    if home is None:
        return
    path = home / ".claude" / "touring" / "agent_model_gate.jsonl"
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(record, ensure_ascii=True) + "\n")
    except Exception as exc:  # noqa: BLE001 — the log is best-effort by contract
        sys.stderr.write(f"agent_model_gate: log indisponível ({type(exc).__name__})\n")


def parse_duration(text: str) -> timedelta:
    """`2h`, `30m`, `1h30m`, `1d`, `45s`; ValueError unless 0 < duration <= the 24 h ceiling."""
    if not re.fullmatch(r"(?:\d+[dhms])+", text):
        raise ValueError(f"duração inválida {text!r}: use 2h, 30m, 1h30m (unidades d/h/m/s, sem espaço)")
    units = {"d": 86_400, "h": 3_600, "m": 60, "s": 1}
    seconds = sum(int(n) * units[u] for n, u in re.findall(r"(\d+)([dhms])", text))
    if not 0 < seconds <= CONSENT_MAX.total_seconds():
        raise ValueError(f"duração {text!r} fora de (0, 24h]: o consentimento do Fable é curto de propósito")
    return timedelta(seconds=seconds)


def write_consent(home: Path, duration: timedelta, motivo: str, now: datetime | None = None) -> str:
    """Write the consent atomically (tmp + replace); returns the `valido_ate` it carries."""
    now = (now or datetime.now(timezone.utc)).astimezone()
    valid_until = (now + duration).isoformat(timespec="seconds")
    path = home / ".claude" / "touring" / CONSENT_NAME
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f"{CONSENT_NAME}.{os.getpid()}.tmp")
    tmp.write_text(json.dumps({"valido_ate": valid_until, "criado_em": now.isoformat(timespec="seconds"),
                               "motivo": motivo[:200]}, ensure_ascii=False) + "\n", encoding="utf-8")
    os.replace(tmp, path)
    return valid_until


def cli(argv: Sequence[str]) -> int:
    """The human's side of the Fable consent: `--consentir-fable <duração>` / `--revogar-fable`."""
    parser = argparse.ArgumentParser(prog="agent_model_gate.py", description="Consentimento do Fable (decisão humana).")
    what = parser.add_mutually_exclusive_group(required=True)
    what.add_argument("--consentir-fable", metavar="DURAÇÃO", help="ex.: 2h, 30m, 1h30m (máximo 24h)")
    what.add_argument("--revogar-fable", action="store_true")
    parser.add_argument("--motivo", default="", help="por que o Fable (fica no arquivo e no log)")
    args = parser.parse_args(list(argv))
    if args.motivo and args.consentir_fable is None:
        parser.error("--motivo só vale com --consentir-fable")
    home = Path.home()
    record: dict[str, Any] = {"ts": datetime.now().astimezone().isoformat(timespec="seconds")}
    if args.revogar_fable:
        try:
            (home / ".claude" / "touring" / CONSENT_NAME).unlink()
            sys.stdout.write("Consentimento do Fable revogado.\n")
        except FileNotFoundError:
            sys.stdout.write("Nenhum consentimento do Fable vigente: nada a revogar.\n")
        log(home, {**record, "evento": "revogar-fable"})
        return 0
    try:
        duration = parse_duration(args.consentir_fable)
    except ValueError as exc:
        parser.error(str(exc))
    valid_until = write_consent(home, duration, args.motivo)
    log(home, {**record, "evento": "consentir-fable", "valido_ate": valid_until, "motivo": args.motivo[:200]})
    sys.stdout.write(f"Fable consentido até {valid_until} ({args.consentir_fable}). "
                     "Revogar: ! python3 ~/.claude/hooks/agent_model_gate.py --revogar-fable\n")
    return 0


def _verdict_fields(verdict: Verdict) -> dict[str, Any]:
    """The log fields a fit verdict adds: the consent state, the model it judged and where that came from."""
    fields: dict[str, Any] = {"consent": verdict.consent} if verdict.consent else {}
    return {**fields, "judged": verdict.judged, "origin": verdict.origin} if verdict.judged else fields


def judge(payload: Mapping[str, Any], home: Path, record: dict[str, Any]) -> dict[str, Any] | None:
    """Decide one `Agent` call: fills `record` with the verdict and returns the deny output, if any."""
    tool_input = payload.get("tool_input")
    if not isinstance(tool_input, dict):
        record.update(allowed=True, why="malformed")
        return None
    env_project = os.environ.get("CLAUDE_PROJECT_DIR")
    cwd = payload.get("cwd") or env_project
    dirs = project_agent_dirs(Path(cwd) if cwd else None, Path(env_project) if env_project else None, home)
    verdict = decide(tool_input, os.environ, dirs, home)
    record.update(allowed=verdict.allowed, why=verdict.why, subagent_type=tool_input.get("subagent_type"),
                  model=tool_input.get("model"), **_verdict_fields(verdict))
    if verdict.allowed:
        return None
    reason = deny_reason(tool_input, session_model(payload.get("transcript_path")), recorded_quota(home),
                         verdict.why, verdict)
    return {"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny",
                                   "permissionDecisionReason": reason}}


def main(argv: Sequence[str] = ()) -> int:
    if argv:
        return cli(argv)
    home: Path | None = None
    record: dict[str, Any] = {"ts": datetime.now().astimezone().isoformat(timespec="seconds")}
    try:
        home = Path.home()
        payload = json.loads(sys.stdin.buffer.read().decode("utf-8", errors="replace"))
        if not isinstance(payload, dict) or payload.get("tool_name") != "Agent":
            return 0
        record.update(session_id=payload.get("session_id"), agent_id=payload.get("agent_id"))
        output = judge(payload, home, record)
        if output is not None:
            sys.stdout.write(json.dumps(output, ensure_ascii=True))
            sys.stdout.flush()
    except Exception as exc:  # noqa: BLE001 — fail-open by contract; the log line keeps it loud
        sys.stderr.write(f"agent_model_gate: erro interno, chamada liberada ({type(exc).__name__}: {exc})\n")
        log(home, {**record, "allowed": True, "why": f"error:{type(exc).__name__}"})
        return 0
    log(home, record)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
