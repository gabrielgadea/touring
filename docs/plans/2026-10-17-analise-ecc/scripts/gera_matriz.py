"""Build the feature-by-feature ECC × Touring matrix from every findings file of the analysis (17/10/2026).

Usage: gera_matriz.py <achados-dir> <saida.md>
First-round findings (A-E: mecanismo/touring_estado/licao_tipo) are mapped onto the same row schema as the
gap rounds (G*: dominio/funcionalidade/ecc/touring/veredito/acao); web claims (W) go to their own section.
"""
import collections
import json
import re
import sys
from pathlib import Path

DOMINIOS = [
    ("instalacao", "Instalação, atualização e distribuição"),
    ("multi-harness", "Multi-harness e multi-modelo"),
    ("cat-agentes", "Catálogo: agentes"),
    ("cat-skills", "Catálogo: skills"),
    ("cat-commands", "Catálogo: commands"),
    ("cat-rules", "Catálogo: regras e contextos"),
    ("hooks", "Hooks e enforcement"),
    ("seguranca", "Segurança"),
    ("memoria", "Memória e aprendizado contínuo"),
    ("contexto-custo", "Contexto, tokens e custo"),
    ("roteamento", "Roteamento de modelo"),
    ("orquestracao", "Orquestração e paralelismo"),
    ("planejamento", "Planejamento e decomposição"),
    ("verificacao-qualidade", "Verificação, avaliação e qualidade"),
    ("observabilidade", "Observabilidade e operação"),
    ("inteligencia-codigo", "Inteligência de código"),
    ("governanca-repo", "Governança do repositório e do catálogo"),
    ("documentacao", "Documentação e guias"),
    ("dominios-nao-codigo", "Domínios fora de código"),
]
MAPA = {  # first-round finding id -> domain
    **{f"A{i}": "hooks" for i in (1, 2, 3, 5, 8, 9, 10)}, "A4": "seguranca", "A6": "verificacao-qualidade",
    "A7": "verificacao-qualidade",
    **{f"B{i}": "memoria" for i in range(1, 11)},
    **{f"C{i}": "contexto-custo" for i in (1, 2, 3, 4, 5, 6, 8)}, "C7": "roteamento", "C9": "roteamento",
    "D1": "verificacao-qualidade", "D2": "verificacao-qualidade", "D3": "verificacao-qualidade", "D4": "planejamento",
    "D5": "orquestracao", "D6": "observabilidade", "D7": "observabilidade", "D8": "contexto-custo",
    "D9": "verificacao-qualidade", "D10": "orquestracao", "D11": "verificacao-qualidade",
    "E1": "seguranca", "E2": "seguranca", "E3": "seguranca", "E4": "hooks", "E5": "instalacao", "E6": "contexto-custo",
    "E7": "multi-harness", "E8": "verificacao-qualidade", "E9": "seguranca", "E10": "seguranca", "E11": "governanca-repo",
}
VEREDITO_DE_ESTADO = {"ausente": "so_ecc", "parcial": "ecc_melhor", "superior": "touring_melhor", "existe": "equivalente",
                      "nao_verificado": "a_verificar"}
ACAO_DE_LICAO = {"corrigir": "corrigir", "complementar": "adotar", "aperfeicoar": "adaptar", "refinar": "adaptar"}
CORRECAO = {  # first-round id -> where its fix stands (updated 18/10/2026, round H; evidence in RELATORIO §8)
    "A1": "FEITO (C1, main = vivo e int97)", "A2": "FEITO (D2, main = vivo e int97)", "A4": "FEITO (C2, implantado)",
    "A5": "FEITO no int97 (C4, gates verdes)", "B5": "FEITO no int97 (C6, gates verdes)",
    "C1": "FEITO no int97 (C10, gates verdes)", "C4": "FEITO (sensor do check_context, main e int97)",
    "C9": "FEITO (why:env, main e int97)", "E1": "FEITO (workflows; detector F4.7 no int97)",
    "E2": "FEITO (job verify; assinatura só avisa, D1)", "E9": "FEITO no int97 (F2.4 rótulo e allowlist)",
    "E10": "FEITO (D4: .gitignore; caminho do kpi no int97)",
}
ALIAS = {"hooks-gates": "hooks", "gates": "hooks", "multiharness": "multi-harness", "custo": "contexto-custo",
         "contexto-custo-memoria": "contexto-custo"}  # the last one is an inventory class a reader used as a domain
