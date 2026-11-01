"""HORUST-REAL — operadores de injeção de erro em texto Rust real.

Acha SÍTIOS (onde uma linha pode receber um erro de nome, import, atributo ou campo de literal) e produz o texto mutado. Nada aqui roda
o rustc nem o motor: a validade de cada injeção é do oráculo (`oraculo_cargo_json.py`). Todo achado vem de uma versão MASCARADA do
texto (comentários e literais viram espaços, mesmo comprimento), então chamada dentro de string ou comentário nunca é sítio.

Zonas que nunca são sítio: atributo `#[..]`, corpo de `macro_rules!` e tudo depois do primeiro `#[cfg(test)]` (pré-registro, seção 3).
"""

from __future__ import annotations

import dataclasses
import hashlib
import random
import re

KEYWORDS = frozenset(
    "as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut pub ref return "
    "self Self static struct super trait true type unsafe use where while abstract become box do final macro override priv typeof "
    "unsized virtual yield try union gen".split()
)
PRELUDE_TIPOS = frozenset({"Option", "Result", "Vec", "String", "Box", "Self", "Some", "None", "Ok", "Err"})
STD_TIPOS_CAMINHO = PRELUDE_TIPOS | frozenset(
    "Arc Rc Cow HashMap HashSet BTreeMap BTreeSet Duration Instant PathBuf Path Mutex RwLock Default Ordering Cell RefCell From Into "
    "Iterator String Utc Local".split()
)
RAIZES_CAMINHO = frozenset({"std", "core", "alloc", "crate", "self", "super"})
PRIMITIVOS = frozenset("i8 i16 i32 i64 i128 isize u8 u16 u32 u64 u128 usize f32 f64 str char bool".split())

CODIGO = {
    "nome_valor": ("nome", "E0425"),
    "nome_tipo": ("nome", "E0425"),
    "import_use": ("import", "E0432"),
    "import_use_chaves": ("import", "E0432"),
    "import_caminho": ("import", "E0433"),
    "atributo_campo": ("atributo", "E0609"),
    "atributo_metodo": ("atributo", "E0599"),
    "kwarg_chave": ("kwarg", "E0560"),
    "kwarg_remove": ("kwarg", "E0063"),
}

# O rustc deste worktree diz `error[E0425]: cannot find type` (e não o E0412 clássico) para tipo ausente: valem os dois.
ACEITOS = {"nome_tipo": ("E0425", "E0412")}


@dataclasses.dataclass(frozen=True)
class Sitio:
    """Um lugar onde `nome` (o token original, em `linha`, colunas [inicio, fim)) pode virar erro."""

    operador: str
    linha: int
    inicio: int
    fim: int
    nome: str
    tipo: str = ""  # só kwarg: o tipo do literal
    posicao: str = ""  # só import_use: 'ultimo' (o nome importado) ou 'penultimo' (o módulo)

    @property
    def classe(self) -> str:
        return CODIGO[self.operador][0]

    @property
    def codigo(self) -> str:
        return CODIGO[self.operador][1]

    @property
    def aceitos(self) -> tuple[str, ...]:
        return ACEITOS.get(self.operador, (self.codigo,))


# ------------------------------------------------------------------ máscara

_RAW = re.compile(r'b?r(#*)"')


def _fim_comentario(texto: str, i: int, n: int) -> int | None:
    """Índice (exclusivo) do fim do comentário que começa em `i`, ou None."""
    if texto.startswith("//", i):
        j = texto.find("\n", i)
        return n if j < 0 else j
    if texto.startswith("/*", i):
        prof, j = 1, i + 2
        while j < n and prof:
            if texto.startswith("/*", j):
                prof, j = prof + 1, j + 2
            elif texto.startswith("*/", j):
                prof, j = prof - 1, j + 2
            else:
                j += 1
        return j
    return None


