"""Extract the final assistant text of a subagent JSONL transcript into the findings folder.

Usage: extrai_agente.py <transcript.jsonl> <saida.json>
The JSON object in the last assistant message is pretty-printed; a non-JSON answer is kept as .txt.
"""
import json
import re
import sys
from pathlib import Path


def ultimo_texto(src: Path) -> str:
    ultimo = ""
    for line in src.open(encoding="utf-8", errors="replace"):
        try:
            rec = json.loads(line)
        except ValueError:
            continue
        msg = rec.get("message") or {}
        if rec.get("type") == "assistant" and isinstance(msg.get("content"), list):
            txt = "".join(c.get("text", "") for c in msg["content"] if isinstance(c, dict) and c.get("type") == "text")
            if txt.strip():
                ultimo = txt
    return ultimo


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__.strip().splitlines()[2], file=sys.stderr)
        return 2
    src, out = Path(sys.argv[1]), Path(sys.argv[2])
    out.parent.mkdir(parents=True, exist_ok=True)
    texto = ultimo_texto(src)
    m = re.search(r"\{.*\}", texto, re.DOTALL)
    corpo = m.group(0) if m else texto
    try:
        dados = json.loads(corpo)
    except ValueError as e:
        out.with_suffix(".txt").write_text(texto, encoding="utf-8")
        print(f"{out.name}: NOT valid JSON ({e}); kept as .txt, {len(texto)} chars")
        return 1
    out.write_text(json.dumps(dados, ensure_ascii=False, indent=1), encoding="utf-8")
    # Two shapes exist: first-round "findings" and gap-round "linhas"; count whichever the answer carries.
    chave = next((k for k in ("findings", "linhas") if k in dados), None)
    print(f"{out.name}: json ok, {chave or 'sem findings nem linhas'}={len(dados.get(chave, [])) if chave else 0}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
