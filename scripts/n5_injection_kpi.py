#!/usr/bin/env python3
"""N5 — KPI do eixo de inspeção: para onde vai o trabalho elegível a code mode.

Minera os transcripts (.jsonl) e classifica cada evento do EIXO de inspeção
(busca/leitura de arquivo) em três classes HONESTAS (S2, 26/08 — renomeadas
porque a versão anterior medria resistência como se fosse adesão):

- bash_native: Bash(grep|rg|egrep|fgrep|ag …) | Bash(find … -name) |
               Bash(cat|head|tail|less|more <alvo>) | Bash(sed -n …)
               → o modelo fez inspeção modelo-direta via Bash.
- native_tool: a tool dedicada equivalente (Grep | Glob | Read)
               → o modelo usou a ferramenta nativa do harness.
- code_route:  Bash(touring run|touring exec …) — o trabalho virou programa
               no sandbox (a rota que a doutrina code mode quer).

KPI principal: code_route_share = code_route / (bash_native+native_tool+code_route)
entre os eventos do eixo. NENHUMA das três classes é "certa" ou "errada" por
si — chamada única de inspeção é o caso ideal nativo (evidência dsh); o KPI
mede a DISTRIBUIÇÃO, e a meta de adoção se lê como code_route_share entre
eventos elegíveis-a-programa (rajadas/laços), não como 100%.

FUNIL PÓS-DENY (S2): quando um deny `[CODE MODE]` aparece num tool_result,
o próximo tool call do assistente é classificado:
  deny_then_route   → touring run/exec (a rota derivada foi tomada)
  deny_then_native  → Grep/Glob/Read nativo
  deny_then_reissue → mesma classe de inspeção via Bash outra vez
  deny_then_bypass  → prefixo TOURING_GATE_OK=1 / TOURING_CODE_MODE=native
  deny_then_other   → qualquer outra ação
Deny seguido de deny conta o anterior como re-emissão. Outros gates (G1-G10)
têm marcadores próprios e NÃO entram neste funil (escopo: classe code-mode).

O classificador de verbo espelha `resolved_tokens`/`effective_tokens` do
cli_suggester.rs (S1, 26/08): prefixos VAR=valor, wrappers env/time/nice/
sudo/command/builtin/timeout/stdbuf/ionice/taskset/chrt e segmentos
`cd <dir>` são transparentes. ANALISADOR offline — o predicado do executor
vive no Rust; este mede o passado e alimenta o relatório do bundle.

Uso:
  python3 scripts/n5_injection_kpi.py [--dir <transcripts>] [--since YYYY-MM-DD]
                                      [--json] [--report <path.md>] [--top N]
Exit 0 sempre (analista). Sessão sem evento do eixo → fora do denominador.
"""
from __future__ import annotations

import argparse
import glob
import json
import os
import sys
from collections import defaultdict

# ── espelho python do resolved_tokens (S1, cli_suggester.rs) ────────────────
_WRAPPER_VALUE_FLAGS = {
    "env": {"-u", "--unset", "-C", "--chdir", "-S", "--split-string"},
    "time": {"-o", "--output", "-f", "--format"},
    "nice": {"-n", "--adjustment"},
    "ionice": {"-c", "-n", "-p", "-P"},
    "sudo": {
        "-u", "-g", "-h", "-p", "-C", "-r", "-t", "-T", "-U", "-D", "-R",
        "--user", "--group", "--host", "--prompt", "--role", "--type",
        "--chdir", "--chroot",
    },
    "command": set(), "builtin": set(),
    "timeout": {"-s", "--signal", "-k", "--kill-after"},
    "stdbuf": {"-i", "-o", "-e"},
    "taskset": {"-c", "--cpu-list"},
    "chrt": set(),
}
_WRAPPER_POSITIONALS = {"timeout": 1, "taskset": 1, "chrt": 1}


def _is_env_assignment(tok: str) -> bool:
    if "=" not in tok:
        return False
    return tok[0].isalpha() or tok[0] == "_"


