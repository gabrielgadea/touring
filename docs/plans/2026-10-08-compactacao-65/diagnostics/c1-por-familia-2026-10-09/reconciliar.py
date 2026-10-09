"""PASSE 2: reconciliacao das duas definicoes + busca do conjunto exato da linha de base.

Duas metricas, deliberadamente NAO misturadas:
  A (medir_conselho, a da linha de base 898): os 3 tipos de advice, chars = len(texto|json)
  B (juiz C1, a do 592): so hook_additional_context, chars = len(json.dumps(content))

O conjunto da base nao foi gravado. Busca-se o que reproduz 8.441 chamadas / 898 chars por chamada
sob a metrica A. Toda janela declarada mostra as DUAS metricas lado a lado.

Uso: python3 reconciliar.py
"""

import json
import sys
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

BUNDLE = Path.home() / "projects/touring/docs/plans/2026-10-08-compactacao-65"
HERE = BUNDLE / "diagnostics/c1-por-familia-2026-10-09"
PROJ = Path.home() / ".claude/projects/-home-gabrielgadea-projects-analise"
DEPLOY_UTC = "2026-10-09T01:11:50Z"
START_UTC = "2026-10-09T13:53:47Z"
BASE_CUTOFF = "2026-10-08T17:00:00"
ADVICE = ("hook_additional_context", "hook_success", "async_hook_response")
ALVO_CALLS, ALVO_PER_CALL = 8441, 898.0


def iso(e):
    return datetime.fromtimestamp(e, timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def texto(content):
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return " ".join(c for c in content if isinstance(c, str))
    return json.dumps(content, ensure_ascii=False)


def metricas(files, since=None, until=None):
    """Chamadas + chars sob A e B. `since`/`until` inclusivos sobre rec['timestamp']."""
    calls = 0
    a_chars = b_chars = 0
    a_blocks = 0
    por_tipo = Counter()
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
            if att.get("type") in ADVICE:
                n = len(texto(att.get("content")))
                a_chars += n
                a_blocks += 1
                por_tipo[att["type"]] += 1
                if att["type"] == "hook_additional_context":
                    b_chars += len(json.dumps(att.get("content"), ensure_ascii=False))
            if rec.get("type") != "assistant":
                continue
            c = (rec.get("message") or {}).get("content") or []
            calls += sum(1 for x in c if isinstance(x, dict) and x.get("type") == "tool_use")
    return {"calls": calls, "A_chars": a_chars, "A_blocos": a_blocks, "A_per_call": a_chars / calls if calls else None,
            "B_chars": b_chars, "B_per_call": b_chars / calls if calls else None,
            "A_por_tipo": dict(por_tipo)}


def main():
    files = sorted(PROJ.glob("*.jsonl"), key=lambda f: f.stat().st_mtime, reverse=True)
    dentro = [f for f in files if iso(f.stat().st_mtime) <= BASE_CUTOFF]

    print("=== BUSCA DO CONJUNTO DA LINHA DE BASE (metrica A: 8.441 chamadas / 898 chars por chamada) ===")
    print("    mtime <= 2026-10-08T17:00Z (a medicao foi ~17h de 08/10); N = mais recentes por mtime\n")
    melhor = None
    for n in range(8, 26):
        m = metricas(dentro[:n])
        marca = ""
        if m["calls"] == ALVO_CALLS:
            marca = "  <== chamadas EXATAS"
        if melhor is None or abs(m["calls"] - ALVO_CALLS) < abs(melhor[1]["calls"] - ALVO_CALLS):
            melhor = (n, m)
        print(f"  N={n:>2}  chamadas={m['calls']:>6}  A/chamada={m['A_per_call']:>8.1f}  "
              f"B/chamada={m['B_per_call']:>8.1f}{marca}")
    print(f"\n  melhor por chamadas: N={melhor[0]} -> {melhor[1]['calls']} chamadas "
          f"(erro {melhor[1]['calls'] - ALVO_CALLS:+d} vs 8.441)")

    print("\n=== JANELA POST (deploy_utc -> START_UTC, a definicao do juiz _advice_since) ===")
    post_set = [f for f in files if iso(f.stat().st_mtime) >= DEPLOY_UTC]
    post_b = metricas(post_set, since=DEPLOY_UTC, until=START_UTC)
    print(f"  transcripts={len(post_set)}  chamadas={post_b['calls']}")
    print(f"  B (juiz)  = {post_b['B_chars']:>8} chars -> {post_b['B_per_call']:.1f}/chamada")
    print(f"  A (medir) = {post_b['A_chars']:>8} chars -> {post_b['A_per_call']:.1f}/chamada")
    print(f"  tipos: {post_b['A_por_tipo']}")

    print("\n=== JANELA POST ABERTA (sem limite superior: e o que o juiz roda hoje) ===")
    post_a = metricas(post_set, since=DEPLOY_UTC)
    print(f"  chamadas={post_a['calls']}  B={post_a['B_chars']} ({post_a['B_per_call']:.1f}/chamada)")

    print("\n=== O QUE O JUIZ C1 DA HOJE ===")
    calls, chars = None, None
    sys.path.insert(0, str(BUNDLE / "scripts"))
    import verificar_compactacao as V  # noqa: E402  (leitura; nada e escrito)
    calls, chars = V._advice_since(DEPLOY_UTC)
    print(f"  _advice_since({DEPLOY_UTC}) = {chars} chars / {calls} chamadas = {chars / calls:.1f} por chamada")
    print(f"  limiar C1 = 898*0.6 = {898 * 0.6:.1f}  ->  {'PASSA' if chars / calls <= 898 * 0.6 else 'REPROVA'}")

    out = {"deploy_utc": DEPLOY_UTC, "start_utc": START_UTC,
           "post_fechado": post_b, "post_aberto": post_a,
           "juiz_hoje": {"calls": calls, "chars": chars, "per_call": chars / calls},
           "base": {str(n): metricas(dentro[:n]) for n in (12, melhor[0])}}
    (HERE / "reconciliacao.json").write_text(json.dumps(out, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    import sys
    sys.exit(main())