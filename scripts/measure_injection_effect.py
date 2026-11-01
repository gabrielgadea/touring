#!/usr/bin/env python3
"""Rodada 3/4: o conteudo injetado foi INTERNALIZADO, OBEDECIDO, ou nada?

Reescrito depois da rodada 3, que carregava TRES defeitos de instrumento
(medir o errado e parecer preciso):

  1. A TAXONOMIA era emprestada da regua (`FAMILY_RULES`), que casa por
     CABECALHO. O caminho de "orcamento de turno esgotado" emite o payload
     SEM cabecalho -> 5.630 sermoes caem em "outros" e a regua nao os ve.
     Aqui a taxonomia e propria e casa por CONTEUDO.
  2. O SPEC de `evidence-guard` e `post-tool-echo` asumia "o bloco traz a
     falha anterior". Nao traz: `evidence-guard` avisa que o GUARDE ESTA
     DEGRADADO, e `post-tool-echo` e um lembrete de pos-lote. Os dois
     famiam 100% INDETERMINADO por um modelo de sinal errado.
  3. O sinal de `code-mode-deny` era "o modelo rodou o comando sugerido".
     O gate nao sugere: ele REESCREVE a chamada. O efeito real e a chamada
     seguinte ter virado um programa sandboxado.

SINAIS (o que o transcrito permite observar)
  proximal_comando   a proxima chamada usa o comando nomeado pelo bloco
  fundiu_programa    a proxima chamada virou `touring run` (o gate reescreveu)
  virou_pipefail     a proxima chamada trata o exit code mascarado
  leu_stderr         a proxima chamada le stderr / exit code
  rodou_orphans      a proxima chamada roda `touring wiring orphans`
  gravou_estado      mais adiante a sessao grava estado em disco
  nao_repetiu_o_similar  o bloco NAO nomeia o comando: nao ha alvo a comparar
  distal_tocou       a sessao toca um caminho nomeado pelo bloco
  distal_simbolo     a sessao usa um simbolo nomeado pelo bloco
  nenhum             nao e contexto (log cru, ruido)

FORMAS DO EFEITO
  internalizou-e-agir / internalizou-sem-agir / agir-sem-internalizar / nenhum

ATENCAO DE METODO: mede CONFORMIDADE (o efeito declarado aconteceu), nao
ATRIBUICAO (o modelo faria isso de qualquer jeito). Atribuicao exige o A/B
de corpus (`crates/touring-hooks-shared/tests/s8_corpus_ab.rs`).

Uso: python3 scripts/measure_injection_effect.py [dirs...] [--max-sessions N]
"""
from __future__ import annotations

import collections
import glob as globmod
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import measure_injection_impact as M

BASE = M.BASE
DEFAULT_DIRS = [
    "-home-gabrielgadea-projects-touring",
    "-home-gabrielgadea-projects-analise",
    "-home-gabrielgadea-projects-konverter",
]

#: (familia, regex do payload, sinal observavel)
FAMILIES = (
    ("touring-suggest", r"\[TOURING SUGGEST", "proximal_comando"),
    ("code-mode-deny", r"\[CODE MODE|rajada|re-inspe[cç][aã]o", "fundiu_programa"),
    ("code-mode-contract", r"MUST\s+touring run --lang", "fundiu_programa"),
    ("session-contract", r"MUST\s+touring doctor -j", "proximal_comando"),
    ("similar-fail", r"Similar command failed recently", "nao_repetiu_o_similar"),
    ("pipefail-evidence", r"exited 0, but its output shows errors after a pipe",
     "virou_pipefail"),
    ("evidence-guard", r"EVIDENCE GUARD", "leu_stderr"),
    ("post-tool-echo", r"post-tool-batch:", "rodou_orphans"),
    ("pre-edit", r"HOTSPOT:|wiring_predict:|related docs:", "distal_tocou"),
    ("pre-edit-prevention", r"Pre-edit prevention", "distal_tocou"),
    ("post-edit", r"post-edit verification:|post-write:", "distal_tocou"),
    ("pre-read", r"defs\[", "distal_simbolo"),
    ("plugin-hook", (r"ATZILUTH|SCOUT PERP|Touring Knowledge|Session intelligence|"
                    r"\[delegation-nudge\]|\[SUGEST[ÃA]O DE SKILL"), "distal_tocou"),
    ("compact-warning", r"Contexto em \d+%", "gravou_estado"),
    ("debug-stdout", r"\[EVENT_READER\]|\[POST_QUALITY_GATE|Hook execution started",
     "nenhum"),
    ("generic-noise", r"^\[generic\]", "nenhum"),
)