def _skip_wrapper(toks: list[str], i: int) -> int:
    if i >= len(toks) or toks[i] not in _WRAPPER_VALUE_FLAGS:
        return i
    verb = toks[i]
    value_flags = _WRAPPER_VALUE_FLAGS[verb]
    positionals = _WRAPPER_POSITIONALS.get(verb, 0)
    j = i + 1
    while j < len(toks):
        t = toks[j]
        if _is_env_assignment(t):
            j += 1
            continue
        if t.startswith("-") and t != "-":
            j += 2 if t in value_flags else 1
            continue
        if positionals > 0:
            positionals -= 1
            j += 1
            continue
        break
    return j


def resolved_tokens(cmd: str) -> list[str]:
    toks = cmd.split()
    i = 0
    while i < len(toks):
        if _is_env_assignment(toks[i]):
            i += 1
            continue
        j = _skip_wrapper(toks, i)
        if j == i:
            break
        i = j
    return toks[i:]


def effective_tokens(cmd: str) -> list[str]:
    """Espelho do `effective_tokens` do Rust (S1-complemento, 26/08):
    segmenta por operadores de sequência, pula segmentos `cd <dir>`,
    resolve wrappers no que sobra."""
    import re as _re
    for seg in _re.split(r"\n|;|&&|\|\|", cmd):
        rest = resolved_tokens(seg)
        if rest and rest[0] == "cd":
            continue
        if rest:
            return rest
    return []


_SEARCH_VERBS = {"grep", "rg", "egrep", "fgrep", "ag"}
_READ_VERBS = {"cat", "head", "tail", "less", "more"}


def is_search_bash(cmd: str) -> bool:
    rest = effective_tokens(cmd)
    if not rest:
        return False
    verb, args = rest[0], rest[1:]
    if verb in _SEARCH_VERBS:
        return bool(args)
    return verb == "find" and any("-name" in a for a in args)


def is_read_bash(cmd: str) -> bool:
    rest = effective_tokens(cmd)
    if not rest:
        return False
    verb, args = rest[0], rest[1:]
    if verb in _READ_VERBS:
        return bool(args) and not any(a.startswith(">") for a in args)
    return verb == "sed" and "-n" in args


def is_code_route(cmd: str) -> bool:
    return "touring run" in cmd or "touring exec" in cmd


def is_bypass_prefix(cmd: str) -> bool:
    return "TOURING_GATE_OK=1" in cmd or "TOURING_CODE_MODE=native" in cmd


def classify_tool_call(name: str, tool_input: dict) -> str | None:
    """'bash_native' | 'native_tool' | 'code_route' | None (fora do eixo)."""
    if name in ("Grep", "Glob", "Read"):
        return "native_tool"
    if name != "Bash":
        return None
    cmd = tool_input.get("command") or ""
    if is_code_route(cmd):
        return "code_route"
    if is_search_bash(cmd) or is_read_bash(cmd):
        return "bash_native"
    return None


# ── funil pós-deny (S2) ─────────────────────────────────────────────────────
DENY_MARKER = "[CODE MODE]"
_DENY_OUTCOMES = (
    "deny_then_route", "deny_then_native", "deny_then_reissue",
    "deny_then_bypass", "deny_then_other",
)


def _result_text(content) -> str:
    """tool_result content: string OU lista de blocos {type:text,text:…}."""
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return " ".join(
            b.get("text", "") for b in content
            if isinstance(b, dict) and b.get("type") == "text"
        )
    return ""


def iter_events(path: str):
    """Sequência cronológica de ('call', ts, name, input) e ('deny', ts)."""
    with open(path, errors="replace") as f:
        for line in f:
            try:
                d = json.loads(line)
            except Exception:
                continue
            if d.get("isSidechain"):
                continue
            ts = (d.get("timestamp") or "")[:10]
            kind = d.get("type")
            if kind == "assistant":
                for c in d.get("message", {}).get("content", []):
                    if isinstance(c, dict) and c.get("type") == "tool_use":
                        yield ("call", ts, c.get("name") or "", c.get("input") or {})
            elif kind == "user":
                for c in d.get("message", {}).get("content", []):
                    if isinstance(c, dict) and c.get("type") == "tool_result":
                        if DENY_MARKER in _result_text(c.get("content")):
                            yield ("deny", ts, None, None)