def _fim_string(texto: str, i: int, n: int) -> int | None:
    """Fim da string (comum, de bytes ou crua) que começa em `i`, ou None."""
    c = texto[i]
    antes_ident = i > 0 and (texto[i - 1].isalnum() or texto[i - 1] == "_")
    if c in "rb" and not antes_ident and (m := _RAW.match(texto, i)):
        fim = texto.find('"' + m.group(1), m.end())
        return n if fim < 0 else fim + 1 + len(m.group(1))
    if c == '"' or (c == "b" and not antes_ident and texto.startswith('b"', i)):
        j = i + (2 if c == "b" else 1)
        while j < n and texto[j] != '"':
            j += 2 if texto[j] == "\\" else 1
        return j + 1
    return None


def _fim_char(texto: str, i: int, n: int) -> int | None:
    """Fim do literal de caractere em `i` (`'x'`, `'\\n'`); None para lifetime (`'a`) ou fora de `'`."""
    if texto[i] != "'":
        return None
    if i + 1 < n and texto[i + 1] == "\\":
        j = texto.find("'", i + 2)
        return n if j < 0 else j + 1
    if i + 2 < n and texto[i + 2] == "'":
        return i + 3
    return None


def mascarar(texto: str) -> str:
    """Mesmo comprimento e mesmas quebras de linha; o conteúdo de comentário e de literal vira espaço."""
    out = list(texto)
    n, i = len(texto), 0
    while i < n:
        fim = _fim_comentario(texto, i, n)
        if fim is None:
            fim = _fim_string(texto, i, n)
        if fim is None:
            fim = _fim_char(texto, i, n)
        if fim is None:
            i += 1
            continue
        for k in range(i, min(fim, n)):
            if out[k] != "\n":
                out[k] = " "
        i = fim
    return "".join(out)


# ------------------------------------------------------------------ zonas vivas


def _linhas_mortas(mascarado: str, linhas: list[str]) -> set[int]:
    """Índices (0-based) de linha que nunca são sítio."""
    mortas: set[int] = set()
    corte = next((i for i, ln in enumerate(linhas) if ln.strip().startswith("#[cfg(test)]")), len(linhas))
    mortas.update(range(corte, len(linhas)))
    for i, ln in enumerate(linhas):
        s = ln.lstrip()
        if s.startswith("#[") or s.startswith("#!["):
            mortas.add(i)
    for m in re.finditer(r"\bmacro_rules!\s*\w+\s*[{(\[]", mascarado):
        abre = mascarado[m.end() - 1]
        fecha = {"{": "}", "(": ")", "[": "]"}[abre]
        prof, j = 1, m.end()
        while j < len(mascarado) and prof:
            prof += (mascarado[j] == abre) - (mascarado[j] == fecha)
            j += 1
        mortas.update(range(mascarado.count("\n", 0, m.start()), mascarado.count("\n", 0, j) + 1))
    return mortas


# ------------------------------------------------------------------ regexes

_VALOR = re.compile(r"(?<![\w.:$'])([a-z_][a-z0-9_]{2,})(?=\s*\()")
_TIPO = re.compile(
    r"(?:->\s*|(?<!:):(?!:)\s*|<\s*|&\s*(?:mut\s+)?|\bimpl\s+|\bdyn\s+)([A-Z][A-Za-z0-9]{2,})(?=[\s<>,;)\]=\{&|]|$)"
)
_USE = re.compile(r"^(\s*)use\s+((?:crate|super|self|touring_[a-z0-9_]+)(?:::[A-Za-z_][A-Za-z0-9_]*)+)\s*;\s*$")
_USE_CH = re.compile(
    r"^(\s*)use\s+((?:crate|super|self|touring_[a-z0-9_]+)(?:::[A-Za-z_][A-Za-z0-9_]*)*)::\{([^{}]*)\}\s*;\s*$"
)
_CAM_T = re.compile(r"(?<![\w:.$'])([A-Z][A-Za-z0-9]{2,})(?=::[a-z_]\w*\s*\()")
_CAM_M = re.compile(r"(?<![\w:.$'])([a-z_][a-z0-9_]{2,})(?=::[A-Za-z_])")
_CAMPO = re.compile(r"(?<=[\w)\]?])\.([a-z_][a-z0-9_]{2,})\b(?!\s*(?:\(|::))")
_METODO = re.compile(r"(?<=[\w)\]?])\.([a-z_][a-z0-9_]{2,})\s*(?:::<[^()]*>)?(?=\()")
_METODO_LINHA = re.compile(r"^\s*\.([a-z_][a-z0-9_]{2,})\s*(?:::<[^()]*>)?(?=\()")
_ABRE_LITERAL = re.compile(r"(?<![\w:.$'])([A-Z][A-Za-z0-9_]*)\s*\{\s*$")
_CAMPO_LITERAL = re.compile(r"^(\s*)([a-z_][a-z0-9_]*)\s*:\s*(.*),\s*$")
_PRE_NAO_LITERAL = re.compile(r"\b(?:struct|enum|impl|trait|union|mod|fn|for|where|type)\b")


