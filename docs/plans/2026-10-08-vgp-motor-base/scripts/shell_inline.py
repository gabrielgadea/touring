"""O programa Python que um comando de shell EXECUTA — fonte única do corpus V0d e do corpus V5 de negativos.

Por que existe (09/10/2026): os dois extratores anteriores eram camadas de regex sobre o texto do comando e
devolviam textos que NÃO eram o Python executado, invisíveis ao controle "compila". Medido no universo de 30.290
comandos Bash do analise: `touring run --lang bash` 2.814 (5 compilam), heredoc de `cat` 592 (315 compilam!),
heredoc de `git commit -F -` 204, `-c "…"` sem desescape 364 de 2.700 que não compilavam.

O desenho: um LEXER de shell (aspas simples, duplas, `$'…'`, escapes, `$(…)`, crases, operadores, redireções,
heredocs com delimitador entre aspas ou não, `<<-`) monta os comandos simples; o programa só é devolvido por um
PREDICADO POSITIVO sobre o comando simples que o consome:

  * interpretador Python (`python`, `python3`, `python3.12`, `.venv/bin/python`…, atrás de `env`/`timeout`/`nice`/
    `stdbuf`/`uv run`/atribuições — inclusive as opções desses embrulhos QUE TOMAM VALOR: `env -u NOME`,
    `timeout -s SIG -k DUR`, `nice -n N`, `stdbuf -o MODO`) com `-c <palavra>` → a palavra, desescapada pelas
    regras do bash;
  * o mesmo interpretador SEM script e com heredoc (ou `<<<`) no stdin → o corpo;
  * `touring run --lang python --code <palavra>` → a palavra; `--file <arq>` → só o caminho (quem tem a sessão
    reconstrói o arquivo);
  * SHELL ANINHADO (`bash|sh|dash|zsh -c X`, `bash -lc X`, `bash`/`sh` sem script lendo heredoc/`<<<` no stdin,
    `touring run --lang bash|sh --code X`): o texto X é um script de shell que também RODOU — é analisado de
    novo, recursivamente (profundidade ≤ 3), e os programas Python achados dentro viram candidatos.

Heredoc SEM aspas no delimitador: o bash processa as barras (`\\$`→`$`, `` \\` ``→crase, `\\\\`→`\\`, barra+quebra
some; `\\n`, `\\"`, `\\a`… ficam) e só recusamos quando houve EXPANSÃO de verdade (`$nome`, `${…}`, `$(…)`, `$((…))`,
`$[…]`, `$1`, `$@ $* $# $? $$ $! $-`, crase não escapada). `$'`, `$"`, `$ ` e `$` solto são literais.

Tudo o mais devolve `texto=None` com o MOTIVO nomeado: outro consumidor do heredoc (`cat`, `tee`, `git`…),
script no argv (o heredoc é DADO no stdin), `--lang` de outra linguagem, expansão do shell no texto (`$x`,
`` `cmd` ``, heredoc sem aspas com expansão — o Python recebeu OUTRO texto), mais de um programa no mesmo
comando (ambíguo: o traceback não diz qual), shell que não se deixa ler (aspas não fechadas).

API: `analisar(cmd) -> Programa` (exatamente UM programa, senão None com motivo) · `programa_python(cmd)` (só o
texto) · `candidatos(cmd) -> list[Programa]` (TODOS os programas válidos, em ordem textual, aninhados
incluídos) · `achados(cmd)` (candidatos + recusas, na mesma ordem). Todos aceitam `substituicoes`, que
analisa o Python dentro de `$(…)` e crase. Padrões: `candidatos`/`achados` True («todo programa que RODOU»);
`analisar`/`programa_python` False (o caso nº 30 do selftest, `x=$(python3 -c …)` → None, é contrato do extrator
antigo e os corpora indexam UM programa). Quem precisa de UM programa de um comando
com vários (`touring run --lang python --code '…' | python3 -c '…'`, ~1.500 no universo) escolhe o candidato pela
EVIDÊNCIA que tem — por exemplo, o traceback dentro do envelope JSON do `touring run` é do `--code`, não do
`python3 -c` do pipe. ⚠ `candidatos` não lista o que o shell expandiu (programa ilegível): esse aparece em
`achados` como recusa, e quem o ignora pode atribuir o traceback ao candidato errado.

Uso: `python3 shell_inline.py --selftest` · `python3 shell_inline.py '<comando>'` (imprime o veredito em JSON).
"""

from __future__ import annotations

import json
import re
import shlex
import sys
from dataclasses import dataclass, field, replace

# --------------------------------------------------------------------------- tipos


@dataclass
class Palavra:
    """Uma palavra do shell: o valor após remover aspas e o que o shell teria EXPANDIDO nela."""

    valor: str
    expande: bool = False  # `$x`, `$(…)`, crase ou glob fora de aspas simples: o processo recebe outro texto
    citada: bool = False  # alguma parte veio entre aspas (decide se o delimitador do heredoc é literal)
    subs: list[str] = field(
        default_factory=list
    )  # corpos de `$(…)` e crase na palavra: scripts de shell que também RODARAM


@dataclass
class Redir:
    """Uma redireção. Para heredoc, `corpo` e `literal` (delimitador entre aspas) são preenchidos."""

    op: str  # "<<", "<<-", "<<<", "<", ">", ">>", ...
    fd: int | None
    alvo: Palavra | None = None
    corpo: str | None = None
    literal: bool = True


@dataclass
class Simples:
    """Um comando simples: as palavras (sem os alvos de redireção) e as redireções."""

    palavras: list[Palavra] = field(default_factory=list)
    redirs: list[Redir] = field(default_factory=list)


@dataclass(frozen=True)
class Programa:
    """O veredito: `texto` só quando é o Python executado; `motivo` sempre diz por quê."""

    texto: str | None
    # "-c" | "heredoc" | "herestring" | "touring_run_code" | "touring_run_file" | "shell_aninhado" | "comando_expandido" | "nenhuma"
    via: str
    motivo: str
    arquivo: str | None = (
        None  # `touring run --file`: o caminho, para quem tem a sessão reconstruir
    )
    prof: int = 0  # 0 = comando de topo; N = dentro de N shells aninhados (`bash -c`, `touring run --lang bash`)


@dataclass(frozen=True)
class _Ctx:
    """Onde a análise está: o nível de aninhamento e se `$(…)`/crase também são analisados como shell aninhado."""

    prof: int = 0
    subst: bool = False

    def dentro(self) -> _Ctx:
        return replace(self, prof=self.prof + 1)


class ShellIlegivel(ValueError):
    """O comando não se deixa ler como shell (aspas ou heredoc sem fecho)."""


# --------------------------------------------------------------------------- lexer

_OPS = ("&&", "||", ";;", "|&", ";", "|", "&", "(", ")")
_REDIR = re.compile(r"(\d*)(<<<|<<-|<<|<>|<&|>&|>>|>\||&>>|&>|<|>)")
_DUPLAS_ESC = {"$", "`", '"', "\\"}
_ANSI = {
    "n": "\n",
    "t": "\t",
    "r": "\r",
    "a": "\a",
    "b": "\b",
    "f": "\f",
    "v": "\v",
    "e": "\x1b",
    "E": "\x1b",
    "\\": "\\",
    "'": "'",
    '"': '"',
    "?": "?",
}
_EXPANSAO = re.compile(r"[A-Za-z_{(0-9@*#?$!-]")