def deny_outcome(name: str, tool_input: dict) -> str:
    """Classe do primeiro tool call após um deny [CODE MODE]."""
    cmd = tool_input.get("command") or "" if name == "Bash" else ""
    if cmd and is_bypass_prefix(cmd):
        return "deny_then_bypass"
    if cmd and is_code_route(cmd):
        return "deny_then_route"
    if name in ("Grep", "Glob", "Read"):
        return "deny_then_native"
    if cmd and (is_search_bash(cmd) or is_read_bash(cmd)):
        return "deny_then_reissue"
    return "deny_then_other"


def mine(paths: list[str], since: str | None):
    per_session: dict[str, dict[str, int]] = {}
    per_day: dict[str, dict[str, int]] = defaultdict(lambda: defaultdict(int))
    for p in sorted(paths):
        sess = os.path.basename(p).replace(".jsonl", "")[:8]
        counts: dict[str, int] = defaultdict(int)
        pending_deny = False
        for ev, ts, name, tool_input in iter_events(p):
            if since and ts and ts < since:
                continue
            if ev == "deny":
                if pending_deny:
                    # deny seguido de deny: o anterior foi re-emissão negada
                    counts["deny_then_reissue"] += 1
                    if ts:
                        per_day[ts]["deny_then_reissue"] += 1
                pending_deny = True
                counts["deny_total"] += 1
                if ts:
                    per_day[ts]["deny_total"] += 1
                continue
            # ev == "call"
            cmd = tool_input.get("command") or "" if name == "Bash" else ""
            if "TOURING_GATE_OK=1" in cmd:
                counts["bypass"] += 1
                if ts:
                    per_day[ts]["bypass"] += 1
            if pending_deny:
                out = deny_outcome(name, tool_input)
                counts[out] += 1
                if ts:
                    per_day[ts][out] += 1
                pending_deny = False
            cls = classify_tool_call(name, tool_input)
            if cls is None:
                continue
            counts[cls] += 1
            if ts:
                per_day[ts][cls] += 1
        if counts:
            per_session[sess] = dict(counts)
    return per_session, per_day


def code_route_share(counts: dict[str, int]) -> float | None:
    """Fração dos eventos do eixo que terminou em programa (rota code)."""
    bn, nt, cr = (counts.get("bash_native", 0), counts.get("native_tool", 0),
                  counts.get("code_route", 0))
    if bn + nt + cr == 0:
        return None
    return cr / (bn + nt + cr)


def deny_route_rate(counts: dict[str, int]) -> float | None:
    """Fração dos denies cuja próxima ação foi tomar a rota code."""
    total = sum(counts.get(k, 0) for k in _DENY_OUTCOMES)
    if total == 0:
        return None
    return counts.get("deny_then_route", 0) / total