ROTULO_V = {"ecc_melhor": "ECC melhor", "touring_melhor": "Touring melhor", "equivalente": "equivalente",
            "so_ecc": "só ECC", "so_touring": "só Touring", "a_verificar": "a verificar"}


def limpa(t: str, n: int = 260) -> str:
    t = " ".join(str(t).split()).replace("|", "/")
    return t if len(t) <= n else t[: n - 1] + "…"


def linhas_primeira_rodada(d: dict, fonte: str) -> list[dict]:
    out = []
    for x in d.get("findings", []):
        out.append({
            "fonte": f"{fonte}:{x['id']}", "dominio": MAPA.get(x["id"], "verificacao-qualidade"),
            "funcionalidade": x["mecanismo"], "ecc": f"{x.get('como_funciona', '')} [{x.get('evidencia_ecc', '')}]",
            "touring": f"{x.get('touring_estado', '?')}: {x.get('touring_evidencia', '')}",
            "veredito": VEREDITO_DE_ESTADO.get(x.get("touring_estado"), "a_verificar"),
            "acao": ACAO_DE_LICAO.get(x.get("licao_tipo"), x.get("licao_tipo", "")),
            "valor": x.get("valor", ""), "estado": CORRECAO.get(x["id"], "proposta" if x.get("licao_tipo") != "corrigir" else "a corrigir"),
            "proposta": x.get("proposta", ""),
        })
    return out


def linhas_lacunas(d: dict, fonte: str) -> list[dict]:
    out = []
    for i, x in enumerate(d.get("linhas", []), 1):
        e, t = x.get("ecc", {}), x.get("touring", {})
        out.append({
            "fonte": f"{fonte}:{i}", "dominio": ALIAS.get(x.get("dominio", ""), x.get("dominio") or "documentacao"), "funcionalidade": x.get("funcionalidade", ""),
            "ecc": f"{e.get('como', '')} [{e.get('evidencia', '')}]",
            "touring": f"{t.get('estado', '?')}: {t.get('como', '')} [{t.get('evidencia', '')}]",
            "veredito": x.get("veredito", "a_verificar"), "acao": x.get("acao", ""), "valor": "",
            "estado": "—", "proposta": x.get("nota", ""), "aprofunda": x.get("aprofunda") or None,
        })
    return out


def carrega(pasta: Path) -> tuple[list[dict], list[dict], list[dict]]:
    linhas, web, defeitos = [], [], []
    for f in sorted(pasta.glob("*.json")):
        d = json.loads(f.read_text(encoding="utf-8"))
        letra = f.stem.split("-")[0]
        if letra == "W":
            web = d.get("findings", [])
        elif "linhas" in d:
            linhas += linhas_lacunas(d, letra)
            defeitos += [{**x, "fonte": letra} for x in d.get("achados_touring", []) if isinstance(x, dict)]
        else:
            linhas += linhas_primeira_rodada(d, letra)
    return linhas, web, defeitos


def chave_de_fonte(ident: str) -> str:
    """Readers write a first-round id as `A3`; the matrix keys it `A:A3`. `G1:19`, `B:B6` pass as they are."""
    ident = ident.strip()
    m = re.fullmatch(r"([A-E])\d+", ident)
    return f"{m.group(1)}:{ident}" if m else ident


def aplica_revisoes(linhas: list[dict]) -> list[tuple[str, str, str, str]]:
    """A deepening row (`aprofunda: <id>`) supersedes the row it revises: the old one stays, marked, out of the tally."""
    por_fonte = {r["fonte"]: r for r in linhas}
    trocas = []
    for r in linhas:
        alvo = por_fonte.get(chave_de_fonte(r.get("aprofunda") or ""))
        if alvo is not None and alvo is not r:
            alvo["revisada_por"] = r["fonte"]
            trocas.append((alvo["fonte"], alvo["veredito"], r["fonte"], r["veredito"]))
    return trocas


def vigentes(linhas: list[dict]) -> list[dict]:
    return [r for r in linhas if not r.get("revisada_por")]


def contagem(rs: list[dict]) -> str:
    return " · ".join(f"{ROTULO_V.get(k, k)} {v}" for k, v in collections.Counter(r["veredito"] for r in rs).most_common())


