#!/usr/bin/env python3
"""Adesão e impacto de TODO o contexto injetado pelo TOURING, por hook e evento.

Método (corpus real, sem instrumentação nova):
  1. cada invocação de hook deixou no transcript o `hookEvent`, o `hookName`,
     o `command` e o stdout real (o `additionalContext` que o modelo viu);
  2. de cada bloco injetado extraem-se as PROPOSIÇÕES: comandos `MUST/SHOULD/MAY`,
     caminhos de arquivo e símbolos entre crases;
  3. olham-se as próximas K chamadas de ferramenta do modelo e classifica-se:
       - `aderiu` : a chamada usa o comando sugerido (adesão de um sermão);
       - `tocou`  : a chamada toca um caminho/símbolo nomeado pelo bloco
                    (impacto de um fato — o bloco apontou e o modelo foi lá);
       - `nenhum` : nenhuma das duas (o bloco não mudou nada);
  4. agregação por (evento, hook, comando) e por evento e por família.

Uso:  python3 scripts/measure_injection_impact.py [dirs...] [--window N] [--max-sessions N]
"""
from __future__ import annotations

import collections
import glob as globmod
import json
import os
import re
import sys

BASE = "/home/gabrielgadea/.claude/projects"

#: NOME DA CHAVE DE FAMÍLIA — constante, e NÃO literal repetido.
#: A lição do repo ("predicado escrito duas vezes nunca se encontra":
#: 'finalized' × 'completed') já cobrou o preço aqui: o literal estava
#: "familias" na definição e "families" (inglês) em um dos acessos — e o
#: KeyError só aparecia em runtime, com n=4 de 5 sessões falhando.
#: Corrigir o padrão num arquivo só não basta: a chave é uma constante.
FAM = "familias"

DIRECTIVE_RE = re.compile(r"^\s*(?:MUST|SHOULD|MAY)\s{2,}(\S+(?:\s+\S+)?)", re.M)
PATH_RE = re.compile(r"(?:~|\$HOME)?/?[\w.\-]+(?:/[\w.\-]+)+\.\w{1,6}")
SYMBOL_RE = re.compile(r"`([A-Za-z_][A-Za-z0-9_]{2,})`")

FAMILY_RULES = (
    ("touring-suggest", ("[TOURING SUGGEST",)),
    ("code-mode-deny", ("[CODE MODE", "rajada", "re-inspeção")),
    ("prompt-enhance", ("PROMPT ENHANCEMENT",)),
    ("plugin-hook", ("[delegation-nudge]", "[SUGESTÃO DE SKILL", "ATZILUTH", "SCOUT PERPÉTUO",
                      "TOURING (slim)", "Touring Knowledge", "Session intelligence")),
    ("loop-outer", ("[OUTER · flow",)),
    ("evidence-guard", ("EVIDENCE GUARD",)),
    ("debug-stdout", ("[EVENT_READER]", "[POST_QUALITY_GATE", "Hook execution started")),
    ("post-tool-echo", ("post-tool-batch:", "batch of 1 tools")),
    ("pre-edit", ("HOTSPOT:", "wiring_predict:", "code-workflow", "rust-workflow",
                  "callers:", "related docs:")),
    ("pre-read", ("defs[",)),
    ("post-edit", ("post-edit verification:", "post-write:")),
    ("session-start", ("CODE MODE — contrato",)),
    ("pre-edit-prevention", ("Pre-edit prevention",)),
)

#: O campo `command` gravado no transcript é uma DESCRIÇÃO do hook
#: ("Touring: analyzing blast radius...", "touring cli-suggest: classifying
#: operation (Rust, in-daemon)...", "$HOME/.claude/hooks/touring-hook post-bash"),
#: não a linha de shell. Os marcadores abaixo são os que realmente aparecem.
TOURING_MARKS = (
    "Touring:",
    "touring ",
    "TACO:",
    "Code Mode:",
    "/.claude/hooks/touring-hook",
    "/.claude/skills/Touring/",
    "/.claude/skills/loop-engineering/",
    "delegation-nudge",
    "doc-coverage-gate",
    "evidence_guard.py",
    "painel_emanacao",
    "scout_perpetuo_reminder",
    "skill_picker_hook",
    "check_context.sh",
    "code_mode_sdk_section",
    "session_startup_intelligence",
    "investigate_on_start",
    "loop_outer_arm",
    "loop_resume",
    "RL: warming up bandit",
)

#: Exclusões: plugin de terceiro, script local do projeto, gitnexus.
NOT_TOURING_MARKS = (
    "CLAUDE_PLUGIN_ROOT",
    "gitnexus",
    "$CLAUDE_PROJECT_DIR",
    "/.claude/projects/",
)