FAMILY_RX = [(name, re.compile(rx, re.MULTILINE)) for name, rx, _ in FAMILIES]
SIGNAL_OF = {name: sig for name, _rx, sig in FAMILIES}

#: Sinal -> chave do dicionario de sinais (evita a cadeia de ifs).
SIGNAL_KEY = {
    "proximal_comando": "proximal_cmd",
    "fundiu_programa": "fundiu",
    "virou_pipefail": "pipefail",
    "leu_stderr": "stderr",
    "rodou_orphans": "orphans",
    "gravou_estado": "estado",
    "distal_tocou": "distal_touch",
    "distal_simbolo": "distal_sym",
}

#: O sermao SEM cabecalho: comeca por diretiva e NAO e o contrato do code mode.
HEADLESS_SERMON = re.compile(r"^\s+(?:MUST|SHOULD|MAY)\s{2,}", re.MULTILINE)
CODE_MODE_DIRECTIVE = re.compile(r"MUST\s+touring run --lang|MUST\s+touring doctor -j")

PIPEFAIL_MARK = re.compile(r"pipefail|PIPESTATUS")
STDERR_MARK = re.compile(r"2>&1|PIPESTATUS|exit code|\$\?")
ORPHANS_MARK = re.compile(r"wiring orphans")
STATE_MARK = re.compile(r"memory store|RETOMADA|STATUS\.md|loop_phase_close")
SANDBOX_MARK = re.compile(r"touring run|touring exec")

#: Palavras que sinalizam que o modelo CITOU o conteudo injetado.
CITE_MARKERS = (
    "HOTSPOT", "defs[", "chamadores", "callers:", "Pre-edit prevention",
    "post-edit verification", "API surface changed", "wiring_predict",
    "blast radius", "quality_score", "cognitive_score", "TOURING SUGGEST",
    "CODE MODE", "EVIDENCE GUARD", "orçamento do turno",
)


def classify(text):
    """(familia, sinal) do payload — casa por CONTEUDO, nao por cabecalho."""
    for name, rx in FAMILY_RX:
        if rx.search(text):
            return name, SIGNAL_OF[name]
    if HEADLESS_SERMON.search(text) and not CODE_MODE_DIRECTIVE.search(text):
        return "touring-suggest", "proximal_comando"
    return "outros", "distal_tocou"


def _first_str(holder, keys):
    for key in keys:
        v = holder.get(key)
        if isinstance(v, str) and v.strip():
            return v
    return ""


def extract_channels(stdout):
    """(contexto, razao_de_deny) — os DOIS canais de carga util do hook."""
    if not stdout:
        return "", ""
    s = stdout.strip()
    if s[:1] not in "{[":
        return s, ""
    try:
        payload = json.loads(s)
    except (ValueError, RecursionError):  # JSONDecodeError é ValueError; aninhamento absurdo estoura a pilha
        return s, ""
    if not isinstance(payload, dict):
        return s, ""
    specific = payload.get("hookSpecificOutput")
    specific = specific if isinstance(specific, dict) else {}
    ctx = _first_str(specific, ("additionalContext", "systemMessage"))
    if not ctx:
        ctx = _first_str(payload, ("additionalContext", "systemMessage"))
    reason = _first_str(specific, ("permissionDecisionReason",))
    if not reason:
        reason = _first_str(payload, ("permissionDecisionReason",))
    return ctx, reason


