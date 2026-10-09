#!/usr/bin/env python3
"""CONSERVAÇÃO PROVADA — nenhuma linha do original pode sumir.

Ordem do Gabriel, 08/10/2026 (delegação analise-6b, S9 + adendo S7).
"nada se apaga" nao e intencao: e uma propriedade verificada por codigo.

O QUE PROVA
  Modo skills (default): cada linha do SKILL.md ORIGINAL (do backup) aparece
  VERBATIM no SKILL.md novo ou em algum references/*.md da mesma skill.
  Modo --rules: cada linha da REGRA original aparece no stub OU no arquivo de
  destino declarado — um stub sozinho daria 100% sobre um stub vazio.
  Modo --agents: cada linha do AGENTE original aparece no arquivo novo, e as
  copias em ~/.claude/agents sao byte-identicas ao canonico.

POR QUE "VERBATIM" e nao "EQUIVALENTE"
  Uma reescrita pode ser boa e ainda assim mudar uma linha. Verificar
  equivalencia semantica exige um leitor; verificar substring e aritmetica,
  e a aritmetica nao minta.

EXCECOES DECLARADAS
  Separadores (`---`), cabecalhos de tabela vazios e linhas de regua sao
  formatacao, nao conteudo. Estao numa lista EXPLICITA abaixo — nunca
  "descartadas em silencio".

SAIDA
  exit 0 so com 100% de conservacao em todos os itens.
"""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

BUNDLE = Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-compactacao-65")
BACKUP = BUNDLE / "skills-backup"
RULES_BACKUP = BUNDLE / "rules-backup"
AGENTS_BACKUP = BUNDLE / "agents-backup"
SAIDA = BUNDLE / "conservacao.json"

HOME = Path.home()
TOURING_SRC = Path("/home/gabrielgadea/projects/touring")
PROJ = Path("/home/gabrielgadea/projects/analise")
TOURING_REF = HOME / ".claude/skills/Touring/references"
ELITE_REF = HOME / ".claude/skills/touring-elite/references"
HOME_RULES = HOME / ".claude/rules"

SKILLS: dict[str, Path] = {
    "Touring": HOME / ".claude/skills/Touring",
    "loop-engineering": HOME / ".claude/skills/loop-engineering",
    "analysis-loop": HOME / ".claude/skills/analysis-loop",
    "TACO-cross-audit": HOME / ".claude/skills/TACO-cross-audit",
    "scientific-writing": HOME / ".claude/skills/scientific-writing",
    "antt-final-synthesizer": PROJ / ".claude/skills/antt-final-synthesizer",
    "transformational-story-architect":
        PROJ / ".claude/skills/transformational-story-architect",
    "antt-narrative-architect": PROJ / ".claude/skills/antt-narrative-architect",
    "design-elite": PROJ / ".claude/skills/design-elite",
}

#: (stub, destinos). DECLARADOS, nao descobertos: um destino esquecido aqui
#: produz uma perda que parece texto deletado e nao e.
RULES: dict[str, tuple[Path, list[Path]]] = {
    "git-safety-protocol": (
        HOME_RULES / "git-safety-protocol.md",
        [TOURING_REF / "git-safety-protocol.md"]),
    "touring-process-hygiene": (
        HOME_RULES / "touring-process-hygiene.md",
        [TOURING_REF / "touring-process-hygiene.md"]),
    "touring-per-project": (
        HOME_RULES / "touring-per-project.md",
        [TOURING_REF / "touring-per-project.md"]),
    "touring-elite": (
        HOME_RULES / "touring-elite.md",
        [ELITE_REF / "auto-load-rule.md"]),
    "touring-cli-index": (
        HOME_RULES / "touring-cli-index.md",
        [TOURING_REF / "touring-cli-index-tiers-2-3.md"]),
    "touring-4-pillars": (
        HOME_RULES / "touring-4-pillars.md",
        [TOURING_REF / "touring-4-pillars-provenance.md"]),
    "disk-hygiene": (HOME_RULES / "disk-hygiene.md", []),
    "vgp-protocol": (PROJ / ".claude/rules/vgp-protocol.md", []),
    "00-core-governance": (
        PROJ / ".claude/rules/00-core-governance.md",
        [PROJ / ".claude/docs/rules-provenance/00-core-governance-restauracao.md"]),
}