def is_touring(command):
    """O comando pertence ao Touring (e não a um plugin ou a um hook do projeto)?"""
    if any(mark in command for mark in NOT_TOURING_MARKS):
        return False
    return any(mark in command for mark in TOURING_MARKS)


def family_of(text):
    for fam, marks in FAMILY_RULES:
        for m in marks:
            if m in text:
                return fam
    return "outros"


def _context_of(payload):
    for key in ("additionalContext", "systemMessage"):
        value = payload.get(key)
        if isinstance(value, str) and value.strip():
            return value
    return ""


def extract_injected(stdout):
    """O texto que o modelo viu nesse hook (vazio se o hook não injetou)."""
    if not stdout:
        return ""
    stripped = stdout.strip()
    if stripped[:1] not in "{[":
        return stripped
    try:
        payload = json.loads(stripped)
    except Exception:
        return stripped
    if not isinstance(payload, dict):
        return stripped
    specific = payload.get("hookSpecificOutput")
    if isinstance(specific, dict):
        text = _context_of(specific)
        if text:
            return text
    return _context_of(payload)


def _propositions(text):
    dirs = [m.strip() for m in DIRECTIVE_RE.findall(text)]
    paths = set(PATH_RE.findall(text))
    syms = set(SYMBOL_RE.findall(text))
    return dirs, paths, syms


def _tool_calls_of(record):
    out = []
    content = (record.get("message") or {}).get("content")
    if not isinstance(content, list):
        return out
    for block in content:
        if isinstance(block, dict) and block.get("type") == "tool_use":
            payload = block.get("input") or {}
            out.append(" ".join(str(v) for v in payload.values()).lower())
    return out


def _calls_after(calls_at, index, window):
    """As `window` chamadas de ferramenta seguintes a uma posição."""
    out = []
    seen = 0
    j = index
    while j < len(calls_at) and seen < window:
        out.extend(calls_at[j][1])
        seen += 1
        j += 1
    return " ".join(out)


def _matched_command(dirs, blob):
    for d in dirs:
        parts = d.lower().split()
        probe = " ".join(parts[:2]) if len(parts) > 1 else parts[0]
        if probe and probe in blob:
            return True
    return False


def _matched_target(paths, syms, blob):
    for path in paths:
        base = os.path.basename(path)
        if len(base) > 4 and (path.lower() in blob or base.lower() in blob):
            return True
    for sym in syms:
        if sym.lower() in blob:
            return True
    return False


def _classify(dirs, paths, syms, blob):
    if _matched_command(dirs, blob):
        return "aderiu"
    if _matched_target(paths, syms, blob):
        return "tocou"
    return "nenhum"


def _new_row():
    return {"n": 0, "bytes": 0, "payloads": set(), "with_prop": 0,
            "aderiu": 0, "tocou": 0, "nenhum": 0, FAM: collections.Counter()}


def _merge(target, row):
    """Acumula `row` em `target`.

    Usa `.get` e NÃO `target[k] += v`: num `defaultdict`, o `+=` numa chave
    ausente dispara `__missing__`, que ARMAZENHA o valor da fábrica no
    lugar da chave — e a aritmética passa a rodar sobre o dict inteiro
    (`dict + int` → TypeError). `.get` não dispara `__missing__`.
    """
    for field in ("n", "bytes", "with_prop", "aderiu", "tocou", "nenhum"):
        target[field] = target.get(field, 0) + row[field]
    target["payloads"] = target.get("payloads", set()) | row["payloads"]
    target[FAM] = target.get(FAM, collections.Counter()) + row[FAM]


def _read_records(session_file):
    records = []
    for line in open(session_file, encoding="utf-8", errors="ignore"):
        if '"attachment"' not in line and '"assistant"' not in line:
            continue
        try:
            records.append(json.loads(line))
        except Exception:
            continue
    return records


def _calls_index(records):
    calls_at = []
    for i, rec in enumerate(records):
        if rec.get("type") == "assistant":
            calls_at.append((i, _tool_calls_of(rec)))
    return calls_at


def _iter_injections(records):
    """(chave, texto, posição) de cada injeção de contexto do transcript."""
    for i, rec in enumerate(records):
        if rec.get("type") != "attachment":
            continue
        attachment = rec.get("attachment") or {}
        if not isinstance(attachment, dict):
            continue
        if not str(attachment.get("type", "")).startswith("hook"):
            continue
        text = extract_injected(attachment.get("stdout") or "")
        if not text:
            continue
        command = str(attachment.get("command") or "?")
        key = (attachment.get("hookEvent") or "?", attachment.get("hookName") or "?",
               command[:70], is_touring(command))
        yield key, text, i


def _first_call_after(calls_at, position):
    index = 0
    while index < len(calls_at) and calls_at[index][0] <= position:
        index += 1
    return index


