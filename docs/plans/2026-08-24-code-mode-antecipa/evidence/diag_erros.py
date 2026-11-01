"""Rodada 2 — os ERROS que as sessões realmente viram (tool_result is_error).

Requisito 4 pedia o histórico de erros DAS SESSÕES, não só a memória curada.
Cada erro é classificado; a pergunta por classe: um programa teria evitado?
"""
import json, glob, re, collections, os
RAIZ = os.path.expanduser("~/.claude/projects")

CLASSES = [
 ("edit_string_not_found", re.compile(r"String to replace not found|old_string", re.I)),
 ("file_not_read_first",   re.compile(r"has not been read yet|Read tool first|read the file", re.I)),
 ("file_not_found",        re.compile(r"No such file|does not exist|not found.*file|ENOENT", re.I)),
 ("timeout",               re.compile(r"timed out|timeout", re.I)),
 ("exit_nonzero",          re.compile(r"Exit code [1-9]|exited with|non-zero", re.I)),
 ("permission_denied",     re.compile(r"[Pp]ermission denied|not allowed|blocked|hook", re.I)),
 ("compile_test_fail",     re.compile(r"error\[E\d+|FAILED|test result: FAILED|panicked", re.I)),
 ("json_parse",            re.compile(r"json|parse error|Unexpected token", re.I)),
 ("daemon_conn",           re.compile(r"Connection refused|reset by peer|daemon", re.I)),
]

cnt = collections.Counter()
por_tool = collections.Counter()
total_result = 0
total_err = 0
apos_erro = collections.Counter()   # o que veio DEPOIS do erro (retry cego?)
retry_identico = 0

for f in sorted(glob.glob(f"{RAIZ}/**/*.jsonl", recursive=True)):
    eventos = []  # (tipo, nome, input_hash, is_error, texto_erro)
    try:
        with open(f, encoding="utf-8", errors="ignore") as fh:
            for linha in fh:
                try: d = json.loads(linha)
                except Exception: continue
                cont = (d.get("message") or {}).get("content")
                if not isinstance(cont, list): continue
                for c in cont:
                    if not isinstance(c, dict): continue
                    if c.get("type") == "tool_use":
                        eventos.append(("use", c.get("name","?"),
                                        json.dumps(c.get("input") or {}, sort_keys=True)[:300], False, ""))
                    elif c.get("type") == "tool_result":
                        err = bool(c.get("is_error"))
                        txt = ""
                        conteudo = c.get("content")
                        if isinstance(conteudo, str): txt = conteudo[:400]
                        elif isinstance(conteudo, list):
                            txt = " ".join(str(x.get("text",""))[:200] for x in conteudo if isinstance(x, dict))[:400]
                        eventos.append(("result", "", "", err, txt))
    except OSError: continue

    ultimo_use = None
    for i, (tipo, nome, ih, err, txt) in enumerate(eventos):
        if tipo == "use":
            ultimo_use = (nome, ih); continue
        total_result += 1
        if not err: continue
        total_err += 1
        if ultimo_use: por_tool[ultimo_use[0]] += 1
        achou = False
        for cn, rx in CLASSES:
            if rx.search(txt): cnt[cn] += 1; achou = True; break
        if not achou: cnt["outro"] += 1
        # o que veio depois?
        prox = next(((n, h) for t, n, h, _, _ in eventos[i+1:i+4] if t == "use"), None)
        if prox:
            apos_erro[prox[0]] += 1
            if ultimo_use and prox == ultimo_use: retry_identico += 1

print(f"TOOL_RESULTS={total_result}  ERROS={total_err} ({100*total_err//max(total_result,1)}%)")
print("\n-- classe do erro")
for n, c in cnt.most_common(): print(f"   {n:<24} {c:>5}")
print("\n-- tool que errou")
for n, c in por_tool.most_common(8): print(f"   {n:<24} {c:>5}")
print(f"\nRETRY_IDENTICO_IMEDIATO={retry_identico}")
print("-- primeira acao apos o erro")
for n, c in apos_erro.most_common(6): print(f"   {n:<24} {c:>5}")