class _Lexer:
    """Lê o comando em tokens: ("w", Palavra) · ("op", str) · ("redir", Redir) · ("nl", None)."""

    def __init__(self, s: str) -> None:
        self.s, self.i, self.toks = s, 0, []
        self.pendentes: list[
            tuple[Redir, str, bool]
        ] = []  # heredocs cujo corpo começa na próxima linha

    def ler(self) -> list[tuple[str, object]]:
        while self.i < len(self.s):
            self._passo()
        if self.pendentes:
            raise ShellIlegivel("heredoc sem corpo")
        return self.toks

    def _passo(self) -> None:
        c = self.s[self.i]
        if c == "\n":
            self.i += 1
            self.toks.append(("nl", None))
            self._corpos()
        elif c in " \t":
            self.i += 1
        elif c == "\\" and self.s.startswith("\\\n", self.i):
            self.i += 2  # continuação de linha
        elif c == "#":
            fim = self.s.find("\n", self.i)
            self.i = len(self.s) if fim < 0 else fim
        elif (m := _REDIR.match(self.s, self.i)) and (
            m.group(1) == "" or self._fd_ok()
        ):
            self._redir(m)
        elif op := next((o for o in _OPS if self.s.startswith(o, self.i)), None):
            self.i += len(op)
            self.toks.append(("op", op))
        else:
            self.toks.append(("w", self._palavra()))

    def _fd_ok(self) -> bool:
        """`2>` é redireção só quando os dígitos começam a palavra (`a2>b` não é)."""
        return self.i == 0 or self.s[self.i - 1] in " \t\n;|&()"

    def _redir(self, m: re.Match[str]) -> None:
        self.i = m.end()
        op, fd = m.group(2), int(m.group(1)) if m.group(1) else None
        while self.i < len(self.s) and self.s[self.i] in " \t":
            self.i += 1
        alvo = self._palavra()
        r = Redir(op=op, fd=fd, alvo=alvo)
        if op in ("<<", "<<-"):
            r.literal = alvo.citada
            self.pendentes.append((r, alvo.valor, op == "<<-"))
        self.toks.append(("redir", r))

    def _corpos(self) -> None:
        """Depois de um fim de linha, os corpos dos heredocs pendentes, na ordem em que foram abertos."""
        for r, delim, tira_tab in self.pendentes:
            linhas = []
            while True:
                if self.i >= len(self.s):
                    raise ShellIlegivel(f"heredoc sem delimitador final: {delim}")
                fim = self.s.find("\n", self.i)
                linha = self.s[self.i :] if fim < 0 else self.s[self.i : fim]
                self.i = len(self.s) if fim < 0 else fim + 1
                if tira_tab:
                    linha = linha.lstrip("\t")
                if linha == delim:
                    break
                linhas.append(linha)
            r.corpo = "".join(ln + "\n" for ln in linhas)
        self.pendentes = []

    def _palavra(self) -> Palavra:
        p = Palavra(valor="")
        out: list[str] = []
        while self.i < len(self.s) and self.s[self.i] not in " \t\n;|&()<>":
            c = self.s[self.i]
            self.i += 1
            out.append(self._trecho(c, p))
        p.valor = "".join(out)
        return p

    def _trecho(self, c: str, p: Palavra) -> str:
        """O que o caractere `c` (já consumido) e o que ele abre contribuem para o valor da palavra."""
        if c == "'":
            p.citada = True
            fim = self.s.find("'", self.i)
            if fim < 0:
                raise ShellIlegivel("aspa simples sem fecho")
            trecho, self.i = self.s[self.i : fim], fim + 1
            return trecho
        if c == '"':
            p.citada = True
            return self._duplas(p)
        if c == "$" and self.s.startswith("'", self.i):
            p.citada = True
            self.i += 1
            return self._ansi()
        if c == "\\":
            p.citada = True  # `<<\EOF` cita o delimitador: o corpo NÃO é expandido
            trecho = self.s[self.i : self.i + 1]
            self.i += len(trecho)
            return trecho
        if c in "$`":
            return self._expansao(c, p)
        if c in "*?[":
            p.expande = True  # glob fora de aspas
        return c

    def _duplas(self, p: Palavra) -> str:
        out: list[str] = []
        while self.i < len(self.s):
            c = self.s[self.i]
            self.i += 1
            if c == '"':
                return "".join(out)
            if c == "\\" and self.i < len(self.s) and self.s[self.i] in _DUPLAS_ESC:
                out.append(self.s[self.i])
                self.i += 1
            elif c == "\\" and self.s.startswith("\n", self.i):
                self.i += 1  # continuação dentro de duplas
            elif c in "$`":
                out.append(self._expansao(c, p))
            else:
                out.append(c)
        raise ShellIlegivel("aspa dupla sem fecho")

    def _ansi(self) -> str:
        out: list[str] = []
        while self.i < len(self.s):
            c = self.s[self.i]
            self.i += 1
            if c == "'":
                return "".join(out)
            if c == "\\" and self.i < len(self.s):
                e = self.s[self.i]
                self.i += 1
                out.append(_ANSI.get(e, "\\" + e))
            else:
                out.append(c)
        raise ShellIlegivel("$'…' sem fecho")

    def _expansao(self, c: str, p: Palavra) -> str:
        """`$x`, `${…}`, `$(…)`, `$((…))`, crase: copia o texto cru e marca a palavra como expandida."""
        ini = self.i - 1
        if c == "`":
            fim = self._fecho_crase()
        elif self.s.startswith("(", self.i) or self.s.startswith("{", self.i):
            fim = self._fecho_balanceado(self.s[self.i])
        elif self.i < len(self.s) and _EXPANSAO.match(self.s[self.i]):
            m = re.match(r"[A-Za-z_]\w*|[0-9@*#?$!-]", self.s[self.i :])
            fim = self.i + (m.end() if m else 1)
        else:
            return "$"  # `$` solto é literal no bash
        p.expande = True
        self.i = fim
        self._registra_subst(p, ini, fim)
        return self.s[ini:fim]

    def _registra_subst(self, p: Palavra, ini: int, fim: int) -> None:
        """Guarda o corpo de `$(…)` e de crase — um script de shell que também RODOU (`$((…))` é aritmética)."""
        bruto = self.s[ini:fim]
        if bruto.startswith("`"):
            p.subs.append(re.sub(r"\\([$`\\])", r"\1", bruto[1:-1]))
        elif bruto.startswith("$(") and not bruto.startswith("$(("):
            p.subs.append(bruto[2:-1])

    def _fecho_crase(self) -> int:
        j = self.i
        while j < len(self.s):
            if self.s[j] == "\\":
                j += 2
                continue
            if self.s[j] == "`":
                return j + 1
            j += 1
        raise ShellIlegivel("crase sem fecho")

    def _fim_duplas(self, j: int) -> int:
        """O índice logo após o `"` que fecha as aspas duplas abertas antes de `j` (ou o fim do texto)."""
        while j < len(self.s):
            if self.s[j] == "\\":
                j += 2
            elif self.s[j] == '"':
                return j + 1
            else:
                j += 1
        return len(self.s)

    _ABRE_HEREDOC = re.compile(r"<<(-?)[ \t]*(?:'([^'\n]+)'|\"([^\"\n]+)\"|\\?(\w+))")

    def _fim_heredoc(self, j: int) -> int:
        """Se em `j` abre um heredoc (`<<'EOF'`), o índice logo após a linha do delimitador; senão `j`.
        Dentro de `$(cat <<'EOF' … EOF)` o corpo NÃO é lido como shell: aspas e parênteses dele não contam."""
        m = self._ABRE_HEREDOC.match(self.s, j)
        if m is None or self.s.startswith("<<<", j):
            return j
        delim = m.group(2) or m.group(3) or m.group(4)
        k = self.s.find("\n", m.end()) + 1
        while 0 < k <= len(self.s):
            fim = self.s.find("\n", k)
            linha = self.s[k : len(self.s) if fim < 0 else fim]
            if (linha.lstrip("\t") if m.group(1) else linha) == delim:
                return len(self.s) if fim < 0 else fim + 1
            k = 0 if fim < 0 else fim + 1
        return j

    def _pula(self, j: int) -> int:
        """Se em `j` começa um trecho em que parênteses não contam (`\\x`, aspas, heredoc), o índice após ele; senão `j`."""
        ch = self.s[j]
        if ch == "\\":
            return j + 2
        if ch == "'":
            k = self.s.find("'", j + 1)
            return len(self.s) if k < 0 else k + 1
        if ch == '"':  # um `)` entre aspas duplas não fecha a substituição
            return self._fim_duplas(j + 1)
        return self._fim_heredoc(j) if ch == "<" else j

    def _fecho_balanceado(self, abre: str) -> int:
        fecha = {"(": ")", "{": "}"}[abre]
        prof, j = 0, self.i
        while j < len(self.s):
            novo = self._pula(j)
            if novo != j:
                j = novo
                continue
            if self.s[j] == abre:
                prof += 1
            elif self.s[j] == fecha:
                prof -= 1
                if prof == 0:
                    return j + 1
            j += 1
        raise ShellIlegivel(f"expansão sem fecho: {abre}")


