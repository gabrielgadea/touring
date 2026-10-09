"""C1 POR FAMILIA — onde esta a queda de 898 -> ~540 chars por chamada de conselho injetado.

Entrega: tabela familia x (base, pos, delta, % do delta total) em DOIS cortes
  Corte 1 = hook emissor (o que medir_conselho.py ja da)
  Corte 2 = prefixo do texto injetado, por predicado POSITIVO; o resto em "nao classificado"

Cada familia traz numerador, denominador e metodo. Conservacao exigida: a soma das familias
reproduz EXATAMENTE o total de chars da janela, nos dois cortes.

Definicoes (deliberadamente nao misturadas):
  B (juiz C1, verificar_compactacao._advice_since): so hook_additional_context,
    chars = len(json.dumps(content, ensure_ascii=False))
  A (medir_conselho.py, a da linha de base 898): os 3 tipos de advice, chars = len(texto)

Reconstrucao da linha de base: o conjunto original NAO foi gravado e o mtime nao serve
(transcripts continuam sendo escritos e o mtime avancou). reconstroi-se por ATIVIDADE: os
transcripts cuja ultima mensagem ate o cutoff e a mais proxima dele sao os que estavam
sendo usados na medicao.

Uso: python3 c1_por_familia.py [--n-base 12]
"""

import json
import re
import sys
from collections import defaultdict
from datetime import datetime
from pathlib import Path
from typing import TypedDict

BUNDLE = Path.home() / "projects/touring/docs/plans/2026-10-08-compactacao-65"
HERE = BUNDLE / "diagnostics/c1-por-familia-2026-10-09"
PROJ = Path.home() / ".claude/projects/-home-gabrielgadea-projects-analise"
DEPLOY_UTC = json.loads((BUNDLE / "diagnostics/item1.json").read_text())["deploy_utc"]
START_UTC = "2026-10-09T13:53:47Z"
BASE_CUTOFF = "2026-10-08T17:00:00"
LIMIAR_C1 = 898 * 0.6
HEAD = 110


class Acumulador(TypedDict):
    """O que uma janela acumula. Tipos explicitos: um dict heterogeneo desorienta o type checker."""
    calls: int
    chars: int
    blocos: int
    cells: dict[str, dict[str, list[int]]]


# ---------------------------------------------------------------- identidade e texto

def hook_name(att):
    """Mesma identidade de medir_conselho.hook_name (linha 33-40), sem reescrever a logica."""
    raw = next((att[k] for k in ("hookName", "command", "hookCommand") if isinstance(att.get(k), str)), "?")
    parts = [p for p in raw.split() if not p.startswith("python")]
    if not parts:
        return "?"
    base = re.sub(r"^.*/", "", parts[0])
    return f"{base} {parts[1]}" if base == "touring-hook" and len(parts) > 1 else base[:48]


def texto(content):
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return " ".join(c for c in content if isinstance(c, str))
    return json.dumps(content, ensure_ascii=False)


def chars_juiz(att):
    return len(json.dumps(att.get("content"), ensure_ascii=False))


def prefixo(s):
    return " ".join((s or "").split())[:HEAD]


# ---------------------------------------------------------------- familias (corte 2)