def analyse(session_file, window):
    """Contadores por (evento, hook, comando) para UMA sessão."""
    rows = collections.defaultdict(_new_row)
    records = _read_records(session_file)
    calls_at = _calls_index(records)
    for key, text, position in _iter_injections(records):
        row = rows[key]
        row["n"] += 1
        row["bytes"] += len(text)
        row["payloads"].add(hash(text))
        row[FAM][family_of(text)] += 1
        dirs, paths, syms = _propositions(text)
        if not (dirs or paths or syms):
            continue
        row["with_prop"] += 1
        index = _first_call_after(calls_at, position)
        blob = _calls_after(calls_at, index, window)
        row[_classify(dirs, paths, syms, blob)] += 1
    return rows


def _collect(files, window):
    total = collections.defaultdict(_new_row)
    failures = 0
    for path in files:
        try:
            rows = analyse(path, window)
        except Exception as exc:  # noqa: BLE001 — falhar alto: um zero silencioso e pior que um erro
            failures += 1
            if failures <= 3:
                print("ERRO ao analisar %s: %s: %s" % (path, type(exc).__name__, exc))
            continue
        for key, row in rows.items():
            _merge(total[key], row)
    if failures:
        print("ATENCAO: %d de %d sessoes falharam" % (failures, len(files)))
    return total


def _accumulator():
    return collections.defaultdict(lambda: _new_row())


def _aggregate(total, key_fn):
    """Agrega as linhas de `total` pela chave produzida por `key_fn`."""
    out = _accumulator()
    for key, row in total.items():
        _merge(out[key_fn(key)], row)
    return out


def _dominant_family(row):
    return row[FAM].most_common(1)[0][0] if row[FAM] else "?"


def _print_rows(title, table):
    print("=== %s ===" % title)
    print("%-22s %-28s %-9s %11s %8s %9s %7s %7s %8s %9s" % (
        "evento", "hook", "familia", "bytes", "blocos", "com_prop", "aderiu", "tocou", "nenhum",
        "payloads"))
    print("-" * 120)
    for key, row in sorted(table.items(), key=lambda kv: -kv[1]["bytes"]):
        event, hook = key if isinstance(key, tuple) else (key, "")
        print("%-22s %-28s %-9s %11d %8d %9d %7d %7d %8d %9d" % (
            str(event)[:22], str(hook)[:28], _dominant_family(row)[:9],
            row["bytes"], row["n"], row["with_prop"], row["aderiu"], row["tocou"],
            row["nenhum"], len(row["payloads"])))
    print()


def _print_rates(title, table):
    print("=== %s ===" % title)
    print("%-24s %11s %8s %9s %8s %7s %9s %9s" % (
        "chave", "bytes", "blocos", "com_prop", "aderiu", "tocou", "adesao%", "impacto%"))
    for key, row in sorted(table.items(), key=lambda kv: -kv[1]["bytes"]):
        w = row["with_prop"]
        ad = 100.0 * row["aderiu"] / w if w else 0.0
        tc = 100.0 * row["tocou"] / w if w else 0.0
        print("%-24s %11d %8d %9d %8d %7d %8.1f%% %8.1f%%" % (
            str(key)[:24], row["bytes"], row["n"], w, row["aderiu"], row["tocou"], ad, tc))
    print()


def main(argv):
    window = 3
    max_sessions = 400
    dirs = []
    i = 0
    while i < len(argv):
        arg = argv[i]
        if arg == "--window":
            window = int(argv[i + 1]); i += 2; continue
        if arg == "--max-sessions":
            max_sessions = int(argv[i + 1]); i += 2; continue
        dirs.append(arg); i += 1
    if not dirs:
        dirs = ["-home-gabrielgadea-projects-touring"]
    files = []
    for d in dirs:
        files.extend(sorted(globmod.glob(os.path.join(BASE, d, "*.jsonl"))))
    files = files[:max_sessions]

    total = _collect(files, window)
    touring = {k: r for k, r in total.items() if k[3]}
    outros = {k: r for k, r in total.items() if not k[3]}

    print("=== ADESAO E IMPACTO DA INJECAO (TOURING) ===")
    print("sessoes=%d  janela=%d chamadas de ferramenta a frente" % (len(files), window))
    print("hooks do Touring: %d combinacoes | de terceiros: %d" % (len(touring), len(outros)))
    print()
    _print_rows("POR HOOK (Touring)", _aggregate(touring, lambda k: (k[0], k[1])))
    _print_rates("POR EVENTO (Touring)", _aggregate(touring, lambda k: k[0]))
    _print_rates("POR FAMILIA (Touring)",
                 _aggregate(touring, lambda k: _dominant_family(touring[k])))
    _print_rates("POR EVENTO (terceiros, para contraste)", _aggregate(outros, lambda k: k[0]))


if __name__ == "__main__":
    main(sys.argv[1:])