def _eh_use(linha: str) -> bool:
    return linha.lstrip().startswith(("use ", "pub use", "pub(crate) use", "mod ", "pub mod", "extern "))


def _adder(i: int, ln: str, vistos: set, out: list):
    def add(op: str, g: re.Match, grupo: int = 1, **kw) -> None:
        a, b = g.span(grupo)
        chave = (op, i + 1, a)
        if chave not in vistos:
            vistos.add(chave)
            out.append(Sitio(op, i + 1, a, b, ln[a:b], **kw))

    return add


def _sitios_nome(ln: str, add) -> None:
    for g in _VALOR.finditer(ln):
        if g.group(1) in KEYWORDS or re.search(r"\b(?:fn|macro_rules!)\s*$", ln[: g.start()]):
            continue
        add("nome_valor", g)
    for g in _TIPO.finditer(ln):
        nome = g.group(1)
        if nome not in PRELUDE_TIPOS and not nome.isupper():
            add("nome_tipo", g)


def _sitios_use(i: int, ln: str, vistos: set, out: list, add) -> None:
    if m := _USE.match(ln):
        segs, base = m.group(2).split("::"), m.start(2)
        pos = [base]
        for s in segs[:-1]:
            pos.append(pos[-1] + len(s) + 2)
        add("import_use", re.compile("(" + re.escape(segs[-1]) + ")").match(ln, pos[-1]), posicao="ultimo")
        if len(segs) >= 3:
            add("import_use", re.compile("(" + re.escape(segs[-2]) + ")").match(ln, pos[-2]), posicao="penultimo")
    if m := _USE_CH.match(ln):
        ini, cursor = m.start(3), 0
        for item in m.group(3).split(","):
            nome = item.strip()
            if re.fullmatch(r"[A-Za-z_]\w*", nome) and nome != "self":
                a = ini + cursor + item.index(nome)
                chave = ("import_use_chaves", i + 1, a)
                if chave not in vistos:
                    vistos.add(chave)
                    out.append(Sitio("import_use_chaves", i + 1, a, a + len(nome), nome))
            cursor += len(item) + 1


def _sitios_caminho_e_atributo(ln: str, add) -> None:
    for g in _CAM_T.finditer(ln):
        if g.group(1) not in STD_TIPOS_CAMINHO and not g.group(1).isupper():
            add("import_caminho", g)
    for g in _CAM_M.finditer(ln):
        if g.group(1) not in RAIZES_CAMINHO | PRIMITIVOS | KEYWORDS:
            add("import_caminho", g)
    for g in _CAMPO.finditer(ln):
        if g.group(1) not in KEYWORDS:
            add("atributo_campo", g)
    for g in [*_METODO.finditer(ln), *([_METODO_LINHA.match(ln)] if _METODO_LINHA.match(ln) else [])]:
        if g.group(1) not in KEYWORDS:
            add("atributo_metodo", g)


def _sitios_de_linha(i: int, ln: str, vistos: set) -> list[Sitio]:
    out: list[Sitio] = []
    add = _adder(i, ln, vistos, out)
    _sitios_nome(ln, add)
    _sitios_use(i, ln, vistos, out, add)
    if not _eh_use(ln):
        _sitios_caminho_e_atributo(ln, add)
    return out


