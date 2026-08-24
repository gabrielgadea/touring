"""Taxonomia dos comandos Bash + distribuicao de rajada + conformidade P9.

Responde a UMA pergunta de projeto: onde um gate PreToolUse teria tracao, e com
que limiar, sem bloquear trabalho legitimo.
"""
import json, glob, re, collections, os
RAIZ = os.path.expanduser("~/.claude/projects")

CLASSES = [
 ("code_mode",   re.compile(r"\btouring run\b")),
 ("touring_cli", re.compile(r"\btouring \w")),
 ("inspecao",    re.compile(r"\b(grep|rg|sed -n|cat |head |tail |ls |find |wc |sqlite3|jq )\b")),
 ("build_test",  re.compile(r"\b(cargo|pytest|npm |make |go test)\b")),
 ("git",         re.compile(r"\bgit \w")),
 ("escrita",     re.compile(r"(>>?\s*[^&\s]|\bcat\s*>|\brm |\bmv |\bmkdir |sed -i)")),
 ("processo",    re.compile(r"\b(ps |kill|pgrep|systemctl|timeout |sleep )\b")),
]
def classificar(cmd):
    for nome, rx in CLASSES:
        if rx.search(cmd): return nome
    return "outro"

freq = collections.Counter()
dist_rajada = collections.Counter()
rajada_por_classe = collections.Counter()
edit_burst_total = 0
edit_burst_validado = 0
inspecao_em_rajada = 0

for f in sorted(glob.glob(f"{RAIZ}/**/*.jsonl", recursive=True)):
    seq = []
    try:
        with open(f, encoding="utf-8", errors="ignore") as fh:
            for linha in fh:
                try: d = json.loads(linha)
                except Exception: continue
                msg = d.get("message") or {}
                cont = msg.get("content")
                if not isinstance(cont, list): continue
                for c in cont:
                    if isinstance(c, dict) and c.get("type") == "tool_use":
                        seq.append((c.get("name","?"), c.get("input") or {}))
    except OSError: continue
    if len(seq) < 5: continue

    # taxonomia + rajadas de Bash
    atual = 0; classes_na_rajada = []
    for nome, inp in seq:
        if nome == "Bash":
            cmd = str(inp.get("command",""))
            cl = classificar(cmd); freq[cl] += 1
            atual += 1; classes_na_rajada.append(cl)
        else:
            if atual >= 1:
                dist_rajada[min(atual, 20)] += 1
                if atual >= 3:
                    dom = collections.Counter(classes_na_rajada).most_common(1)[0][0]
                    rajada_por_classe[dom] += 1
                    inspecao_em_rajada += sum(1 for x in classes_na_rajada if x == "inspecao")
            atual = 0; classes_na_rajada = []
    if atual >= 1:
        dist_rajada[min(atual, 20)] += 1

    # P9: uma RAJADA de Edit termina em validacao (Bash build/test)?
    i = 0
    while i < len(seq):
        if seq[i][0] in ("Edit","Write"):
            j = i
            while j < len(seq) and seq[j][0] in ("Edit","Write"): j += 1
            edit_burst_total += 1
            # olha as 4 chamadas seguintes por um build/test
            achou = any(seq[k][0]=="Bash" and classificar(str(seq[k][1].get("command","")))=="build_test"
                        for k in range(j, min(j+4, len(seq))))
            edit_burst_validado += achou
            i = j
        else: i += 1

tot = sum(freq.values())
print(f"BASH_CLASSIFICADOS={tot}")
for n,c in freq.most_common():
    print(f"   {n:<14} {c:>5}  {100*c//max(tot,1):>3}%")
print("\n-- distribuicao do tamanho da rajada de Bash (20+ agrupado)")
acum = 0; total_r = sum(dist_rajada.values())
for k in sorted(dist_rajada):
    acum += dist_rajada[k]
    print(f"   len={k:<3} n={dist_rajada[k]:<5} acum={100*acum//max(total_r,1):>3}%")
print("\n-- classe DOMINANTE nas rajadas >=3")
for n,c in rajada_por_classe.most_common():
    print(f"   {n:<14} {c:>5}")
print(f"\nCHAMADAS_DE_INSPECAO_DENTRO_DE_RAJADA={inspecao_em_rajada}")
print(f"\n-- P9 (Verify-After)")
print(f"   rajadas de Edit/Write: {edit_burst_total}")
print(f"   seguidas de build/test em <=4 chamadas: {edit_burst_validado} "
      f"({100*edit_burst_validado//max(edit_burst_total,1)}%)")