def _iter_payloads(records):
    """(chave, contexto, razao, posicao) de cada hook do Touring."""
    for i, attachment in M.hook_attachments(records):
        ctx, reason = extract_channels(attachment.get("stdout") or "")
        if not (ctx.strip() or reason.strip()):
            continue
        command = str(attachment.get("command") or "?")
        key = (attachment.get("hookEvent") or "?", attachment.get("hookName") or "?",
               command[:70], M.is_touring(command))
        yield key, ctx, reason, i


def _commands_suggested(text):
    return [m.strip() for m in M.DIRECTIVE_RE.findall(text)]


def _matches_command(cmd, blob):
    parts = cmd.lower().split()
    if not parts:
        return False
    probe = " ".join(parts[:2]) if len(parts) > 1 else parts[0]
    return bool(probe) and probe in blob


def _touches(paths, blob):
    for path in paths:
        base = os.path.basename(path)
        if (path.lower() in blob) or (len(base) > 4 and base.lower() in blob):
            return True
    return False


def _uses_symbol(syms, blob):
    return any(s.lower() in blob for s in syms)


def _cited(text, blob):
    low = blob.lower()
    return any(m.lower() in low and m.lower() in text.lower() for m in CITE_MARKERS)


def _signals(text, near, later, later_text):
    """Todos os sinais observaveis de uma injecao."""
    cmds = _commands_suggested(text)
    paths = {p for p in M.PATH_RE.findall(text) if len(p) > 6}
    syms = {s for s in M.SYMBOL_RE.findall(text) if len(s) > 3}
    return {
        "cmds": cmds,
        "proximal_cmd": any(_matches_command(c, near) for c in cmds),
        "fundiu": bool(SANDBOX_MARK.search(near)),
        "pipefail": bool(PIPEFAIL_MARK.search(near)),
        "stderr": bool(STDERR_MARK.search(near)),
        "orphans": bool(ORPHANS_MARK.search(near)),
        "estado": bool(STATE_MARK.search(later)),
        "distal_touch": _touches(paths, later),
        "distal_sym": _uses_symbol(syms, later),
        "cited": _cited(text, later_text),
    }


def _classify(signal, s):
    if signal == "nenhum":
        return "SEM EFEITO (nao e contexto)"
    if signal == "nao_repetiu_o_similar":
        # O payload NAO nomeia o comando que falhou: nao ha alvo a comparar.
        return "INDETERMINADO"
    key = SIGNAL_KEY.get(signal)
    if key is None:
        return "INDETERMINADO"
    return "CONFORME" if s[key] else "SEM EFEITO"


def _shape(verdict, s):
    if verdict != "CONFORME":
        return "nenhum"
    if s["cited"]:
        return "internalizou-e-agir" if (s["distal_touch"] or s["fundiu"]) \
            else "internalizou-sem-agir"
    if s["fundiu"]:
        return "executou-o-programa"
    if s["distal_touch"]:
        return "usou-o-fato-depois"
    if s["proximal_cmd"]:
        return "executou-o-comando"
    if s["pipefail"]:
        return "virou-pipefail"
    if s["stderr"]:
        return "leu-stderr"
    if s["orphans"]:
        return "rodou-orphans"
    if s["estado"]:
        return "gravou-estado"
    return "nenhum"


def _accumulate(r, verdict, forma, s):
    r["n"] += 1
    r[verdict] += 1
    r["forma:" + forma] += 1
    r["citou"] += 1 if s["cited"] else 0


def _texts_by_position(records):
    """(posicao, texto) de cada bloco de texto do modelo."""
    out = []
    for i, rec in enumerate(records):
        if rec.get("type") != "assistant":
            continue
        content = (rec.get("message") or {}).get("content")
        if not isinstance(content, list):
            continue
        body = " ".join(
            str(b.get("text") or "") for b in content
            if isinstance(b, dict) and b.get("type") == "text"
        )
        out.append((i, body))
    return out