def resumo(linhas: list[dict], por_dom: dict, trocas: list[tuple[str, str, str, str]]) -> list[str]:
    # The first round (A-E) hunted Touring gaps by design and maps "parcial" to "ECC melhor"; report it apart.
    lacunas = [r for r in linhas if r["fonte"][0] not in "GH"]
    comparativa = [r for r in linhas if r["fonte"].startswith("G")]
    aprofundamento = [r for r in linhas if r["fonte"].startswith("H")]
    viva = vigentes(linhas)
    md = ["## Matriz por funcionalidade (gerada por `scripts/gera_matriz.py`)", "",
          (f"{len(linhas)} linhas de {len({r['fonte'].split(':')[0] for r in linhas})} fontes; {len(viva)} vigentes "
           f"depois de {len(trocas)} revisões · {contagem(viva)}"), "",
          (f"- Rodada de lacunas (A–E, {len(lacunas)} linhas): {contagem(lacunas)}. Os agentes procuraram lacunas do "
           "Touring por desenho, e o estado «parcial» do Touring entra como «ECC melhor»: essa coluna está inflada."),
          (f"- Rodada comparativa (G1–G5, {len(comparativa)} linhas): {contagem(comparativa)}. Cada linha descreve os "
           "dois lados com evidência."),
          (f"- Rodada de aprofundamento (H, {len(aprofundamento)} linhas): {contagem(aprofundamento)}. Corpos lidos e lado "
           "Touring verificado por comando; uma linha H que revisa outra a substitui no placar."), ""]
    if trocas:
        md += ["Revisões de veredito (antiga → nova):", ""]
        md += [f"- {a} {ROTULO_V.get(va, va)} → {n} {ROTULO_V.get(vn, vn)}" for a, va, n, vn in trocas]
        md += [""]
    md += ["| Domínio (linhas vigentes) | linhas | ECC melhor | só ECC | Touring melhor | só Touring | equivalente | a verificar |",
           "|---|---|---|---|---|---|---|---|"]
    for chave, nome in DOMINIOS:
        rs = vigentes(por_dom.get(chave, []))
        c = collections.Counter(r["veredito"] for r in rs)
        md.append(f"| {nome} | {len(rs)} | {c['ecc_melhor']} | {c['so_ecc']} | {c['touring_melhor']} | {c['so_touring']} | "
                  f"{c['equivalente']} | {c['a_verificar']} |")
    return md


def tabelas(por_dom: dict) -> list[str]:
    md = []
    for chave, nome in DOMINIOS:
        rs = por_dom.get(chave, [])
        if not rs:
            continue
        md += ["", f"### {nome}", "", "| Funcionalidade | ECC | Touring | Veredito | Ação · estado | Fonte |", "|---|---|---|---|---|---|"]
        for r in sorted(rs, key=lambda r: (bool(r.get("revisada_por")), r["veredito"] != "so_ecc", r["veredito"] != "ecc_melhor", r["fonte"])):
            estado = f"revisada por {r['revisada_por']}" if r.get("revisada_por") else r["estado"]
            acao = f"{r['acao']} · {estado}" + (f" — {limpa(r['proposta'], 160)}" if r["proposta"] else "")
            md.append(f"| {limpa(r['funcionalidade'], 120)} | {limpa(r['ecc'])} | {limpa(r['touring'])} | "
                      f"{ROTULO_V.get(r['veredito'], r['veredito'])} | {limpa(acao, 260)} | {r['fonte']} |")
    return md


def main() -> int:
    pasta, saida = Path(sys.argv[1]), Path(sys.argv[2])
    linhas, web, defeitos = carrega(pasta)
    trocas = aplica_revisoes(linhas)
    por_dom = collections.defaultdict(list)
    for r in linhas:
        por_dom[r["dominio"]].append(r)
    cont = collections.Counter(r["veredito"] for r in vigentes(linhas))
    md = resumo(linhas, por_dom, trocas) + tabelas(por_dom)
    if defeitos:
        md += ["", "### Defeitos do Touring vistos pelos leitores (a confirmar antes de corrigir)", ""]
        md += [f"- [{d['fonte']}] {limpa(d.get('defeito', ''), 300)} — `{limpa(d.get('evidencia', ''), 160)}`" for d in defeitos]
    if web:
        md += ["", "### Evidência externa verificada (pesquisa web, 3 votos por afirmação)", ""]
        for i, x in enumerate(web, 1):
            md.append(f"- **W{i}** [{x.get('confidence')}; {x.get('vote', '')}] {limpa(x['claim'], 600)}")
    saida.write_text("\n".join(md) + "\n", encoding="utf-8")
    print(f"{len(linhas)} linhas · {dict(cont)} · domínios sem linha: "
          f"{[n for k, n in DOMINIOS if not por_dom.get(k)]} · fora da taxonomia: "
          f"{sorted({r['dominio'] for r in linhas} - {k for k, _ in DOMINIOS})}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
