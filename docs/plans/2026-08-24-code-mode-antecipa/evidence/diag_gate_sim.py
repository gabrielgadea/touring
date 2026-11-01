"""Rodada 2 — SIMULAÇÃO dos gates G1-G6 contra as 55 sessões.

Para cada gate: quantas vezes dispararia, em quantas sessões, e uma proxy de
falso-positivo. A proxy de FP do G1 (rajada de inspeção): o disparo é seguido
de MAIS inspeção da mesma classe (verdadeiro-positivo — a rajada ia continuar)
ou a rajada morria ali sozinha (o gate teria atrapalhado de graça)?
"""
import json, glob, re, collections, os
RAIZ = os.path.expanduser("~/.claude/projects")

RX_INSPECT = re.compile(r"\b(grep|rg|sed -n|cat |head |tail |ls |find |wc |sqlite3|jq )\b")
RX_MUT = re.compile(r"\b(cargo|git|touring adw|pytest|rm |mv )\b|>>?\s*[^&\s]|sed -i")
RX_PIPEEXIT = re.compile(r"\|[^;&|]*;\s*(echo\s+)?\S*(EXIT\S*=)?\$\?")

g = collections.Counter()          # gate -> disparos
gs = collections.defaultdict(set)  # gate -> sessões
g1_tp = 0; g1_fp = 0               # G1: rajada continuaria vs morria
g5_fires = 0

for f in sorted(glob.glob(f"{RAIZ}/**/*.jsonl", recursive=True)):
    seq = []
    try:
        with open(f, encoding="utf-8", errors="ignore") as fh:
            for linha in fh:
                try: d = json.loads(linha)
                except Exception: continue
                cont = (d.get("message") or {}).get("content")
                if not isinstance(cont, list): continue
                for c in cont:
                    if isinstance(c, dict) and c.get("type") == "tool_use":
                        seq.append((c.get("name","?"), c.get("input") or {}))
    except OSError: continue
    if len(seq) < 8: continue
    sess = f.rsplit("/",1)[-1][:8]

    vistos = set()
    insp_run = 0
    edits_sem_valid = 0
    for i, (nome, inp) in enumerate(seq):
        cmd = str(inp.get("command","")) if nome == "Bash" else ""
        eh_insp = nome == "Bash" and bool(RX_INSPECT.search(cmd)) and not RX_MUT.search(cmd) and "touring run" not in cmd

        # G1: 4ª inspeção consecutiva
        if eh_insp:
            insp_run += 1
            if insp_run == 4:
                g["G1_rajada"] += 1; gs["G1_rajada"].add(sess)
                # a rajada continuaria? olhe as próximas 3
                seguiu = any(
                    s[0] == "Bash" and RX_INSPECT.search(str(s[1].get("command","")))
                    for s in seq[i+1:i+4]
                )
                if seguiu: g1_tp += 1
                else: g1_fp += 1
        else:
            insp_run = 0

        if nome == "Bash":
            # G2: exit através de pipe
            if RX_PIPEEXIT.search(cmd) and "pipefail" not in cmd:
                g["G2_pipe_exit"] += 1; gs["G2_pipe_exit"].add(sess)
            # G6: chamada redundante exata
            h = (nome, cmd[:300])
            if h in vistos:
                g["G6_redundante"] += 1; gs["G6_redundante"].add(sess)
            vistos.add(h)

        # G3: Edit sem Read do mesmo arquivo
        if nome == "Edit":
            alvo = str(inp.get("file_path",""))
            leu = any(n == "Read" and str(j.get("file_path","")) == alvo
                      for n, j in seq[max(0,i-12):i])
            if alvo and not leu:
                g["G3_edit_sem_read"] += 1; gs["G3_edit_sem_read"].add(sess)

        # G4: Read sem localizar
        if nome == "Read":
            janela = [n for n, _ in seq[max(0,i-3):i]]
            if not any(x in ("Grep","Glob","Bash") for x in janela):
                g["G4_read_sem_loc"] += 1; gs["G4_read_sem_loc"].add(sess)

        # G5: 3+ edits sem build/test nas próximas 4
        if nome in ("Edit","Write"):
            edits_sem_valid += 1
            if edits_sem_valid == 3:
                validou = any(
                    s[0]=="Bash" and re.search(r"\b(cargo|pytest|npm test|make )", str(s[1].get("command","")))
                    for s in seq[i+1:i+5]
                )
                if not validou:
                    g["G5_edit_sem_valid"] += 1; gs["G5_edit_sem_valid"].add(sess)
        elif nome == "Bash" and re.search(r"\b(cargo|pytest|npm test)", cmd):
            edits_sem_valid = 0

print("gate                 disparos  sessoes")
for n, c in g.most_common():
    print(f"   {n:<20} {c:>6}   {len(gs[n]):>3}/55")
print(f"\nG1 proxy de precisao: rajada CONTINUARIA depois do disparo em "
      f"{g1_tp}/{g1_tp+g1_fp} ({100*g1_tp//max(g1_tp+g1_fp,1)}%) — o resto morria sozinha")