def _payload_family(ctx, reason):
    """Familia do payload: o contexto manda; o motivo do deny e o reserva."""
    if ctx.strip():
        fam, sig = classify(ctx)
        if fam != "outros":
            return fam, sig
    if reason.strip():
        fam, sig = classify(reason)
        if fam != "outros":
            return fam, sig
    return "outros", "distal_tocou"


def analyse_session(path, window):
    rows = collections.defaultdict(collections.Counter)
    records = M._read_records(path)
    calls_at = M._calls_index(records)
    texts_at = _texts_by_position(records)

    for key, ctx, reason, pos in _iter_payloads(records):
        if not key[3]:
            continue  # so hooks do Touring
        family, signal = _payload_family(ctx, reason)
        text = (ctx + "\n" + reason).strip()
        index = M._first_call_after(calls_at, pos)
        near = " ".join(M._calls_after(calls_at, index, window)).lower()
        later = " ".join(" ".join(c[1]) for c in calls_at if c[0] > pos).lower()
        later_text = " ".join(t for i, t in texts_at if i > pos).lower()
        s = _signals(text, near, later, later_text)
        verdict = _classify(signal, s)
        forma = _shape(verdict, s)
        _accumulate(rows[family], verdict, forma, s)
    return rows


def _report(total, sessions, failures, window):
    print("=== IMPACTO POR FAMILIA (somente Touring) ===")
    print(f"sessoes={sessions} janela={window} | distal = sessao inteira")
    if failures:
        print(f"ATENCAO: {failures} sessoes falharam")
    print()
    hdr = (
        f"{'familia':<22} {'n':>7} {'CONFORME':>9} {'CONTRARIO':>9} "
        f"{'SEM EFEITO':>11} {'INDET':>8} {'citou':>8}"
    )
    print(hdr)
    print("-" * len(hdr))
    for family in sorted(total, key=lambda f: -total[f]["n"]):
        c = total[family]
        n = c["n"]
        if not n:
            continue
        conforme = 100.0 * c["CONFORME"] / n
        contrario = 100.0 * c["CONTRARIO"] / n
        sem_efeito = 100.0 * c["SEM EFEITO (nao e contexto)"] / n
        indet = 100.0 * c["INDETERMINADO"] / n
        citou = 100.0 * c["citou"] / n
        print(
            f"{family:<22} {n:>7d} {conforme:>8.1f}% {contrario:>8.1f}% "
            f"{sem_efeito:>10.1f}% {indet:>7.1f}% {citou:>7.1f}%"
        )
    print()
    print("=== FORMA DO EFEITO ===")
    for family in sorted(total, key=lambda f: -total[f]["n"]):
        c = total[family]
        formas = {k[5:]: v for k, v in c.items() if k.startswith("forma:")}
        if not formas:
            continue
        ranked = sorted(formas.items(), key=lambda kv: -kv[1])
        print(f"  {family:<22} {', '.join(f'{k}={v}' for k, v in ranked)}")
    print()
    print("=== SINALIZACAO QUE O INSTRUMENTO NAO PODE MEDIR ===")
    for family in sorted(total):
        c = total[family]
        if c["INDETERMINADO"]:
            print(f"  {family:<22} {c['INDETERMINADO']} casos")


def main(argv):
    window, max_sessions, dirs = M.parse_cli(argv, DEFAULT_DIRS)
    files = []
    for d in dirs:
        files.extend(sorted(globmod.glob(os.path.join(BASE, d, "*.jsonl"))))
    files = files[:max_sessions]

    total = collections.defaultdict(collections.Counter)
    failures = 0
    for path in files:
        try:
            rows = analyse_session(path, window)
        except Exception as exc:  # noqa: BLE001 — falhar alto, nao zerar calado
            failures += 1
            if failures <= 2:
                print(f"ERRO {os.path.basename(path)}: {type(exc).__name__}: {exc}")
            continue
        for family, counter in rows.items():
            total[family].update(counter)

    _report(total, len(files), failures, window)


if __name__ == "__main__":
    main(sys.argv[1:])
