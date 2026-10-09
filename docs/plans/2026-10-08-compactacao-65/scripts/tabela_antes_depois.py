"""A7 — a tabela antes->depois, medida com a MESMA logica de `medir_piso.py`.

A DECISAO QUE A TABELA ENCARNA
------------------------------
Um stub so nao e uma economia: e uma economia SO SE o corpo foi para um lugar
que so entra em contexto quando a tarefa o pede. Por isso a tabela tem tres
colunas de destino, e nao so "antes/depois":

  sempre carregado  — o que continua entrando em toda sessao (o stub)
  sob demanda       — o corpo movido para references/ (carrega ao tocar o tema)
  fora de contexto  — o que saiu de vez (nada saiu: 100% de conservacao)

Se a coluna "sob demanda" fosse zero, a compactacao teria DESTRUIDido conteudo
em vez de move-lo — e a conservacao (exit 0) seria a unica coisa que diria.
"""
import pathlib
import re
import sys

B = pathlib.Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-compactacao-65")
sys.path.insert(0, str(B / "scripts"))

HOME = pathlib.Path.home()
PROJ = pathlib.Path("/home/gabrielgadea/projects/analise")
RULES_BKP = B / "rules-backup"
AGENTS_BKP = B / "agents-backup"
TOURING_REF = HOME / ".claude/skills/Touring/references"
ELITE_REF = HOME / ".claude/skills/touring-elite/references"
CLIENT = pathlib.Path("/home/gabrielgadea/projects/touring/client/agents")

REGRAS = {
    "git-safety-protocol": (HOME / ".claude/rules/git-safety-protocol.md",
                            [TOURING_REF / "git-safety-protocol.md"]),
    "touring-process-hygiene": (HOME / ".claude/rules/touring-process-hygiene.md",
                                [TOURING_REF / "touring-process-hygiene.md"]),
    "touring-per-project": (HOME / ".claude/rules/touring-per-project.md",
                            [TOURING_REF / "touring-per-project.md"]),
    "touring-elite": (HOME / ".claude/rules/touring-elite.md",
                      [ELITE_REF / "auto-load-rule.md"]),
    "touring-cli-index": (HOME / ".claude/rules/touring-cli-index.md",
                          [TOURING_REF / "touring-cli-index-tiers-2-3.md"]),
    "touring-4-pillars": (HOME / ".claude/rules/touring-4-pillars.md",
                          [TOURING_REF / "touring-4-pillars-provenance.md"]),
    "disk-hygiene": (HOME / ".claude/rules/disk-hygiene.md", []),
    "vgp-protocol": (PROJ / ".claude/rules/vgp-protocol.md", []),
    "00-core-governance": (
        PROJ / ".claude/rules/00-core-governance.md",
        [PROJ / ".claude/docs/rules-provenance/00-core-governance-restauracao.md"]),
}

AGENTES = {f"touring-{n}": CLIENT / f"touring-{n}.md"
           for n in ("architect", "auditor", "engineer", "scouter", "scriber")}
AGENTES.update({
    "antt-vote-drafter": PROJ / ".claude/agents/antt-vote-drafter.md",
    "erickson-paradox-reformulator":
        PROJ / ".claude/agents/erickson-paradox-reformulator.md",
    "transformational-narrative-architect":
        PROJ / ".claude/agents/transformational-narrative-architect.md",
})


def tamanho(p: pathlib.Path) -> int:
    return p.stat().st_size if p.exists() else 0


def tem_paths(p: pathlib.Path) -> bool:
    """`paths:` no frontmatter = carrega condicionalmente, nao sempre."""
    if not p.exists():
        return False
    head = p.read_text(encoding="utf-8")[:400]
    return bool(re.search(r"^---\s*\n(?:.*\n)*?paths:", head, re.M))


def descricao_agente(p: pathlib.Path) -> int:
    """O TAMANHO da `description` — e so ela que entra no contexto do agente."""
    linhas = p.read_text(encoding="utf-8").split("\n")
    for i, linha in enumerate(linhas):
        if not re.match(r"^description:\s*[|>][-+]?\s*$", linha):
            continue
        corpo, j = [], i + 1
        while j < len(linhas) and (linhas[j].strip() == "" or linhas[j][:1] in (" ", "\t")):
            corpo.append(linhas[j])
            j += 1
        recuo = min((len(x) - len(x.lstrip()) for x in corpo if x.strip()), default=0)
        return sum(len(x) - recuo for x in corpo if x.strip())
    return 0


print("=" * 92)
print("A7 — REGRAS: onde o texto está agora")
print("=" * 92)
print(f"{'regra':<26} {'antes':>7} {'stub agora':>11} {'sob demanda':>13} {'economia':>9}")
antes_r = depois_r = demanda_r = 0
for nome, (stub, destinos) in REGRAS.items():
    a = tamanho(RULES_BKP / f"{nome}.md")
    d = tamanho(stub)
    dem = sum(tamanho(x) for x in destinos)
    antes_r += a
    # uma regra com `paths:` saiu do conjunto sempre-carregado
    depois_r += 0 if tem_paths(stub) else d
    demanda_r += dem
    eco = a - d if not tem_paths(stub) else a
    print(f"{nome:<26} {a:>7} {d:>11}{'  (cond)' if tem_paths(stub) else '':>9} "
          f"{dem:>13} {eco:>9}")
print(f"{'TOTAL (9 regras)':<26} {antes_r:>7} {depois_r:>11} {demanda_r:>13} "
      f"{antes_r - depois_r:>9}")

print()
print("=" * 92)
print("A7 — AGENTES: o campo `description` (única parte que entra no contexto)")
print("=" * 92)
print(f"{'agente':<38} {'antes':>7} {'agora':>7} {'economia':>9}")
antes_a = depois_a = 0
for nome, canonico in AGENTES.items():
    a = descricao_agente(AGENTS_BKP / f"{nome}.md")
    d = descricao_agente(canonico)
    antes_a += a
    depois_a += d
    print(f"{nome:<38} {a:>7} {d:>7} {a - d:>9}")
print(f"{'TOTAL (8 agentes)':<38} {antes_a:>7} {depois_a:>7} {antes_a - depois_a:>9}")

print()
print("=" * 92)
print("A7 — O QUE A SESSÃO PAGA AGORA")
print("=" * 92)
print(f"  regras sempre carregadas   {antes_r:>8} -> {depois_r:>8} chars  "
      f"({-100.0*(antes_r-depois_r)/antes_r:.1f}%)")
print(f"  descrições de agentes      {antes_a:>8} -> {depois_a:>8} chars  "
      f"({-100.0*(antes_a-depois_a)/antes_a:.1f}%)")
print(f"  TOTAL no contexto fixo     {antes_r+antes_a:>8} -> "
      f"{depois_r+depois_a:>8} chars  "
      f"({-100.0*((antes_r+antes_a)-(depois_r+depois_a))/(antes_r+antes_a):.1f}%)")
print(f"  movido para sob demanda    {demanda_r:>8} chars (carrega so quando o tema pede)")