def _pct(x: float | None) -> str:
    return f"{x:.1%}" if x is not None else "n/d"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--dir", default=os.path.expanduser(
        "~/.claude/projects/-home-gabrielgadea-projects-touring"))
    ap.add_argument("--since", default=None, help="YYYY-MM-DD (inclusive)")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--report", default=None, help="grava relatório markdown")
    ap.add_argument("--top", type=int, default=10)
    args = ap.parse_args()

    paths = sorted(glob.glob(os.path.join(args.dir, "*.jsonl")))
    per_session, per_day = mine(paths, args.since)

    total = defaultdict(int)
    for c in per_session.values():
        for k, v in c.items():
            total[k] += v
    share = code_route_share(total)
    drate = deny_route_rate(total)
    payload = {
        "sessions_with_signal": len(per_session),
        "transcripts_scanned": len(paths),
        "overall": {**dict(total), "code_route_share": share,
                    "deny_route_rate": drate},
        "per_day": {d: {**c, "code_route_share": code_route_share(c),
                        "deny_route_rate": deny_route_rate(c)}
                    for d, c in sorted(per_day.items())},
        "per_session_top": sorted(
            ({"session": s, **c, "code_route_share": code_route_share(c)}
             for s, c in per_session.items()),
            key=lambda x: -(x.get("bash_native", 0) + x.get("native_tool", 0)
                            + x.get("code_route", 0)),
        )[: args.top],
    }

    if args.json:
        print(json.dumps(payload, indent=2, ensure_ascii=False))
    else:
        print(f"sessões com sinal: {payload['sessions_with_signal']} "
              f"(de {payload['transcripts_scanned']} transcripts)")
        bn, nt, cr = (total.get("bash_native", 0), total.get("native_tool", 0),
                      total.get("code_route", 0))
        print(f"eixo: bash_native={bn} native_tool={nt} code_route={cr} "
              f"→ rota-code {_pct(share)} do eixo")
        dt = total.get("deny_total", 0)
        if dt:
            print(f"pós-deny ({dt}): rota {_pct(deny_route_rate(total))} "
                  f"| nativa {total.get('deny_then_native',0)} "
                  f"| re-emissão {total.get('deny_then_reissue',0)} "
                  f"| bypass {total.get('deny_then_bypass',0)} "
                  f"| outra {total.get('deny_then_other',0)}")
        print(f"gate-fatigue: TOURING_GATE_OK=1 ×{total.get('bypass', 0)} (bypass por-comando)")
        for d, c in payload["per_day"].items():
            print(f"  {d}: bash_native={c.get('bash_native',0):4d} "
                  f"native_tool={c.get('native_tool',0):4d} "
                  f"code_route={c.get('code_route',0):4d} "
                  f"→ {_pct(c['code_route_share'])}")

    if args.report:
        total_d = payload["overall"]
        lines = [
            "---", "type: Report",
            "title: N5 — eixo de inspeção e funil pós-deny (code mode)",
            f"description: classes honestas (S2) + funil pós-deny, "
            f"minerado de {payload['transcripts_scanned']} transcripts",
            "tags: [n5, kpi, code-mode, affordance]", "---", "",
            "# N5 — eixo de inspeção e funil pós-deny", "",
            f"- sessões com sinal: **{payload['sessions_with_signal']}** / {payload['transcripts_scanned']} transcripts",
            f"- eixo: bash_native={total_d.get('bash_native',0)} · "
            f"native_tool={total_d.get('native_tool',0)} · "
            f"code_route={total_d.get('code_route',0)}",
            f"- **rota-code no eixo: {_pct(total_d['code_route_share'])}**",
            f"- **pós-deny → rota: {_pct(total_d['deny_route_rate'])}** "
            f"(denies={total_d.get('deny_total',0)}: "
            f"rota {total_d.get('deny_then_route',0)} · "
            f"nativa {total_d.get('deny_then_native',0)} · "
            f"re-emissão {total_d.get('deny_then_reissue',0)} · "
            f"bypass {total_d.get('deny_then_bypass',0)} · "
            f"outra {total_d.get('deny_then_other',0)})", "",
            "| dia | bash_native | native_tool | code_route | share |",
            "|---|---|---|---|---|",
        ]
        for d, c in payload["per_day"].items():
            lines.append(f"| {d} | {c.get('bash_native',0)} | {c.get('native_tool',0)} | "
                         f"{c.get('code_route',0)} | {_pct(c['code_route_share'])} |")
        lines += ["", f"Gate-fatigue (S5b): `TOURING_GATE_OK=1` ×**{total.get('bypass',0)}** "
                      f"no período (bypass por-comando).", ""]
        lines += ["", "Top sessões por volume no eixo:", "",
                  "| sessão | bash_native | native_tool | code_route | share |",
                  "|---|---|---|---|---|"]
        for s in payload["per_session_top"]:
            lines.append(f"| {s['session']} | {s.get('bash_native',0)} | "
                         f"{s.get('native_tool',0)} | {s.get('code_route',0)} | "
                         f"{_pct(s['code_route_share'])} |")
        lines += ["", "Classes (S2): `bash_native` = inspeção via Bash; "
                  "`native_tool` = Grep/Glob/Read; `code_route` = touring run/exec. "
                  "Funil pós-deny: primeiro tool call após um deny `[CODE MODE]` "
                  "(rota / nativa / re-emissão / bypass / outra). "
                  "Verbos espelham `resolved_tokens`/`effective_tokens` S1 do Rust.", ""]
        with open(args.report, "w") as fh:
            fh.write("\n".join(lines))
        print(f"relatório: {args.report}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
