"""Abre o balde 'nao classificado': o que ELE contem, com chars e contagem, nas duas janelas.

O balde carrega 63,5% do delta. Um balde por eliminacao nao mede: aqui ele se paga por PREDICADO
POSITIVO. Cada linha impressa e um prefixo literal real, com os chars que ele carrega nas duas
janelas — o veredito e 'a classe existe e temeste peso', nao 'o resto ficou de fora'.

Uso: python3 abrir_nao_classificado.py [--top 30]
"""

import json
import sys
from collections import defaultdict
from datetime import datetime
from pathlib import Path

HERE = Path(__file__).resolve().parent
BUNDLE = HERE.parents[1]
PROJ = Path.home() / ".claude/projects/-home-gabrielgadea-projects-analise"
DEPLOY_UTC = json.loads((BUNDLE / "diagnostics/item1.json").read_text())["deploy_utc"]
START_UTC = "2026-10-09T13:53:47Z"
BASE_CUTOFF = "2026-10-08T17:00:00"
HEAD = 110


def texto(content):
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return " ".join(c for c in content if isinstance(c, str))
    return json.dumps(content, ensure_ascii=False)


# A taxonomia e IMPORTADA do script principal, nunca reescrita: duas listas de Predicados sao duas
# listas que divergem, e a divergencia aparece como 'nao classificado' sem nome.
sys.path.insert(0, str(HERE))
from c1_por_familia import classificar  # noqa: E402


def extremos(f):
    first = last = None
    with f.open(encoding="utf-8", errors="replace") as fh:
        for line in fh:
            if '"timestamp"' not in line:
                continue
            try:
                ts = str(json.loads(line).get("timestamp", ""))
            except json.JSONDecodeError:
                continue
            if not ts:
                continue
            if first is None:
                first = ts
            last = ts
    return first, last


def varre_nc(files, since=None, until=None):
    """So os prefixos que caem em 'nao classificado'; chars do juiz."""
    acc = defaultdict(lambda: [0, 0])   # prefixo -> [chars, blocos]
    for f in files:
        for line in f.open(encoding="utf-8", errors="replace"):
            if '"hook_additional_context"' not in line:
                continue
            try:
                rec = json.loads(line)
            except json.JSONDecodeError:
                continue
            ts = str(rec.get("timestamp", ""))
            if since is not None and ts < since:
                continue
            if until is not None and ts > until:
                continue
            att = rec.get("attachment") or {}
            if att.get("type") != "hook_additional_context":
                continue
            pf = " ".join(texto(att.get("content")).split())[:HEAD]
            if classificar(pf) != "nao classificado":
                continue
            cell = acc[pf]
            cell[0] += len(json.dumps(att.get("content"), ensure_ascii=False))
            cell[1] += 1
    return acc


def main():
    top = 30
    if "--top" in sys.argv:
        top = int(sys.argv[sys.argv.index("--top") + 1])
    files = sorted(PROJ.glob("*.jsonl"), key=lambda f: f.stat().st_mtime, reverse=True)
    base_set = json.loads((HERE / "c1_por_familia.json").read_text())["base_set"]
    base_set = [PROJ / n for n in base_set]
    post_set = [f for f in files if f.stat().st_mtime >= datetime.fromisoformat(
        DEPLOY_UTC.replace("Z", "+00:00")).timestamp()]

    nb = varre_nc(base_set)
    npc = varre_nc(post_set, since=DEPLOY_UTC, until=START_UTC)
    print(f"base: {len(nb)} prefixos distintos · {sum(v[0] for v in nb.values()):,} chars")
    print(f"pos : {len(npc)} prefixos distintos · {sum(v[0] for v in npc.values()):,} chars\n")
    print(f"  {'chars_pos':>10} {'base':>10} {'pos':>9} {'blocos':>7}  prefixo literal")
    for pf, (c, b) in sorted(npc.items(), key=lambda kv: -kv[1][0])[:top]:
        cb = nb.get(pf, [0, 0])[0]
        print(f"  {c:>10,} {cb:>10,} {c:>9,} {b:>7}  {pf[:96]!r}")
    (HERE / "nao_classificado.json").write_text(json.dumps(
        {"base": {k: v for k, v in nb.items()}, "pos": {k: v for k, v in npc.items()}},
        ensure_ascii=False, indent=1))


if __name__ == "__main__":
    sys.exit(main())