def comandos_simples(comando: str) -> list[Simples]:
    """Os comandos simples do texto, na ordem. Levanta ShellIlegivel se o shell não se deixa ler."""
    atual, out = Simples(), []
    for tipo, val in _Lexer(comando).ler():
        if tipo == "w":
            atual.palavras.append(val)  # type: ignore[arg-type]
        elif tipo == "redir":
            atual.redirs.append(val)  # type: ignore[arg-type]
        else:  # operador ou fim de linha fecham o comando simples
            if atual.palavras or atual.redirs:
                out.append(atual)
            atual = Simples()
    if atual.palavras or atual.redirs:
        out.append(atual)
    return out


# --------------------------------------------------------------------------- o predicado

_PY = re.compile(r"python(3(\.\d+)?)?")
_ATRIB = re.compile(r"[A-Za-z_]\w*=.*", re.S)
_RESERVADAS = {"do", "then", "else", "elif", "{", "!", "exec", "command", "nohup"}
# embrulho → (letras curtas que TOMAM VALOR, opções longas que tomam valor, operandos posicionais após as opções)
_EMBRULHOS: dict[str, tuple[str, frozenset[str], int]] = {
    "env": ("uCS", frozenset({"--unset", "--chdir", "--split-string"}), 0),
    "timeout": ("sk", frozenset({"--signal", "--kill-after"}), 1),
    "nice": ("n", frozenset({"--adjustment"}), 0),
    "stdbuf": ("ioe", frozenset({"--input", "--output", "--error"}), 0),
    "time": ("fo", frozenset({"--format", "--output"}), 0),
    "setsid": ("", frozenset(), 0),
    # opções de `unshare --help`: as de argumento OPCIONAL (`--mount[=ARQ]`) só levam valor com `=`
    "unshare": (
        "lRwSG",
        frozenset(
            {
                "--load-interp",
                "--propagation",
                "--root",
                "--wd",
                "--setuid",
                "--setgid",
                "--map-user",
                "--map-group",
                "--map-users",
                "--map-groups",
                "--owner",
                "--setgroups",
                "--monotonic",
                "--boottime",
            }
        ),
        0,
    ),
    # opções de `systemd-run --help` com argumento OBRIGATÓRIO (a 1ª palavra que não é opção é o comando)
    "systemd-run": (
        "HMCupE",
        frozenset(
            {
                "--host",
                "--machine",
                "--capsule",
                "--unit",
                "--property",
                "--description",
                "--slice",
                "--expand-environment",
                "--service-type",
                "--uid",
                "--gid",
                "--nice",
                "--working-directory",
                "--root-directory",
                "--setenv",
                "--output",
                "--json",
                "--job-mode",
                "--background",
                "--path-property",
                "--socket-property",
                "--timer-property",
                "--on-active",
                "--on-boot",
                "--on-startup",
                "--on-unit-active",
                "--on-unit-inactive",
                "--on-calendar",
            }
        ),
        0,
    ),
}
_UV_OPC_COM_VALOR = {"--project", "--with", "--python", "--directory"}
_PY_OPC_COM_VALOR = {"-W", "-X", "--check-hash-based-pycs"}
_SHELLS = frozenset({"bash", "sh", "dash", "zsh"})
_SH_LONGAS_COM_VALOR = frozenset({"--rcfile", "--init-file"})
_PROF_MAX = 3  # níveis de shell aninhado analisados (o comando de topo é o nível 0)
_MENCIONA_PY = re.compile(r"python|touring\s+run")
_EXPANSAO_HEREDOC = re.compile(
    r"[A-Za-z_{(\[0-9@*#?$!-]"
)  # o que vem depois de `$` e o bash expande
_RECUSA_HEREDOC = (
    "heredoc sem aspas no delimitador com expansão do shell "
    "($nome, ${…}, $(…), $[…], crase): o bash o expandiu"
)


def _opcoes_uv(p: list[Palavra], j: int) -> int:
    """Quantas palavras as opções de `uv run` ocupam a partir de `j`."""
    n = 0
    while j + n < len(p) and p[j + n].valor.startswith("-"):
        n += 2 if p[j + n].valor in _UV_OPC_COM_VALOR else 1
    return n


def _largura_curta(v: str, curtas: str) -> int:
    """Palavras que a opção curta `v` ocupa: 2 se uma letra que TOMA VALOR fecha a palavra (`-u NOME`), senão 1
    (`-uNOME` e `-oL` carregam o valor colado; `-i` e `-10` não têm valor)."""
    for k, c in enumerate(v[1:], start=1):
        if c in curtas:
            return 1 if k + 1 < len(v) else 2
    return 1


def _opcoes_embrulho(
    p: list[Palavra], j: int, curtas: str, longas: frozenset[str]
) -> int:
    """Quantas palavras as opções de um embrulho (`env`, `timeout`, `nice`, `stdbuf`) ocupam a partir de `j`,
    VALORES incluídos: `--opt=v` é 1 palavra; `--opt v` só quando `--opt` toma valor; `--` encerra as opções."""
    n = 0
    while j + n < len(p):
        v = p[j + n].valor
        if v == "--":
            return n + 1
        if v.startswith("--"):
            n += 2 if v in longas else 1
        elif v.startswith("-"):
            n += _largura_curta(v, curtas)
        else:
            break
    return n