# Cada familia e um PREDICADO POSITIVO sobre o inicio literal do texto injetado. A lista nao veio de
# suposicao: foi aberta medindo o balde 'nao classificado' (abrir_nao_classificado.py), que mostrou
# os prefixos reais. Os emojis sao literais de proposito — escape de byte erra o codepoint em silencio.
FAMILIAS = [
    ("env-context/navegador (Touring Knowledge, Next, ANN)", r"^(Touring Knowledge|🔮|📚|📖|🔁|🪞|🔍|ANN related|health-delta|scope shadowing|kind context)"),
    ("SCOUT PERPETUO (gerador de demanda)", r"^\[SCOUT PER"),
    ("ATZILUTH (painel de emanacao)", r"^🌅 ATZILUTH|^\[Guidance from previous|^Pendências registradas"),
    ("TOURING SUGGEST (enriquecimento do indice)", r"^\[TOURING SUGGEST"),
    ("CODE MODE (contrato da sessao)", r"^\[CODE MODE"),
    ("HOTSPOT (wiring: quem chama)", r"^HOTSPOT:"),
    ("PROMPT ENHANCEMENT (general mode)", r"^\[PROMPT ENHANCEMENT"),
    ("ast overview / leitura virtualizada", r"^💡"),
    ("code-workflow (complexidade/simbolos)", r"^⚙ code-workflow|^⚙ health-delta"),
    ("defs[] (mapa de definicoes)", r"^📌 defs\["),
    ("SHADOW (scope shadowing)", r"^SHADOW:"),
    ("Pre-edit prevention", r"^Pre-edit prevention"),
    ("EVIDENCE GUARD", r"EVIDENCE GUARD"),
    ("medicao sem relacao verificada", r"^\[medi[çc][ãa]o sem rela[çc][ãa]o"),
    ("post-edit verification", r"^post-edit verification"),
    ("EVENT_READER (hook reader)", r"^\[EVENT_READER\]"),
    ("post-tool-batch (sugestao de consumidor)", r"^post-tool-batch:"),
    ("post-write (health-delta)", r"^post-write:"),
    ("hook modify notice (PostToolUse hook modified)", r"^PostToolUse hook modified"),
    ("MUST/SHOULD/MAY (diretivas)", r"^(MUST|SHOULD|MAY)"),
    ("CODE-FIRST / hook gate generico [generic]", r"^\[generic\]"),
    ("CODE MODE hook-error/collision", r"^(CODE MODE|write the file|conflict|Hooks pr)"),
    ("seguranca de plugin", r"^\[from .*plugin\]"),
    ("vazio (content == '')", r"^$"),
    ("literal 'null' (content ausente)", r"^null$"),
]


def classificar(pfx):
    """Predicado POSITIVO por familia; o resto em 'nao classificado' (visivel, nunca somado)."""
    for nome, pat in FAMILIAS:
        if re.search(pat, pfx):
            return nome
    return "nao classificado"


# ---------------------------------------------------------------- varredura

def extremos(f):
    """(primeiro_ts, ultimo_ts) do arquivo. Le a primeira e a ultima linha com timestamp."""
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


def varre(files, since=None, until=None) -> Acumulador:
    """Acumula chamadas + chars(B) e o cruzamento bruto hook x prefixo."""
    a: Acumulador = {"calls": 0, "chars": 0, "blocos": 0, "cells": {}}
    for f in files:
        for line in f.open(encoding="utf-8", errors="replace"):
            if '"tool_use"' not in line and '"hook_additional_context"' not in line:
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
            if att.get("type") == "hook_additional_context":
                hk, pf = hook_name(att), prefixo(texto(att.get("content")))
                self_map = a["cells"].setdefault(hk, {})
                cell = self_map.setdefault(pf, [0, 0])
                n = chars_juiz(att)
                cell[0] += n
                cell[1] += 1
                a["chars"] += n
                a["blocos"] += 1
            if rec.get("type") == "assistant":
                c = (rec.get("message") or {}).get("content") or []
                a["calls"] += sum(1 for x in c if isinstance(x, dict) and x.get("type") == "tool_use")
    return a


def por_corte(ac, corte):
    """chars e blocos por familia no corte pedido (hook emissor | prefixo)."""
    fam = defaultdict(lambda: [0, 0])
    for hook, pfx_map in ac["cells"].items():
        for pf, (ch, bl) in pfx_map.items():
            nome = hook if corte == "hook" else classificar(pf)
            fam[nome][0] += ch
            fam[nome][1] += bl
    return fam