def _fecha_bloco(linhas: list[str], i: int, col: int) -> tuple[int, int] | None:
    """(linha, coluna) do `}` que fecha o `{` em (i, col), ou None."""
    prof = 0
    for j in range(i, len(linhas)):
        for k in range(col if j == i else 0, len(linhas[j])):
            prof += linhas[j][k] == "{"
            prof -= linhas[j][k] == "}"
            if prof == 0 and linhas[j][k] == "}":
                return j, k
    return None


def _e_padrao(resto: str) -> bool:
    r = resto.lstrip()
    return r.startswith("|") or (r.startswith("=") and not r.startswith("==")) or re.match(r"if\b", r) is not None


def _sitios_kwarg(linhas: list[str], originais: list[str], mortas: set[int]) -> list[Sitio]:
    out: list[Sitio] = []
    for i, ln in enumerate(linhas):
        m = _ABRE_LITERAL.search(ln)
        if i in mortas or not m or m.group(1) == "Self" or _PRE_NAO_LITERAL.search(ln[: m.start()]):
            continue
        fim = _fecha_bloco(linhas, i, m.end() - 1)
        if fim is None or _e_padrao(linhas[fim[0]][fim[1] + 1 :]):
            continue
        campos, update, prof = [], False, 0
        for j in range(i + 1, fim[0]):
            lj = linhas[j]
            if prof == 0 and lj.strip().startswith(".."):
                update = True
            if prof == 0 and (c := _CAMPO_LITERAL.match(lj)):
                valor = c.group(3)
                inteira = sum(valor.count(x) for x in "([{") == sum(valor.count(x) for x in ")]}")
                campos.append((j, c, inteira))
            elif prof == 0 and re.match(r"^\s*[a-z_][a-z0-9_]*\s*:", lj):
                campos.append((j, None, False))
            prof += sum(lj.count(x) for x in "([{") - sum(lj.count(x) for x in ")]}")
        tipo = m.group(1)
        for j, c, inteira in campos:
            if c is not None:
                a, b = c.span(2)
                out.append(Sitio("kwarg_chave", j + 1, a, b, originais[j][a:b], tipo))
                if inteira and len(campos) >= 2 and not update:
                    out.append(Sitio("kwarg_remove", j + 1, a, b, originais[j][a:b], tipo))
    return out


def sitios(texto: str) -> list[Sitio]:
    """Todo sítio de todo operador, ordenado por (linha, coluna, operador)."""
    mascarado = mascarar(texto)
    linhas, originais = mascarado.split("\n"), texto.split("\n")
    mortas = _linhas_mortas(mascarado, linhas)
    vistos: set = set()
    achados: list[Sitio] = []
    for i, ln in enumerate(linhas):
        if i not in mortas:
            achados += _sitios_de_linha(i, ln, vistos)
    achados += _sitios_kwarg(linhas, originais, mortas)
    return sorted(achados, key=lambda s: (s.linha, s.inicio, s.operador))


def tipo_do_nome(s: Sitio) -> str:
    """Que espécie de nome é (decide as variantes plausíveis): valor | tipo | campo | metodo | modulo."""
    if s.operador in ("atributo_campo", "kwarg_chave"):
        return "campo"
    if s.operador == "atributo_metodo":
        return "metodo"
    if s.operador == "nome_valor":
        return "valor"
    if s.operador == "nome_tipo":
        return "tipo"
    return "tipo" if s.nome[:1].isupper() else "modulo"


# ------------------------------------------------------------------ nomes novos


def distancia(a: str, b: str) -> int:
    """Distância de Levenshtein (uma transposição custa 2)."""
    ant = list(range(len(b) + 1))
    for i, x in enumerate(a, 1):
        atual = [i]
        for j, y in enumerate(b, 1):
            atual.append(min(ant[j] + 1, atual[j - 1] + 1, ant[j - 1] + (x != y)))
        ant = atual
    return ant[-1]