def _prefixo(p: list[Palavra], i: int) -> int:
    """Quantas palavras o prefixo em `i` ocupa; 0 quando `p[i]` já é o comando de verdade."""
    v = p[i].valor
    if _ATRIB.fullmatch(v) or v in _RESERVADAS:
        return 1
    nome = _base(v)
    if nome in _EMBRULHOS:
        curtas, longas, operandos = _EMBRULHOS[nome]
        # `env [opções] NOME=valor… comando`: o `NOME=valor` é pulado pela próxima volta de `_efetivas`
        return 1 + _opcoes_embrulho(p, i + 1, curtas, longas) + operandos
    if v == "uv" and i + 1 < len(p) and p[i + 1].valor == "run":
        return 2 + _opcoes_uv(p, i + 2)
    return 0


def _efetivas(palavras: list[Palavra]) -> list[Palavra]:
    """As palavras a partir do comando de verdade: sem atribuições, palavras reservadas e embrulhos."""
    i = 0
    while i < len(palavras):
        n = _prefixo(palavras, i)
        if n == 0:
            return palavras[i:]
        i += n
    return []


def _base(v: str) -> str:
    return v.rsplit("/", 1)[-1]


def _modo_python(argv: list[Palavra]) -> tuple[str, int]:
    """Como o interpretador recebe o programa: ("c", i do -c) · ("m", i) · ("script", i) · ("stdin", -1)."""
    i = 1
    while i < len(argv):
        v = argv[i].valor
        if re.fullmatch(r"-[A-Za-z]*c", v):
            return "c", i
        if re.fullmatch(r"-[A-Za-z]*m.*", v):
            return "m", i
        if v == "-":
            return "stdin", -1
        if not v.startswith("-"):
            return "script", i
        i += 2 if v in _PY_OPC_COM_VALOR else 1
    return "stdin", -1


def _do_python(s: Simples, argv: list[Palavra]) -> Programa | None:
    """O veredito de um interpretador Python; None se o comando não é um."""
    if not argv or not _PY.fullmatch(_base(argv[0].valor)):
        return None
    modo, i = _modo_python(argv)
    if modo == "m":
        return Programa(None, "nenhuma", "python -m: módulo, não programa inline")
    if modo == "script":
        return Programa(
            None, "nenhuma", "python com script no argv: o stdin é DADO, não programa"
        )
    if modo == "stdin":
        return _do_stdin(s)
    if i + 1 >= len(argv):
        return Programa(None, "-c", "-c sem programa")
    w = argv[i + 1]
    if w.expande:
        return Programa(
            None,
            "-c",
            "-c com expansão do shell ($, crase ou glob): o Python recebeu outro texto",
        )
    return Programa(w.valor + "\n", "-c", "python -c")


def _redir_stdin(s: Simples) -> Redir | None:
    """A redireção que alimenta o fd 0 (a última vence, como no bash); None se não há."""
    stdin = [
        r for r in s.redirs if r.op in ("<<", "<<-", "<<<", "<") and r.fd in (None, 0)
    ]
    return stdin[-1] if stdin else None


def _sem_continuacao(corpo: str) -> tuple[str, bool]:
    """Remove barra+quebra como o bash: os pares `\\x` são consumidos da esquerda para a direita (`\\\\` + quebra
    NÃO é continuação). O bash faz isto ANTES de procurar expansões — `$\\<quebra>?` vira `$?`. O 2º valor diz se
    a ÚLTIMA quebra do corpo era continuação: a barra engoliu a linha do delimitador."""
    out: list[str] = []
    i, engoliu = 0, False
    while i < len(corpo):
        par = corpo[i : i + 2]
        if par == "\\\n":
            engoliu = i + 2 == len(corpo)
            i += 2
        elif par.startswith("\\") and len(par) == 2:
            out.append(par)
            i += 2
        else:
            out.append(corpo[i])
            i += 1
    return "".join(out), engoliu


def _corpo_heredoc(r: Redir) -> tuple[str | None, str]:
    """O texto que o consumidor do heredoc RECEBEU: delimitador entre aspas → o corpo cru; sem aspas → o corpo
    com as barras processadas como o bash (`\\$`→`$`, `` \\` ``→crase, `\\\\`→`\\`, barra+quebra some; as demais
    barras ficam). Há EXPANSÃO de verdade (`$nome`, `${`, `$(`, `$[`, `$1`, `$@`…, crase) → (None, motivo)."""
    if r.literal:
        return r.corpo or "", ""
    corpo, engoliu = _sem_continuacao(r.corpo or "")
    if engoliu:
        return None, (
            "heredoc sem aspas: a barra no fim do corpo engoliu a linha do delimitador"
        )
    out: list[str] = []
    i = 0
    while i < len(corpo):
        c, nxt = corpo[i], corpo[i + 1 : i + 2]
        if c == "\\" and nxt != "" and nxt in "$`\\":
            out.append(nxt)
            i += 2
        elif c == "`" or (c == "$" and _EXPANSAO_HEREDOC.match(nxt)):
            return None, _RECUSA_HEREDOC
        else:
            out.append(c)
            i += 1
    return "".join(out), ""


def _stdin_texto(r: Redir) -> tuple[str | None, str]:
    """O texto que o consumidor do stdin (heredoc ou herestring) recebeu, ou (None, motivo da recusa)."""
    if r.op == "<<<":
        if r.alvo is None or r.alvo.expande:
            return None, "herestring com expansão do shell"
        return r.alvo.valor + "\n", ""
    return _corpo_heredoc(r)


def _stdin_cru(r: Redir) -> str:
    """O texto do stdin SEM expandir (para saber se uma recusa esconde algo que parece Python)."""
    if r.op == "<<<":
        return r.alvo.valor if r.alvo else ""
    return r.corpo or ""


def _do_stdin(s: Simples) -> Programa:
    """Python sem script nem -c lê o programa do stdin: heredoc ou herestring no fd 0."""
    r = _redir_stdin(s)
    if r is None:
        return Programa(
            None, "nenhuma", "python sem -c, sem script e sem heredoc (REPL ou pipe)"
        )
    if r.op == "<":
        return Programa(None, "nenhuma", "python lê o programa de um ARQUIVO no stdin")
    via = "herestring" if r.op == "<<<" else "heredoc"
    texto, recusa = _stdin_texto(r)
    if texto is None:
        return Programa(None, via, recusa)
    return Programa(
        texto,
        via,
        "python <<< palavra"
        if via == "herestring"
        else "python lendo heredoc no stdin",
    )


def _opcoes_longas(args: list[Palavra]) -> dict[str, Palavra]:
    """`--k v` e `--k=v` → {"--k": Palavra(v)}; flags sem valor (`--brief`) não entram."""
    opc: dict[str, Palavra] = {}
    i = 0
    while i < len(args):
        v = args[i].valor
        tem_valor = i + 1 < len(args) and not args[i + 1].valor.startswith("--")
        if v.startswith("--") and "=" in v:
            k, val = v.split("=", 1)
            opc[k] = Palavra(val, args[i].expande, args[i].citada)
        elif v.startswith("--") and tem_valor:
            opc[v] = args[i + 1]
            i += 1
        i += 1
    return opc