def tabela(saida, corte, base, post):
    """Monta as linhas da tabela e verifica a conservacao exigida.

    Conservacao e sobre CHARS: a soma das familias tem de reproduir base['chars'] e post['chars'],
    exatamente. O denominador de '% do delta total' e o delta de chars (post - base), nao o delta
    de chamadas — misturar os dois produz percentuais acima de 100, que nao sao erro de medicao:
    sao erro de denominador.
    """
    fb, fp = por_corte(base, corte), por_corte(post, corte)
    n_b, n_p = base["calls"], post["calls"]
    cc_b, cc_p = base["chars"], post["chars"]
    delta_total = cc_p - cc_b
    linhas = []
    for nome in sorted(set(fb) | set(fp), key=lambda k: -(fp[k][0] - fb[k][0])):
        cb, npc = fb[nome][0], fp[nome][0]
        pcb, pcp = cb / n_b, npc / n_p
        d = npc - cb
        linhas.append({"familia": nome, "base_chars": cb, "pos_chars": npc,
                       "base_blocos": fb[nome][1], "pos_blocos": fp[nome][1],
                       "base_por_chamada": round(pcb, 4), "pos_por_chamada": round(pcp, 4),
                       "delta_chars": d, "delta_por_chamada": round(pcp - pcb, 4),
                       "pct_do_delta_total": round(d / delta_total * 100, 2)})
    soma_b, soma_p = sum(l["base_chars"] for l in linhas), sum(l["pos_chars"] for l in linhas)
    ok = soma_b == cc_b and soma_p == cc_p
    print(f"\n=== CORTE {corte.upper()} · conservacao de CHARS: base {soma_b:,}=={cc_b:,} · "
          f"pos {soma_p:,}=={cc_p:,} -> {'EXATA' if ok else 'FALHA'} ===")
    print(f"  {'familia':<50} {'base/ch':>8} {'pos/ch':>8} {'D/ch':>8} {'% do D':>8}")
    for l in linhas:
        print(f"  {l['familia'][:50]:<50} {l['base_por_chamada']:>8.1f} {l['pos_por_chamada']:>8.1f} "
              f"{l['delta_por_chamada']:>8.1f} {l['pct_do_delta_total']:>7.1f}%")
    saida["cortes"][corte] = {"conservacao": bool(ok), "conservacao_base": soma_b,
                              "conservacao_pos": soma_p, "linhas": linhas}


def main():
    n_base = 12
    if "--n-base" in sys.argv:
        n_base = int(sys.argv[sys.argv.index("--n-base") + 1])

    files = sorted(PROJ.glob("*.jsonl"), key=lambda f: f.stat().st_mtime, reverse=True)

    print(f"### reconstruindo o conjunto da base por atividade ate {BASE_CUTOFF} …", flush=True)
    candidatos = []
    for f in files:
        first, last = extremos(f)
        if first is None or last is None or first > BASE_CUTOFF:
            continue                      # comecou depois da medicao: nao existia
        candidatos.append((min(last, BASE_CUTOFF), f))
    candidatos.sort(key=lambda t: t[0], reverse=True)
    base_set = [f for _, f in candidatos[:n_base]]

    corte_deploy = datetime.fromisoformat(DEPLOY_UTC.replace("Z", "+00:00")).timestamp()
    post_set = [f for f in files if f.stat().st_mtime >= corte_deploy]

    print("### varredura BASE …", flush=True)
    base = varre(base_set)
    print("### varredura POST …", flush=True)
    post = varre(post_set, since=DEPLOY_UTC, until=START_UTC)

    print("\n=== TOTAIS (metrica B do juiz C1) ===")
    for nome, a in (("BASE", base), ("POST", post)):
        print(f"  {nome}: chamadas={a['calls']:>6}  chars={a['chars']:>9,}  blocos={a['blocos']:>6}  "
              f"{a['chars'] / a['calls']:.1f}/chamada")
    pb, pp = base["chars"] / base["calls"], post["chars"] / post["calls"]
    print(f"  queda = {(1 - pp / pb) * 100:.1f}%   limiar C1 = {LIMIAR_C1:.1f}/chamada   "
          f"-> {'PASSA' if pp <= LIMIAR_C1 else 'REPROVA'}")
    print(f"\n  BASE set ({len(base_set)}): {[f.name[:8] for f in base_set]}")

    saida = {"deploy_utc": DEPLOY_UTC, "start_utc": START_UTC, "base_cutoff": BASE_CUTOFF,
             "metrica": "B: so hook_additional_context, chars = len(json.dumps(content))",
             "base_set": [f.name for f in base_set], "post_set": [f.name for f in post_set],
             "base": {"calls": base["calls"], "chars": base["chars"], "blocos": base["blocos"]},
             "post": {"calls": post["calls"], "chars": post["chars"], "blocos": post["blocos"]},
             "queda_pct": round((1 - pp / pb) * 100, 2), "limiar": LIMIAR_C1,
             "cortes": {}}
    for corte in ("hook", "prefixo"):
        tabela(saida, corte, base, post)

    (HERE / "c1_por_familia.json").write_text(json.dumps(saida, ensure_ascii=False, indent=1))
    print(f"\njson -> {HERE / 'c1_por_familia.json'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())