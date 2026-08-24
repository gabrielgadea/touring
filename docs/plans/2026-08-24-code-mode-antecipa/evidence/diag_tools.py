"""Diagnóstico de TODAS as tool calls sobre a amostra de sessões.

Emite só o agregado (reflexo D5): frequências, bigramas, rajadas, redundância e
os candidatos a gate — nunca o dump das chamadas.
"""
import json, glob, re, collections, os

RAIZ = os.path.expanduser("~/.claude/projects")
arquivos = sorted(glob.glob(f"{RAIZ}/**/*.jsonl", recursive=True))

freq = collections.Counter()
bigrama = collections.Counter()
por_sessao = collections.Counter()
redundante = collections.Counter()      # (tool, hash do input) repetido na MESMA sessão
rajadas = collections.Counter()         # tool -> nº de rajadas >=3 consecutivas
maior_rajada = collections.Counter()
sessoes_validas = 0
total_calls = 0

# antipadrões estruturais
ap = collections.Counter()

for f in arquivos:
    seq = []
    vistos = {}
    try:
        with open(f, encoding="utf-8", errors="ignore") as fh:
            for linha in fh:
                try: d = json.loads(linha)
                except Exception: continue
                msg = d.get("message") or {}
                cont = msg.get("content")
                if not isinstance(cont, list): continue
                for c in cont:
                    if not isinstance(c, dict) or c.get("type") != "tool_use": continue
                    nome = c.get("name", "?")
                    inp = c.get("input") or {}
                    seq.append((nome, inp))
    except OSError:
        continue
    if len(seq) < 5:
        continue
    sessoes_validas += 1
    por_sessao[f.split("/")[-2]] += len(seq)
    total_calls += len(seq)

    for i, (nome, inp) in enumerate(seq):
        freq[nome] += 1
        chave = (nome, json.dumps(inp, sort_keys=True)[:400])
        vistos[chave] = vistos.get(chave, 0) + 1
        if i + 1 < len(seq):
            bigrama[(nome, seq[i+1][0])] += 1

    for (nome, _), n in vistos.items():
        if n > 1:
            redundante[nome] += n - 1

    # rajadas por tool
    atual_nome, atual_n = None, 0
    for nome, _ in seq:
        if nome == atual_nome:
            atual_n += 1
        else:
            if atual_n >= 3:
                rajadas[atual_nome] += 1
                maior_rajada[atual_nome] = max(maior_rajada[atual_nome], atual_n)
            atual_nome, atual_n = nome, 1
    if atual_n >= 3:
        rajadas[atual_nome] += 1
        maior_rajada[atual_nome] = max(maior_rajada[atual_nome], atual_n)

    # Read sem localizar antes (janela de 3)
    for i, (nome, inp) in enumerate(seq):
        if nome == "Read":
            janela = [n for n, _ in seq[max(0, i-3):i]]
            if not any(x in ("Grep", "Glob", "Bash") for x in janela):
                ap["ReadSemLocalizar"] += 1
        if nome == "Edit":
            alvo = str(inp.get("file_path", ""))
            leu = any(n == "Read" and str(j.get("file_path", "")) == alvo
                      for n, j in seq[max(0, i-12):i])
            if alvo and not leu:
                ap["EditSemRead"] += 1

print(f"SESSOES={sessoes_validas}  TOOL_CALLS={total_calls}")
print("\n-- frequencia por tool (top 14)")
for n, c in freq.most_common(14):
    print(f"   {n:<28} {c:>6}  {100*c//max(total_calls,1):>3}%")
print("\n-- bigramas mais fortes (top 16)  A -> B")
for (a, b), c in bigrama.most_common(16):
    saidas = sum(v for (x, _), v in bigrama.items() if x == a)
    print(f"   {a:>14} -> {b:<16} {c:>5}   P(B|A)={100*c//max(saidas,1):>3}%")
print("\n-- rajadas >=3 do MESMO tool")
for n, c in rajadas.most_common(8):
    print(f"   {n:<28} {c:>5} rajadas   maior={maior_rajada[n]}")
print("\n-- chamadas REDUNDANTES (mesmo tool+input na sessao)")
for n, c in redundante.most_common(8):
    print(f"   {n:<28} {c:>5} repeticoes")
print("\n-- antipadroes estruturais")
for n, c in ap.most_common():
    print(f"   {n:<28} {c:>5}")
