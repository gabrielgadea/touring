#!/usr/bin/env python3
"""Audita uma SESSÃO real: quantidade e padrão de tool calls vs Code Mode.

Complementa `audit_code_mode.py` (que audita as 12 dimensões da infraestrutura)
respondendo a outra pergunta, a que Gabriel fez em 25/08: *nesta sessão, o que
já poderia estar sendo feito em code mode e não está?*

A resposta é o agregado — nunca o dump do transcript (D5, reflexo
agregado-não-dump): a saída cabe em ~60 linhas e cada linha nomeia uma
oportunidade acionável, com o comando canônico que a substituiria.

Classificação de cada chamada Bash:
  - `code_mode`      : já é `touring run` / `touring exec`
  - `master_cli`     : já é um master (`scout`/`blast`/`investigate`/`explore`…)
  - `burst`          : N-ésima da mesma classe de inspeção numa janela — a
                       rajada acumulada JÁ É o programa (gate G1)
  - `loop`           : carrega `for`/`while` explícito — 1 sandbox faria tudo
  - `pipe_exit`      : lê `$?` depois de pipe sem `pipefail` (gate G2)
  - `atomic_inspect` : grep/cat/head/tail/find/ls soltos (antipadrões A2/A10)
  - `legit`          : cargo/git/deploy e afins — não são candidatos

Uso:
    python3 scripts/audit_session_code_mode.py <transcript.jsonl> [--json]
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

# Classes de inspeção para a janela de rajada (espelha scan_class_of do gate G1).
#
# ⚠ `head`/`tail` DEPOIS DE PIPE é truncador de saída — a prática CORRETA de
# manter o contexto enxuto (STR), não o antipadrão A2. Só conta como inspeção
# atômica quando lê um ARQUIVO diretamente (`cat f`, `head -20 f`), que é o que
# a tool Read faria melhor. A primeira versão deste auditor contava os dois
# juntos e inflava `cat` para 466 numa sessão — instrumento antes de veredito
# (memória `sinais-de-progresso-que-mentem`).
_PIPED_PAGER = re.compile(r"\|\s*(?:head|tail)\b")
SCAN_CLASSES = {
    "grep": re.compile(r"\b(?:grep|rg|ripgrep)\b"),
    "cat": re.compile(r"(?:^|;|&&|\|\|)\s*(?:cat|head|tail|sed -n)\s+[^|]*[\w./]"),
    "find": re.compile(r"\b(?:find|fd)\s"),
    "ls": re.compile(r"(?:^|;|&&|\|\|)\s*ls\b"),
}
CODE_MODE = re.compile(r"\btouring\s+(?:run|exec)\b")
MASTER_CLI = re.compile(
    r"\btouring\s+(?:scout|read|map|blast|investigate|guard|audit|explore|adw|factory|portfolio)\b"
)
LOOP = re.compile(r"(?:^|;|\||&&|\s)(?:for|while)\s")
PIPE_EXIT = re.compile(r"\|.*\$\?|\$\?.*\|")
PIPEFAIL = re.compile(r"pipefail")
# Comandos que NÃO são candidatos a code mode (trabalho real, não inspeção).
LEGIT = re.compile(
    r"\b(?:cargo|update-touring|git|python3|pkill|kill|npm|make|jq\s+-c\s+'\{)\b"
)
BURST_WINDOW = 4  # a 4ª da mesma classe é o limiar do gate G1


def classify(cmd: str) -> str:
    """Rotula um comando Bash com a categoria de oportunidade."""
    if CODE_MODE.search(cmd):
        return "code_mode"
    if MASTER_CLI.search(cmd):
        return "master_cli"
    if LOOP.search(cmd) and not LEGIT.search(cmd):
        return "loop"
    if PIPE_EXIT.search(cmd) and not PIPEFAIL.search(cmd):
        return "pipe_exit"
    if LEGIT.search(cmd):
        return "legit"
    if any(rx.search(cmd) for rx in SCAN_CLASSES.values()):
        return "atomic_inspect"
    return "other"


def scan_class(cmd: str) -> str | None:
    """Classe de inspeção do comando, ou None se não for inspeção atômica.

    Um `grep` DENTRO de `touring run` já é code mode — casá-lo aqui faria o
    predicado contradizer `classify`, que é a assimetria de
    `verificador-usa-menos-que-o-extrator`. O `audit` já filtrava por fora;
    o predicado agora é correto sozinho.
    """
    if CODE_MODE.search(cmd) or MASTER_CLI.search(cmd):
        return None
    for name, rx in SCAN_CLASSES.items():
        if rx.search(cmd):
            return name
    return None


def audit(path: Path) -> dict[str, object]:
    """Varre o transcript e devolve o agregado da sessão."""
    tools: Counter[str] = Counter()
    kinds: Counter[str] = Counter()
    per_class: Counter[str] = Counter()
    bursts: list[dict[str, object]] = []
    samples: dict[str, list[str]] = defaultdict(list)
    recent: list[str] = []  # classes de inspeção, em ordem

    with path.open(encoding="utf-8", errors="ignore") as handle:
        for line in handle:
            try:
                rec = json.loads(line)
            except json.JSONDecodeError:
                continue
            message = rec.get("message")
            if not isinstance(message, dict):
                continue
            for block in message.get("content") or []:
                if not isinstance(block, dict) or block.get("type") != "tool_use":
                    continue
                name = block.get("name", "?")
                tools[name] += 1
                if name != "Bash":
                    continue
                cmd = (block.get("input") or {}).get("command", "")
                kind = classify(cmd)
                kinds[kind] += 1
                if kind in {"loop", "pipe_exit", "atomic_inspect"} and len(samples[kind]) < 3:
                    samples[kind].append(cmd[:110])
                cls = scan_class(cmd)
                if cls and kind not in {"code_mode", "master_cli"}:
                    per_class[cls] += 1
                    recent.append(cls)
                    tail = recent[-BURST_WINDOW:]
                    if len(tail) == BURST_WINDOW and len(set(tail)) == 1:
                        bursts.append({"class": cls, "cmd": cmd[:90]})
                        recent.clear()
                elif kind in {"code_mode", "master_cli"}:
                    recent.clear()

    bash = tools.get("Bash", 0)
    adopted = kinds["code_mode"] + kinds["master_cli"]
    candidates = kinds["loop"] + kinds["pipe_exit"] + kinds["atomic_inspect"]
    denom = adopted + candidates
    return {
        "transcript": str(path),
        "tool_calls_total": sum(tools.values()),
        "by_tool": dict(tools.most_common()),
        "bash_total": bash,
        "bash_by_kind": dict(kinds.most_common()),
        # Só conta o que era ELEGÍVEL: cargo/git/deploy nunca virariam code mode,
        # então incluí-los no denominador esconde a adoção real (sinal honesto).
        "adoption_ratio_eligible": (adopted / denom) if denom else None,
        "eligible_total": denom,
        "missed_opportunities": candidates,
        "bursts_detected": len(bursts),
        "burst_samples": bursts[:3],
        "scan_classes": dict(per_class.most_common()),
        "samples": {k: v for k, v in samples.items()},
    }


def render(report: dict[str, object]) -> str:
    """Agregado legível — o que decide o próximo passo, nada mais."""
    ratio = report["adoption_ratio_eligible"]
    ratio_txt = "n/a (nada elegível)" if ratio is None else f"{ratio:.0%}"
    lines = [
        "=" * 62,
        "AUDITORIA DE CODE MODE — sessão real",
        "=" * 62,
        f"  tool calls totais        {report['tool_calls_total']}",
        f"  Bash                     {report['bash_total']}",
        f"  adoção (só elegíveis)    {ratio_txt}  ({report['eligible_total']} elegíveis)",
        f"  oportunidades perdidas   {report['missed_opportunities']}",
        f"  rajadas detectadas       {report['bursts_detected']}",
        "",
        "  Bash por categoria:",
    ]
    for kind, count in (report["bash_by_kind"] or {}).items():
        lines.append(f"    {kind:16} {count}")
    if report["scan_classes"]:
        lines.append("")
        lines.append("  Inspeção atômica por classe:")
        for cls, count in report["scan_classes"].items():
            lines.append(f"    {cls:16} {count}")
    for kind, cmds in (report["samples"] or {}).items():
        if not cmds:
            continue
        lines.append("")
        lines.append(f"  Amostras — {kind}:")
        for cmd in cmds:
            lines.append(f"    $ {cmd}")
    return "\n".join(lines)


def main() -> int:
    """CLI."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("transcript", type=Path, help="caminho do .jsonl da sessão")
    parser.add_argument("--json", action="store_true", help="saída JSON")
    args = parser.parse_args()

    if not args.transcript.is_file():
        print(f"transcript não encontrado: {args.transcript}", file=sys.stderr)
        return 2
    report = audit(args.transcript)
    if args.json:
        json.dump(report, sys.stdout, indent=2, ensure_ascii=False)
        print()
    else:
        print(render(report))
    return 0


if __name__ == "__main__":
    sys.exit(main())
