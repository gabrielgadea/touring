"""Rodada 2 — motifs de workflow (trigramas+) e re-inspeção do mesmo alvo.

Pergunta de projeto: que SEQUÊNCIAS têm forma fixa o bastante para um gate
reconhecê-las e oferecer o colapso (1 programa) em vez do passo N+1?
"""
import json, glob, re, collections, os
RAIZ = os.path.expanduser("~/.claude/projects")

def classe_bash(cmd):
    if "touring run " in cmd: return "b:run"
    if re.search(r"\btouring \w", cmd): return "b:touring"
    if re.search(r"\b(cargo|pytest|npm |make )", cmd): return "b:build"
    if re.search(r"\bgit \w", cmd): return "b:git"
    if re.search(r"(>>?\s*[^&\s]|\bcat\s*>|\brm |\bmv |sed -i)", cmd): return "b:write"
    if re.search(r"\b(grep|rg|sed -n|cat |head |tail |ls |find |wc |sqlite3|jq )", cmd): return "b:inspect"
    return "b:other"

tri = collections.Counter()
quad = collections.Counter()
reinspecao = collections.Counter()   # mesmo arquivo inspecionado N vezes na sessão
arquivos_reinspec = collections.Counter()
ARQ_RE = re.compile(r"[\w./~-]+\.(?:rs|py|toml|md|json|sh|js|ts)\b")

for f in sorted(glob.glob(f"{RAIZ}/**/*.jsonl", recursive=True)):
    seq = []
    alvos = collections.Counter()
    try:
        with open(f, encoding="utf-8", errors="ignore") as fh:
            for linha in fh:
                try: d = json.loads(linha)
                except Exception: continue
                cont = (d.get("message") or {}).get("content")
                if not isinstance(cont, list): continue
                for c in cont:
                    if not isinstance(c, dict) or c.get("type") != "tool_use": continue
                    nome = c.get("name","?"); inp = c.get("input") or {}
                    if nome == "Bash":
                        cmd = str(inp.get("command",""))
                        seq.append(classe_bash(cmd))
                        for m in ARQ_RE.findall(cmd)[:3]:
                            if "inspect" in classe_bash(cmd): alvos[m] += 1
                    elif nome in ("Read","Edit","Write"):
                        seq.append(nome)
                        fp = str(inp.get("file_path",""))
                        if nome == "Read" and fp: alvos[fp.rsplit("/",1)[-1]] += 1
                    else:
                        seq.append(nome if len(nome) < 14 else "MCP/other")
    except OSError: continue
    if len(seq) < 8: continue
    for i in range(len(seq)-2):
        tri[tuple(seq[i:i+3])] += 1
    for i in range(len(seq)-3):
        quad[tuple(seq[i:i+4])] += 1
    for arq, n in alvos.items():
        if n >= 3:
            reinspecao[min(n,10)] += 1
            arquivos_reinspec[arq] += n

print("-- trigramas mais frequentes (top 14)")
for t, c in tri.most_common(14):
    print(f"   {c:>5}  {' > '.join(t)}")
print("\n-- quadrigramas de FORMA FIXA (top 8, excluindo b:inspect^4)")
mostrados = 0
for t, c in quad.most_common(60):
    if len(set(t)) == 1: continue
    print(f"   {c:>5}  {' > '.join(t)}")
    mostrados += 1
    if mostrados >= 8: break
print("\n-- re-inspecao do MESMO alvo (>=3x na sessao)")
tot = sum(reinspecao.values())
print(f"   casos={tot}")
for k in sorted(reinspecao): print(f"   {k}x: {reinspecao[k]}")
print("\n-- alvos mais re-inspecionados")
for a, n in arquivos_reinspec.most_common(8): print(f"   {n:>4}  {a[:60]}")