def _aninhado(
    texto: str | None, cru: str, recusa: str, ctx: _Ctx, rotulo: str
) -> list[Programa]:
    """Os veredictos de um SHELL ANINHADO (`bash -c`, `touring run --lang bash`, `bash` lendo o stdin, `$(…)`).

    `texto` é o script que o shell recebeu; None quando o shell externo o expandiu (`cru` é o texto sem expandir):
    só então há recusa, e só se `cru` parece mencionar Python — senão não há o que dizer e a lista é vazia."""
    if texto is None:
        if _MENCIONA_PY.search(cru):
            return [Programa(None, "shell_aninhado", f"{rotulo}: {recusa}")]
        return []
    return [
        replace(a, prof=a.prof + 1, motivo=f"{a.motivo} [em {rotulo}]")
        for a in _varrer(texto, ctx.dentro())
    ]


def _opcoes_shell(argv: list[Palavra]) -> tuple[bool, bool, int]:
    """(tem -c, tem -s, índice do 1º operando) das opções de um shell: `-lc`, `-o pipefail`, `--norc`, `--`."""
    i, tem_c, tem_s = 1, False, False
    while i < len(argv):
        v = argv[i].valor
        if v in ("--", "-"):
            return tem_c, tem_s, i + 1
        if v.startswith("--"):
            i += 2 if v in _SH_LONGAS_COM_VALOR else 1
        elif len(v) > 1 and v[0] in "-+":
            tem_c = tem_c or (v[0] == "-" and "c" in v[1:])
            tem_s = tem_s or (v[0] == "-" and "s" in v[1:])
            i += 2 if v[-1] in "oO" else 1
        else:
            break
    return tem_c, tem_s, i


def _do_shell(s: Simples, argv: list[Palavra], ctx: _Ctx) -> list[Programa] | None:
    """`bash|sh -c X` · `bash -lc X` · `bash`/`sh` sem script com heredoc/herestring: o script também RODOU.
    None se o comando não é um shell; lista vazia se é um shell sem texto a analisar (script no argv, pipe)."""
    if not argv or _base(argv[0].valor) not in _SHELLS:
        return None
    nome = _base(argv[0].valor)
    tem_c, tem_s, i = _opcoes_shell(argv)
    if tem_c:
        if i >= len(argv):
            return []
        w = argv[i]
        return _aninhado(
            None if w.expande else w.valor,
            w.valor,
            "shell com expansão: o aninhado recebeu outro texto",
            ctx,
            f"{nome} -c",
        )
    r = None if (i < len(argv) and not tem_s) else _redir_stdin(s)
    if r is None or r.op == "<":
        return []  # script no argv ou pipe: o stdin é DADO
    texto, recusa = _stdin_texto(r)
    return _aninhado(texto, _stdin_cru(r), recusa, ctx, f"{nome} lendo o stdin")


def _touring_python(opc: dict[str, Palavra]) -> Programa:
    """`touring run --lang python`: `--code` é o programa; `--file` devolve o caminho."""
    if "--code" in opc:
        if opc["--code"].expande:
            return Programa(None, "touring_run_code", "--code com expansão do shell")
        code = opc["--code"].valor
        return Programa(
            code if code.endswith("\n") else code + "\n",
            "touring_run_code",
            "touring run --lang python --code",
        )
    if "--file" in opc:
        return Programa(
            None,
            "touring_run_file",
            "touring run --file: o programa é o arquivo",
            arquivo=opc["--file"].valor,
        )
    return Programa(None, "nenhuma", "touring run sem --code nem --file")


def _touring_shell(opc: dict[str, Palavra], nome: str, ctx: _Ctx) -> list[Programa]:
    """`touring run --lang bash|sh --code X`: X é um script de shell que RODOU; o Python dentro dele também."""
    nao_python = Programa(None, "nenhuma", f"touring run --lang {nome}: não é Python")
    code = opc.get("--code")
    if code is None:
        return [nao_python]
    dentro = _aninhado(
        None if code.expande else code.valor,
        code.valor,
        "--code com expansão do shell",
        ctx,
        f"touring run --lang {nome}",
    )
    return dentro or [nao_python]


def _do_touring_run(argv: list[Palavra], ctx: _Ctx) -> list[Programa] | None:
    """`touring run`: `--lang python` (`--code` é programa, `--file` o caminho) · `--lang bash|sh` (aninhado)."""
    if len(argv) < 2 or _base(argv[0].valor) != "touring" or argv[1].valor != "run":
        return None
    opc = _opcoes_longas(argv[2:])
    lang = opc.get("--lang")
    nome = lang.valor if lang else "(ausente)"
    if nome == "python":
        return [_touring_python(opc)]
    if nome in ("bash", "sh"):
        return _touring_shell(opc, nome, ctx)
    return [Programa(None, "nenhuma", f"touring run --lang {nome}: não é Python")]


def _subs_do_comando(s: Simples, ctx: _Ctx) -> list[Programa]:
    """Os veredictos dos `$(…)`/crase do comando simples (só com `ctx.subst`): rodam ANTES do próprio comando."""
    if not ctx.subst:
        return []
    palavras = s.palavras + [r.alvo for r in s.redirs if r.alvo is not None]
    corpos = [b for w in palavras for b in w.subs]
    return [a for b in corpos for a in _aninhado(b, b, "", ctx, "$(…)")]


def _do_comando(s: Simples, ctx: _Ctx) -> list[Programa]:
    """Os veredictos de UM comando simples: substituições, `touring run`, interpretador Python ou shell aninhado."""
    argv = _efetivas(s.palavras)
    proprios = _do_touring_run(argv, ctx)
    if proprios is None:
        py = _do_python(s, argv)
        proprios = (
            [py]
            if py is not None
            else (_do_shell(s, argv, ctx) or _comando_expandido(argv))
        )
    return _subs_do_comando(s, ctx) + proprios


def _comando_expandido(argv: list[Palavra]) -> list[Programa]:
    """O comando começa por uma expansão (`$E python3 -c …`): o que roda é desconhecido, e o resto parece Python."""
    texto = " ".join(w.valor for w in argv)
    if argv and argv[0].expande and _MENCIONA_PY.search(texto):
        return [
            Programa(
                None,
                "comando_expandido",
                f"comando em posição de expansão ({argv[0].valor}): o programa real é desconhecido",
            )
        ]
    return []


def _varrer(comando: str, ctx: _Ctx) -> list[Programa]:
    """Todos os veredictos do texto no nível `ctx.prof`, em ordem textual; os dos shells aninhados entram no lugar."""
    if ctx.prof > _PROF_MAX:
        if _MENCIONA_PY.search(comando):
            return [
                Programa(
                    None,
                    "shell_aninhado",
                    f"shell aninhado além da profundidade {_PROF_MAX}",
                )
            ]
        return []
    try:
        simples = comandos_simples(comando)
    except ShellIlegivel as e:
        if ctx.prof == 0 or _MENCIONA_PY.search(comando):
            return [Programa(None, "nenhuma", f"shell ilegível: {e}")]
        return []
    return [a for s in simples for a in _do_comando(s, ctx)]


