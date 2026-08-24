import json,sys,re
p=sys.argv[1]
bash=0; trun=0; outros=0
pipe_exit=0; ex=[]
for linha in open(p,encoding="utf-8",errors="ignore"):
    try: d=json.loads(linha)
    except Exception: continue
    msg=d.get("message") or {}
    cont=msg.get("content")
    if not isinstance(cont,list): continue
    for c in cont:
        if not isinstance(c,dict) or c.get("type")!="tool_use": continue
        nome=c.get("name"); inp=c.get("input") or {}
        if nome=="Bash":
            bash+=1
            cmd=str(inp.get("command",""))
            if "touring run " in cmd: trun+=1
            if re.search(r"\|[^;&|]*;\s*(echo\s+)?\S*EXIT\S*=\$\?", cmd) and "pipefail" not in cmd:
                pipe_exit+=1; ex.append(cmd[:60])
        else: outros+=1
print(f"CHAMADAS_BASH={bash}")
print(f"BASH_QUE_USAM_TOURING_RUN={trun} ({100*trun//max(bash,1)}%)")
print(f"OUTRAS_TOOLS={outros}")
print(f"EXIT_APOS_PIPE_SEM_PIPEFAIL={pipe_exit}")
for e in ex[:8]: print("   ex:",e)
