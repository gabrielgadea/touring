# #tags: kind:snippet purpose:autoavaliacao-sem-dado domain:code-mode
"""R7 forense de transcript: ler o PRÓPRIO histórico por programa.

'Quantas vezes eu...' se responde com contagem, não com memória. Parametrize o
transcript e o padrão; sai só o agregado.
"""
import json, sys, collections, glob, os
pat = sys.argv[1] if len(sys.argv) > 1 else "Bash"
base = os.path.expanduser("~/.claude/projects/-home-gabrielgadea-projects-touring")
alvo = sys.argv[2] if len(sys.argv) > 2 else max(glob.glob(f"{base}/*.jsonl"), key=os.path.getmtime)
contagem = collections.Counter()
with open(alvo, encoding="utf-8") as fh:
    for linha in fh:
        if '"tool_use"' in linha and pat in linha:
            try:
                m = json.loads(linha).get("message", {})
                for c in m.get("content", []):
                    if isinstance(c, dict) and c.get("type") == "tool_use":
                        contagem[c.get("name", "?")] += 1
            except (json.JSONDecodeError, AttributeError):
                pass
print({"transcript": alvo.rsplit("/", 1)[-1][:20], "por_tool": dict(contagem.most_common(6))})
