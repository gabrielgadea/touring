"""PASSE 1 (cache): varre os transcripts e guarda o cruzamento (hookName x prefixo) por janela.

Nao classifica nada e nao decide nada: so materializa o dado bruto para o classificador do passe 2
operar sem re-varrer centenas de MB. Rodavel de novo; idempotente (sobrepoe o cache).

Janelas:
  POST  = [deploy_utc, start_utc]  por _advice_since (tipos e chars do juiz C1)
  BASE  = os N transcripts mais recentes por mtime com mtime <= BASE_CUTOFF (conjunto declarado)

Uso: python3 varrer.py [--n-base 12]
"""

import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

BUNDLE = Path.home() / "projects/touring/docs/plans/2026-10-08-compactacao-65"
HERE = BUNDLE / "diagnostics/c1-por-familia-2026-10-09"
PROJ = Path.home() / ".claude/projects/-home-gabrielgadea-projects-analise"
DEPLOY_UTC = json.loads((BUNDLE / "diagnostics/item1.json").read_text())["deploy_utc"]
START_UTC = "2026-10-09T13:53:47Z"          # instante em que a sessao COMECOU a medir (fixado)
BASE_CUTOFF = "2026-10-08T17:00:00"        # ~17h de 08/10, quando a linha de base foi medida
ADVICE = ("hook_additional_context", "hook_success", "async_hook_response")
HEAD = 110                                 # chars do prefixo preservados como chave


def iso(e):
    return datetime.fromtimestamp(e, timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def hook_name(att):
    """Mesma identidade de hook de medir_conselho.hook_name (importada la, nao reescrita aqui)."""
    raw = next((att[k] for k in ("hookName", "command", "hookCommand") if isinstance(att.get(k), str)), "?")
    parts = [p for p in raw.split() if not p.startswith("python")]
    if not parts:
        return "?"
    base = os.path.basename(parts[0])
    return f"{base} {parts[1]}" if base == "touring-hook" and len(parts) > 1 else base[:48]


def texto(content):
    """O conteudo injetado como texto. str e usada crua; lista e unida; resto vai ao JSON."""
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return " ".join(c for c in content if isinstance(c, str))
    return json.dumps(content, ensure_ascii=False)


def chars_juiz(att):
    """Chars do juiz C1: len(json.dumps(content)) — NAO o len do texto cru. As duas diferem em aspas."""
    return len(json.dumps(att.get("content"), ensure_ascii=False))


def prefixo(s):
    """Chave de classificacao: o comeco literal do texto injetado, com espacos normalizados."""
    return " ".join((s or "").split())[:HEAD]


def varre(files, acc, since=None, until=None):
    """Acumula (calls do juiz, chars do juiz, chars de medir_conselho por tipo) e o cruzamento bruto.

    `acc` = {"calls", "juiz_chars", "tool_chars", "tool_blocks", "cross", "prefixos", "events"}
    `since`/`until` sao limites INCLUSIVOS em ISO UTC sobre rec["timestamp"] (comparacao de string,
    a mesma do juiz).
    """
    for f in files:
        for line in f.open(encoding="utf-8", errors="replace"):
            # Pre-filtro identico ao do juiz: so linhas que podem conter tool_use ou advice entram.
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
                hk = hook_name(att)
                pfx = prefixo(texto(att.get("content")))
                cj = chars_juiz(att)
                ct = len(texto(att.get("content")))
                acc["calls"] += 0
                acc["juiz_chars"] += cj
                acc["tool_chars"] += ct
                acc["tool_blocks"] += 1
                cell = acc["cross"].setdefault(hk, {}).setdefault(pfx, [0, 0, 0])
                cell[0] += cj          # chars do juiz
                cell[1] += ct          # chars do medir_conselho
                cell[2] += 1           # blocos
                acc["prefixos"][pfx] = acc["prefixos"].get(pfx, 0) + cj
                acc["events"][att.get("hookEvent") or "?"] = \
                    acc["events"].get(att.get("hookEvent") or "?", 0) + cj

            if rec.get("type") != "assistant":
                continue
            content = (rec.get("message") or {}).get("content") or []
            acc["calls"] += sum(1 for c in content if isinstance(c, dict) and c.get("type") == "tool_use")
    return acc


def novo_acc():
    return {"calls": 0, "juiz_chars": 0, "tool_chars": 0, "tool_blocks": 0,
            "cross": {}, "prefixos": {}, "events": {}}


def main():
    n_base = 12
    if "--n-base" in sys.argv:
        n_base = int(sys.argv[sys.argv.index("--n-base") + 1])

    files = sorted(PROJ.glob("*.jsonl"), key=lambda f: f.stat().st_mtime, reverse=True)
    base_set = [f for f in files if iso(f.stat().st_mtime) <= BASE_CUTOFF][:n_base]

    # POST: todo transcript cujo mtime >= deploy_utc pode conter registros na janela. mtime anterior
    # ao deploy prova que o arquivo nao foi escrito depois dele -> nao pode conter a janela.
    post_set = [f for f in files if iso(f.stat().st_mtime) >= DEPLOY_UTC]

    post = varre(post_set, novo_acc(), since=DEPLOY_UTC, until=START_UTC)
    base = varre(base_set, novo_acc())

    cache = {
        "deploy_utc": DEPLOY_UTC, "start_utc": START_UTC, "base_cutoff": BASE_CUTOFF,
        "n_base_requested": n_base,
        "base_set": [{"name": f.name, "mtime": iso(f.stat().st_mtime),
                      "mb": round(f.stat().st_size / 1e6, 1)} for f in base_set],
        "post_set": [{"name": f.name, "mtime": iso(f.stat().st_mtime),
                      "mb": round(f.stat().st_size / 1e6, 1)} for f in post_set],
        "post": post, "base": base,
    }
    out = HERE / "cache_bruto.json"
    out.write_text(json.dumps(cache, ensure_ascii=False, indent=1))
    print(f"cache -> {out}")
    for nome, a in (("BASE", base), ("POST", post)):
        print(f"  {nome}: calls={a['calls']} juiz_chars={a['juiz_chars']} "
              f"tool_chars={a['tool_chars']} blocos={a['tool_blocks']}")
    print(f"\nBASE set ({len(base_set)}): {[f.name[:8] for f in base_set]}")
    print(f"POST set ({len(post_set)}): {[f.name[:8] for f in post_set]}")


if __name__ == "__main__":
    sys.exit(main())