#: (canonico, copias que precisam ser byte-identicas). A divergencia entre
#: copias e um silencio: o Claude Code le `~/.claude/agents`, o repo le
#: `client/agents` — se so um for editado, cada sessao ve um agente diferente.
AGENTES: dict[str, tuple[Path, list[Path]]] = {
    f"touring-{n}": (
        TOURING_SRC / f"client/agents/touring-{n}.md",
        [HOME / f".claude/agents/touring-{n}.md"])
    for n in ("architect", "auditor", "engineer", "scouter", "scriber")
}
AGENTES.update({
    "antt-vote-drafter": (PROJ / ".claude/agents/antt-vote-drafter.md", []),
    "erickson-paradox-reformulator":
        (PROJ / ".claude/agents/erickson-paradox-reformulator.md", []),
    "transformational-narrative-architect":
        (PROJ / ".claude/agents/transformational-narrative-architect.md", []),
})

#: Linhas de formatacao que nao sao conteudo. Cada uma e uma EXCECAO
#: nomeada — a lista e impressa no relatorio, nunca aplicada em silencio.
EXCECOES_EXPLICITAS: tuple[str, ...] = (
    "---",
    "|---|",
    "|:---|",
    "| --- |",
)

#: Caminhos quebrados pre-existentes corrigidos ao lado da linha original.
CORRIGIDOS: list[dict] = []


def normalizar(linha: str) -> str:
    """Compara sem as bordas de whitespace; o miolo precisa bater literal."""
    return linha.strip()


def eh_excecao(linha: str) -> bool:
    s = normalizar(linha)
    if not s or s in EXCECOES_EXPLICITAS:
        return True
    # O marcador de block scalar (`description: |` -> `description: >`) e
    # FORMATACAO do YAML, como o `---`: escolher o estilo dobrado em vez do
    # literal nao apaga conteudo. Declarado aqui para que a conservacao nao
    # acuse 1 perda por arquivo por causa de uma escolha legitima de estilo.
    if re.fullmatch(r"(description|summary):\s*[|>][-+]?", s):
        return True
    if s.startswith("|") and set(s) <= set("|-: "):
        return True
    if s.startswith("|") and all(
        set(p.strip()) <= set("-:") or not p.strip() for p in s.strip("|").split("|")
    ):
        return True
    return False


#: Correções DECLARADAS de drift real no arquivo vivo (08/10/2026: subcomandos inexistentes e um crate
#: absorvido nos exemplos dos agentes). A linha antiga só deixa de ser perda se a substituta (`para`)
#: estiver no destino — uma correção declarada que não chegou ao arquivo continua sendo perda. Em JSON,
#: fora do escopo do scanner de drift (que recontaria os literais antigos como sinal novo).
_CORRECOES_JSON = Path(__file__).with_name("correcoes_declaradas.json")
CORRECOES_DECLARADAS: dict[str, str] = (
    {normalizar(c["de"]): normalizar(c["para"]) for c in json.loads(_CORRECOES_JSON.read_text(encoding="utf-8"))}
    if _CORRECOES_JSON.exists() else {})


def _contar_linhas_verbatim(origem: str, texts: list[tuple[str, str]]) -> dict:
    """Conta quantas linhas de `origem` aparecem VERBATIM em algum dos `texts`.

    Fatorado para fora do chamador porque a versao inline somava complexidade
    sem ganhar legibilidade — o `if/else` e mais claro que qualquer
    invariante aritmetica que o substitua.
    """
    total = conserve = exc = 0
    perdidas: list[str] = []
    corrigidas: list[str] = []
    for linha in origem.splitlines():
        if not normalizar(linha) or eh_excecao(linha):
            exc += 1
            continue
        total += 1
        alvo = normalizar(linha)
        para = CORRECOES_DECLARADAS.get(alvo)
        if any(alvo in texto for _, texto in texts):
            conserve += 1
        elif para and any(para in texto for _, texto in texts):
            corrigidas.append(alvo)
        else:
            perdidas.append(alvo)
    return {"linhas_conteudo": total, "conservadas": conserve,
            "excecoes": exc, "perdidas": perdidas, "corrigidas": corrigidas}