def _h(chave: str) -> int:
    return int(hashlib.sha256(chave.encode()).hexdigest(), 16)


_LETRAS = "abcdefghijklmnopqrstuvwxyz"


def _op_unica(nome: str, rng: random.Random, kind: str) -> str | None:
    """Uma edição interna (o primeiro caractere nunca muda, então tipo continua tipo)."""
    n = len(nome)
    if n < 3:
        return None
    pos = rng.randrange(1, n)
    if kind == "del":
        return nome[:pos] + nome[pos + 1 :]
    if kind == "sub":
        c = rng.choice([x for x in _LETRAS if x != nome[pos].lower()])
        return nome[:pos] + c + nome[pos + 1 :]
    if kind == "ins":
        return nome[:pos] + rng.choice(_LETRAS) + nome[pos:]
    if kind == "trans" and n >= 4 and pos < n - 1 and nome[pos] != nome[pos + 1]:
        return nome[:pos] + nome[pos + 1] + nome[pos] + nome[pos + 2 :]
    return None


def _near_miss(nome: str, chave: str, existentes: set[str]) -> str | None:
    alvo = 1 if _h(chave + "|d") % 10 < 7 else 2
    for tentativa in range(48):
        rng = random.Random(_h(f"{chave}|{tentativa}"))
        if alvo == 1:
            v = _op_unica(nome, rng, rng.choice(["del", "sub", "ins"]))
        elif rng.random() < 0.4:
            v = _op_unica(nome, rng, "trans")
        else:
            v = _op_unica(nome, rng, rng.choice(["del", "sub", "ins"]))
            v = _op_unica(v, rng, rng.choice(["del", "sub", "ins"])) if v else None
        if v and v != nome and distancia(nome, v) == alvo and re.fullmatch(r"[A-Za-z_]\w*", v) and v not in KEYWORDS and v not in existentes:
            return v
    return None


_PLAUSIVEL = {
    "valor": ("get_{n}", "try_{n}", "{n}_all", "{n}_with"),
    "tipo": ("{n}Info", "{n}Config", "{n}Data", "Raw{n}"),
    "campo": ("{n}_id", "{n}_count", "{n}_all", "try_{n}"),
    "metodo": ("get_{n}", "try_{n}", "{n}_all", "{n}_with"),
    "modulo": ("{n}_utils", "{n}_impl", "{n}_core"),
}


def variante(nome: str, tipo: str, estilo: str, chave: str, existentes: set[str]) -> str | None:
    """Nome novo para `nome`. `estilo`: near_miss (distância 1 em 70%, 2 em 30%) ou plausivel (nome inventado verossímil).
    Devolve None se nenhuma variante escapa dos `existentes` (tokens do crate) e das palavras reservadas."""
    if estilo == "near_miss":
        return _near_miss(nome, chave, existentes)
    modelos = list(_PLAUSIVEL[tipo])
    random.Random(_h(chave + "|p")).shuffle(modelos)
    for m in modelos:
        v = m.format(n=nome)
        if v not in existentes and v not in KEYWORDS:
            return v
    return None


# ------------------------------------------------------------------ aplicar


def mutar(texto: str, sitio: Sitio, novo: str) -> str:
    """O texto com o sítio trocado por `novo` (kwarg_remove: a linha inteira sai)."""
    linhas = texto.split("\n")
    if sitio.linha > len(linhas) or linhas[sitio.linha - 1][sitio.inicio : sitio.fim] != sitio.nome:
        raise ValueError(f"o sítio {sitio.operador}@{sitio.linha} não bate com o texto: {sitio.nome!r}")
    if sitio.operador == "kwarg_remove":
        del linhas[sitio.linha - 1]
    else:
        ln = linhas[sitio.linha - 1]
        linhas[sitio.linha - 1] = ln[: sitio.inicio] + novo + ln[sitio.fim :]
    return "\n".join(linhas)


def tokens(texto: str) -> set[str]:
    return set(re.findall(r"[A-Za-z_][A-Za-z0-9_]*", texto))
