"""Classifica o corpus de memorias por MODO DE FALHA e por prevenibilidade.

Os padroes sao lexicais e conservadores: uma memoria pode cair em varios modos.
O objetivo nao e um numero exato, e a ORDEM DE GRANDEZA de cada classe.
"""
import glob, re, collections, os
RAIZ = os.path.expanduser("~/.claude/projects")

MODOS = [
 ("inferencia_sem_medida", re.compile(r"assum|inferi|supus|sem (medir|verificar|executar)|nunca (foi )?(exercit|execut)|n[ãa]o (medi|verifiquei|testei)|por acidente|acreditei", re.I)),
 ("familia_parcial",       re.compile(r"\b(cinco|quatro|tr[êe]s|\d+) s[íi]tios|todos os s[íi]tios|s[óo] um|apenas um|fam[íi]lia|um sítio|outro sítio|4 de|3 dos", re.I)),
 ("ausencia_como_zero",    re.compile(r"aus[êe]ncia|sinal ausente|0/0|zero.{0,20}(indistin|significa)|vazio.{0,25}(indistin|sucesso)|silenci|n[ãa]o reporta", re.I)),
 ("staleness",             re.compile(r"stale|desatualiz|cache|bin[áa]rio (velho|antigo)|drift|espelho|r[óo]tulo", re.I)),
 ("teste_vacuo",           re.compile(r"falso.?verde|passava (por|sem)|vac[uú]o|n[ãa]o prova|asser[çc][ãa]o gen[ée]rica|teste que passa", re.I)),
 ("instrumento_errado",    re.compile(r"instrumento|pipe|exit code|\$\?|medi[çc][ãa]o errada|o pr[óo]prio (teste|monitor|auditor)|errou", re.I)),
 ("comentario_falso",      re.compile(r"coment[áa]rio|doc.{0,12}(afirma|diz)|promete|anúncio|anuncio|declara.{0,15}(falso|errado)", re.I)),
 ("config_desligada",      re.compile(r"desligad|default.?OFF|nunca (rodou|registrad)|n[ãa]o (est[áa]|foi) registrad|inerte|ado[çc][ãa]o zero", re.I)),
]

# Prevenibilidade: um PROGRAMA que varre a familia inteira teria pego?
PREVENIVEL = {"familia_parcial", "ausencia_como_zero", "staleness",
              "teste_vacuo", "config_desligada", "instrumento_errado"}

modo_cnt = collections.Counter()
por_projeto = collections.Counter()
total = 0
sem_modo = 0
exemplos = collections.defaultdict(list)

for f in sorted(glob.glob(f"{RAIZ}/*/memory/*.md")):
    try: txt = open(f, encoding="utf-8", errors="ignore").read()
    except OSError: continue
    if "MEMORY.md" in f: continue
    total += 1
    proj = f.split("/")[-3]
    achou = False
    for nome, rx in MODOS:
        if rx.search(txt):
            modo_cnt[nome] += 1; achou = True
            if len(exemplos[nome]) < 3:
                exemplos[nome].append(os.path.basename(f)[:52])
    if achou: por_projeto[proj] += 1
    else: sem_modo += 1

print(f"MEMORIAS={total}   sem_modo_identificado={sem_modo} ({100*sem_modo//max(total,1)}%)")
print("\n-- modo de falha (memoria pode ter varios)")
prev = 0
for n, c in modo_cnt.most_common():
    marca = "PREVENIVEL" if n in PREVENIVEL else "outro"
    if n in PREVENIVEL: prev += c
    print(f"   {n:<24} {c:>4}  {100*c//max(total,1):>3}%  [{marca}]")
    print(f"        ex: {', '.join(exemplos[n])}")
soma = sum(modo_cnt.values())
print(f"\nMARCACOES_TOTAIS={soma}  PREVENIVEL_POR_PROGRAMA={prev} ({100*prev//max(soma,1)}%)")
print("\n-- por projeto (memorias com >=1 modo)")
for n, c in por_projeto.most_common(6):
    print(f"   {n:<46} {c:>4}")