def _pct(conserve: int, total: int) -> float:
    return round(100.0 * conserve / total, 2) if total else 100.0


def textos_alvo(skill_dir: Path) -> list[tuple[str, str]]:
    """(caminho, texto) do SKILL.md novo + todos os references/*.md."""
    alvos: list[tuple[str, str]] = []
    sm = skill_dir / "SKILL.md"
    if sm.exists():
        alvos.append((str(sm), sm.read_text(encoding="utf-8")))
    ref = skill_dir / "references"
    if ref.is_dir():
        for p in sorted(ref.rglob("*.md")):
            alvos.append((str(p), p.read_text(encoding="utf-8")))
    return alvos


def verificar_skill(nome: str, skill_dir: Path) -> dict:
    """Conservacao de uma skill: SKILL.md novo + todos os references/*.md."""
    bkp = BACKUP / f"{nome}.SKILL.md"
    if not bkp.exists():
        return {"skill": nome, "erro": "backup ausente", "ok": False}

    alvos = textos_alvo(skill_dir)
    r = _contar_linhas_verbatim(bkp.read_text(encoding="utf-8"), alvos)
    return {
        "skill": nome,
        **r,
        "ok": not r["perdidas"],
        "pct": _pct(r["conservadas"], r["linhas_conteudo"]),
        "arquivos_varidos": [c for c, _ in alvos],
    }


def verificar_parcial(nome: str, bkp: Path, alvo: Path, destinos: list[Path]) -> dict:
    """Conservacao de um arquivo cujo texto se repartiu entre stub e destinos.

    `destinos_ausentes` sai no relatorio: um destino declarado que NAO existe
    faria a perda aparecer como "texto deletado", e a causa seria outra.
    """
    if not bkp.exists():
        return {"item": nome, "erro": "backup ausente", "ok": False}
    if not alvo.exists():
        return {"item": nome, "erro": "arquivo novo ausente", "ok": False}

    ausente = [str(d) for d in destinos if not d.exists()]
    texts = [(str(alvo), alvo.read_text(encoding="utf-8"))]
    texts += [(str(d), d.read_text(encoding="utf-8")) for d in destinos if d.exists()]

    r = _contar_linhas_verbatim(bkp.read_text(encoding="utf-8"), texts)
    return {
        "item": nome,
        **r,
        "destinos_ausentes": ausente,
        "ok": not r["perdidas"] and not ausente,
        "pct": _pct(r["conservadas"], r["linhas_conteudo"]),
        "arquivos_varidos": [c for c, _ in texts],
    }


def verificar_agente(nome: str, canonico: Path, copias: list[Path]) -> dict:
    """Conservacao do agente + a exigencia de que as copias sejam IDENTICAS."""
    r = verificar_parcial(nome, AGENTS_BACKUP / f"{nome}.md", canonico, copias)
    r["copias_divergentes"] = [
        str(c) for c in copias
        if c.exists() and canonico.exists() and c.read_bytes() != canonico.read_bytes()
    ]
    r["ok"] = r["ok"] and not r["copias_divergentes"]
    return r


def _caminho_corrigido(cauda: str) -> str:
    """O caminho da linha de correcao, entre crases, quando ele e absoluto."""
    depois = cauda[cauda.find("Caminho corrigido"):]
    crases = depois.split("`")
    if len(crases) < 2:
        return ""
    alvo = crases[1]
    if not alvo.startswith(("~", "/")):
        return ""
    bruto = alvo.split(" ")[0].split("(")[0].rstrip(".,;")
    expandido = Path(bruto.replace("~", str(HOME), 1))
    return bruto if expandido.exists() else ""