def _valido(a: Programa) -> bool:
    """Um veredito é programa quando tem texto, ou é `touring run --file` (o programa é o arquivo)."""
    return a.texto is not None or a.via == "touring_run_file"


def achados(comando: str, substituicoes: bool = True) -> list[Programa]:
    """TODOS os veredictos do comando, em ordem textual — programas E recusas (os de shells aninhados incluídos).

    Semântica de `achados`/`candidatos`: «todo programa Python que RODOU» — por isso `substituicoes=True` por
    padrão: o Python dentro de `$(…)` e crase roda. `analisar`/`programa_python` ficam em False: o caso nº 30
    do selftest (`x=$(python3 -c …)` → None) é contrato do extrator antigo e os corpora indexam UM programa."""
    return _varrer(comando, _Ctx(subst=substituicoes))


def candidatos(comando: str, substituicoes: bool = True) -> list[Programa]:
    """Todos os programas Python válidos do comando, em ordem textual, os aninhados incluídos (`prof` > 0).

    `touring run --file` conta (tem `arquivo`, não `texto`). Quem precisa de UM programa escolhe pela evidência
    que tem — p. ex. o traceback dentro do envelope JSON do `touring run` é do `--code`, não do `python3 -c` do
    pipe. Programas ilegíveis (o shell os expandiu) NÃO entram aqui: estão em `achados` como recusa."""
    return [a for a in achados(comando, substituicoes) if _valido(a)]


def _recusa_principal(todos: list[Programa]) -> Programa:
    """A recusa que melhor explica a ausência de programa: a de um Python que EXISTIU mas não é legível
    (`-c`/heredoc/herestring/`--code`/shell aninhado expandido) vence a de um comando que nem era programa
    (script no argv, `-m`) — senão o motivo cita o `python3 script.py` do pipeline e esconde o `-c` com `$VAR`."""
    return next((a for a in todos if a.via != "nenhuma"), todos[0])


def analisar(comando: str, substituicoes: bool = False) -> Programa:
    """O veredito sobre o comando inteiro: exatamente UM programa Python inline, ou None com o motivo."""
    todos = achados(comando, substituicoes)
    validos = [a for a in todos if _valido(a)]
    if len(validos) > 1:
        return Programa(
            None,
            "nenhuma",
            f"ambíguo: {len(validos)} programas Python no mesmo comando",
        )
    if validos:
        return validos[0]
    if todos:
        return _recusa_principal(todos)
    return Programa(
        None,
        "nenhuma",
        "nenhum interpretador Python nem touring run --lang python no comando",
    )


def programa_python(comando: str, substituicoes: bool = False) -> str | None:
    """Atalho: o texto do programa, ou None."""
    return analisar(comando, substituicoes).texto


# --------------------------------------------------------------------------- selftest


def _envolve(cmd: str, n: int) -> str:
    """`bash -c '<cmd>'` aninhado `n` vezes (`shlex.quote` cuida das aspas internas)."""
    for _ in range(n):
        cmd = "bash -c " + shlex.quote(cmd)
    return cmd


