#!/usr/bin/env python3
"""P2.3 — Calibração das 5 classes com 20 prompts reais.

Responde à pergunta que o RETOMAR-AQUI deixou aberta: a matriz de roteamento
(o que o modo `code` nega, o que passa, o que vira programa) está certa —
ou a fronteira T2/T3 é chute?

Taxonomia (reconstruída do RETOMAR-AQUI + strategy §T2 — a definição original
não foi persistida; este script É a persistência):

| classe | nome | definição operacional | exemplos |
|---|---|---|---|
| T0 | execução | roda trabalho/verificação com veredito | cargo test, pytest, guards, touring run |
| T1 | iteração cega | comando byte-idêntico anterior, sem mutação entre | retry de grep idêntico (o G6 mede) |
| T2 | informação | lê estado para consumo, sem efeito | grep/cat/find/ls, Read, git status |
| T3 | instrução | altera estado/configuração do mundo | git commit, mkdir, daemon-ctl, memory store |
| T4 | escrita verificável | mutação de código/artefato com contrato | Write/Edit, cat >, sed -i, python open(w) |

A fronteira que importa é T2/T3: um comando que LÊ pode colapsar num programa;
um que ESCREVE não pode ser negado dessa forma. O falso-positivo canônico é
`cat > file` — prefixo de T2, efeito de T4 (medido vivo em 25/08).

Saída: data/calibration_p23.json + este relatório em stdout (agregado).
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import sys
from collections import Counter, defaultdict
from glob import glob

TRANSCRIPTS = os.path.expanduser("~/.claude/projects/-home-gabrielgadea-projects-touring/*.jsonl")
OUT = "docs/plans/2026-08-25-code-mode-afordancia/data/calibration_p23.json"
N_PROMPTS = 20

# ── classificador determinístico ──────────────────────────────────────────────

INSPECTION_BINS = ("grep", "rg", "cat", "head", "tail", "find", "ls", "wc", "sed", "awk", "sort", "uniq", "diff", "jq", "cut", "tr")
COLLAPSED = ("grep", "rg", "cat", "head", "tail", "find")  # matriz atual: grep/cat/find colapsam
EXEC_BINS = ("cargo", "pytest", "make", "nextest", "shellcheck")
GIT_READONLY = ("status", "log", "diff", "show", "blame", "ls-files", "branch", "reflog", "stash list")


def _segments(cmd: str) -> list[str]:
    """Segmentos de execução: separa por ; && || | — cada um classificado, a
    precedência T4>T3>T0>T2 decide (um composto é da sua operação mais forte)."""
    parts = re.split(r";|&&|\|\||\|", cmd)
    return [p.strip() for p in parts if p.strip() and not p.strip().startswith(("2>", "echo ", "print"))]


def _classify_segment(seg: str) -> tuple[str, bool]:
    """Classifica UM segmento. Devolve (classe, fronteira)."""
    toks = seg.split()
    if not toks:
        return "T2", False
    # tokens estruturais de shell e atribuições são transparentes — a classe é
    # do primeiro token REAL (for-grep é inspeção; A=x; grep é inspeção)
    i = 0
    while i < len(toks) and (
        toks[i] in ("{", "}", "for", "do", "done", "then", "if", "fi", "while", "(", ")", "!", "time")
        or re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", toks[i])
    ):
        i += 1
    if i >= len(toks):
        return "T2", False
    toks = toks[i:]
    first = toks[0]
    base = os.path.basename(first)

    # T4 — escrita: redirect de saída, sed -i, python abrindo para escrita,
    # cat cujo primeiro operando não-flag já é um redirect (cat > f / cat >> f)
    if "sed" == base and "-i" in toks:
        return "T4", False
    if base == "cat":
        operands = [t for t in toks[1:] if not t.startswith("-")]
        if operands and operands[0].startswith(">"):
            return "T4", False  # o falso-positivo medido: cat > heredoc é ESCRITA
    if base in ("cp", "mv", "tee"):
        return "T4", False
    if re.search(r"(?<![0-9<>])>>?", seg) and base in ("cat", "echo", "printf", "jq", "grep", "rg"):
        # grep > out: inspeção cujo resultado vira artefato — fronteira T2/T4
        return "T4", True
    if re.search(r"open\([^)]*['\"]w['\"]", seg) or (".write(" in seg and base.startswith("python")):
        return "T4", False

    if "touring run" in seg or "touring exec" in seg:
        return "T0", False
    if base in EXEC_BINS or re.match(r"python3?\s+(-m\s+)?(pytest|scripts/)", seg):
        return "T0", False
    if re.match(r"python3?\s+~?/?[.\w/-]*\.py\b", seg):
        return "T0", True  # script arbitrário — veredito ou leitura, o corpo decide

    if base == "sleep":
        return "T0", False  # espera ativa — efeito de tempo, não leitura
    if base == "git":
        sub = toks[1] if len(toks) > 1 else ""
        return ("T2", False) if sub in GIT_READONLY else ("T3", False)
    if base == "touring":
        sub = toks[1] if len(toks) > 1 else ""
        if sub in ("memory", "decompose", "diary") and any(w in seg for w in ("store", "create", "add", "update", "link", "claim", "release")):
            return "T3", False
        if sub in ("daemon-ctl", "update", "toolchain", "component"):
            # daemon-ctl status é leitura; restart/stop é instrução
            return ("T2", True) if "status" in seg else ("T3", False)
        if sub == "learning" and "reward" in seg:
            return "T3", False  # escreve no RL — instrução ao sistema de aprendizado
        return "T2", False
    if base == "update-touring":
        return "T3", False  # deploy
    if base in ("mkdir", "rm", "ln", "chmod", "chown", "kill", "pkill", "killall", "export", "cd", "sudo", "systemctl", "snapper", "touch"):
        return "T3", False
    if base in ("python3", "python", "python3.12"):
        return "T2", True  # inline — o corpo decide
    if base in INSPECTION_BINS:
        return "T2", False
    if base == "set" or base == "{":
        return "T2", True  # invólucro de composto — marca fronteira
    return "T2", True  # desconhecido → informação por default, marcado


_PRECEDENCE = {"T4": 4, "T3": 3, "T0": 2, "T1": 1, "T2": 0}


def classify_bash(cmd: str) -> tuple[str, bool]:
    """(classe, fronteira_T2T3) por precedência sobre os segmentos."""
    segs = _segments(cmd)
    if not segs:
        return "T2", True
    best = max((_classify_segment(s) for s in segs), key=lambda cf: _PRECEDENCE[cf[0]])
    # heredoc python3 << EOF é execução de programa (T0), não leitura
    if "<<" in cmd and re.match(r"\s*python3?", cmd):
        return "T0", False
    return best


def classify_tool(name: str, inp: dict) -> tuple[str, bool]:
    if name == "Bash":
        return classify_bash(str(inp.get("command", "")))
    if name in ("Write", "Edit", "NotebookEdit"):
        return "T4", False
    if name in ("Read", "Grep", "Glob"):
        return "T2", False
    if name in ("Task", "Agent"):
        return "T0", False
    if name in ("WebFetch", "WebSearch"):
        return "T2", False
    if name.startswith("mcp__touring__"):
        if any(w in name for w in ("store", "decompose", "link")):
            return "T3", True
        return "T2", False
    if name in ("TaskCreate", "TaskUpdate"):
        return "T3", False
    return "T0", True  # Monitor/Cron/etc — fronteira


# ── extrator de transcripts ───────────────────────────────────────────────────


def substantive(text: str) -> bool:
    t = text.strip()
    if len(t) < 40:
        return False
    skip = ("<command-", "Caveat:", "<local-command", "[{", "tooluse_id")
    return not any(t.startswith(s) for s in skip)


def extract_sessions() -> list[dict]:
    """Pares (prompt substantivo → tool calls até o próximo prompt), por sessão."""
    sessions = []
    files = sorted(glob(TRANSCRIPTS), key=os.path.getmtime)
    for f in files[-10:]:  # 10 sessões mais recentes — mistura de contextos
        pairs = []
        current = None
        sid = os.path.basename(f).replace(".jsonl", "")[:8]
        for line in open(f, encoding="utf-8", errors="ignore"):
            try:
                d = json.loads(line)
            except json.JSONDecodeError:
                continue
            if d.get("type") == "user":
                c = d.get("message", {}).get("content")
                if isinstance(c, str) and substantive(c):
                    if current and current["tools"]:
                        pairs.append(current)
                    current = {"prompt": c[:200], "tools": [], "session": sid}
            elif d.get("type") == "assistant" and current is not None:
                for c in d.get("message", {}).get("content", []):
                    if isinstance(c, dict) and c.get("type") == "tool_use":
                        current["tools"].append({"name": c.get("name", "?"), "input": c.get("input", {})})
        if current and current["tools"]:
            pairs.append(current)
        sessions.extend(pairs)
    return sessions


def main() -> int:
    pairs = extract_sessions()
    # amostra estratificada: os 20 com MAIS tool calls (trabalho substantivo),
    # cap de 6 por sessão para misturar contextos
    by_session = defaultdict(list)
    for p in sorted(pairs, key=lambda p: -len(p["tools"])):
        by_session[p["session"]].append(p)
    sample = []
    for sid, ps in by_session.items():
        sample.extend(ps[:8])
    sample = sample[:N_PROMPTS]

    dist = Counter()
    per_prompt = []
    frontier_cases = []
    t2_collapsed = Counter()
    t2_passed = Counter()
    false_positive_candidates = []
    blind_iterations = Counter()

    for p in sample:
        seen_hashes = {}
        pclasses = Counter()
        for t in p["tools"]:
            cls, frontier = classify_tool(t["name"], t["input"])
            # T1: byte-identical anterior sem mutação entre
            if t["name"] == "Bash":
                h = hashlib.sha1(str(t["input"].get("command", "")).encode()).hexdigest()[:12]
                if h in seen_hashes:
                    cls = "T1"
                    blind_iterations[p["session"]] += 1
                seen_hashes[h] = True
            dist[cls] += 1
            pclasses[cls] += 1
            if cls == "T2" and t["name"] == "Bash":
                cmd = str(t["input"].get("command", ""))
                first = os.path.basename(cmd.strip().split()[0]) if cmd.strip() else ""
                if first in COLLAPSED or (first == "rg"):
                    t2_collapsed[first] += 1
                else:
                    t2_passed[first] += 1
            if frontier:
                frontier_cases.append({"session": p["session"], "prompt": p["prompt"][:80], "tool": t["name"], "cmd": str(t["input"].get("command", ""))[:120], "class": cls})
            # candidato a falso positivo da matriz: T3/T4 com prefixo de inspeção
            if cls in ("T3", "T4") and t["name"] == "Bash":
                cmd = str(t["input"].get("command", ""))
                first = os.path.basename(cmd.strip().split()[0]) if cmd.strip() else ""
                if first in COLLAPSED or first == "rg":
                    false_positive_candidates.append({"class": cls, "cmd": cmd[:140], "prompt": p["prompt"][:80]})
        per_prompt.append({"session": p["session"], "prompt": p["prompt"][:100], "n_tools": len(p["tools"]), "classes": dict(pclasses)})

    report = {
        "sample": len(sample),
        "sessions": sorted(by_session.keys()),
        "total_tools": sum(dist.values()),
        "distribution": dict(dist.most_common()),
        "t2_bash_collapsed_by_matrix": dict(t2_collapsed.most_common()),
        "t2_bash_passing_by_matrix": dict(t2_passed.most_common()),
        "false_positive_candidates": false_positive_candidates,
        "blind_iterations_by_session": dict(blind_iterations),
        "frontier_cases_count": len(frontier_cases),
        "frontier_cases": frontier_cases[:40],
        "per_prompt": per_prompt,
    }
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w") as fh:
        json.dump(report, fh, indent=1, ensure_ascii=False)

    # agregado legível
    total = sum(dist.values())
    print(f"amostra: {len(sample)} prompts de {len(by_session)} sessões | {total} tool calls")
    for cls, n in dist.most_common():
        print(f"  {cls}: {n} ({100*n/total:.0f}%)")
    print(f"T2-Bash que a matriz COLAPSARIA: {sum(t2_collapsed.values())} | PASSARIA: {sum(t2_passed.values())}")
    print(f"falsos-positivos potenciais (T3/T4 com prefixo de inspeção): {len(false_positive_candidates)}")
    for fp in false_positive_candidates[:8]:
        print(f"  [{fp[chr(39)+'class'+chr(39)] if False else fp['class']}] {fp['cmd']}")
    print(f"fronteiras p/ revisão: {len(frontier_cases)} | T1 iterações cegas: {sum(blind_iterations.values())}")
    print(f"→ {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
