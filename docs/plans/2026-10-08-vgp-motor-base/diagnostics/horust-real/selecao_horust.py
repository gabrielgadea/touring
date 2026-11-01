"""HORUST-REAL — seleção determinística: ordem por hash, round-robin entre pacotes, teto de concentração por crate, elegibilidade,
disjunção por arquivo e identidade das injeções. Nada aqui lê o disco nem roda cargo.
"""

from __future__ import annotations

import hashlib
from collections.abc import Hashable, Sequence

PREFIXOS_FIXTURE_VRS = (
    "crates/touring-code/src/vgp/",
    "crates/touring-pyright/",
    "crates/touring-identity/",
)
LINHAS_MIN, BYTES_MAX = 15, 200_000
FASE = {"nome": "R", "import": "R", "atributo": "T", "kwarg": "T", "cfg": "R"}
# operador → peso da tentativa (pré-registro, seção 3)
PESOS = {
    "nome": {"nome_valor": 3, "nome_tipo": 2},
    "import": {"import_use": 2, "import_use_chaves": 1, "import_caminho": 2},
    "atributo": {"atributo_campo": 1, "atributo_metodo": 1},
    "kwarg": {"kwarg_chave": 2, "kwarg_remove": 1},
}


def _h(semente: str, chave: str) -> str:
    return hashlib.sha256(f"{semente}|{chave}".encode()).hexdigest()


def ordem(chaves: Sequence[str], semente: str) -> list[str]:
    return sorted(chaves, key=lambda k: _h(semente, k))


def round_robin(grupos: dict[str, list[str]], n: int, semente: str) -> list[str]:
    """Até `n` itens, um de cada grupo por vez; grupos e itens ordenados por hash (não pela ordem de entrada)."""
    ordenados = {g: ordem(itens, semente) for g, itens in grupos.items()}
    nomes = ordem(list(ordenados), semente + "|grupo")
    saida: list[str] = []
    posicao = dict.fromkeys(nomes, 0)
    while len(saida) < n:
        andou = False
        for g in nomes:
            if len(saida) >= n:
                break
            if posicao[g] < len(ordenados[g]):
                saida.append(ordenados[g][posicao[g]])
                posicao[g] += 1
                andou = True
        if not andou:
            break
    return saida


def selecao_ate(por_crate: dict[str, list[str]], teto: float, semente: str, maximo: int) -> list[str]:
    """O maior N <= `maximo` tal que nenhum crate passe de floor(teto·N); o excedente sai por ordem de hash. Vazio se nenhum N é viável."""
    melhor = 0
    total = sum(len(v) for v in por_crate.values())
    for n in range(1, min(total, maximo) + 1):
        cap = int(teto * n)
        if sum(min(len(v), cap) for v in por_crate.values()) >= n:
            melhor = n
    if not melhor:
        return []
    cap = int(teto * melhor)
    todos = ordem([x for itens in por_crate.values() for x in itens], semente + "|teto")
    dono = {x: c for c, itens in por_crate.items() for x in itens}
    contagem: dict[str, int] = {}
    saida: list[str] = []
    for x in todos:
        if len(saida) == melhor:
            break
        if contagem.get(dono[x], 0) < cap:
            contagem[dono[x]] = contagem.get(dono[x], 0) + 1
            saida.append(x)
    return saida


def selecao_com_teto(por_crate: dict[str, list[str]], teto: float, semente: str) -> list[str]:
    """`selecao_ate` sem máximo: o maior N viável com o teto de concentração por crate."""
    return selecao_ate(por_crate, teto, semente, sum(len(v) for v in por_crate.values()))


def tamanho_elegivel(linhas: int, tamanho: int) -> bool:
    return linhas >= LINHAS_MIN and tamanho <= BYTES_MAX


def motivo_exclusao(rel: str, dev: set[str], controle: set[str]) -> str:
    if rel in dev:
        return "dev"
    if rel in controle:
        return "controle"
    if rel.startswith(PREFIXOS_FIXTURE_VRS):
        return "fixture_vrs"
    return ""


def arquivos_ambiguos(por_alvo: dict[tuple, set[str]]) -> set[str]:
    """Arquivo lido por mais de um alvo (do mesmo pacote ou de outro): não dá para dizer qual compilação o julga."""
    visto: dict[str, int] = {}
    for arquivos in por_alvo.values():
        for a in arquivos:
            visto[a] = visto.get(a, 0) + 1
    return {a for a, n in visto.items() if n > 1}


def vazamento(a: set[Hashable], b: set[Hashable]) -> set:
    return set(a) & set(b)


def id_injecao(rel: str, classe: str, linha: int, nome_novo: str, semente: str) -> str:
    return hashlib.sha256(f"{rel}|{classe}|{linha}|{nome_novo}|{semente}".encode()).hexdigest()[:12]


def fase(classe: str) -> str:
    return FASE[classe]


def cotas(classe: str, total: int) -> dict[str, int]:
    """`total` tentativas repartidas pelos operadores da classe, na proporção pré-registrada (maior resto)."""
    pesos = PESOS[classe]
    soma = sum(pesos.values())
    base = {op: total * p // soma for op, p in pesos.items()}
    restos = sorted(pesos, key=lambda op: (-(total * pesos[op] % soma), list(pesos).index(op)))
    for op in restos[: total - sum(base.values())]:
        base[op] += 1
    return base


def estilo_do(indice: int) -> str:
    return "near_miss" if indice % 2 == 0 else "plausivel"


def _alcance(no: str, deps: dict[str, set[str]]) -> set[str]:
    visto: set[str] = set()
    pilha = list(deps.get(no, ()))
    while pilha:
        x = pilha.pop()
        if x not in visto:
            visto.add(x)
            pilha.extend(deps.get(x, ()))
    return visto


def antecadeias(nos: list[str], deps: dict[str, set[str]]) -> list[list[str]]:
    """Grupos de nós SEM par relacionado por dependência (direta ou transitiva, mesmo por nó fora da lista): cada grupo cabe num `cargo
    check` só, porque um erro num pacote nunca esconde o erro de outro do mesmo grupo. Folhas primeiro, primeiro grupo que serve."""
    alvo = sorted(set(nos))
    alcance = {n: _alcance(n, deps) for n in alvo}
    grupos: list[list[str]] = []
    for n in sorted(alvo, key=lambda x: (len(alcance[x]), x)):
        for g in grupos:
            if not any(m in alcance[n] or n in alcance[m] for m in g):
                g.append(n)
                break
        else:
            grupos.append([n])
    return [sorted(g) for g in grupos]