CASOS: list[tuple[str, str | None, str]] = [
    # (comando, programa esperado ou None, fragmento do motivo esperado)
    ("python3 -c 'print(1)'", "print(1)\n", "python -c"),
    ("python3 -c 'print('\\''a'\\'')'", "print('a')\n", "python -c"),
    ('python3 -c "print(\\"a\\")"', 'print("a")\n', "python -c"),
    ('python3 -c "print(\\$HOME)"', "print($HOME)\n", "python -c"),
    ('python3 -c "import os; print($HOME)"', None, "expansão"),
    ('python3 -c "x = 1" && echo "fim"', "x = 1\n", "python -c"),
    (
        "cd /x && .venv/bin/python -c 'print(2)' 2>&1 | tail -3",
        "print(2)\n",
        "python -c",
    ),
    ("timeout 60 python3 -I -c 'print(3)'", "print(3)\n", "python -c"),
    ("PYTHONPATH=. uv run --project p python -c 'print(4)'", "print(4)\n", "python -c"),
    ("python3 -Ic 'print(5)'", "print(5)\n", "python -c"),
    ("python3 - <<'EOF'\nprint($x)\nEOF", "print($x)\n", "heredoc"),
    ("python3 <<EOF\nprint(1)\nEOF", "print(1)\n", "heredoc"),
    ("python3 - <<EOF\nprint('$HOME')\nEOF", None, "expandiu"),
    ("python3 - <<-'EOF'\n\tprint(1)\n\tEOF", "print(1)\n", "heredoc"),
    ("cat > /tmp/x.py <<'EOF'\nprint(1)\nEOF", None, "nenhum interpretador"),
    ("tee a.py <<'EOF'\nprint(1)\nEOF", None, "nenhum interpretador"),
    ("git commit -F - <<'EOF'\nmsg\nEOF", None, "nenhum interpretador"),
    ("python3 script.py <<'EOF'\ndado\nEOF", None, "script no argv"),
    (
        "mkdir -p d && cat >> t.py <<'PYEOF'\nx = 1\nPYEOF\npython3 -m pytest t.py",
        None,
        "módulo",
    ),
    ("touring run --lang python --code 'print(1)'", "print(1)\n", "touring run"),
    ("touring run --lang bash --code 'head -30 /x'", None, "não é Python"),
    (
        "touring run --lang python --code \"print('\\$a')\"",
        "print('$a')\n",
        "touring run",
    ),
    ("touring run --lang python --file /tmp/a.py --brief", None, "--file"),
    ("python3 -c 'print(1)'; python3 -c 'print(2)'", None, "ambíguo"),
    ("python3 -c 'print(1)", None, "ilegível"),
    ("python3 <<< 'print(7)'", "print(7)\n", "<<<"),
    ("echo python3 -c 'print(1)'", None, "nenhum interpretador"),
    (
        "python3 -c 'print(1)' # comentário com python3 -c 'x'",
        "print(1)\n",
        "python -c",
    ),
    ("for f in a b; do python3 -c 'print(8)'; done", "print(8)\n", "python -c"),
    ("x=$(python3 -c 'print(9)')", None, "nenhum interpretador"),
    # --- lacuna 1: embrulhos com opção QUE TOMA VALOR (o valor não é o comando)
    (
        "env -u TOURING_DAEMON_SOCKET python3 - <<'EOF'\nprint(11)\nEOF",
        "print(11)\n",
        "heredoc",
    ),
    (
        "env -i -u A --unset=B -C /tmp --chdir /x FOO=1 python3 -c 'print(12)'",
        "print(12)\n",
        "python -c",
    ),
    ("env -uA python3 -c 'print(13)'", "print(13)\n", "python -c"),
    ("timeout -s KILL -k 5 60 python3 -c 'print(14)'", "print(14)\n", "python -c"),
    (
        "timeout --signal=INT --kill-after 3 9 python3 -c 'print(15)'",
        "print(15)\n",
        "python -c",
    ),
    ("nice -n 10 python3 -c 'print(16)'", "print(16)\n", "python -c"),
    (
        "nice --adjustment=5 stdbuf -oL -e0 python3 -c 'print(17)'",
        "print(17)\n",
        "python -c",
    ),
    ("stdbuf -i0 -o L python3 -c 'print(18)'", "print(18)\n", "python -c"),
    (
        "env -u python3 echo hi",
        None,
        "nenhum interpretador",
    ),  # o valor de -u NÃO é o comando
    # `--` encerra as opções do embrulho: sem consumi-lo, o `--` viraria o comando (e o operando do timeout, idem)
    ("env -- python3 -c 'print(81)'", "print(81)\n", "python -c"),
    ("timeout -s KILL -- 5 python3 -c 'print(82)'", "print(82)\n", "python -c"),
    ("nice -n 5 -- python3 -c 'print(83)'", "print(83)\n", "python -c"),
    ("stdbuf -oL -- python3 -c 'print(84)'", "print(84)\n", "python -c"),
    ("env -u A -- FOO=1 python3 - <<'EOF'\nprint(85)\nEOF", "print(85)\n", "heredoc"),
    # --- lacuna 2: shells aninhados (o programa dentro também RODOU)
    ("bash -c \"python3 -c 'print(21)'\"", "print(21)\n", "em bash -c"),
    (
        "bash -lc \"cd /x && python3 - <<'EOF'\nprint(22)\nEOF\"",
        "print(22)\n",
        "em bash -c",
    ),
    (
        "touring run --lang bash --code 'python3 -c \"print(23)\"'",
        "print(23)\n",
        "em touring run --lang bash",
    ),
    (
        "bash <<'EOF'\npython3 -c 'print(24)'\nEOF",
        "print(24)\n",
        "em bash lendo o stdin",
    ),
    ("sh <<< \"python3 -c 'print(25)'\"", "print(25)\n", "em sh lendo o stdin"),
    (
        _envolve("python3 -c 'print(26)'", 3),
        "print(26)\n",
        "em bash -c",
    ),  # profundidade 3: ainda lida
    (
        _envolve("python3 -c 'print(27)'", 4),
        None,
        "profundidade",
    ),  # profundidade 4: recusa nomeada
    ("bash -c \"echo $HOME; python3 -c 'print(28)'\"", None, "shell com expansão"),
    (
        'bash -c "echo $HOME"',
        None,
        "nenhum interpretador",
    ),  # expansão sem Python: nada a dizer
    (
        "bash run.sh <<'EOF'\npython3 -c 'x'\nEOF",
        None,
        "nenhum interpretador",
    ),  # heredoc é DADO do script
    ("bash -c 'echo hi' && python3 -c 'print(29)'", "print(29)\n", "python -c"),
    ("touring run --lang sh --code 'echo hi'", None, "não é Python"),
    (
        "bash -o pipefail -c 'python3 -c \"print(30)\"'",
        "print(30)\n",
        "em bash -c",
    ),
    # --- lacuna 3: heredoc SEM aspas — as barras do bash, e recusa só com expansão de verdade
    ('python3 - <<EOF\nprint("a\\nb")\nEOF', 'print("a\\nb")\n', "heredoc"),
    (
        'python3 - <<EOF\nprint("\\$HOME \\\\ \\`x\\`")\nEOF',
        'print("$HOME \\ `x`")\n',
        "heredoc",
    ),
    ("python3 - <<EOF\nx = 1 + \\\n2\nEOF", "x = 1 + 2\n", "heredoc"),
    ('python3 - <<EOF\nprint("$5.00")\nEOF', None, "expandiu"),
    (
        "python3 - <<EOF\nprint('$')\nprint(f\"{x}\")\nx = '$ y $.'\nEOF",
        "print('$')\nprint(f\"{x}\")\nx = '$ y $.'\n",
        "heredoc",
    ),
    ('python3 - <<\\EOF\nprint("$HOME")\nEOF', 'print("$HOME")\n', "heredoc"),
    ("python3 - <<EOF\nprint($(date))\nEOF", None, "expandiu"),
    ("python3 - <<EOF\nx = 1 \\\nEOF", None, "engoliu"),
    ("python3 - <<EOF\nprint($[1+2])\nEOF", None, "expandiu"),
    (
        'python3 - <<EOF\nx = "$\\\n5"\nEOF',
        None,
        "expandiu",
    ),  # `$\<quebra>5`: o bash junta ANTES de expandir
    (
        "python3 - <<EOF\nx = '\\\\'\ny = 1\nEOF",
        "x = '\\'\ny = 1\n",
        "heredoc",
    ),  # `\\` + quebra não é continuação
    # heredoc DENTRO de `$(…)` (idioma do `git commit -m "$(cat <<'EOF' … EOF)"`): aspas e parênteses do corpo não contam
    (
        "git commit -q -m \"$(cat <<'EOF'\nmsg com \" aspa solta e ) parêntese (\nEOF\n)\"; python3 -c 'print(71)'",
        "print(71)\n",
        "python -c",
    ),
    # comando em posição de expansão: o programa real é desconhecido, e o motivo diz isso (não 'nenhum interpretador')
    ("E='env A=1'; $E python3 -c 'print(72)'", None, "posição de expansão"),
    ("E='env A=1'; $E echo hi", None, "nenhum interpretador"),
    ("$(which python3) -c 'print(73)'", None, "posição de expansão"),
    # comentário com operador: sem ignorar o `#`, o `;` abriria um 2º comando (ambíguo)
    ("python3 -c 'print(1)' # ; python3 -c 'print(2)'", "print(1)\n", "python -c"),
    # --- lacuna 4: dois programas → `analisar` continua ambíguo; `candidatos` lista os dois
    (
        "touring run --lang python --code 'print(41)' | python3 -c \"import json,sys; print(42)\"",
        None,
        "ambíguo",
    ),
    # --- resíduos da amostra (item 5): embrulhos que faltavam e o MOTIVO da recusa
    ('setsid nohup .venv/bin/python3 -c "print(51)"', "print(51)\n", "python -c"),
    ("unshare -rn .venv/bin/python3 -c 'print(52)'", "print(52)\n", "python -c"),
    ("unshare --map-user 1000 -f python3 -c 'print(53)'", "print(53)\n", "python -c"),
    (
        "systemd-run --user --scope -q -p MemoryMax=4G -p MemorySwapMax=0 env A=1 python3 -c 'print(54)'",
        "print(54)\n",
        "python -c",
    ),
    (
        "systemd-run --property MemoryMax=1G python3 -c 'print(55)'",
        "print(55)\n",
        "python -c",
    ),
    (
        "python3 s.py; python3 -c \"print('$X')\"",  # o -c com $X é o motivo, não o script anterior
        None,
        "-c com expansão",
    ),
    ('python3 s.py && python3 - <<EOF\nprint("$S")\nEOF', None, "expandiu"),
    (
        "python3 s.py; python3 -m pytest",
        None,
        "script no argv",
    ),  # nenhum Python ilegível: vale a 1ª
]