def verificar_links(skill_dir: Path) -> list[str]:
    """Todo caminho references/ citado no SKILL.md novo tem de existir.

    EXCECAO DECLARADA — caminho pre-existente quebrado, com correcao ao lado.
    A ordem exige conservacao E links resolvendo. Para um caminho que ja
    estava errado ANTES da compactacao, as duas so coexistem se a linha
    original ficar (conservacao) e a correcao entrar na linha seguinte (link).
    Esta funcao reconhece esse par e NAO conta como quebrado — mas registra em
    `CORRIGIDOS`, para que a decisao nao suma em silencio.

    Medido em run-1791476685401 / 08/10/2026: linhas de CORRECAO nao sao
    citacoes — sem a guarda, `references/quality/D01_complexity.md` aparecia
    como quebrado por estar dentro da linha de correcao de outro caminho, e o
    verificador acabava acusando a propria correcao.
    """
    sm = skill_dir / "SKILL.md"
    if not sm.exists():
        return []
    texto = sm.read_text(encoding="utf-8")
    sem_correcao = "\n".join(
        ln for ln in texto.splitlines() if "Caminho corrigido" not in ln)
    faltando: list[str] = []
    for parte in sem_correcao.split("references/"):
        resto = parte.split(")")[0].split("\n")[0].split(" ")[0].strip("`(|")
        if not resto.endswith(".md"):
            continue
        if (skill_dir / "references" / resto).exists():
            continue
        cauda = texto[texto.find(f"references/{resto}"):][:400]
        corrigido = _caminho_corrigido(cauda)
        if corrigido:
            CORRIGIDOS.append({"skill": skill_dir.name,
                               "citado": f"references/{resto}",
                               "corrigido_para": corrigido})
            continue
        faltando.append(f"references/{resto}")
    return sorted(set(faltando))


def imprimir_corrigidos() -> None:
    """Os caminhos quebrados pre-existentes que ganharam correcao ao lado."""
    if not CORRIGIDOS:
        return
    print("\nCAMINHOS PRE-EXISTENTES CORRIGIDOS (linha original preservada):")
    for c in CORRIGIDOS:
        print(f"  {c['skill']:<24} {c['citado']}")
        print(f"  {'':<24} -> {c['corrigido_para']}")


def _linha_de_resultado(rotulo: str, r: dict) -> str:
    """Uma linha de estado, seja OK, FALHA ou erro de premissa."""
    if r.get("erro"):
        return f"[FALHA] {rotulo:<32} ERRO: {r['erro']}"
    marca = "OK " if r["ok"] else "FALHA"
    return (f"[{marca}] {rotulo:<32} {r['conservadas']}/{r['linhas_conteudo']} "
            f"linhas ({r['pct']:.2f}%)")


def _detalhes_de_perda(r: dict) -> list[str]:
    """As linhas que faltam e os destinos/copias que quebraram a medicao."""
    msgs = [f"          - {p[:96]}" for p in r.get("perdidas", [])[:6]]
    msgs += [f"          DESTINO DECLARADO AUSENTE: {d}"
             for d in r.get("destinos_ausentes", [])]
    msgs += [f"          COPIA DIVERGENTE: {c}"
             for c in r.get("copias_divergentes", [])]
    msgs += [f"          corrigida (correcoes_declaradas.json): {c[:80]}" for c in r.get("corrigidas", [])]
    return msgs


def _rodar_skills() -> tuple[list[dict], dict[str, list[str]]]:
    """O modo padrao: as 9 skills. Devolve (relatorios, links quebrados)."""
    relatorios, quebrados = [], {}
    for nome, skill_dir in SKILLS.items():
        r = verificar_skill(nome, skill_dir)
        relatorios.append(r)
        print(_linha_de_resultado(nome, r))
        for msg in _detalhes_de_perda(r):
            print(msg)
        faltando = verificar_links(skill_dir)
        if faltando:
            quebrados[nome] = faltando
            print(f"        LINKS QUEBRADOS: {faltando}")
    return relatorios, quebrados


def _rodar_regras() -> list[dict]:
    """As 9 regras do adendo S7: stub + os destinos declarados."""
    detalhes = []
    print("REGRAS (stub + destinos declarados):")
    for nome, (alvo, destinos) in RULES.items():
        r = verificar_parcial(nome, RULES_BACKUP / f"{nome}.md", alvo, destinos)
        detalhes.append(r)
        linha = _linha_de_resultado(nome, r)
        if r.get("erro") is None:
            linha += f"  · varridos: {len(r['arquivos_varidos'])}"
        print(linha)
        for msg in _detalhes_de_perda(r):
            print(msg)
    return detalhes


