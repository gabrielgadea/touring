#!/usr/bin/env python3
# #tags: kind:script purpose:kv-cache-hygiene domain:code-mode
"""S-8.2 — auditoria por programa da higiene KV-cache das injeções de hook.

O postmortem do dsh: bytes VARIÁVEIS no início do contexto invalidam o prefixo
estável do KV-cache. No Claude Code as injeções de hook entram como conteúdo
APPEND-ONLY do stream (não reescrevem prefixo) — o risco real é (a) qualquer
variância por-request em posição inicial e (b) o CUSTO em tokens dos bytes
variáveis repetidos em toda chamada. Este script MEDE, por família de injeção:
total de bytes, bytes variáveis (timestamps/sig=/cached/counters) e a posição
do primeiro byte variável. Saída: só o agregado.
"""
import json, re, sys, glob, os, collections

base = os.path.expanduser("~/.claude/projects/-home-gabrielgadea-projects-touring")
alvo = sys.argv[1] if len(sys.argv) > 1 else max(glob.glob(f"{base}/*.jsonl"), key=os.path.getmtime)

VARIAVEIS = [
    ("timestamp_iso", re.compile(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}[\d:.Z+-]*")),
    ("sig=", re.compile(r"sig=[\w:.-]+")),
    ("cached_ttl", re.compile(r"\(cached \d+s[^)]*\)")),
    ("contadores", re.compile(r"\b(count|score|health)[\"=: ]+[\d.]+", re.I)),
    ("session_id", re.compile(r"session_id=[0-9a-f-]{8,}")),
]
FAMILIA_RE = re.compile(r"\[(TOURING SUGGEST[^\]·]*(?:· [\w-]+)?|G\d [\w-]+|FLOW GUARD|SCOUT PERP[ÉE]TUO)")

familias = collections.defaultdict(lambda: {"n": 0, "bytes": 0, "var_bytes": 0, "primeira_pos_var": []})
MARCADORES = ("[TOURING SUGGEST", "[G1 ", "[G2 ", "[G6 ", "[G7 ", "[FLOW GUARD]", "[SCOUT PERP")
with open(alvo, encoding="utf-8") as fh:
    for linha in fh:
        if not any(m in linha for m in MARCADORES):
            continue
        try:
            reg = json.loads(linha)
        except json.JSONDecodeError:
            continue
        def blocos(o):
            if isinstance(o, str):
                if any(m in o for m in MARCADORES):
                    yield o
            elif isinstance(o, dict):
                for v in o.values(): yield from blocos(v)
            elif isinstance(o, list):
                for i in o: yield from blocos(i)
        for texto in blocos(reg):
            m = FAMILIA_RE.search(texto)
            fam = (m.group(1)[:40] if m else "(sem-rotulo)").strip()
            f = familias[fam]
            f["n"] += 1
            f["bytes"] += len(texto)
            posicoes = []
            for _, rx in VARIAVEIS:
                for mm in rx.finditer(texto):
                    f["var_bytes"] += len(mm.group(0))
                    posicoes.append(mm.start())
            if posicoes:
                f["primeira_pos_var"].append(min(posicoes) / max(1, len(texto)))

resumo = {}
for fam, f in sorted(familias.items(), key=lambda kv: -kv[1]["bytes"])[:10]:
    pos = f["primeira_pos_var"]
    resumo[fam] = {
        "injecoes": f["n"],
        "kb_total": round(f["bytes"] / 1024, 1),
        "pct_variavel": round(100 * f["var_bytes"] / max(1, f["bytes"]), 1),
        "pos_relativa_1o_variavel": round(sum(pos) / len(pos), 2) if pos else None,
    }
print(json.dumps({"transcript": os.path.basename(alvo)[:16],
                  "veredito_arquitetural": "injecoes de hook sao append-only no stream (nao reescrevem prefixo) — risco KV real e o custo em tokens dos bytes variaveis, nao invalidacao de cache",
                  "familias": resumo}, ensure_ascii=False, indent=1))