# `analisar(cmd, substituicoes=True)`: o Python dentro de `$(…)`/crase também rodou
CASOS_SUBST: list[tuple[str, str | None, str]] = [
    ("x=$(python3 -c 'print(61)')", "print(61)\n", "em $(…)"),
    ("y=`python3 -c 'print(62)'`", "print(62)\n", "em $(…)"),
    (
        'echo "$(python3 -c "print(\')\')")"',
        "print(')')\n",
        "em $(…)",
    ),  # `)` entre aspas não fecha
    (
        'D=$(.venv/bin/python3 -c "import os; print(os.getcwd())") && echo "$D"',
        "import os; print(os.getcwd())\n",
        "em $(…)",
    ),
    ("bash -c 'x=$(python3 -c \"print(63)\")'", "print(63)\n", "em $(…)"),
    (
        "python3 -c \"$(python3 -c 'print(64)')\"",
        "print(64)\n",
        "em $(…)",
    ),  # o externo é ilegível
    (
        "echo $((1+2)) && python3 -c 'print(65)'",
        "print(65)\n",
        "python -c",
    ),  # `$((…))` é aritmética
    ("a=$(python3 -c 'print(1)') b=$(python3 -c 'print(2)')", None, "ambíguo"),
    ("echo $(echo hi)", None, "nenhum interpretador"),
    # `)` entre aspas DUPLAS (escapadas) dentro de `$(…)`: só o balanceamento ciente de aspas fecha no lugar certo
    ('x=$(python3 -c "print(\\")\\")")', 'print(")")\n', "em $(…)"),
    (
        'st=$(touring index status -j | python3 -c "import json,sys; print(66)" 2>/dev/null)',
        "import json,sys; print(66)\n",
        "em $(…)",
    ),
]

# (comando, [(texto, via, prof), …]) — TODOS os programas válidos, em ordem textual
CASOS_CANDIDATOS: list[tuple[str, list[tuple[str | None, str, int]]]] = [
    (
        "touring run --lang python --code 'print(41)' | python3 -c \"import json,sys; print(42)\"",
        [
            ("print(41)\n", "touring_run_code", 0),
            ("import json,sys; print(42)\n", "-c", 0),
        ],
    ),
    (
        "bash -c \"python3 -c 'print(43)'\" && python3 -c 'print(44)'",
        [("print(43)\n", "-c", 1), ("print(44)\n", "-c", 0)],
    ),
    (_envolve("python3 -c 'print(45)'", 3), [("print(45)\n", "-c", 3)]),
    ("echo hi", []),
    ("python3 -c 'print(46)", []),  # shell ilegível: nenhum candidato
    (
        "touring run --lang python --file /tmp/a.py | python3 -c 'print(47)'",
        [(None, "touring_run_file", 0), ("print(47)\n", "-c", 0)],
    ),
    (
        "bash -c \"echo $HOME; python3 -c 'print(48)'\" | python3 -c 'print(49)'",
        [
            ("print(49)\n", "-c", 0)
        ],  # o que o shell expandiu NÃO é candidato (está em `achados`)
    ),
]

# (comando, [fragmento do motivo de cada veredito, em ordem]) — programas E recusas
CASOS_ACHADOS: list[tuple[str, list[str]]] = [
    (
        "bash -c \"echo $HOME; python3 -c 'print(48)'\" | python3 -c 'print(49)'",
        ["shell com expansão", "python -c"],
    ),
    (
        "python3 script.py; python3 -c 'print(50)'; touring run --lang bash --code 'echo'",
        ["script no argv", "python -c", "não é Python"],
    ),
]


CASOS_CANDIDATOS_SUBST: list[tuple[str, list[tuple[str | None, str, int]]]] = [
    (
        "x=$(python3 -c 'print(67)') && python3 -c 'print(68)'",
        [("print(67)\n", "-c", 1), ("print(68)\n", "-c", 0)],
    ),
]


def _falhas_casos(
    casos: list[tuple[str, str | None, str]], substituicoes: bool
) -> list[str]:
    """Os casos de `analisar` que não batem: o PROGRAMA e um fragmento do MOTIVO."""
    out = []
    for cmd, esperado, motivo in casos:
        v = analisar(cmd, substituicoes)
        if v.texto != esperado or motivo not in v.motivo:
            out.append(
                f"ERRADO {cmd!r}\n  esperado {esperado!r} ({motivo!r})\n  obtido   {v.texto!r} ({v.motivo!r})"
            )
    return out


def _falhas_candidatos(
    casos: list[tuple[str, list[tuple[str | None, str, int]]]], substituicoes: bool
) -> list[str]:
    out = []
    for cmd, esperados in casos:
        obtidos = [(c.texto, c.via, c.prof) for c in candidatos(cmd, substituicoes)]
        if obtidos != esperados:
            out.append(
                f"ERRADO candidatos {cmd!r}\n  esperado {esperados!r}\n  obtido   {obtidos!r}"
            )
    return out


def _falhas_achados() -> list[str]:
    out = []
    for cmd, fragmentos in CASOS_ACHADOS:
        motivos = [a.motivo for a in achados(cmd)]
        casam = len(motivos) == len(fragmentos) and all(
            f in m for f, m in zip(fragmentos, motivos, strict=True)
        )
        if not casam:
            out.append(
                f"ERRADO achados {cmd!r}\n  esperado {fragmentos!r}\n  obtido   {motivos!r}"
            )
    return out


_PADRAO_CMD = "x=$(python3 -c 'print(91)')"
N_PADROES = 4


def _falhas_padroes() -> list[str]:
    """Os PADRÕES de `substituicoes` (chamadas SEM a flag): `analisar`/`programa_python` ignoram `$(…)`;
    `candidatos`/`achados` o analisam — «todo programa que RODOU» (decisão da coordenadora, 09/10/2026)."""
    candidato = [(c.texto, c.prof) for c in candidatos(_PADRAO_CMD)]
    motivos = [a.motivo for a in achados(_PADRAO_CMD)]
    checagens = [
        ("analisar sem flag ignora $(…)", analisar(_PADRAO_CMD).texto is None),
        ("programa_python sem flag ignora $(…)", programa_python(_PADRAO_CMD) is None),
        ("candidatos sem flag analisa $(…)", candidato == [("print(91)\n", 1)]),
        (
            "achados sem flag analisa $(…)",
            len(motivos) == 1 and "em $(…)" in motivos[0],
        ),
    ]
    return [f"ERRADO padrão: {nome}" for nome, ok in checagens if not ok]


def selftest() -> int:
    """Cada caso fixa o PROGRAMA e o MOTIVO; mutação: trocar um predicado derruba os casos dele."""
    total = (
        len(CASOS)
        + len(CASOS_SUBST)
        + len(CASOS_CANDIDATOS)
        + len(CASOS_CANDIDATOS_SUBST)
        + len(CASOS_ACHADOS)
        + N_PADROES
    )
    ruins = (
        _falhas_casos(CASOS, False)
        + _falhas_casos(CASOS_SUBST, True)
        + _falhas_candidatos(CASOS_CANDIDATOS, False)
        + _falhas_candidatos(CASOS_CANDIDATOS_SUBST, True)
        + _falhas_achados()
        + _falhas_padroes()
    )
    for r in ruins:
        print(r)
    print(f"selftest: {total - len(ruins)}/{total}")
    return 0 if not ruins else 1


if __name__ == "__main__":
    if sys.argv[1:] == ["--selftest"]:
        raise SystemExit(selftest())
    if len(sys.argv) == 2:
        v = analisar(sys.argv[1])
        print(json.dumps(v.__dict__, ensure_ascii=False))
        raise SystemExit(0)
    print(__doc__)
    raise SystemExit(2)