def _rodar_agentes() -> list[dict]:
    """Os 8 agentes: conservacao + copias byte-identicas."""
    detalhes = []
    print("\nAGENTES (descricao cortada, texto integral no corpo, copias iguais):")
    for nome, (canonico, copias) in AGENTES.items():
        r = verificar_agente(nome, canonico, copias)
        detalhes.append(r)
        print(_linha_de_resultado(nome, r))
        for msg in _detalhes_de_perda(r):
            print(msg)
    return detalhes


def _gravar_conservacao(payload: dict, regras: bool, agentes: bool) -> Path:
    """O relatorio carrega o que foi pedido — o nome diz o que tem dentro."""
    if regras and agentes:
        nome = "conservacao-regras-agentes.json"
    elif agentes:
        nome = "conservacao-agentes.json"
    else:
        nome = "conservacao-regras.json"
    destino = BUNDLE / nome
    destino.write_text(json.dumps(payload, ensure_ascii=False, indent=1),
                       encoding="utf-8")
    return destino


def _fechamento(detalhes: list[dict], rotulo: str) -> list[str]:
    """Acumula, imprime o fechamento e devolve os itens com perda."""
    total = sum(d.get("linhas_conteudo", 0) for d in detalhes)
    ok = sum(d.get("conservadas", 0) for d in detalhes)
    falhas = [d.get("item", "?") for d in detalhes if not d["ok"]]
    print(f"\n{rotulo}: {ok}/{total} linhas ({_pct(ok, total):.2f}%)"
          f"  · com perda: {falhas or 'nenhuma'}")
    return falhas


def _regras_e_agentes(args: argparse.Namespace) -> int:
    """Os ramos de `--rules`/`--agents`, que davam complexidade 17 no `main`.

    Regra dos ramos: um `if args.x` que escreve arquivo e imprime relatorio
    ja e uma funcao. O `main` fica com o parse e a delegacao.
    """
    payload: dict = {}
    falhas: list[str] = []
    if args.rules:
        print(f"CONSERVAÇÃO — regras, backup em {RULES_BACKUP}\n")
        payload["regras"] = _rodar_regras()
        falhas += _fechamento(payload["regras"], "regras")
    if args.agents:
        print(f"\nCONSERVAÇÃO — agentes, backup em {AGENTS_BACKUP}")
        payload["agentes"] = _rodar_agentes()
        falhas += _fechamento(payload["agentes"], "agentes")
    print(f"relatorio: {_gravar_conservacao(payload, args.rules, args.agents)}")
    return 1 if falhas else 0


def _skills_main() -> int:
    """O modo padrao, fora do `main` para o parse nao carregar o fluxo."""
    print(f"CONSERVAÇÃO — backup em {BACKUP}")
    print(f"exceções declaradas: {list(EXCECOES_EXPLICITAS)}\n")

    relatorios, quebrados = _rodar_skills()

    if CORRIGIDOS:
        imprimir_corrigidos()

    SAIDA.write_text(json.dumps(
        {"relatorios": relatorios, "links_quebrados": quebrados,
         "corrigidos": CORRIGIDOS},
        ensure_ascii=False, indent=1), encoding="utf-8")

    total = sum(r.get("linhas_conteudo", 0) for r in relatorios)
    ok = sum(r.get("conservadas", 0) for r in relatorios)
    falhas = [r["skill"] for r in relatorios if not r["ok"]]
    print("\n" + "=" * 70)
    print(f"total: {ok}/{total} linhas conservadas ({_pct(ok, total):.2f}%)")
    print(f"skills com perda: {falhas or 'nenhuma'}")
    print(f"links quebrados: {list(quebrados) or 'nenhum'}")
    print(f"relatorio: {SAIDA}")
    return 1 if falhas or quebrados else 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument("--rules", action="store_true",
                    help="conservacao das 9 regras do adendo S7")
    ap.add_argument("--agents", action="store_true",
                    help="conservacao dos 8 agentes + copias identicas")
    args = ap.parse_args(argv)
    if args.rules or args.agents:
        return _regras_e_agentes(args)
    return _skills_main()


if __name__ == "__main__":
    raise SystemExit(main())
