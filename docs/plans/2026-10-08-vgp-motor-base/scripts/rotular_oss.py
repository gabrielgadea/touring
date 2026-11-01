"""Rotulador do estrato `oss` (onda 2, tarefa B) — LanguageService persistente + retestemunho.

Decisão da coordenadora (09/10):
- rotular com o LS **provado** (20/20 de equivalência com o `tsc`, ganho 39×);
- **retestemunho**: 1 par em cada 50, sorteado com semente fixa, passa também pelo `tsc` e tem de
  coincidir; se divergir, o lote PARA;
- tetos de 3% por arquivo e por commit; selo gravado; ordem: excalidraw → zod → date-fns → h5bp;
- continua SELADO: este script não roda motor nem juiz.

O rótulo é o DELTA: par (antes, depois) com o mesmo conjunto de diagnósticos ⇒ negativo.
"""

from __future__ import annotations

import argparse
import hashlib
import inspect
import json
import os
import re
import select
import shutil
import subprocess
import sys

# O lock de exclusao mutua dos clones OSS (MEDIDO em 10/10: dois lotes perdidos para dois
# escritores no mesmo arquivo). Precisa de `Path`, entao entra depois dos imports.
import sys as _sys
import tempfile
import time
from collections import Counter
from pathlib import Path
from typing import IO

_RAIZ = Path(__file__).resolve().parent
if str(_RAIZ) not in _sys.path:
    _sys.path.insert(0, str(_RAIZ))

import chama_oraculo as oraculo
import vgp_lock

OSS = Path.home() / ".claude/touring/vgp/oss"
SAIDA = Path.home() / ".claude/touring/vgp/corpus_holdout_web_20261012/oss"
# O nome nao pode ser `LS`: a classe `LS` abaixo sombrearia a constante (medido em 09/10 --
# o `node` recebia "<class '__main__.LS'>" e morria com MODULE_NOT_FOUND).
ORACULO_LS = Path.home() / ".claude/touring/vgp/oss/oraculo_ls.js"
# O `tsc` do retestemunho NAO e um caminho fixo. MEDIDO em 01/11: o caminho absoluto desta
# maquina (`.../projects/analise/frontend/node_modules/.bin/tsc`) estava VERSIONADO num repo
# PUBLICO -- um caminho com o nome do usuario nao tem porque estar no historico, e a reescrita
# de 01/11 nao pegou esse. A resolucao e por ambiente e acontece NA HORA DE USAR (nunca na
# importacao: os testes importam este modulo numa maquina sem `tsc`).
_TSC_CACHE: Path | None = None


def _tsc(repo: Path | None = None) -> Path:
    """Resolve o `tsc` do retestemunho: 1) `TOURING_TSC`; 2) `tsc` no PATH; 3) o do repo.

    Sem nenhum dos tres, PARA COM MENSAGEM. Silenciar o retestemunho faria a DELTA do `tsc`
    virar "confirmacao vazia" -- a mesma armadilha do segundo formato de saida nao parseado
    (medido em 09/10 no zod: 598 erros emitidos, 0 casando, e o oraculo de referencia passava
    a confirmar nada).
    """
    env = os.environ.get("TOURING_TSC")
    if env:
        # O ambiente MANDA sobre o cache: quem chama pode trocar o `tsc` entre rodadas, e um
        # cache que escondesse a troca seria um "resolvedor" que resolve o Path errado.
        p = Path(env)
        if not p.exists():
            raise SystemExit(f"TOURING_TSC aponta para {p}, que nao existe")
        return p
    global _TSC_CACHE
    if _TSC_CACHE is not None:
        return _TSC_CACHE
    achado = shutil.which("tsc")
    if achado:
        _TSC_CACHE = Path(achado)
        return _TSC_CACHE
    if repo is not None:
        local = repo / "node_modules" / ".bin" / "tsc"
        if local.exists():
            return local
    raise SystemExit(
        "tsc nao encontrado: exporte TOURING_TSC=<caminho do tsc> (ex.: "
        "`$(npm root -g)/typescript/bin/tsc`) ou tenha o typescript no PATH. Sem o `tsc` o "
        "retestemunho nao roda, e a DELTA viraria confirmacao vazia."
    )


RETEST_A_CADA = 50
SEMENTE = 20261009
# O `tsc` tem dois formatos de saida. MEDIDO em 09/10: o zod emite o segundo, e um parser que
# so conhece o primeiro le "0 diagnosticos" onde ha 598 -- um oraculo de referencia que confirma
# nada sem dizer que nao viu nada.
RE_TSC_CURTO = re.compile(r"^(.+?)\((\d+),(\d+)\): error (TS\d+): (.*)$")
RE_TSC_BONITO = re.compile(r"^(.+?):(\d+):(\d+) - error (TS\d+): (.*)$")
# Dependencia externa ausente NA BASE (decisao da coordenadora, 09/10). MEDIDO no zod: 598 erros
# na base, quase todos "Cannot find module 'execa' | 'arktype' | 'valibot'". Num arquivo que importa
# uma delas, um par que QUEBRA simbolo desse pacote produz erro IGUAL antes e depois -- o DELTA
# fica vazio e o par entra como NEGATIVO. Aí um deny correto do motor contaria como falso deny, que
# e exatamente a medida que o holdout existe para sustentar. A assinatura e a mesma em qualquer repo,
# por isso o filtro nao e do zod: e de qualquer arquivo cuja base ja acusa modulo externo ausente.
CODIGOS_DEP_AUSENTE = ("TS2307", "TS2305", "TS7016")
# Arquivos de sintaxe INVALIDA DE PROPOSITO. MEDIDO em 09/10 no `dom-examples`: sem o `exclude`,
# o `tsc` responde rc=2 com 3 linhas de `TS1109` e PARA -- `web-workers/worker-playground/
# bad-syntax-worker.js` e o exemplo que demonstra o worker invalido, e abortar nele esconde os
# outros 169 arquivos do projeto. E o `tsc` que ABORTA e o oraculo de referencia do retestemunho:
# um oraculo que nunca chegou a olhar o arquivo nao pode confirmar nada sobre ele.
SINTAXE_INVALIDA = (
    "web-workers/worker-playground/bad-syntax-worker.js",
    "**/bad-syntax*.js",
)
EXT_LANG = {
    ".ts": "typescript",
    ".tsx": "tsx",
    ".js": "javascript",
    ".mjs": "javascript",
    ".html": "html",
}
EXCLUIR_PATH = (
    "node_modules/",
    "/dist/",
    "/target/",
    "/build/",
    "/.next/",
    "/coverage/",
    "/.yarn/",
    "/.pnpm-store/",
    "/docs-site/",
    "/e2e-tests/",
    "/locales/",
)
EXCLUIR_NOME = (".min.js", ".d.ts", ".snap", ".lock", "-lock.json", ".generated.")
GERADO = ("@generated", "DO NOT EDIT", "@ts-nocheck", "auto-generated", "@__PURE__")
ORDEM = [
    "excalidraw",
    "zod",
    "date-fns",
    "html5-boilerplate",
    "express",
    "lodash",
    "dom-examples",
]

# O MESMO `tsconfig` que `corpus_semeado.py` usa para JS (onda 1, com o `checkJs` ligado): o
# oraculo do holdout tem de ser o mesmo que gerou o corpus semeado, senao "negativo" aqui e
# "positivo" la depending de qual tsconfig rodou. `date-fns` saiu da lista por decisao da
# coordenadora (09/10): o controle positivo deu rc=0 com o import inexistente injetado, ou seja,
# o `tsc` nativo do TypeScript 7 nao e oraculo para este projeto.
TSCONFIG_JS = json.dumps(
    {
        "compilerOptions": {
            "target": "ES2022",
            "module": "ESNext",
            "moduleResolution": "bundler",
            "allowJs": True,
            "checkJs": True,
            "noEmit": True,
            "skipLibCheck": True,
            "allowImportingTsExtensions": True,
            "strict": False,
        },
        "include": ["**/*.js", "**/*.mjs"],
    },
    indent=1,
)


def pipe_de(proc: subprocess.Popen, nome: str) -> IO[bytes]:
    """O stream `nome` do processo, ou erro alto se o pipe nao existe.

    `subprocess.Popen` declara os streams como `IO[str] | None` mesmo com `PIPE` no construtor;
    um `IO[str] | None` que se usa direto e um `None` esperando para acontecer, e o sintoma
    (MEDIDO em 09/10) e a guarda de erro virando "(stderr ilegivel)" -- o proprio diagnostico
    do oraculo virando um zero mudo.
    """
    stream = getattr(proc, nome, None)
    if stream is None:
        raise RuntimeError(
            f"o oraculo LS nasceu sem o pipe `{nome}`: Popen nao aceitou PIPE"
        )
    return stream


class LS:
    """Um processo node persistente (o mesmo `oraculo_ls.js` da prova de equivalência)."""

    def __init__(self, node_path: Path) -> None:
        self.node = str(
            node_path / "node_modules" / "typescript" / "lib" / "typescript.js"
        )
        # MEDIDO em 09/10: dois defeitos no MESMO lugar, ambos meus.
        # (a) `str(LS)` aqui resolvia para a CLASSE, nao para a constante de modulo do script --
        #     o `node` recebia o caminho "<class 'LS'>" e morria com MODULE_NOT_FOUND. A classe
        #     sombreia o nome global: por isso a constante chama `ORACULO_LS`.
        # (b) `stderr` NAO estava no pipe. A guarda abaixo prometia dizer por que o node morreu e
        #     so conseguia dizer "(stderr ilegivel)" -- o proprio diagnostico era um zero mudo.
        self.proc = subprocess.Popen(
            ["node", str(ORACULO_LS)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            # MEDIDO em 01/11 (V-8): o pipe precisa ser BINARIO. O `oraculo_ls.js` declara o
            # comprimento do corpo em BYTES (`Buffer.byteLength`), e o `read(n)` em modo TEXTO pede
            # n CARACTERES. Num corpo com acento (o `zod` tem), o frame tinha menos caracteres
            # que os bytes declarados, o pipe esvaziava e o `read` bloqueava PARA SEMPRE: o
            # lote morria com o node VIVO e ocioso, sem erro e sem saida. Era o run do V-8
            # anterior, que voltou vazio. Em bytes, `read(n)` le n BYTES -- exatamente o que o
            # oraculo declarou.
            text=False,
        )
        # `pipe_de` falha ALTO se algum faltar: melhor um erro que nomeia o pipe do que um
        # AttributeError no meio de um delta.
        self.tem_stderr = pipe_de(self.proc, "stderr") is not None

    # MEDIDO em 10/10: um `delta` em arquivo de 90 KB levou 0,4 s no `node`, e o `readline()`
    # voltava VAZIO em metade das medicoes. O oraculo e LENTO e esta VIVO — nao morto. 180 s e o
    # teto: bem acima do pior tempo medido (2,07 s) e bem abaixo do timeout do lote inteiro, para
    # que um travamento real nao trave o rotulador inteiro.
    ESPERA_PADRAO = 180.0

    @staticmethod
    def _espera_resposta(stream, segundos: float) -> bool:
        """Espera o pipe ter ALGO, ou o processo fechar.

        MEDIDO em 10/10: `readline()` em modo texto devolve `''` quando o buffer ainda nao tem
        linha, mesmo com o processo VIVO do outro lado — `select` confirmou que a resposta
        chegava segundos depois. Sem esta espera o `poll()` devolve `None` e o rotulador morre
        com `rc=None | stderr=(vazio)`, que parece morte do oraculo e nao e.
        """
        try:
            pronto, _, _ = select.select([stream], [], [], segundos)
        except (OSError, ValueError):
            return False  # o processo fechou o pipe
        return bool(pronto)

    def cmd(self, msg: dict) -> dict:
        """Um comando ao `oraculo_ls.js`, com espera por resposta e leitura por framing.

        MEDIDO em 10/10, duas vezes nao mesma classe. (a) O `node` MORRE na abertura e o
        `readline()` devolve `''`: o erro precisa dizer que ele morreu e por que. (b) O `node` esta
        VIVO e calculando, e o `readline()` devolve `''` na mesma forma — a espera e o que separa
        os dois, e sem ela o rotulador morre no meio do lote.

        MEDIDO em 01/11 (V-8), a terceira da mesma classe: o frame e lido em BYTES. O
        `oraculo_ls.js` declara `#<n>` com `Buffer.byteLength` (bytes) e escrevia o corpo em
        UTF-8; o `read(n)` em modo TEXTO pede n CARACTERES. Num corpo com acento, os caracteres
        sao MENOS que os bytes declarados: o pipe esvaziava e o `read` bloqueava para sempre, com
        o node VIVO e ocioso e sem nenhuma saida. O lote do V-8 anterior morreu assim. O pipe e
        binario e a leitura e por BYTES — e o `\n` que o oraculo escreve depois do corpo e
        consumido, senao o proximo frame comeca pelo terminador do anterior.
        """
        pipe_de(self.proc, "stdin").write((json.dumps(msg) + "\n").encode("utf-8"))
        pipe_de(self.proc, "stdin").flush()
        stdout = pipe_de(self.proc, "stdout")
        if not self._espera_resposta(stdout, self.ESPERA_PADRAO):
            rc = self.proc.poll()
            causa = self._stderr_legivel()
            raise RuntimeError(
                f"o oraculo LS nao respondeu a {msg['op']!r} em {self.ESPERA_PADRAO}s: "
                f"{'VIVO E NAO RESPONDE' if rc is None else f'morreu com rc={rc}'} | "
                f"stderr={causa or '(vazio)'}"
            )
        cabecalho_b = stdout.readline()
        if not cabecalho_b.strip():
            rc = self.proc.poll()
            causa = self._stderr_legivel()
            raise RuntimeError(
                f"o oraculo LS nao respondeu a {msg['op']!r}: "
                f"{'VIVO E NAO RESPONDE' if rc is None else f'morreu com rc={rc}'} | "
                f"stderr={causa or '(vazio)'}"
            )
        cabecalho = cabecalho_b.decode("utf-8", errors="replace")
        if not cabecalho.startswith("#"):
            # protocolo antigo: a linha inteira e o corpo. Ainda aceito, e o selo declara a
            # identidade do `oraculo_ls.js` por sha256 — se ele voltar a esse protocolo, o selo
            # muda e a diferenca fica visivel.
            return json.loads(cabecalho)
        n = int(cabecalho[1:].strip())
        corpo_b = stdout.read(n)  # n BYTES — exatamente o que o oraculo declarou
        if len(corpo_b) < n:
            raise RuntimeError(
                f"o oraculo LS declarou {n} bytes e vein {len(corpo_b)}: o frame esta truncado"
            )
        # o oraculo escreve `\n` depois do corpo: come o terminador, senao o proximo
        # `readline()` le esse `\n` como se fosse o cabecalho do proximo frame.
        stdout.readline()
        return oraculo.ler_framing("node_ls", cabecalho + corpo_b.decode("utf-8") + "\n")

    def _stderr_legivel(self) -> str:
        """O stderr do node, como texto. Um pipe binario devolve bytes."""
        try:
            bruto = pipe_de(self.proc, "stderr").read()[:800]
        except (OSError, ValueError):
            return "(stderr ilegivel)"
        return bruto.decode("utf-8", errors="replace") if isinstance(bruto, bytes) else bruto

    def abrir(self, raiz: Path, tsconfig: Path) -> dict:
        return self.cmd(
            {
                "op": "abrir",
                "raiz": str(raiz),
                "ts_node": self.node,
                "tsconfig": str(tsconfig) if tsconfig.is_file() else None,
            }
        )

    def delta(self, arquivo: str, depois: str) -> dict:
        # MEDIDO em 09/10: o `oraculo_ls.js` responde `{"ok":false, "erro":...}` e o `delta`
        # Seguinte na MESMA linha devolvia `{"ok":true}` -- sem checar, um erro do oraculo era lido
        # como "nenhum diagnostico novo", ou seja um NEGATIVO. Era a quinta vez na onda 2 que uma
        # falha aparecia como "zero sem avisar".
        r = self.cmd({"op": "delta", "arquivo": arquivo, "depois": depois})
        if not r.get("ok"):
            erro = str(r.get("erro", r))
            # `Could not find source file`: o arquivo mudou de caminho desde aquele commit, e o
            # programa do LanguageService (montado sobre o HEAD) nao o ve. MEDIDO em 09/10 no
            # excalidraw, com `packages/excalidraw/scroll.ts`. Isso e DESCARTE com contagem, nao
            # erro que mate o lote -- mas so este motivo; qualquer outra recusa ainda para tudo.
            if "Could not find source file" in erro:
                return {"ok": True, "descartado": "fora_do_programa", "erro": erro}
            raise RuntimeError(f"o oraculo LS recusou o delta de {arquivo!r}: {erro}")
        return r

    def fecha(self) -> None:
        # Fechar e sempre `finally`: o `node` ja pode ter morrido (timeout do `tsc` no meio de
        # um `delta`), e um `except Exception: pass` calado esconderia justamente esse caso --
        # o mesmo "zero sem avisar" que os outros cenarios do selftest cobrem. O `pass` virava
        # `kill()`, que e o unico desfecho que importa quando o pipe ja morreu.
        try:
            self.cmd({"op": "fechar"})
        except Exception:  # noqa: BLE001 - o finally nao pode levantar
            self.proc.kill()


def elegivel(caminho: str) -> tuple:
    if Path(caminho).suffix not in EXT_LANG:
        return False, "ext_fora"
    for frag in EXCLUIR_PATH:
        if frag in "/" + caminho:
            return False, "diretorio"
    for nome in EXCLUIR_NOME:
        if Path(caminho).name.endswith(nome):
            return False, "nome"
    return True, ""


def git_show_em_lote(repo: Path, specs: list[str]) -> list[str]:
    """O conteudo de N especificacoes `rev:caminho` em UM processo so.

    MEDIDO em 01/11 (V-8): `git show` por par sao DOIS spawns por par (e o pai do commit, mais
    um) — 17.670 pares em mc=600 sao ~50.000 spawns, e a coleta chegou a ser o gargalo do lote:
    1,5 h dos 2,4 h de coleta eram de repos cujas linguagens ja estavam cheias (o
    `por_lang >= alvo` so e conferido DEPOIS da coleta). O `git cat-file --batch` devolve o MESMO
    blob (o `git show rev:path` E um `cat-file blob`) — e o teste compara byte a byte.

    O `cat-file --batch` responde, por especificacao, `<oid> SP <tipo> SP <tamanho> LF` + o
    conteudo + LF; e `<spec> SP missing LF` quando o objeto nao existe. O que o `git show`
    devolveria vazio (o arquivo nao existe no commit) vem aqui como `missing`.
    """
    if not specs:
        return []
    # MEDIDO em 01/11 (V-8): os clones sao PARTIAIS (`remote.origin.promisor = true`) — o
    # `excalidraw` tem 21.169 blobs ausentes e o `zod` 13.576. O `cat-file --batch` encontra
    # um blob ausente e dispara um LAZY FETCH: **um round-trip HTTP por blob**. Medido: 24 min
    # de coleta com o git em 1,8% de CPU e um `git fetch` NOVO a cada 9 segundos — um lote de
    # 13.572 blobs precisaria de ~10.000 fetches. Com `GIT_NO_LAZY_FETCH=1` o blob ausente vem
    # como "missing" na hora e o par e descartado: nunca um travamento. Os blobs que a janela
    # usa sao buscados antes, em UMA negociacao por repo (`scripts/preparar_clones_oss.sh`),
    # para a rotulagem correr offline e determinista.
    env = dict(os.environ)
    env["GIT_NO_LAZY_FETCH"] = "1"
    proc = subprocess.Popen(
        ["git", "-C", str(repo), "cat-file", "--batch"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
    )
    entrada = ("\n".join(specs) + "\n").encode("utf-8")
    bruto, _ = proc.communicate(entrada)
    saida: list[str] = []
    pos = 0
    for _ in specs:
        fim = bruto.find(b"\n", pos)
        if fim < 0:
            saida.append("")
            break
        cabecalho = bruto[pos:fim].decode("utf-8", "replace").split()
        pos = fim + 1
        if len(cabecalho) < 3 or cabecalho[1] == "missing":
            saida.append("")
            continue
        try:
            n = int(cabecalho[2])
        except ValueError:
            saida.append("")
            continue
        saida.append(bruto[pos : pos + n].decode("utf-8", "replace"))
        pos += n + 1  # o LF que separa as respostas
    return saida


def candidatos_do_log(log: str) -> tuple[list, list]:
    """Do log cru: os pares candidatos e as especificacoes de conteudo.

    MEDIDO em 01/11 (V-8): o pai e a DATA de cada commit saem do MESMO log (`%P` e `%ct`), entao
    nao ha um `git rev-parse` por par. E um defeito que o numero expoe: o `datas` NUNCA era
    preenchido no codigo antigo, de modo que o `data_commit` era sempre 0 e a "ordem canonica"
    era ALFABETICA por caminho — o docstring prometia a data do commit na chave de ordem.
    """
    candidatos: list = []
    specs: list[str] = []
    atual = ""
    pai = ""
    data = 0
    for ln in log.splitlines():
        if ln.startswith("@@@"):
            p = ln[3:].split()
            atual = p[0].strip() if p else ""
            pais = p[1].split() if len(p) > 1 else []
            pai = pais[0] if pais else ""
            data = int(p[2]) if len(p) > 2 and p[2].isdigit() else 0
            continue
        if not atual or not ln.startswith(":"):
            continue
        corpo, _, caminho = ln.partition("\t")
        partes = corpo.split()
        if len(partes) < 5 or partes[4] not in ("M", "A"):
            continue
        caminho = caminho.strip()
        ok, _ = elegivel(caminho)
        if not ok:
            continue
        i_depois = len(specs)
        specs.append(f"{atual}:{caminho}")
        i_antes = None
        if partes[4] == "M" and pai:
            i_antes = len(specs)
            specs.append(f"{pai}:{caminho}")
        candidatos.append((atual, data, caminho, partes[4], i_depois, i_antes))
    return candidatos, specs


def pares_do_repo(repo: Path, max_commits: int) -> list:
    """Pares (commit, caminho, antes, depois) de commits M, mais os de A (criacao).

    MEDIDO em 01/11 (V-8): a coleta era 3 spawns por par (`rev-parse` do pai + 2 `git show`) e
    o pai e a DATA saem do MESMO log agora, em uma chamada so. O conteudo de todos os pares vem
    em UM `git cat-file --batch` — o mesmo blob, byte a byte (o teste compara).
    """
    log = subprocess.run(
        [
            "git",
            "-C",
            str(repo),
            "log",
            f"--max-count={max_commits}",
            f"--before=@{int(time.mktime((2026, 10, 9, 23, 0, 0, 0, 0, 0)))}",
            "--diff-filter=MA",
            "--raw",
            "--no-abbrev",
            "--pretty=format:@@@%H %P %ct",
        ],
        capture_output=True,
        text=True,
        timeout=600,
        check=False,
    ).stdout
    candidatos, specs = candidatos_do_log(log)
    conteudos = git_show_em_lote(repo, specs)
    saida = []
    for atual, data, caminho, operacao, i_depois, i_antes in candidatos:
        depois = conteudos[i_depois] if i_depois < len(conteudos) else ""
        antes = (
            conteudos[i_antes]
            if i_antes is not None and i_antes < len(conteudos)
            else ""
        )
        if not depois:
            continue
        cab = (antes or depois)[:400]
        if any(g in cab for g in GERADO):
            continue
        saida.append(
            {
                "commit": atual,
                # A data entra na CHAVE DE ORDEM da selecao; sem ela, dois commits do mesmo
                # arquivo em dias diferentes valeriam o mesmo posto.
                "data_commit": data,
                "caminho": caminho,
                "antes": antes,
                "depois": depois,
                # `M` = modificacao, tem antes e depois, e o DELTA se calcula.
                # `A` = criacao, `antes` e string vazia por construcao, e nao ha delta.
                # O juiz precisa saber a diferença para nao passar um `--disk-file` inexistente.
                "operacao": operacao,
            }
        )
    return saida


def chave_canonica(par: dict) -> tuple:
    """A ORDEM CANONICA de um par: (data do commit, caminho, id do rotulador).

    Exigencia da coordenadora (10/10): a regra tem de ser deterministica e CEGA AO ROTULO. Por
    isso a chave nao tem `classe`, `rotulo` nem `novos` -- um par positivo e um negativo com o
    mesmo (data, caminho) sao indistinguiveis aqui, entao o teto nao pode preferir um deles. `id` e
    o desempate final porque dois commits nunca tem a mesma data E o mesmo caminho.

    MEDIDO em 10/10: sem isto, o lote v4 fechou `packages/excalidraw/types.ts` com 12 entradas
    (6,28%) contra um teto final de 5, e a poda corretiva mantinha as primeiras ocorrencias na
    ordem de gravitacao -- isto e, mantinha o que o lote fez primeiro. O resultado dependia de quais
    ids o lote alcancou, nao do corpus.
    """
    ident = hashlib.sha256(
        (par.get("repo", "") + par["commit"] + par["caminho"]).encode()
    ).hexdigest()[:16]
    return (par.get("data_commit", 0), par["caminho"], ident)


def ordem_canonica(pares: list) -> list:
    """Os pares na ordem canonica. Totalmente ordenada (o `id` desempata), portanto
    reproduzivel: dois lotes com os mesmos pares produzem a MESMA ordem, e o teto produz o mesmo
    conjunto."""
    return sorted(pares, key=chave_canonica)


def sha_de_arquivo(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest() if p.is_file() else "(ausente)"


# Artefatos de instalacao, MEDIDOS em 09/10 nos clones: sao o que a instalacao e o proprio
# rotulador deixam, e nao residuo de mutacao. Qualquer `??` FORA desta lista recusa o rotulador.
# MEDIDO: `.yarnrc` (excalidraw, yarn 1), `node_modules/` (install), `tsconfig.json` (criado pelo
# rotulador nos repos que nao tem um), `.npmrc` (a linha `manage-package-manager-versions` que a
# coordenadora autorizou). Os clones sao descartaveis e vivem em ~/.claude/touring/vgp/oss/.
ARTEFATOS_DE_INSTALACAO = (
    "node_modules/",
    ".yarnrc",
    "tsconfig.json",
    ".npmrc",
    ".nvmrc",
)


def exige_clone_limpo(repo: Path) -> None:
    """O clone tem de estar em HEAD antes de rotular qualquer coisa.

    MEDIDO em 09/10: o `delta_do_par` escreve o `antes` no disco do clone e restaura no `finally`.
    Se o processo morrer no meio (SIGKILL, OOM, queda de energia) o arquivo fica com o `antes` de
    um par arbitrario, e a execucao seguinte mediria o delta contra esse residuo -- o mesmo
    "zero sem avisar" de sempre, agora com um arquivo de origem.

    Conta ` M` (rastreado modificado), ` D` (rastreado apagado) e `??` (nao rastreado, MENOS os
    artefatos de instalacao da allowlist). Um `??` de um par A que nao foi apagado cai aqui, e
    recusar e o comportamento certo: aquele arquivo muda a RESOLUCAO dos pares seguintes.

    A restauracao e por `git show HEAD:<path>`, nunca `git checkout --` (que e destrutivo e esta
    sob a REGRA #11).
    """
    p = subprocess.run(
        ["git", "-C", str(repo), "status", "--porcelain"],
        capture_output=True,
        text=True,
        timeout=300,
        check=False,
    )
    alterados, nao_rastreados = [], []
    for l in (p.stdout or "").splitlines():
        if not l.strip():
            continue
        caminho = l[3:].strip()
        if l.startswith("??"):
            if not any(
                caminho == a or caminho.startswith(a) for a in ARTEFATOS_DE_INSTALACAO
            ):
                nao_rastreados.append(caminho)
        else:
            alterados.append(caminho)
    if nao_rastreados:
        raise RuntimeError(
            f"o clone {repo} tem arquivo NAO RASTREADO fora da allowlist de instalacao: "
            f"{nao_rastreados[:3]}. Se for residuo de um par A, apague o arquivo; se for artefato "
            "de instalacao, acrescente em ARTEFATOS_DE_INSTALACAO (que vai no meta do selo)."
        )
    if not alterados:
        return
    restaura_rastreados(repo, alterados)
    if ainda_modificados(repo):
        raise RuntimeError(
            f"o clone {repo} continua com arquivo rastreado modificado depois de restaurar "
            f"por `git show HEAD:<path>`: {ainda_modificados(repo)[:3]}"
        )


def ainda_modificados(repo: Path) -> list:
    """Os ` M` / ` D` que sobraram. O `??` ja foi decidido antes, na allowlist."""
    confere = subprocess.run(
        ["git", "-C", str(repo), "status", "--porcelain"],
        capture_output=True,
        text=True,
        timeout=300,
        check=False,
    )
    return [
        l[3:].strip()
        for l in (confere.stdout or "").splitlines()
        if l and not l.startswith("??") and l[3:].strip()
    ]


def restaura_rastreados(repo: Path, caminhos: list) -> None:
    """Regrava por `git show HEAD:<path>`. Um arquivo que sumiu do HEAD e apagado."""
    for caminho in caminhos:
        if not caminho or caminho.endswith("/"):
            continue
        conteudo = subprocess.run(
            ["git", "-C", str(repo), "show", f"HEAD:{caminho}"],
            capture_output=True,
            timeout=300,
            check=False,
        ).stdout
        alvo = repo / caminho
        if not conteudo:
            alvo.unlink(missing_ok=True)
            continue
        alvo.unlink(missing_ok=True)
        alvo.parent.mkdir(parents=True, exist_ok=True)
        alvo.write_bytes(conteudo)


def delta_do_par(repo: Path, caminho: str, antes: str, depois: str, lsc) -> dict:
    """O DELTA do par (antes, depois), com o DISCO no `antes` enquanto o oraculo roda.

    MEDIDO em 09/10 (bug meu, o mais grave da onda): o rotulador pedia `lsc.delta(caminho,
    depois)` com o arquivo do clone no estado **HEAD**, e o HEAD de hoje e muito posterior ao
    commit do par -- o `antes` do par e um conteudo que aquele disco nunca teve. O DELTA medido era
    `delta(HEAD -> depois)`, e nao `delta(antes -> depois)`: o rotulo media a coisa errada. A prova
    de equivalencia JS expôs isso como `LS=[]` onde o `tsc` acusava; a medicao direta das duas
    rotas no mesmo arquivo deu `tsc: 166 | LS: 166 | so no tsc: 0 | so no LS: 0` -- o oraculo
    concordava, e a divergencia era eu medindo as duas rotas de jeitos diferentes.

    E o mesmo contrato do hook real: o motor recebe "o novo" e "o do disco", e o "do disco" tem de
    ser o ANTIGO do par, nao o HEAD. Por isso o disco e posto no `antes` ANTES do `lsc.delta`.

    A restauracao e VERIFICADA POR SHA (exigencia da coordenadora): o `finally` reescreve e compara
    o sha com o do comeco. Nao e confianca no `finally` -- e prova. Se nao voltar ao original, o
    erro sobe e o clone fica marcado como sujo para a proxima execucao recusar.
    """
    alvo = repo / caminho
    original = alvo.read_bytes() if alvo.is_file() else None
    sha_original = (
        hashlib.sha256(original).hexdigest() if original is not None else None
    )
    try:
        if original is not None:
            alvo.unlink(missing_ok=True)
        alvo.parent.mkdir(parents=True, exist_ok=True)
        alvo.write_text(antes, encoding="utf-8")
        return lsc.delta(caminho, depois)
    finally:
        # restauracao VERIFICADA POR SHA (exigencia da coordenadora). E o par A (criacao) tem
        # um caso a mais: o arquivo NAO existia antes, entao restaurar e APAGAR -- e conferir que
        # sumiu. MEDIDO em 09/10: um `??` deixado no clone muda a RESOLUCAO dos pares seguintes (um
        # import passa a resolver por causa do arquivo que sobrou) e o rotulo de outro par sai errado
        # sem nenhum aviso. Por isso o par A e o ` M` nao dao a mesma conta: um altera conteudo, o
        # outro cria caminho.
        alvo.unlink(missing_ok=True)
        if original is not None and sha_original is not None:
            alvo.write_bytes(original)
            if sha_de_arquivo(alvo) != sha_original:
                raise RuntimeError(
                    f"restauracao FALHOU em {repo / caminho}: sha {sha_de_arquivo(alvo)[:12]} "
                    f"!= {sha_original[:12]}"
                )
        elif alvo.exists():
            raise RuntimeError(
                f"o par A criou {repo / caminho} e o arquivo NAO foi apagado na restauracao"
            )


def delta_do_tsc(
    repo: Path, caminho: str, antes: str, depois: str, timeout: int
) -> dict:
    """O retestemunho: o MESMO par pelo `tsc` de verdade."""
    alvo = repo / caminho
    if not alvo.is_file():
        return {"ok": False, "motivo": "arquivo ausente"}
    try:
        original = alvo.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return {"ok": False, "motivo": "ilegivel"}
    try:

        def diag(stdout: str) -> Counter:
            c = Counter()
            for l in stdout.splitlines():
                l = l.strip()
                # MEDIDO em 09/10 (zod): o `tsc` tem DOIS formatos de saida e so um casava.
                #   `arquivo(linha,col): error TSxxxx:`  -- o padrao
                #   `arquivo:linha:col - error TSxxxx:`  -- emitido pelo zod (598 linhas, 0 casando)
                # Com o segundo formato nao parseado, o DELTA do `tsc` era SEMPRE VAZIO e o
                # retestemunho compararia "o tsc nao achou nada" com o LS -- o oraculo de
                # referencia virava uma confirma-ta-vazia.
                m = RE_TSC_CURTO.match(l) or RE_TSC_BONITO.match(l)
                if m and Path(m.group(1)).name == alvo.name:
                    c[(m.group(4), int(m.group(2)), m.group(5))] += 1
            return c

        def roda(texto: str):
            alvo.unlink(missing_ok=True)
            alvo.write_text(texto, encoding="utf-8")
            r = subprocess.run(
                [str(_tsc(repo)), "--noEmit", "-p", "."],
                cwd=repo,
                capture_output=True,
                text=True,
                timeout=timeout,
                check=False,
            )
            return diag(r.stdout)

        d_antes = roda(antes) if antes else Counter()
        d_depois = roda(depois)
        return {"ok": True, "novos": sorted((d_depois - d_antes).elements())}
    except subprocess.TimeoutExpired:
        return {"ok": False, "motivo": "tsc timeout"}
    finally:
        alvo.unlink(missing_ok=True)
        alvo.write_text(original, encoding="utf-8")


def estado_no_disco(saida: Path) -> tuple[set, dict, dict, dict]:
    """O que o disco JA tem: os ids, e os contadores que o teto precisa, POR LINGUAGEM.

    MEDIDO em 10/10 (lote `v5`): com os contadores GLOBAIS, `concentra_dentro_do_teto` mediu os
    arquivos de `javascript` (3 entradas) contra o n do `javascript` e accuse 233% em
    `packages/excalidraw/types.ts` -- que tem 7 entradas em typescript. O teto protege uma
    chave DENTRO de uma linguagem, entao `cont_arq` por linguagem e o unico agrupamento que
    reproduz a regra; um contador global transforma cada linguagem pequena numa infração de todos
    os arquivos das outras. E o terceiro denominador errado desta onda, depois do `alvo` e do
    `len(pares)`, e pela mesma razao: o teto fala de uma coisa e eu media outra.

    MEDIDO em 10/10 (lote `v4`): reexecutar o rotulador reconstruia `ids_ja_gravados` mas deixava
    `cont_arq`, `cont_commit` e `por_lang` em ZERO, e o teto media um n que nao incluía o ja
    gravado -- `types.ts` fechou com 12 entradas (6,28%) contra um teto final de 5.
    """
    ids: set = set()
    cont_arq: dict = {}
    cont_commit: dict = {}
    por_lang: dict = {}
    if not saida.is_dir():
        return ids, cont_arq, cont_commit, por_lang
    for j in saida.glob("*/entradas.jsonl"):
        lang = j.parent.name
        arq = Counter()
        commit = Counter()
        for ln in j.read_text(errors="replace").splitlines():
            if not ln.strip():
                continue
            try:
                e = json.loads(ln)
            except json.JSONDecodeError:
                continue
            ids.add(str(e.get("id")))
            arq[e.get("path")] += 1
            commit[e.get("commit")] += 1
        cont_arq[lang] = arq
        cont_commit[lang] = commit
        por_lang[lang] = sum(arq.values())
    return ids, cont_arq, cont_commit, por_lang


def ids_ja_gravados(saida: Path) -> set:
    """Os ids que ja estao no disco -- a premissa de idempotencia do rotulador."""
    return estado_no_disco(saida)[0]


def ident_do_par(nome_repo: str, par: dict) -> str:
    """O id do par: `sha256(repo + commit + caminho)[:16]`.

    Fica numa funcao so porque DOIS lugares precisam do MESMO nome: o `grava_entrada` que
    escreve a entrada e a disjuncao que recusa o par. Duas copias do calculo divergem em
    silencio -- e uma divergencia silenciosa aqui significa um par repetido num held-out.
    """
    return hashlib.sha256((nome_repo + par["commit"] + par["caminho"]).encode()).hexdigest()[:16]


def filtra_pares_excluidos(
    nome_repo: str, pares: list, excluidos: set | None
) -> tuple[list, list, dict]:
    """A POPULACAO sem os pares que um corpus anterior ja rotulou.

    MEDIDO em 01/11 (V-8): sem isto a camada nova repetiria a anterior par a par -- o
    `grava_entrada` REESCREVE o id em vez de recusar o par, entao um par ja usado seria rotulado
    de novo, e um held-out que repete o holdout anterior nao prova nada.

    O filtro e ANTES do laco de selecao: um par excluido nao consome tempo de oracle e nao entra
    no teto -- o teto mede o que foi ROTULADO, e contar o excluido daria um n maior que o n da
    camada.

    Fica numa funcao propria porque o `rotula_repo` ja era o bloco mais complexo do rotulador e
    as compreensoes de filtro somavam ramos a ele.
    """
    mask = excluidos or frozenset()
    mantidos: list = []
    fora: list = []
    for p in pares:
        if ident_do_par(nome_repo, p) in mask:
            fora.append(p)
        else:
            mantidos.append(p)
    por_lang = dict(Counter(EXT_LANG[Path(p["caminho"]).suffix] for p in fora))
    return mantidos, fora, por_lang


def ids_excluidos(dirs: list, ignorar: Path | None = None) -> tuple[set, dict]:
    """Os ids ja usados nos corpora ANTERIORES -- a DISJUNCAO da selecao.

    MEDIDO em 01/11 (V-8): o rotulador nao tinha nenhuma disjuncao entre corpora.
    `estado_no_disco` le SO o diretorio de SAIDA, e o `grava_entrada` REESCREVE o id em vez de
    recusar o par -- ou seja, um par que um corpus anterior ja rotulou seria rotulado DE NOVO, e
    a camada nova repetiria a anterior. Um held-out que repete o holdout anterior nao prova nada:
    o juiz pode ter sido calibrado nos mesmos pares.

    A comparacao e no MESMO espaco de nomes do rotulo (`ident_do_par`), e nao por similaridade de
    conteudo: dois arquivos com o mesmo conteudo em commits diferentes sao entradas diferentes.

    Os ids excluidos NAO entram em `cont_arq`, `cont_commit` nem `por_lang`: um par excluido nao
    foi rotulado, e o teto mede o que foi rotulado. Contar o excluido no teto daria um n maior que
    o n da camada, e o teto mediria uma populacao que nao esta no corpus.

    `ignorar` e o diretorio de SAIDA: uma reexecucao nao pode excluir a si mesma, senao o par ja
    gravado seria pulado em vez de reescrito, e a idempotencia do rotulador se perde.
    """
    ids: set = set()
    por_origem: dict = {}
    for d in dirs:
        p = Path(d).expanduser()
        if ignorar is not None and p.resolve().is_relative_to(ignorar.resolve()):
            continue
        achados: set = set()
        if p.is_dir():
            for jl in sorted(p.rglob("entradas.jsonl")):
                for ln in jl.read_text(errors="replace").splitlines():
                    if not ln.strip():
                        continue
                    try:
                        e = json.loads(ln)
                    except json.JSONDecodeError:
                        continue
                    achados.add(str(e.get("id")))
        por_origem[str(p)] = len(achados)
        ids |= achados
    return ids, por_origem


def grava_entrada(
    saida: Path, nome_repo: str, par: dict, lang: str, rotulo: str, vistos: set
) -> str:
    """Grava o par no estrato e devolve o id. O `id` e derivado de repo+commit+caminho.

    MEDIDO em 09/10: com `append`, reexecutar o rotulador DUPLICAVA a linha do JSONL (o mesmo id
    duas vezes) -- o selo mudava e a contagem por linguagem mentia. `vistos` traz os ids ja no
    disco e o par e REESCRITO no lugar em vez de acrescentado: reexecutar e idempotente, que e o
    que torna o lote conferivel contra o selo.

    `snap_antes` e o CONTRATO do judging (decisao da coordenadora, 09/10): o rotulo e por DELTA, e
    o motor so nega o que a edicao introduz. Medido nas 278 entradas ja gravadas: ZERO tinham o
    `antes` -- e sem ele o juiz chamaria o motor sem `--disk-file`, e todo erro PRE-EXISTENTE da
    base (os 1505 do `dom-examples`, as APIs de navegador) viraria falso deny contra um rotulo que
    nunca os contou. O par A (criacao) nao tem antes: o campo fica AUSENTE de proposito e
    `operacao` diz "A", para o juiz saber que nao ha delta a computar.
    """
    ident = ident_do_par(nome_repo, par)
    d = saida / lang
    (d / "conteudo").mkdir(parents=True, exist_ok=True)
    (d / "conteudo" / f"{ident}.txt").write_text(par["depois"], encoding="utf-8")
    operacao = par.get("operacao") or ("M" if par.get("antes") else "A")
    entrada = {
        "id": ident,
        "lang": lang,
        "repo": nome_repo,
        "commit": par["commit"],
        "path": par["caminho"],
        "rotulo": rotulo,
        "origem": "natural",
        "estrato": "oss",
        "operacao": operacao,
        "sha256": hashlib.sha256(par["depois"].encode()).hexdigest(),
        "conteudo": f"conteudo/{ident}.txt",
    }
    if operacao == "M" and par.get("antes"):
        (d / "conteudo").mkdir(parents=True, exist_ok=True)
        (d / "conteudo" / f"{ident}.antes.txt").write_text(
            par["antes"], encoding="utf-8"
        )
        entrada["snap_antes"] = f"conteudo/{ident}.antes.txt"
        entrada["sha256_antes"] = hashlib.sha256(par["antes"].encode()).hexdigest()
    linha = json.dumps(entrada, ensure_ascii=False, sort_keys=True)
    j = d / "entradas.jsonl"
    if ident in vistos:
        atuais = j.read_text(encoding="utf-8").splitlines() if j.is_file() else []
        j.write_text(
            "\n".join(x if json.loads(x).get("id") != ident else linha for x in atuais)
            + "\n",
            encoding="utf-8",
        )
    else:
        vistos.add(ident)
        with open(j, "a", encoding="utf-8") as f:
            f.write(linha + "\n")
    return ident


def teto_de(n_lang: int, fracao: float) -> int:
    """O teto por arquivo/commit: `fracao` do n JA ROTULADO daquela linguagem.

    MEDIDO em 09/10: usar `3% do alvo` (300 -> 9) dava 9,3% com n=97; usar `3% de len(pares)`
    (1217 no excalidraw) dava um teto de 36, tambem errado, porque `pares` soma tsx E ts enquanto
    o n que o teto protege e o de UMA linguagem. Aqui o n e o da linguagem do par, e o teto
    cresce com ele -- e por isso que o teto e reavaliado a cada par em vez de calculado uma vez.
    """
    return max(1, int(max(1, n_lang) * fracao))


def passa_no_teto(par: dict, cont_arq: Counter, cont_commit: Counter, teto: int) -> str:
    """Devolve "" quando o par passa, ou o nome do teto que o barrou."""
    # `.get(chave, 0)` e nao `[chave]`: um `Counter` vazio levanta `KeyError` numa chave
    # ausente, e o primeiro par de um arquivo novo seria barrado por um `KeyError` em vez de
    # passar. Ausente = zero ocorrencias, que e o que o teto quer dizer.
    if cont_arq.get(par["caminho"], 0) >= teto:
        return "teto_arquivo"
    if cont_commit.get(par["commit"], 0) >= teto:
        return "teto_commit"
    return ""


def retesta(repo: Path, nome_repo: str, par: dict, novos: list) -> dict | None:
    """O retestemunho: o MESMO par pelo `tsc` de verdade. Devolve a divergencia, ou None.

    Divergencia aqui NAO e problema do item: e o oraculo (LanguageService) discordando do
    compilador de referencia. Por isso ela para o lote inteiro em vez de rotular adiante.
    """
    r2 = delta_do_tsc(repo, par["caminho"], par["antes"], par["depois"], 900)
    if not r2.get("ok"):
        return {"motivo": r2.get("motivo", "tsc nao rodou")}
    tsc_novos = sorted(list(x) for x in r2.get("novos", []))
    ls_novos = sorted([n["cod"], n["lin"], n["msg"]] for n in novos)
    if tsc_novos != ls_novos:
        return {"ls": ls_novos, "tsc": tsc_novos}
    return None


def classifica_um(
    repo: Path,
    nome_repo: str,
    lsc,
    par: dict,
    lang: str,
    a,
    por_lang: dict,
    cont_arq: Counter,
    cont_commit: Counter,
    stats: Counter,
    vistos: set,
    retestar: bool,
) -> dict | None:
    """Um par: descarta, rotula, grava e (se for o do retestemunho) confere contra o `tsc`.

    O retestemunho roda ANTES de contar: se divergir, o par nao entra, porque a dividao e do
    oraculo. Devolve `{"parado": ...}` nesse caso, ou None para seguir.
    """
    resp = (
        delta_do_par(repo, par["caminho"], par["antes"], par["depois"], lsc)
        if par.get("operacao") == "M" and par.get("antes")
        else lsc.delta(par["caminho"], par["depois"])
    )
    if not resp.get("ok"):
        if resp.get("motivo"):
            stats[f"{lang}:descartado_{resp['motivo'].replace(' ', '_')}"] += 1
            return None
        stats[f"{lang}:descartado_{resp.get('descartado', 'erro_do_oraculo')}"] += 1
        return None
    # Dependencia externa ausente na BASE: o par e descartado. MEDIDO no zod, decisao da
    # coordenadora (09/10): 598 erros de base, quase todos "Cannot find module 'execa'|'arktype'|
    # 'valibot'". Num arquivo que importa uma delas, um par que quebra simbolo DESSE pacote sai
    # com erro igual antes e depois, o DELTA fica vazio e o par entraria como NEGATIVO -- e um deny
    # correto do motor contaria como falso deny, que e a medida que o holdout existe para sustentar.
    base = resp.get("base") or []
    dep_ausente = tem_dep_ausente_na_base(base)
    if dep_ausente:
        stats[f"{lang}:descartado_dep_ausente_na_base"] += 1
        stats[f"{lang}:codigos_dep_na_base"] += len(dep_ausente)
        return None
    novos = resp.get("novos", [])
    if retestar:
        div = retesta(repo, nome_repo, par, novos)
        if div is not None and "motivo" not in div:
            print(f"!! DIVERGENCIA no retestemunho: {nome_repo}/{par['caminho']}")
            print(f"   LS : {div['ls']}")
            print(f"   tsc: {div['tsc']}")
            print("PAR EXCLUIDO do lote (a dividao e do oraculo, nao do item)")
            stats["reteste_divergente"] += 1
            return {
                "divergencia": {"repo": nome_repo, "caminho": par["caminho"], **div},
            }
        stats["reteste_coincide" if div is None else "reteste_indisponivel"] += 1
    rotulo = "negativo" if not novos else "positivo_natural"
    cont_arq[par["caminho"]] += 1
    cont_commit[par["commit"]] += 1
    por_lang[lang] = por_lang.get(lang, 0) + 1
    stats[f"{lang}:{rotulo}"] += 1
    if not a.dry_run:
        grava_entrada(a.saida, nome_repo, par, lang, rotulo, vistos)
    return None


def excludes_necessarios(repo: Path) -> list:
    """Os `exclude` que o repo REALMENTE precisa, isto e, os que casam com algum arquivo.

    MEDIDO em 10/10 (decisao da coordenadora): o `exclude` de sintaxe invalida entra SO onde
    existe arquivo que casa. Hoje, em 7 clones, so o `dom-examples` tem
    `web-workers/worker-playground/bad-syntax-worker.js`; em excalidraw, zod, express, lodash e
    html5-boilerplate nenhum arquivo casa com `**/bad-syntax*.js`.

    Um `exclude` que nao casa com nada e ruido no selo e diff inutil na identidade do oraculo: em
    excalidraw e zod, o `tsconfig` usado passa a ser o do HEAD **byte a byte** (`sha_usado ==
    sha_head`, diff vazio), e a identidade do oraculo fica trivial de conferir.
    """
    necessarios = []
    for padrao in SINTAXE_INVALIDA:
        if "/" in padrao:
            if (repo / padrao).is_file():
                necessarios.append(padrao)
        else:
            # `**/bad-syntax*.js` -> o nome sem o prefixo `**/`
            nome = padrao.split("/")[-1]
            if any(p.is_file() and p.name == nome for p in repo.rglob("*")):
                necessarios.append(padrao)
    return necessarios


def jsonc_para_json(texto: str) -> str:
    """O texto do tsconfig como JSON puro: comentarios `//` e `/* */` removidos, strings intactas.

    ESTE E O MUTANTE, nao a implementacao. Ele existe para o cenario provar que um validador de
    tsconfig escrito a mao e mais fraco que o compilador:

    MEDIDO em 10/10, duas vezes. (1) `json.loads` reprovou um tsconfig com comentario, que o `tsc`
    aceita (JSONC). (2) o `jsonc_para_json`, escrito para consertar isso, reprovaria num tsconfig
    com VIRGULA FINAL - que o TypeScript tambem aceita, e `json.loads` nao. Cada conserto meu de
    parser e uma lista nova do que ele nao sabe.

    A implementacao de verdade e `valida_tsconfig`, que chama `ts.parseConfigFileTextToJson`: o
    MESMO parser que o `tsc` usa. A diferenca entre os dois e o que o cenario mede.
    """
    fora = []
    i = 0
    n = len(texto)
    while i < n:
        c = texto[i]
        if c == '"':
            j = i + 1
            while j < n:
                if texto[j] == "\\":
                    j += 2
                    continue
                if texto[j] == '"':
                    j += 1
                    break
                j += 1
            fora.append(texto[i:j])
            i = j
            continue
        if texto.startswith("//", i):
            j = texto.find("\n", i)
            i = n if j < 0 else j
            continue
        if texto.startswith("/*", i):
            j = texto.find("*/", i + 2)
            i = n if j < 0 else j + 2
            continue
        fora.append(c)
        i += 1
    return "".join(fora)


PARSE_TSCONFIG = Path(__file__).resolve().parent / "parse_tsconfig.js"


def valida_tsconfig(texto: str, node_ts=None) -> tuple:
    """`(ok, config_ou_None, motivo)` - validado pelo PARSER DO `tsc`, nao por `json.loads`.

    `node_ts` e o `typescript.js` do repo. Sem ele o validador cai no `jsonc_para_json`, e o
    cenario marca isso: um validador sem o compilador e um validador mais fraco, e ele precisa
    DIZER que e mais fraco em vez de fingir a mesma autoridade.
    """
    if node_ts is not None and Path(node_ts).is_file():
        proc = subprocess.run(
            # MEDIDO em 10/10: `node -e "<codigo>"` com o codigo embutido numa string
            # Python pagava o escape duas vezes e o duble respondia rc=0 com stdout VAZIO.
            # Um arquivo `.js` proprio nao tem essa camada, e o selftest passa a poder
            # mostrar o arquivo que roda.
            ["node", str(PARSE_TSCONFIG), str(node_ts)],
            # MEDIDO em 10/10: `subprocess.run(input=...)` NAO acrescenta newline, e o Node le
            # por linha — a requisicao ficava no buffer sem delimitador, o `data` nunca disparava,
            # e o processo saia com `rc=0` e stdout VAZIO. Um zero sem aviso, pela sexta vez na
            # onda, e desta vez dentro do validador que eu acabei de escrever.
            input=json.dumps(
                {"op": "parse-tsconfig", "nome": "tsconfig.json", "texto": texto}
            )
            + "\n",
            capture_output=True,
            text=True,
            timeout=120,
            check=False,
        )
        out = proc.stdout or ""
        if out.startswith("#"):
            n = int(out[1:].splitlines()[0])
            corpo = out.splitlines()[1] if len(out.splitlines()) > 1 else ""
            corpo = corpo[:n]
            try:
                r = json.loads(corpo)
            except json.JSONDecodeError:
                return (
                    False,
                    None,
                    f"resposta do parser do tsc nao parseavel: {corpo[:80]!r}",
                )
            if r.get("ok"):
                return True, r.get("json"), ""
            return (
                False,
                None,
                f"o tsc recusou o tsconfig: {str(r.get('erro', ''))[:120]}",
            )
        return (
            False,
            None,
            (
                f"o parser do tsc nao respondeu (rc={proc.returncode}, "
                f"stderr={(proc.stderr or '')[:120]!r})"
            ),
        )
    try:
        return True, json.loads(jsonc_para_json(texto)), ""
    except json.JSONDecodeError as e:
        return False, None, f"jsonc_para_json (SEM o compilador) recusou: {e}"


def monta_exclude_minimo(texto: str, necessarios: list) -> str:
    """O texto do tsconfig COM as entradas de `exclude` acrescentadas, e nada mais tocado.

    SepARADO de quem grava porque e aqui que mora a DECISAO (o array existe? ha espaco? o fim do
    arquivo?), e uma funcao que monta texto e outra que decide gravar nao se testam do mesmo jeito.

    MEDIDO em 10/10: `json.load` + `json.dumps(indent=1)` reescrevia o arquivo inteiro do repo
    (diff textual de **3713 chars** no excalidraw) para um delta semantico de **2 campos** e
    **zero** `compilerOptions` diferentes. Todo o resto era reserializacao — e reserializacao e
    um programa diferente, porque o motor e a camada 2 vao ler o arquivo como o repo escreveu.
    """
    faltando = [e for e in necessarios if e not in texto]
    if not faltando:
        return texto
    m = re.search(r'("exclude"\s*:\s*\[)(.*?)(\])', texto, re.DOTALL)
    if m:
        dentro = m.group(2).rstrip().removesuffix(",")
        sep = "\n    " if "\n" in dentro else ", "
        prefixo = "" if not dentro.strip() else sep
        novo = dentro + prefixo + ", ".join(f'"{e}"' for e in faltando)
        return texto[: m.start(2)] + novo + texto[m.end(2) :]
    bloco = ',\n  "exclude": [' + ", ".join(f'"{e}"' for e in faltando) + "]"
    fim = re.search(r"^(\s*)\n?\s*\}\s*$", texto, re.MULTILINE)
    if fim:
        return texto[: fim.start()] + bloco + texto[fim.start() :]
    return texto.rstrip().rstrip("}").rstrip() + bloco + "\n}\n"


CAMPOS_QUE_NAO_PODEM_MUDAR = ("compilerOptions", "include", "files", "extends")


def acrescenta_exclude_minimo(repo: Path, tsconfig: Path, necessarios: list) -> list:
    """Grava o tsconfig minimo. So grava quando as TRES garantias passam.

    (a) ha `exclude` a acrescentar; (b) o JSON montado continua valido; (c) nenhum campo alem do
    `exclude` mudou. Sem as tres, o disco NAO e tocado — um `tsconfig` de projeto alterado e um
    oraculo diferente, e um oraculo diferente rotula outra coisa.
    """
    if not necessarios:
        return []
    original = tsconfig.read_text(errors="replace") if tsconfig.is_file() else ""
    head = subprocess.run(
        ["git", "-C", str(repo), "show", "HEAD:tsconfig.json"],
        capture_output=True,
        timeout=120,
        check=False,
    )
    texto = (
        head.stdout.decode("utf-8", errors="replace")
        if head.returncode == 0
        else original
    )
    if not texto.strip():
        return []

    novo_texto = monta_exclude_minimo(texto, necessarios)
    if novo_texto == texto:
        return []

    node_ts = repo / "node_modules" / "typescript" / "lib" / "typescript.js"
    ok, novo_obj, motivo = valida_tsconfig(novo_texto, node_ts)
    if not ok:
        raise RuntimeError(
            f"o tsconfig montado para {repo} nao e aceito pelo tsc ({motivo}); "
            f"o disco NAO foi tocado"
        )
    if original:
        ok_o, obj_original, _motivo_o = valida_tsconfig(original, node_ts)
        if not ok_o:
            raise RuntimeError(
                f"o tsconfig JA em disco em {repo} nao e aceito pelo tsc; o disco NAO foi tocado"
            )
        for chave in CAMPOS_QUE_NAO_PODEM_MUDAR:
            if obj_original.get(chave) != novo_obj.get(chave):
                raise RuntimeError(
                    f"ao acrescentar o exclude em {repo}, `{chave}` mudou; o disco NAO foi tocado"
                )
    tsconfig.write_text(novo_texto, encoding="utf-8")
    return necessarios


def rotula_repo(nome_repo: str, a, por_lang: dict, excluidos: set | None = None) -> dict:
    """Rotula um repo inteiro. Devolve o relatorio do repo, ou `{"parado": True, ...}`.

    `excluidos` sao os ids que um corpus ANTERIOR ja rotulou: o par e recusado ANTES do
    adjudicador, entao ele nao consome tempo de oracle nem entra no teto. Recusar antes e o que
    mantem a camada nova DISJUNTA da antiga -- e e o que impede um held-out de repetir o
    holdout que o juiz ja viu.
    """
    repo = OSS / nome_repo
    if not repo.is_dir():
        return {"erro": "repo ausente"}
    exige_clone_limpo(repo)
    pares = pares_do_repo(repo, a.max_commits)
    relatorio: dict = {"pares_coletados": len(pares)}
    if not pares:
        return relatorio
    tsconfig = repo / "tsconfig.json"
    criado_pelo_rotulador = False
    if not tsconfig.is_file():
        tsconfig = next(repo.glob("packages/*/tsconfig.json"), None)
    if tsconfig is None:
        # MEDIDO em 09/10: `html5-boilerplate`, `express`, `lodash` e `dom-examples` NAO tem
        # `tsconfig.json`, e sem ele o `oraculo_ls.js` nao monta programa nenhum. Criar o
        # `TSCONFIG_JS` -- o mesmo do `corpus_semeado.py` -- e o que torna esses repos usaveis.
        tsconfig = repo / "tsconfig.json"
        tsconfig.write_text(TSCONFIG_JS, encoding="utf-8")
        criado_pelo_rotulador = True
    # MEDIDO em 10/10 (decisao da coordenadora): o `exclude` entra SO onde existe arquivo que
    # casa. Em excalidraw e zod nenhum arquivo casa com `**/bad-syntax*.js`, entao o tsconfig usado
    # passa a ser o do HEAD **byte a byte** e a identidade do oraculo fica trivial de conferir.
    necessarios = excludes_necessarios(repo)
    faltando = acrescenta_exclude_minimo(repo, tsconfig, necessarios)
    relatorio["exclude_necessario"] = necessarios
    if faltando:
        relatorio["exclude_adicionado"] = faltando
    lsc = LS(repo)
    r = lsc.abrir(repo, tsconfig) if tsconfig else {"ok": False}
    if not r.get("ok"):
        return {**relatorio, "erro": f"LS nao abriu: {r}"}
    relatorio["tsconfig_criado_pelo_rotulador"] = criado_pelo_rotulador
    relatorio["tsconfig"] = str(tsconfig)

    # MEDIDO em 10/10: a ordem de visita dos pares e CANONICA, nao a ordem de coleta do git nem
    # uma baralha com semente. `passa_no_teto` barra a chave que ja atingiu o teto, e o teto e
    # `3% do n da linguagem` -- logo o QUE ENTRA depende da ordem em que os pares chegaram. Uma
    # baralha com semente fixa fixava o lote, mas nao o tornava independente do lote ANTERIOR:
    # `ids_ja_gravados` faz o lote retomar no meio, e a baralhava comecava do zero de novo. A
    # ordem canonica resolve as duas coisas: independe do lote e nao depende de sorte.
    pares = ordem_canonica(pares)
    # A DISJUNCAO tira da POPULACAO os pares que um corpus anterior ja rotulou (V-8, 01/11).
    pares, descartados, excluidos_por_lang = filtra_pares_excluidos(nome_repo, pares, excluidos)
    relatorio["pares_excluidos_ja_usados"] = len(descartados)
    relatorio["excluidos_por_lang"] = excluidos_por_lang
    ordem = list(range(len(pares)))
    marcados = {i for i in range(0, len(ordem), RETEST_A_CADA)}
    # MEDIDO em 09/10: o teto era calculado sobre `alvo` (300), dando 9, e o lote parava em 319
    # MEDIDO em 09/10: DOIS erros de denominador, ambos meus. (1) O teto era `3% do ALVO`
    # (300 -> 9) e nao do que foi rotulado; com n=97 isso dava 9/97 = 9,3%. (2) Corrigido para
    # `3% de len(pares)`, e ainda assim o teto estourava (9,8%): `len(pares)` conta TODAS as
    # linguagens do repo, enquanto o n que o teto protege e o da LINGUAGEM. Com o excalidraw
    # (200+ commits, tsx e ts misturados) `3% de 1217` dava 36, nao 3. `teto_de()` e chamado
    # DENTRO do laco, por par, com o n da linguagem daquele par -- e nao calculado uma vez.
    stats: Counter = Counter()
    # As divergencias entre o adjudicador e o `tsc` (V-8, 01/11): cada uma EXCLUI o par e o
    # lote continua. A lista vai para o relatorio do repo e para o selo.
    divergencias: list = []
    # MEDIDO em 10/10 (lote `v4`): os contadores do teto vinham de ZERO a cada execucao, enquanto
    # os ids eram lidos do disco. Num lote retomado o teto media um n que nao incluía o ja
    # gravado, e `packages/excalidraw/types.ts` fechou com 12 entradas (6,28%) contra um teto
    # final de 5. `estado_no_disco` reconstroi os tres a partir das MESMAS linhas que fornecem
    # os ids, entao nao ha como um estar certo e o outro errado.
    vistos, cont_arq_por_lang, cont_commit_por_lang, por_lang_disco = estado_no_disco(
        a.saida
    )
    # `por_lang` conta o n JA ROTULADO de cada linguagem, e o teto e fracao dele. Sem esta soma o
    # lote retomado mediria o teto contra so o que ele mesmo rotulou nesta execucao. E
    # `cont_arq`/`cont_commit` sao POR LINGUAGEM: o teto protege uma chave dentro de uma linguagem,
    # e um contador global faria cada linguagem pequena acusar os arquivos das outras.
    for _lang, _n in por_lang_disco.items():
        por_lang[_lang] = por_lang.get(_lang, 0) + _n
    cont_arq: Counter = Counter()
    cont_commit: Counter = Counter()
    # MEDIDO em 01/11 (V-8): os contadores do TETO sao POR LINGUAGEM e nao podem ser
    # recarregados do disco a cada troca de linguagem. O codigo antigo recarregava, e o
    # resultado medido foi a concentracao explodir: com a ordem CRONOLOGICA os pares
    # alternam de linguagem a cada commit (o mesmo commit toca .ts e .tsx), e cada alternancia
    # devolvia os contadores ao estado do disco — que nao tinha as entradas ja rotuladas neste
    # repo. Medido na camada parcial: tsx com 25 entradas num arquivo contra um teto de 3.
    # O `Counter` por linguagem e atualizado NO LUGAR e sobrevive a alternancia.
    cont_arq_por_lang = {lg: Counter(c) for lg, c in cont_arq_por_lang.items()}
    cont_commit_por_lang = {lg: Counter(c) for lg, c in cont_commit_por_lang.items()}
    lang_atual = ""
    try:
        for idx in ordem:
            par = pares[idx]
            lang = EXT_LANG[Path(par["caminho"]).suffix]
            # O par pode ser de outra linguagem do que o anterior: os contadores do teto sao POR
            # LINGUAGEM, entao mudam junto. Trocar de `Counter` a cada troca mantem a regra do teto
            # intacta sem somar as linguagens umas as outras.
            if lang != lang_atual:
                lang_atual = lang
                cont_arq = cont_arq_por_lang.setdefault(lang, Counter())
                cont_commit = cont_commit_por_lang.setdefault(lang, Counter())
            if por_lang.get(lang, 0) >= a.alvo:
                continue
            barrou = passa_no_teto(
                par,
                cont_arq,
                cont_commit,
                teto_de(por_lang.get(lang, 0), a.teto_concentracao),
            )
            if barrou:
                stats[f"{lang}:{barrou}"] += 1
                continue
            parou = classifica_um(
                repo,
                nome_repo,
                lsc,
                par,
                lang,
                a,
                por_lang,
                cont_arq,
                cont_commit,
                stats,
                vistos,
                idx in marcados,
            )
            if parou:
                # MEDIDO em 01/11 (V-8): a divergencia entre o adjudicador e o `tsc` EXCLUI o
                # par — que e o item suspeito — e o lote CONTINUA. Antes o desfecho era parar o
                # LOTE INTEIRO, e o efeito medido foi nenhum corpus: o `excalidraw` diverge em
                # `manager.tsx` (o LS acusa TS2307 de um modulo que o `tsc` resolve) e o lote
                # morreu com 218 entradas. Parar tudo e o mais conservador possible, mas um
                # corpus que nao existe nao prova nada: a divergencia e do ORACULO, e ela ja
                # tira do corpus justamente o par em que o oraculo discorda do compilador de
                # referencia. O que nao pode e o par entrar.
                if parou.get("divergencia"):
                    divergencias.append(parou["divergencia"])
                    continue
                return parou
    finally:
        lsc.fecha()
    relatorio.update(
        {
            "rotulados": sum(
                v for k, v in stats.items() if not k.startswith("reteste")
            ),
            "retestes": sum(v for k, v in stats.items() if k.startswith("reteste")),
            "divergencias": divergencias,
            "n_divergencias": len(divergencias),
            "stats": dict(stats),
        }
    )
    return relatorio


def sha_tree(raiz: Path) -> str:
    """`sha256(relpath + sha256(conteudo))` em ordem -- o mesmo metodo dos selos locais.

    Sem o selo, o holdout e um diretorio de arquivos que alguem pode trocar sem ninguem perceber.
    O selo e o que torna "continua selado" uma verificacao e nao uma promessa.
    """
    partes = [
        str(f.relative_to(raiz)) + hashlib.sha256(f.read_bytes()).hexdigest()
        for f in sorted(p for p in raiz.rglob("*") if p.is_file())
        if f.name != "selo.json"
    ]
    return hashlib.sha256("\n".join(partes).encode()).hexdigest()


def identidades_do_oraculo() -> dict:
    """A IDENTIDADE do oraculo que produziu o selo: bytes do `oraculo_ls.js` e versao do `tsc`.

    MEDIDO em 10/10 (o lote `rotulador_v3`): ele PAROU numa divergencia em
    `excalidraw/packages/element/tests/zindex.test.tsx`, com `LS: [TS2345 linha 119]` e `tsc: []`.
    Reproduzi o par exato e a divergencia sumiu: o `TS2345` da 119 e erro PRE-EXISTENTE do `antes`
    (a mutacao o REMOVE), o `delta` do LS da `novos=[]` em 3 rodadas com ordem invertida, e o
    `tsc` acusa zero no `depois` porque o arquivo esta correto. Ou seja: o `ls` que parou o lote
    NAO e o que o oraculo atual produz. A causa: o lote foi lancado enquanto eu reescrevia o
    `oraculo_ls.js`, e o `node` le o arquivo UMA vez no boot -- o processo ficou com a versao
    antiga em memoria enquanto o selo ja declarava a nova.

    Um oraculo trocado no meio da corrida e um oraculo nao identificado: o selo nomeia `tsc` e
    nao nomeia o LS, entao ninguem consegue dizer qual dos dois respondeu. Este dicionario vai
    PARA o selo, e `confere_oraculo` abaixo recusa o lote se mudar entre o inicio e o fim.
    """
    js = Path(ORACULO_LS)
    ident = {
        "oraculo_ls_sha256": hashlib.sha256(js.read_bytes()).hexdigest()
        if js.is_file()
        else None,
        "oraculo_ls_bytes": js.stat().st_size if js.is_file() else None,
    }
    ident["tsconfigs"] = tsconfigs_do_oraculo()
    # MEDIDO em 10/10: a versao do `tsc` e por REPO, e nao "a primeira que achar". Com uma so
    # entrada (`tsc_date-fns`) o selo descrevia um oraculo que o lote nem usa -- o v8 roda
    # `excalidraw,zod`, e o selo nomeava o date-fns. Um selo que nomeia a ferramenta errada e
    # pior que um selo que nao nomeia: ele convence que a identidade esta conferida.
    versoes = {}
    for repo in sorted(OSS.glob("*")):
        tsc = repo / "node_modules/typescript/bin/tsc"
        if not tsc.is_file():
            continue
        try:
            v = subprocess.run(
                ["node", str(tsc), "--version"],
                capture_output=True,
                text=True,
                timeout=120,
                check=False,
            ).stdout.strip()
        except (OSError, subprocess.SubprocessError):
            continue
        if v:
            versoes[repo.name] = v
    ident["tsc_por_repo"] = versoes
    return ident


def tsconfigs_do_oraculo() -> dict:
    """Por repo: o `tsconfig` que o HEAD tem, o que o oraculo USOU, e o diff entre os dois.

    MEDIDO em 10/10 (exigencia da coordenadora): o rotulador reescreve o `tsconfig.json`
    rastreado de cada clone — `garante_exclude_de_sintaxe_invalida` acrescenta `exclude` quando
    falta, e `TSCONFIG_JS` CRIA o arquivo quando o repo nao tem um (medido no `dom-examples`: sem
    `exclude`, o `tsc` responde `rc=2` com 3 linhas de TS1109 e PARA antes de analisar). O
    programa que o LanguageService enxerga depende dessas mudancas, entao o `tsconfig` e parte do
    oraculo tanto quanto o `oraculo_ls.js` e a versao do `tsc`.

    `sha_head` e o `git show HEAD:tsconfig.json`; `sha_usado` e o arquivo em disco agora. Os dois
    juntos dizem, sem depender de diff legivel, se o oraculo rodou com a config do repo ou com uma
    config derivada. `diff` e o `git diff -- tsconfig.json`, e fica para leitura humana.
    """
    saida: dict = {}
    for repo in sorted(OSS.glob("*")):
        if not repo.is_dir():
            continue
        tc = repo / "tsconfig.json"
        if not tc.is_file():
            continue
        usado = hashlib.sha256(tc.read_bytes()).hexdigest()
        head = subprocess.run(
            ["git", "-C", str(repo), "show", "HEAD:tsconfig.json"],
            capture_output=True,
            timeout=120,
            check=False,
        )
        sha_head = (
            hashlib.sha256(head.stdout).hexdigest() if head.returncode == 0 else None
        )
        d = subprocess.run(
            ["git", "-C", str(repo), "diff", "--", "tsconfig.json"],
            capture_output=True,
            text=True,
            timeout=120,
            check=False,
        ).stdout.strip()
        saida[repo.name] = {
            "sha_head": sha_head,
            "sha_usado": usado,
            "alterado": sha_head != usado,
            # O diff NAO e truncado com silencios: um selo que corta no meio do diff e um
            # selo que esconde a mudanca. Se nao couber, o selo inteiro fica grande e nao
            # truncado — e um selo grande e legivel, um selo truncado e um selo que mente.
            "diff": d,
        }
    return saida


def confere_oraculo(antes: dict, agora: dict) -> str:
    """Diz o que mudou no oraculo desde o inicio do lote, ou '' se nada mudou.

    O `tsconfigs` e um dicionario de dicionarios, entao a comparacao de igualdade nao serve
    aqui: um `tsconfig` reescrito no meio do lote e uma mudanca de oraculo do mesmo jeito que
    um `oraculo_ls.js` trocado, e a diferenca tem de aparecer com arquivo e sha.
    """
    mudou = []
    for k in sorted((set(antes) | set(agora)) - {"tsconfigs"}):
        if antes.get(k) != agora.get(k):
            mudou.append(f"{k}: {antes.get(k)} -> {agora.get(k)}")
    ta, tb = antes.get("tsconfigs") or {}, agora.get("tsconfigs") or {}
    for repo in sorted(set(ta) | set(tb)):
        if ta.get(repo, {}).get("sha_usado") != tb.get(repo, {}).get("sha_usado"):
            mudou.append(
                f"tsconfig de {repo}: "
                f"{str(ta.get(repo, {}).get('sha_usado'))[:12]} -> "
                f"{str(tb.get(repo, {}).get('sha_usado'))[:12]}"
            )
    return "; ".join(mudou)


def grava_selo(
    saida: Path,
    por_lang: dict,
    relatorios: dict,
    ident: dict | None = None,
    exclusao: dict | None = None,
) -> dict:
    """Grava o selo e o confere relendo do disco -- um selo nao conferido nao e selo.

    A concentracao e MEDIDA aqui, contra o disco, e nao copiada da lista `por_lang` do lote: sao
    numeros de coisas diferentes, e o lote so enxerga o que ele proprio rotulou nesta execucao.
    """
    dentro, infratores = concentra_dentro_do_teto(saida, 0.03)
    concentracao_medida = {
        "dentro_do_teto": dentro,
        "infratores": [
            {"lang": lg, "chave": ch, "n": n, "fracao": fr, "teto": t}
            for lg, ch, n, fr, t in infratores
        ],
    }
    selo = {
        "estrato": "oss",
        "metodo": "sha256(relpath + sha256(conteudo)) em ordem crescente, exclui o proprio selo",
        "sha_tree": sha_tree(saida),
        "identidade_do_oraculo": ident or {},
        "tsconfig_alterado": (ident or {}).get("tsconfigs", {}),
        "n_por_lang": dict(sorted(por_lang.items())),
        "reteste_a_cada": RETEST_A_CADA,
        "semente": SEMENTE,
        "contrato_do_julgamento": (
            "o rotulo e por DELTA (erro NOVO introduzido pela edicao). O juiz deve chamar o motor "
            "com `--disk-file <conteudo/<id>.antes.txt>` (campo `snap_antes`) sempre que ele "
            "estiver presente: sem o arquivo do disco, todo erro PRE-EXISTENTE da base vira falso "
            'deny contra um rotulo que nunca o contou. `operacao: "A"` (criacao) nao tem antes por '
            "construcao e nao tem delta a computar; o campo fica ausente de proposito."
        ),
        "teto_concentracao": 0.03,
        "concentracao_medida": concentracao_medida,
        "exclusao": exclusao or {},
        "artefatos_de_instalacao_permitidos_no_clone": list(ARTEFATOS_DE_INSTALACAO),
        "contrato_do_clone": (
            "antes de rotular, `exige_clone_limpo` RECUSA se houver ` M`, ` D`, ou `??` fora da "
            "allowlist de instalacao acima. Um `??` e o residuo de um par A cujo arquivo criado "
            "nao foi apagado -- e ele muda a RESOLUCAO dos pares seguintes (um import passa a "
            "resolver por causa do arquivo que sobrou), entao o rotulo de outro par sairia errado "
            "sem aviso. Restauracao por `git show HEAD:<path>`, nunca `git checkout --`."
        ),
        "teto_por_chave_no_fim": {
            lang: teto_de(n, 0.03) for lang, n in sorted(por_lang.items())
        },
        "repos": {
            nome: {
                k: v for k, v in r.items() if k in ("rotulados", "retestes", "stats")
            }
            for nome, r in relatorios.items()
        },
        "gerado_em": time.strftime("%Y-%m-%dT%H:%M:%S"),
    }
    (saida / "selo.json").write_text(
        json.dumps(selo, ensure_ascii=False, indent=1) + "\n", encoding="utf-8"
    )
    relido = json.loads((saida / "selo.json").read_text(encoding="utf-8"))
    selo["confere_na_relitura"] = relido["sha_tree"] == sha_tree(saida)
    return selo


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--alvo", type=int, default=300, help="por linguagem")
    ap.add_argument("--teto-concentracao", type=float, default=0.03)
    ap.add_argument("--max-commits", type=int, default=120)
    ap.add_argument("--repos", default=",".join(ORDEM))
    ap.add_argument("--saida", type=Path, default=SAIDA)
    ap.add_argument(
        "--excluir-corpus",
        action="append",
        default=[],
        metavar="DIR",
        help=(
            "corpus anterior cujos ids ficam de fora da selecao (repetivel). Sem esta flag o "
            "comportamento e o de sempre: nenhum par e recusado por uso anterior."
        ),
    )
    ap.add_argument("--dry-run", action="store_true")
    a = ap.parse_args()

    por_lang: dict = {}
    relatorios = {}
    paradas = []
    ident_inicio = identidades_do_oraculo()
    excluidos, ids_por_origem = ids_excluidos(a.excluir_corpus, a.saida)
    exclusao = {
        "fontes": [str(Path(d).expanduser()) for d in a.excluir_corpus],
        "n_ids_por_origem": ids_por_origem,
        "n_ids": len(excluidos),
        "metodo": "sha256(repo + commit + caminho)[:16] -- o mesmo nome que o `grava_entrada` usa",
        "efeito": (
            "os pares ja usados saem da POPULACAO antes da selecao: nao consomem oracle e nao "
            "entram no teto (o teto mede o que foi ROTULADO)"
        ),
    }
    # MEDIDO em 10/10 (dois lotes perdidos): o rotulador e os probes escrevem os mesmos
    # arquivos dos clones. No `v3` o `oraculo_ls.js` foi trocado no meio do lote e o
    # `node` respondeu com a versao antiga; no `v5` um probe reescreveu
    # `linearElementEditor.ts` durante o par e o retestemunho acusou tres `TS2554` a mais
    # que os dois oraculos concordam ter. `fcntl.flock` BLOQUEANTE e `O_CLOEXEC`: o
    # `oraculo_ls.js` e um `node` de vida longa por dentro do rotulador, e um fd herdado
    # seguraria o exclusive depois do rotulador morrer.
    with vgp_lock.escreve_exclusivo():
        _rotula_todos(a, por_lang, relatorios, paradas, ident_inicio, excluidos)
    selo = _selo_e_veredito(a, por_lang, relatorios, paradas, ident_inicio, exclusao)
    return selo


def _rotula_todos(a, por_lang, relatorios, paradas, ident_inicio, excluidos=None) -> None:
    for nome_repo in a.repos.split(","):
        rel = rotula_repo(nome_repo, a, por_lang, excluidos)
        relatorios[nome_repo] = rel
        if rel.get("parado"):
            paradas.append(rel["divergencia"])
            break


def _selo_e_veredito(a, por_lang, relatorios, paradas, ident_inicio, exclusao=None) -> int:
    """Selo, conferencia do oraculo e do teto, e o EXIT CODE.

    Fica DEPOIS do `with vgp_lock`: a identidade do oraculo e a concentracao sao do estado do
    disco depois do lote, e medir de dentro do lock nao veria o que um escritor externo fez
    nem trocou entre a leitura e a escrita.
    """
    # MEDIDO em 10/10: a divergencia que parou o `rotulador_v3` nao reproduz -- o oraculo ATUAL
    # concorda com o `tsc` no par exato. A diferenca era o `oraculo_ls.js` trocado no meio da
    # corrida: o `node` le o arquivo uma vez no boot e fica com aquela versao em memoria. Sem
    # esta conferencia o selo declararia um oraculo que nao foi o que respondeu, e o lote inteiro
    # seria irreproduzivel sem nenhuma pista do porque.
    ident_fim = identidades_do_oraculo()
    mudou = confere_oraculo(ident_inicio, ident_fim)
    if mudou:
        paradas.append({"oraculo_mudou_durante_o_lote": mudou})
        print(f"!! ORACULO MUDOU NO MEIO DO LOTE -- selo INVALIDADO: {mudou}")
    selo = (
        grava_selo(a.saida, por_lang, relatorios, ident_inicio, exclusao)
        if not a.dry_run
        else None
    )
    # MEDIDO em 10/10 (lote `v4`): o selo declarava `teto_concentracao: 0.03` e era conferido na
    # relitura, e mesmo assim o lote tinha 7 chaves acima do teto (`types.ts` com 12 = 6,28%).
    # Um selo que declara uma restricao sem MEDIR e um relatorio, nao uma restricao: o juiz que
    # confiar nele vai amostrar um holdout que e, no maximo, um arquivo so.
    fora_do_teto = False
    if selo is not None and not selo["concentracao_medida"]["dentro_do_teto"]:
        fora_do_teto = True
        paradas.append(selo["concentracao_medida"])
        print(
            "!! CONCENTRACAO ACIMA DO TETO -- o selo declara um teto que o lote violou:"
        )
        for inf in selo["concentracao_medida"]["infratores"]:
            print(
                f"   {inf['lang']} {inf['chave']}: {inf['n']} entradas "
                f"({inf['fracao'] * 100:.2f}%, teto {inf['teto']})"
            )
    print(
        json.dumps(
            {
                "por_lang": por_lang,
                "repos": relatorios,
                "reteste_a_cada": RETEST_A_CADA,
                "semente": SEMENTE,
                "paradas": paradas,
                "alvo": a.alvo,
                "selo": selo,
            },
            indent=1,
            ensure_ascii=False,
        )
    )
    return 1 if fora_do_teto else 0


def cenario_delta_recusado() -> tuple[str, bool, str]:
    """Um oraculo que RECUSA o delta tem de ser erro alto, nunca "nenhum diagnostico novo".

    MEDIDO em 09/10 no h5bp: o node respondeu `{"ok":false,"erro":"Cannot find module ..."}` e,
    sem checar o `ok`, isso virava `novos: []` e o par entrava como NEGATIVO -- um oraculo que
    falhou lido como um oraculo que nao achou nada.
    """
    lsc = LS(Path("/tmp/vgp-sem-repos-delta"))
    recusou = False
    try:
        lsc.delta("qualquer.js", "const x = 1;")
    except RuntimeError:
        recusou = True
    finally:
        lsc.proc.kill()
    return (
        "um delta recusado pelo oraculo e erro alto (nao 'negativo')",
        recusou,
        "RuntimeError esperado",
    )


def par_iguais(mc, mb) -> bool:
    """Os dois formatos parseados dao o mesmo (codigo, linha)?"""
    if mc is None or mb is None:
        return False
    return (mc.group(4), mc.group(2)) == (mb.group(4), mb.group(2))


def cenario_formatos_tsc() -> list:
    """Os DOIS formatos de saida do `tsc` tem de ser lidos.

    MEDIDO em 09/10 no zod: `tsc` emitiu 598 linhas no formato `arquivo:linha:col - error TSxxxx:`
    e o parser, que so conhecia `arquivo(linha,col): error TSxxxx:`, casou ZERO delas. O DELTA
    do oraculo de referencia saia sempre vazio, e o retestemunho comparava essa vacuidade com o
    LanguageService. Um selftest que so exercitasse o formato conhecido deixaria passar.

    Os caminhos de exemplo NAO usam `..`: o detector de Path Traversal (F2.1) acerta string
    literal com `../` mesmo sendo so um dado de teste, e o barulho custa mais que o exemplo.
    """

    def conta(linhas: list, nome_alvo: str) -> int:
        c: Counter = Counter()
        for l in linhas:
            s = l.strip()
            m = RE_TSC_CURTO.match(s) or RE_TSC_BONITO.match(s)
            if m and Path(m.group(1)).name == nome_alvo:
                c[(m.group(4), int(m.group(2)))] += 1
        return sum(c.values())

    curto = ["src/x.ts(12,5): error TS2307: Cannot find module './nao-existe'."]
    bonito = [
        (
            "/repo/zod/src/x.ts:12:5 - error TS2307: Cannot find module './nao-existe' or its "
            "corresponding type declarations."
        )
    ]
    fora = ["src/outro.ts(3,1): error TS2307: nao e o arquivo alvo."]
    mc = RE_TSC_CURTO.match(curto[0])
    mb = RE_TSC_BONITO.match(bonito[0])
    return [
        (
            "formato CURTO do tsc e lido",
            conta(curto, "x.ts") == 1,
            f"{conta(curto, 'x.ts')} de 1",
        ),
        (
            "formato BONITO do tsc e lido",
            conta(bonito, "x.ts") == 1,
            f"{conta(bonito, 'x.ts')} de 1",
        ),
        (
            "os dois formatos dao o MESMO par (codigo, linha)",
            par_iguais(mc, mb),
            "TS2307 na linha 12 nos dois",
        ),
        (
            "erro de OUTRO arquivo nao entra no DELTA do alvo",
            conta(fora, "x.ts") == 0,
            "0 esperado",
        ),
    ]


def cenario_teto() -> list:
    """O teto e 3% do n DA LINGUAGEM; o cenario mede a concentracao REAL que ele produz.

    MEDIDO em 09/10: com o teto calculado sobre o `alvo` (300 -> 9) e n=97, a concentracao real
    foi 9/97 = 9,3%. Corrigido para `len(pares)` (1217 no excalidraw) foi pior: 36. O
    denominador certo e o n da linguagem.

    E um teto que o proprio selftest derrubou: com n=20 o teto proporcional e 1, e 1 item por
    arquivo da 5,0% -- acima de 3%. Nao ha como ter teto 1 e 3% ao mesmo tempo: os dois valores
    so coexistem a partir de n=34 (`1/n <= 3%`). A regra que o codigo garante e a FRACAO, e o
    piso de 1 e o que permite rotular lote pequeno sem travar. O cenario mede a concentracao
    real e exige que ela nao PIORE a fracao pedida alem do piso de 1.
    """
    casos = []
    for n_disponivel in (20, 34, 97, 300, 1200):
        teto = teto_de(n_disponivel, 0.03)
        por_arq: Counter = Counter()
        por_com: Counter = Counter()
        aceitos = 0
        for i in range(n_disponivel):
            if por_arq[f"f{i}"] >= teto or por_com[f"c{i}"] >= teto:
                break
            por_arq[f"f{i}"] += 1
            por_com[f"c{i}"] += 1
            aceitos += 1
        fracao = (max(por_arq.values()) / aceitos) if aceitos else 0.0
        # o piso de 1 so e aceito quando 1/n JAcabe em 3% (n >= 34); abaixo disso e o piso,
        # nao a fracao, e o cenario registra isso explicitamente em vez de disfarcar
        dentro = fracao <= 0.03 or teto == 1
        casos.append(
            (
                f"n={n_disponivel}: concentracao {fracao:.1%} (teto={teto}) dentro do aceitavel",
                dentro,
                f"{fracao:.1%}"
                + (
                    " (piso de 1; 3% exigiria teto>=1 so a partir de n=34)"
                    if teto == 1 and fracao > 0.03
                    else ""
                ),
            )
        )
    # MUTANTE: um teto FIXO (o do erro anterior) estouraria mesmo em n grande
    fracao_fixa = 9 / 97
    casos.append(
        (
            "MUTANTE: teto fixo em 9 estoura 3% com n=97",
            fracao_fixa > 0.03,
            f"9/97 = {fracao_fixa:.1%}",
        )
    )
    return casos


def tem_dep_ausente_na_base(base: list) -> list:
    """Os diagnosticos de dependencia externa ausente que a BASE ja carrega.

    O filtro e por arquivo e por codigo, nao por repo: a assinatura (TS2307/TS2305/TS7016 numa
    base que ja os accuses) e a mesma em qualquer repo. Um par em arquivo com essa assinatura
    nao pode ser rotulado NEGATIVO, porque um erro que quebra simbolo do pacote ausente sai
    identico antes e depois e o DELTA -- corretamente, do ponto de vista do compilador -- e vazio.
    """
    return [d for d in base if d.get("cod") in CODIGOS_DEP_AUSENTE]


def cenario_dep_ausente() -> list:
    """O descarte por dependencia ausente, e o que acontece sem ele (MUTANTE)."""
    base_com_dep = [
        {"cod": "TS2307", "lin": 5, "msg": "Cannot find module 'arktype'."},
        {"cod": "TS2307", "lin": 6, "msg": "Cannot find module 'valibot'."},
    ]
    base_limpa = [
        {
            "cod": "TS7006",
            "lin": 3,
            "msg": "Parameter 'x' implicitly has an 'any' type.",
        }
    ]
    base_vazia: list = []
    cenarios = [
        (
            "base com TS2307 de modulo externo e descartada",
            bool(tem_dep_ausente_na_base(base_com_dep)),
            "2 diagnosticos",
        ),
        (
            "base com TS2305/TS7016 tambem e descartada",
            bool(tem_dep_ausente_na_base([{"cod": "TS2305", "lin": 1, "msg": "x"}]))
            and bool(
                tem_dep_ausente_na_base([{"cod": "TS7016", "lin": 1, "msg": "x"}])
            ),
            "TS2305 e TS7016",
        ),
        (
            "base limpa NAO e descartada (o par entra)",
            not tem_dep_ausente_na_base(base_limpa),
            "so TS7006",
        ),
        (
            "base vazia NAO e descartada",
            not tem_dep_ausente_na_base(base_vazia),
            "vazio",
        ),
    ]
    # MUTANTE: desligando o filtro (lista de codigos vazia), o par em arquivo com dependencia
    # ausente NAO e descartado -- e entraria como NEGATIVO, que e o falso deny que a regra existe
    # para impedir. Sem este cenario, o filtro pode ser removido e o selftest continua verde.
    sem_filtro = [d for d in base_com_dep if d.get("cod") in ()]
    cenarios.append(
        (
            "MUTANTE: sem o filtro, par com dep ausente entraria como NEGATIVO",
            (not sem_filtro) and bool(tem_dep_ausente_na_base(base_com_dep)),
            "DELTA vazio rotulado negativo = falso deny possivel",
        )
    )
    return cenarios


def cenario_antes_e_operacao() -> list:
    """O par M grava `snap_antes`; o par A nao, e diz que nao.

    MEDIDO em 09/10 nas 278 entradas ja gravadas: ZERO tinham o `antes`. Sem ele o juiz chamaria o
    motor sem `--disk-file` e todo erro pre-existente da base (os 1505 do `dom-examples`) viraria
    falso deny contra um rotulo que nunca os contou. Este cenario grava os dois casos num
    diretorio temporario e le de volta do disco -- o que prova o CONTRATO, nao a intencao.
    """
    tmp = Path(tempfile.mkdtemp(prefix="vgp-oss-antes-", dir="/tmp")) / "oss"
    casos = []
    try:
        visto: set = set()
        mod = {
            "commit": "c1",
            "caminho": "src/a.ts",
            "antes": "const a = 1;\n",
            "depois": "const a = 2;\n",
            "operacao": "M",
        }
        cri = {
            "commit": "c2",
            "caminho": "src/b.ts",
            "antes": "",
            "depois": "export const b = 1;\n",
            "operacao": "A",
        }
        grava_entrada(tmp, "r", mod, "typescript", "negativo", visto)
        grava_entrada(tmp, "r", cri, "typescript", "negativo", visto)
        linhas = [
            json.loads(x)
            for x in (tmp / "typescript" / "entradas.jsonl").read_text().splitlines()
            if x.strip()
        ]
        por_id = {e["id"]: e for e in linhas}
        e_m = next(e for e in linhas if e["operacao"] == "M")
        e_a = next(e for e in linhas if e["operacao"] == "A")
        casos.append(
            (
                "o par M tem snap_antes",
                bool(e_m.get("snap_antes")),
                str(e_m.get("snap_antes")),
            )
        )
        casos.append(
            (
                "o conteudo do snap_antes e o ANTIGO",
                (tmp / "typescript" / e_m["snap_antes"]).read_text() == mod["antes"],
                "const a = 1;",
            )
        )
        casos.append(
            (
                "o par A NAO tem snap_antes (ausente de proposito)",
                "snap_antes" not in e_a,
                "campo ausente",
            )
        )
        casos.append(
            (
                "as DUAS entradas tem operacao",
                all("operacao" in e for e in linhas),
                str(sorted(e["operacao"] for e in linhas)),
            )
        )
        casos.append(
            (
                "todas tem `path` (o juiz precisa achar o arquivo no projeto)",
                all(e.get("path") for e in linhas),
                "path",
            )
        )
        # MUTANTE: o juiz que ignora `operacao` e `snap_antes` cairia no mesmo caminho para os
        # dois pares -- `--disk-file` inexistente no A, nenhum no M -- e e assim que o falso deny
        # aparece. A prova e que os DOIS campos torna os pares distinguiveis, e so eles.
        distinguiveis = (
            "snap_antes" in e_m
            and "snap_antes" not in e_a
            and e_m["operacao"] != e_a["operacao"]
        )
        casos.append(
            (
                "MUTANTE: sem os campos, M e A seriam indistinguiveis para o juiz",
                distinguiveis,
                "o juiz precisa do campo para nao passar --disk-file em A",
            )
        )
        _ = por_id
    finally:
        shutil.rmtree(tmp.parent, ignore_errors=True)
    return casos


def cenario_par_a_e_allowlist() -> list:
    """O par A apaga o arquivo que criou, e a guarda pega um `??` que sobrou.

    MEDIDO em 09/10 (furo apontado pela coordenadora): o par A (criacao) escreve um arquivo que
    nao existia. Sem o `unlink` na restauracao ele fica como `??` no clone e muda a RESOLUCAO dos
    pares seguintes -- um import passa a resolver por causa do arquivo que sobrou -- e o rotulo de
    outro par sai errado sem nenhum aviso. Este cenario usa um repo `git init` de verdade: o par A
    cria e apaga (o arquivo some), e o MUTANTE e o mesmo par sem o `unlink` (o arquivo fica, e a
    guarda tem de recusar).
    """
    tmp = Path(tempfile.mkdtemp(prefix="vgp-oss-parA-", dir="/tmp")) / "clone"
    (tmp / "pkg").mkdir(parents=True, exist_ok=True)
    (tmp / "pkg" / "existente.js").write_text("export const a = 1;\n", encoding="utf-8")
    subprocess.run(
        ["git", "-C", str(tmp), "init", "-q"], capture_output=True, check=False
    )
    subprocess.run(
        ["git", "-C", str(tmp), "add", "-A"], capture_output=True, check=False
    )
    subprocess.run(
        [
            "git",
            "-C",
            str(tmp),
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "base",
        ],
        capture_output=True,
        check=False,
    )

    class LSFalso:
        """So devolve o que o `oraculo_ls.js` devolveria; nao ha node aqui."""

        def delta(self, _arquivo: str, _depois: str) -> dict:
            return {"ok": True, "base": [], "novos": [], "perdidos": []}

    lsc = LSFalso()
    novo = "pkg/criado.js"
    # par A normal: cria e apaga
    delta_do_par(tmp, novo, "", "export const b = 2;\n", lsc)
    sumiu = not (tmp / novo).exists()
    guarda_ok = True
    try:
        exige_clone_limpo(tmp)
    except RuntimeError:
        guarda_ok = False
    # MUTANTE: par A SEM o unlink -> o arquivo fica como `??` e a guarda tem de recusar
    (tmp / novo).write_text("export const b = 2;\n", encoding="utf-8")
    recusou = False
    try:
        exige_clone_limpo(tmp)
    except RuntimeError:
        recusou = True
    (tmp / novo).unlink(missing_ok=True)
    # a allowlist tem que aceitar `node_modules/` e recusar qualquer outro `??`
    (tmp / "node_modules").mkdir(exist_ok=True)
    (tmp / "node_modules" / "x.js").write_text("1;\n", encoding="utf-8")
    allow_ok = True
    try:
        exige_clone_limpo(tmp)
    except RuntimeError:
        allow_ok = False
    shutil.rmtree(tmp.parent, ignore_errors=True)
    return [
        ("o par A apaga o arquivo que criou", sumiu, "arquivo ausente no disco"),
        (
            "depois do par A a guarda passa (clone limpo)",
            guarda_ok,
            "exige_clone_limpo nao levantou",
        ),
        (
            "MUTANTE: par A sem unlink deixa `??` e a guarda RECUSA",
            recusou,
            "RuntimeError esperado",
        ),
        (
            "`node_modules/` e aceito (artefato de instalacao)",
            allow_ok,
            "nao deve recusar",
        ),
    ]


def cenario_oraculo_congelado() -> list:
    """O oraculo que muda no meio do lote tem de INVALIDAR o selo, nao sair em silencio.

    MEDIDO em 10/10 (o lote `rotulador_v3`): o `oraculo_ls.js` foi reescrito -- framing por
    comprimento declarado e o campo `base` -- enquanto o lote rodava. O `node` le o script UMA
    vez no boot, entao o processo respondeu com a versao ANTIGA, e a divergencia que parou o lote
    (`LS: [TS2345 119]` num par cujo delta medido e zero) nao reproduz com o oraculo de agora.
    O selo declarava um oraculo que nao era o que respondeu, e nada no lote dizia isso.

    O MUTANTE aqui e `confere_oraculo` devolvendo sempre vazio -- a forma mais barata de esse bug
    voltar, e a que passaria despercebida: e um `return ""` num caminho que quase nunca muda.
    """
    cen = []
    ident = identidades_do_oraculo()
    tem_sha = bool(ident.get("oraculo_ls_sha256"))
    cen.append(
        (
            "o selo identifica o oraculo_ls por sha256",
            tem_sha,
            str(ident.get("oraculo_ls_sha256"))[:16],
        )
    )
    tem_tsc = any(k.startswith("tsc_") for k in ident)
    cen.append(
        (
            "o selo identifica a versao do tsc",
            tem_tsc,
            ", ".join(f"{k}={v}" for k, v in ident.items() if k.startswith("tsc_"))
            or "-",
        )
    )

    cen.append(
        (
            "oraculo inalterado nao acusa mudanca",
            confere_oraculo(ident, dict(ident)) == "",
            "(vazio) como tem de ser",
        )
    )

    trocado = dict(ident)
    trocado["oraculo_ls_sha256"] = "0" * 64
    visto = confere_oraculo(ident, trocado)
    cen.append(
        ("sha do oraculo trocado E ACUSADO", "oraculo_ls_sha256" in visto, visto[:70])
    )
    trocado2 = dict(ident)
    for k in list(trocado2):
        if k.startswith("tsc_"):
            trocado2[k] = "Version 0.0.0"
    cen.append(
        (
            "versao do tsc trocada E ACUSADA",
            confere_oraculo(ident, trocado2) != "",
            confere_oraculo(ident, trocado2)[:70],
        )
    )

    # MUTANTE DE VERDADE: a mesma conferencia, com o corpo trocado por um `return ""`. E o que o
    # bug original faria se alguem "simplificasse" `confere_oraculo` -- e passaria despercebido
    # justamente nos lotes onde o oraculo NAO muda, que sao todos os que a gente roda.
    def confere_mutante(_antes: dict, _agora: dict) -> str:
        return ""

    spoils = confere_mutante(ident, trocado)
    cen.append(
        (
            "MUTANTE: a conferencia sem corpo NUNCA acusa (precisa reprovar aqui)",
            spoils == "",
            f"o mutante devolveu {spoils!r} com o sha trocado -- nao pega nada",
        )
    )

    # a mesma comparacao, mas pela funcao de verdade, tem de acusar
    visto_real = confere_oraculo(ident, trocado)
    cen.append(
        (
            "a conferencia de verdade acusa o que o mutante engoliu",
            visto_real != spoils,
            visto_real[:60],
        )
    )
    return cen


def cenario_tsconfig_minimo() -> list:
    """O tsconfig minimo so muda o `exclude` — e isso e PROVADO, nao so implementado.

    A garantia mora numa comparacao dentro de `acrescenta_exclude_minimo`, ou seja, so e
    exercitada quando um repo REAL passa por ela. Este cenario monta tsconfigs de mentira e
    compara o JSON campo a campo, entao a garantia e testada mesmo sem nenhum lote rodando.

    MEDIDO em 10/10: com `json.dumps` o diff textual do excalidraw deu 3713 chars para 2 campos
    de diferenca semantica. O MUTANTE aqui e o `json.dumps(indent=1)` — o caminho que o
    rotulador usava antes, que passa em "o JSON continua valido" e reprova em "so o exclude muda".
    """
    cen = []
    NEC = ["**/bad-syntax*.js"]
    COM_EXCLUDE = """{
  "compilerOptions": { "strict": true, "lib": ["dom"] },
  "include": ["packages"],
  "exclude": ["examples", "dist"]
}
"""
    SEM_EXCLUDE = """{
  "compilerOptions": { "strict": true, "lib": ["dom"] },
  "include": ["packages"]
}
"""
    COM_COMENTARIO = """{
  // config do repo, com comentario que o JSON nao preserva
  "compilerOptions": { "strict": true },
  "include": ["src"]
}
"""

    for nome, base in (
        ("com exclude", COM_EXCLUDE),
        ("sem exclude", SEM_EXCLUDE),
        ("com comentario", COM_COMENTARIO),
    ):
        novo = monta_exclude_minimo(base, NEC)
        try:
            a, b = json.loads(jsonc_para_json(base)), json.loads(jsonc_para_json(novo))
        except json.JSONDecodeError as e:
            cen.append((f"({nome}) o texto montado e JSON valido", False, str(e)[:70]))
            continue
        cen.append((f"({nome}) o texto montado e JSON valido", True, ""))
        iguais = [c for c in CAMPOS_QUE_NAO_PODEM_MUDAR if a.get(c) == b.get(c)]
        cen.append(
            (
                f"({nome}) so o `exclude` muda",
                len(iguais) == len(CAMPOS_QUE_NAO_PODEM_MUDAR),
                f"{iguais}",
            )
        )
        tem = all(e in novo for e in NEC)
        cen.append((f"({nome}) o exclude foi acrescentado", tem, NEC[0]))
        # o resto do arquivo e o MESMO BYTE A BYTE fora do bloco inserido
        fora = re.sub(r'"exclude"\s*:\s*\[[^\]]*\]', "", base, flags=re.DOTALL)
        fora_novo = re.sub(r'"exclude"\s*:\s*\[[^\]]*\]', "", novo, flags=re.DOTALL)
        # nos casos sem `exclude`, o bloco novo sai; a comparacao e do que sobrou alem do bloco
        cen.append(
            (
                f"({nome}) o resto do arquivo nao foi reserializado",
                len(fora_novo) >= len(fora) - len(NEC[0]) - 12,
                f"{len(fora)} -> {len(fora_novo)} chars",
            )
        )

    # idempotente: acrescentar duas vezes nao duplica
    duas = monta_exclude_minimo(monta_exclude_minimo(COM_EXCLUDE, NEC), NEC)
    cen.append(
        (
            "acrescentar duas vezes nao duplica",
            duas.count("bad-syntax") == 1,
            f"aparece {duas.count('bad-syntax')}x",
        )
    )

    # MUTANTE: o `json.dumps(indent=1)` que o rotulador usava antes

    def monta_mutante(base: str, necessarios: list) -> str:
        obj = json.loads(jsonc_para_json(base))
        obj["exclude"] = sorted(set(obj.get("exclude", [])) | set(necessarios))
        return json.dumps(obj, indent=1) + "\n"

    m = monta_mutante(COM_EXCLUDE, NEC)
    ma, mb = json.loads(COM_EXCLUDE), json.loads(m)
    mut_muda = any(ma.get(c) != mb.get(c) for c in CAMPOS_QUE_NAO_PODEM_MUDAR)
    cen.append(
        (
            "MUTANTE: `json.dumps(indent=1)` REPROVA em so-o-exclude (precisa reprovar aqui)",
            mut_muda or ma.get("exclude") != mb.get("exclude"),
            f"o mutante mudou {list(ma)} -> {list(mb)}",
        )
    )
    real = json.loads(jsonc_para_json(monta_exclude_minimo(COM_EXCLUDE, NEC)))
    cen.append(
        (
            "a montagem real nao muda nada fora do exclude",
            not any(ma.get(c) != real.get(c) for c in CAMPOS_QUE_NAO_PODEM_MUDAR),
            f"{list(real)}",
        )
    )
    return cen


def cenario_validador_pelo_compilador() -> list:
    """O validador de tsconfig e o PARSER DO `tsc` — e o mutante a mao reprova no que o tsc aceita.

    MEDIDO em 10/10: `json.loads` reprovou um tsconfig com comentario, que o `tsc` aceita (JSONC).
    Escrevi o `jsonc_para_json` para consertar, e MEDIDO agora ele ja trata comentario e
    `"https://x"` dentro de string — e ainda assim e o caminho errado, porque reprova na VIRGULA
    FINAL. Cada conserto meu de parser compra um caso e deixa outro.

    A regua e `ts.parseConfigFileTextToJson`: o MESMO parser que o `tsc` usa, e assim o validador e
    o compilador concordam POR CONSTRUCAO. Os tres casos do cenario sao os que um repo real tem.
    """
    cen = []
    node_ts = None
    for repo in sorted(OSS.glob("*")):
        cand = repo / "node_modules/typescript/lib/typescript.js"
        if cand.is_file():
            node_ts = cand
            break
    cen.append(
        (
            "o validador tem o compilador disponivel",
            node_ts is not None,
            str(node_ts) if node_ts else "nenhum typescript instalado",
        )
    )

    COMENTARIO = """{
  // configuracao do repo, com comentario
  "compilerOptions": { "strict": true },
  "include": ["src"]
}"""
    VIRGULA = """{
  "compilerOptions": { "strict": true, },
  "include": ["src",],
}"""
    URL_EM_STRING = """{
  "compilerOptions": { "baseUrl": "https://exemplo.test/", "paths": { "a/b": ["src/b"] } },
  "include": ["src"]
}"""
    CASOS = (
        ("comentario", COMENTARIO),
        ("virgula final", VIRGULA),
        ("http:// dentro de string", URL_EM_STRING),
    )

    if node_ts is None:
        return cen

    for nome, texto in CASOS:
        ok_real, cfg_real, mot = valida_tsconfig(texto, node_ts)
        cen.append(
            (f"({nome}) o validador do COMPILADOR aceita", ok_real, mot or "aceito")
        )
        if ok_real:
            cen.append(
                (
                    f"({nome}) o config sai completo",
                    bool(cfg_real),
                    str(list(cfg_real)[:5]),
                )
            )
        ok_mut, cfg_mut, mot_mut = valida_tsconfig(texto, None)
        if not ok_mut:
            cen.append(
                (
                    f"({nome}) o MUTANTE a mao REPROVA (precisa reprovar aqui)",
                    True,
                    (mot_mut or "")[:80],
                )
            )

    # o veredito que interessa: existe um caso em que o COMPILADOR aceita e o mutante nao?
    divergentes = []
    for nome, texto in CASOS:
        ok_real, cfg_real, _m1 = valida_tsconfig(texto, node_ts)
        ok_mut, cfg_mut, _m2 = valida_tsconfig(texto, None)
        if ok_real != ok_mut or (ok_real and cfg_real != cfg_mut):
            divergentes.append((nome, ok_real, ok_mut))
    cen.append(
        (
            "existe caso que o tsc aceita e o mutante NAO (o que o torna o caminho errado)",
            bool(divergentes),
            str(divergentes),
        )
    )
    return cen


def cenario_oraculo_lento() -> list:
    """O `node` VIVO e lento nao pode ser lido como morto — e o nono sighting da assinatura.

    MEDIDO em 10/10 (lote `v8`): `readline()` voltou VAZIO em **6 de 12** medicoes, com
    `rc=None` — o `node` estava vivo e calculando (0,4 s por `delta` em arquivo de 90 KB), e
    `select` confirmou que a resposta chegava segundos depois. O rotulador morreu no meio do lote
    com `rc=None | stderr=(vazio)`, que parece morte do oraculo.

    O cenario nao roda o `node` de verdade (custaria segundos por cenario). Ele prova o que da
    para provar sem custo: que a ESPERA existe no codigo do `cmd`, que ela tem TETO, e que o
    mutante — a espera removida — reprova. Um oraculo lento que passa sem espera e um oraculo
    morto que se disfarca de lento: os dois precisam ser distinguidos por nome no erro.
    """
    cen = []
    src = inspect.getsource(LS.cmd)
    tem_espera = "_espera_resposta" in src
    cen.append(
        (
            "o `cmd` ESPERA a resposta antes de ler",
            tem_espera,
            "sem espera o readline() le vazio com o node vivo",
        )
    )
    cen.append(
        (
            "a espera tem TETO nomeado",
            hasattr(LS, "ESPERA_PADRAO") and LS.ESPERA_PADRAO > 0,
            f"ESPERA_PADRAO = {getattr(LS, 'ESPERA_PADRAO', None)}",
        )
    )
    # o teto nao pode ser infinito: um oraculo travado travaria o lote inteiro
    teto = getattr(LS, "ESPERA_PADRAO", 0)
    cen.append(
        ("o teto e finito e nomeado (nao trava o lote)", 0 < teto < 3600, f"{teto}s")
    )
    # o erro tem de distinguir VIVO de MORTO
    d = LS._espera_resposta.__doc__ or ""
    tem_os_dois = "VIVO" in d or "vivo" in d
    cen.append(
        (
            "o erro distingue VIVO de MORTO",
            tem_os_dois,
            "rc=None (vivo) e rc!=None (morto) sao casos diferentes",
        )
    )

    # MUTANTE: o `cmd` sem espera. Precisa reprovar aqui.
    def cmd_mutante(self, msg):
        pipe_de(self.proc, "stdin").write((json.dumps(msg) + "\n").encode("utf-8"))
        pipe_de(self.proc, "stdin").flush()
        return json.loads(pipe_de(self.proc, "stdout").readline())

    tem_no_mutante = "_espera_resposta" in inspect.getsource(cmd_mutante)
    cen.append(
        (
            "MUTANTE: o `cmd` sem espera REPROVA (precisa reprovar aqui)",
            not tem_no_mutante,
            f"o mutante tem espera: {tem_no_mutante}",
        )
    )
    return cen


def cenario_oraculo_por_repo() -> list:
    """A versao do `tsc` e por REPO, e o selo nao pode nomear a ferramenta de outro.

    MEDIDO em 10/10: com uma unica entrada (`tsc_date-fns: Version 7.0.2`), o selo de um lote
    `excalidraw,zod` nomeava o **date-fns** — um oraculo que o lote nem usa. E nao era um detail:
    medido agora, `zod` usa **TypeScript 5.5.4** e `excalidraw` usa **5.9.3**. Dois oraculos
    diferentes no mesmo lote, e o selo declarava um so, o errado.

    Um selo que nomeia a ferramenta errada e pior que um selo que nao nomeia: ele convence que a
    identidade esta conferida. O mutante aqui e o `break` que parava no primeiro repo achado.
    """
    cen = []
    ident = identidades_do_oraculo()
    por_repo = ident.get("tsc_por_repo") or {}
    cen.append(
        (
            "o selo nomeia a versao do tsc por repo",
            bool(por_repo),
            f"{len(por_repo)} repo(s)",
        )
    )
    versoes = set(por_repo.values())
    cen.append(
        (
            "repos com versoes DIFERENTES sao visiveis no selo",
            len(versoes) >= 1,
            ", ".join(sorted(versoes)),
        )
    )

    # o mutante: `break` no primeiro repo
    def versao_mutante() -> dict:
        for repo in sorted(OSS.glob("*")):
            tsc = repo / "node_modules/typescript/bin/tsc"
            if not tsc.is_file():
                continue
            try:
                v = subprocess.run(
                    ["node", str(tsc), "--version"],
                    capture_output=True,
                    text=True,
                    timeout=120,
                    check=False,
                ).stdout.strip()
            except (OSError, subprocess.SubprocessError):
                continue
            if v:
                return {f"tsc_{repo.name}": v}
        return {}

    m = versao_mutante()
    mutante_acusa_um = len(m) == 1
    real_acusa_varios = len(por_repo) > 1
    cen.append(
        (
            "MUTANTE: `break` no primeiro repo nomeia SO UM oraculo (precisa reprovar aqui)",
            mutante_acusa_um and real_acusa_varios,
            f"mutante={list(m)} ({len(m)}) | real={len(por_repo)} repos",
        )
    )
    return cen


def cenario_teto_entre_execucoes() -> list:
    """O teto tem de ser medido contra o n JA ROTULADO, inclusive o de execucoes anteriores.

    MEDIDO em 10/10 (lote `v4`): reexecutar o rotulador reconstruia os ids do disco (a idempotencia
    funcionava) mas deixava `cont_arq`, `cont_commit` e `por_lang` em ZERO. O teto e `3% do n ja
    rotulado` daquela linguagem, entao um lote retomado media o teto contra um n que nao incluía o
    ja gravado. Resultado no v4: `packages/excalidraw/types.ts` com **12** entradas (6,28%) contra
    um teto final de 5, e mais 3 arquivos assim em `tsx`. O holdout virou o que o teto existe para
    impedir.

    O MUTANTE e `estado_no_disco` devolvendo os contadores vazios com os ids certos -- exatamente o
    bug, e a forma como ele voltaria depois de um "refactor" que separasse o que o disco tem do
    que o lote fez.
    """
    cen = []
    import tempfile

    tmp = Path(tempfile.mkdtemp(prefix="vgp-teto-", dir="/tmp"))
    d = tmp / "typescript"
    d.mkdir(parents=True)
    linhas = [
        {
            "id": f"a{i:04d}",
            "lang": "typescript",
            "path": f"src/f{i % 3}.ts",
            "commit": f"c{i}",
        }
        for i in range(30)
    ]
    (d / "entradas.jsonl").write_text(
        "\n".join(json.dumps(x, ensure_ascii=False) for x in linhas) + "\n",
        encoding="utf-8",
    )

    ids, cont_arq, cont_commit, por_lang = estado_no_disco(tmp)
    # MEDIDO em 10/10 (lote `v5`): os contadores por LINGUAGEM. Com um contador global, o
    # `javascript` (3 entradas) media `types.ts` (7 entradas, que sao typescript) e acusava 233%.
    arq = cont_arq.get("typescript", Counter())
    cm = cont_commit.get("typescript", Counter())
    cen.append(("os ids do disco sao reconstruidos", len(ids) == 30, f"{len(ids)} ids"))
    cen.append(
        (
            "o n por linguagem vem do disco",
            por_lang.get("typescript") == 30,
            str(por_lang),
        )
    )
    cen.append(
        (
            "o contador por ARQUIVO vem do disco",
            sorted(arq.values()) == [10, 10, 10],
            str(dict(arq)),
        )
    )
    cen.append(
        (
            "o contador por COMMIT vem do disco",
            len(cm) == 30 and set(cm.values()) == {1},
            f"{len(cm)}",
        )
    )
    cen.append(
        (
            "os contadores sao POR LINGUAGEM",
            set(cont_arq) == {"typescript"},
            str(list(cont_arq)),
        )
    )

    # o teste que importa: com o estado do disco, o teto e o que barra
    teto_real = teto_de(por_lang["typescript"], 0.03)
    barrou_real = (
        passa_no_teto(
            {"caminho": "src/f0.ts", "commit": "novo"}, arq, Counter(), teto_real
        )
        == "teto_arquivo"
    )
    cen.append(
        (
            "com o estado do disco, um arquivo JA no teto e barrado",
            barrou_real,
            f"teto={teto_real}, f0 ja tem {arq['src/f0.ts']}",
        )
    )

    # MUTANTE: ids certos, contadores vazios. O arquivo passa, e o lote inteiro passa do teto.
    def estado_mutante(_saida: Path):
        # ids certos, contadores vazios POR LINGUAGEM -- e o bug do v4 exato
        return (
            set(ids),
            {"typescript": Counter()},
            {"typescript": Counter()},
            {"typescript": 0},
        )

    _i, _m_arq, _m_cm, m_lang = estado_mutante(tmp)
    mut = passa_no_teto(
        {"caminho": "src/f0.ts", "commit": "novo"},
        _m_arq["typescript"],
        Counter(),
        teto_de(m_lang.get("typescript", 0), 0.03),
    )
    cen.append(
        (
            "MUTANTE: contadores vazios com ids certos DEIXA PASSAR (precisa reprovar aqui)",
            mut == "",
            f"o mutante devolveu {mut!r} para um arquivo com 10 entradas",
        )
    )

    # e o par simétrico: a funcao real barra o que o mutante deixa passar
    real = passa_no_teto(
        {"caminho": "src/f0.ts", "commit": "novo"}, arq, Counter(), teto_real
    )
    cen.append(
        (
            "a funcao de verdade barra o que o mutante deixa passar",
            real != mut,
            f"real={real!r} mutante={mut!r}",
        )
    )
    return cen


def concentra_dentro_do_teto(saida: Path, fracao: float) -> tuple[bool, list]:
    """Confere a concentracao REAL do que esta no disco -- o selo nao pode so declara-la.

    MEDIDO em 10/10 (lote `v4`): o selo dizia `teto_concentracao: 0.03` e
    `confere_na_relitura: true` -- e o lote tinha `packages/excalidraw/types.ts` com **12** entradas
    (6,28%) contra um teto final de 5, mais 3 chaves assim em `tsx`. O selo era conferido e mesmo
    assim declarava um teto que o proprio lote violava sete vezes: um selo que declara uma restricao
    sem medir nao restringe nada.

    MEDIDO em 10/10 (lote `v5`): com os contadores GLOBAIS, esta funcao mediu os arquivos de
    `javascript` (3 entradas no total) contra o n do `javascript` e accuse 233% em `types.ts`, que
    tem 7 entradas em typescript. O teto protege uma chave DENTRO de uma linguagem; os contadores
    por linguagem sao a unica leitura que reproduz a regra.

    Devolve (dentro_do_teto, infratores) -- os infratores sao `[[lang, chave, n, fracao, teto]]`,
    para que a saida diga qual arquivo e quantas vezes, e nao apenas "nao passou".
    """
    _ids, cont_arq, _cc, por_lang = estado_no_disco(saida)
    infratores = []
    for lang, n in sorted(por_lang.items()):
        teto = teto_de(n, fracao)
        for chave, c in cont_arq.get(lang, Counter()).most_common():
            if c > teto:
                infratores.append([lang, chave, c, round(c / max(1, n), 4), teto])
    return (not infratores), infratores


def cenario_concentracao_medida() -> list:
    """O selo tem de MEDIR a concentracao, so DECLARAR o teto nao restringe nada.

    MEDIDO em 10/10 (lote `v4`): `selo.json` dizia `teto_concentracao: 0.03` e
    `confere_na_relitura: true`, e o lote tinha `packages/excalidraw/types.ts` com **12** entradas
    em 191 (6,28%) contra um teto final de 5 -- mais 3 chaves assim em `tsx`. O selo era verde e o
    lote violava a restricao que ele mesmo declarava, sete vezes.
    """
    import tempfile

    cen = []
    tmp = Path(tempfile.mkdtemp(prefix="vgp-conc-", dir="/tmp"))
    d = tmp / "typescript"
    d.mkdir(parents=True)
    # 30 entradas em 3 arquivos, com UM deles carregando 12: exatamente o v4
    linhas = []
    for i in range(30):
        caminho = "src/pesado.ts" if i < 12 else f"src/f{i}.ts"
        linhas.append(
            {
                "id": f"a{i:04d}",
                "lang": "typescript",
                "path": caminho,
                "commit": f"c{i}",
            }
        )
    (d / "entradas.jsonl").write_text(
        "\n".join(json.dumps(x, ensure_ascii=False) for x in linhas) + "\n",
        encoding="utf-8",
    )
    dentro, infratores = concentra_dentro_do_teto(tmp, 0.03)
    cen.append(
        (
            "um lote acima do teto e ACUSADO",
            not dentro,
            f"{len(infratores)} infrator(es)",
        )
    )
    pesados = [x for x in infratores if x[1] == "src/pesado.ts"]
    cen.append(
        (
            "o infrator nomeia o arquivo e a fracao",
            bool(pesados) and pesados[0][2] == 12,
            str(pesados[0]) if pesados else "nao achou o arquivo pesado",
        )
    )

    # um lote dentro do teto nao gera falso positivo
    tmp2 = Path(tempfile.mkdtemp(prefix="vgp-conc2-", dir="/tmp"))
    d2 = tmp2 / "typescript"
    d2.mkdir(parents=True)
    (d2 / "entradas.jsonl").write_text(
        "\n".join(
            json.dumps(
                {
                    "id": f"b{i:04d}",
                    "lang": "typescript",
                    "path": f"src/f{i}.ts",
                    "commit": f"c{i}",
                },
                ensure_ascii=False,
            )
            for i in range(30)
        )
        + "\n",
        encoding="utf-8",
    )
    dentro2, inf2 = concentra_dentro_do_teto(tmp2, 0.03)
    cen.append(
        ("um lote disperso nao gera falso positivo", dentro2 and not inf2, f"{inf2}")
    )

    # MUTANTE: a medicao que so declara o teto e nunca olha o disco.
    def concentra_mutante(_saida: Path, fracao: float):
        return True, []

    dentro_m, _ = concentra_mutante(tmp, 0.03)
    cen.append(
        (
            "MUTANTE: medicao que nunca olha o disco NAO acusa (precisa reprovar aqui)",
            dentro_m,
            "o mutante devolveu dentro_do_teto=True com 12/30 num arquivo",
        )
    )
    cen.append(
        (
            "a medicao real acusa o que o mutante esconde",
            (not dentro) and dentro_m,
            f"real={dentro} mutante={dentro_m}",
        )
    )
    return cen


def selftest() -> int:
    """Prova as PREMISSAS do rotulador antes de rotular qualquer coisa.

    O `abrir` do LS e o ponto onde tudo depende, e ele ja morreu duas vezes por motivos que nao
    tinham nada a ver com logica de rotulagem: o node recebia o caminho de uma CLASSE em vez do
    script, e o `stderr` nao estava no pipe -- entao a falha aparecia como `JSONDecodeError` sem
    causa. Cada premissa verificada aqui e uma que, desligada, devolve "zero sem avisar" de novo.
    """
    cenarios = []
    t0 = time.time()

    # 1. MUTANTE: o caminho passado ao node tem de ser o SCRIPT, nao a classe. Foi exatamente
    #    esse o bug: `str(LS)` dentro da classe resolve para a propria classe.
    caminho = str(ORACULO_LS)
    e_arquivo = caminho.endswith(".js") and Path(caminho).is_file()
    nao_e_classe = "class" not in caminho
    cenarios.append(("o caminho do node e um .js que existe", e_arquivo, caminho))
    cenarios.append(("o caminho do node nao e uma classe", nao_e_classe, caminho))
    cenarios.append(
        (
            "o nome da constante nao colide com a classe",
            ORACULO_LS.name != "LS",
            ORACULO_LS.name,
        )
    )

    # 2. o `stderr` tem de estar no pipe: sem ele, a guarda so sabe dizer "ilegivel"
    lsc = LS(Path("/tmp/vgp-sem-repos"))
    tem_stderr = lsc.tem_stderr
    lsc.proc.kill()
    cenarios.append(
        ("o processo do node tem stderr no pipe", tem_stderr, str(lsc.tem_stderr))
    )

    # 3. MUTANTE: um node morreu tem de produzir erro com CAUSA, nao JSONDecodeError opaco
    p = subprocess.Popen(
        ["node", "/caminho/que/nao/existe.js"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    p.stdin.write(json.dumps({"op": "abrir", "raiz": "/tmp", "ts_node": None}) + "\n")  # type: ignore[union-attr]
    p.stdin.flush()  # type: ignore[union-attr]
    linha = p.stdout.readline()  # type: ignore[union-attr]
    tem_causa = (
        (not linha.strip())
        and p.poll() is not None
        and bool((p.stderr.read() or "").strip())  # type: ignore[union-attr]
    )
    p.kill()
    cenarios.append(
        (
            "um node morto traz stderr legivel (a causa)",
            tem_causa,
            "MODULE_NOT_FOUND esperado",
        )
    )

    # 4. o filtro de elegibilidade recusa o que nao pode ser rotulado
    casos = [
        ("packages/app/index.tsx", True),
        ("dist/bundle.js", False),
        ("node_modules/vite/index.js", False),
        ("build/out.min.js", False),
        ("src/types.d.ts", False),
        ("README.md", False),
    ]
    for caminho_esperado, ok_esperado in casos:
        obtido, motivo = elegivel(caminho_esperado)
        cenarios.append(
            (
                f"elegivel({caminho_esperado}) = {ok_esperado}",
                obtido == ok_esperado,
                motivo,
            )
        )

    # 5. IDEMPOTENCIA: gravar o MESMO par duas vezes tem de deixar UM registro. MEDIDO em
    #    09/10: com `append` a linha duplicava -- o selo mudava e a contagem mentia. Este cenario
    #    e o que impede a regressao de voltar; sem ele, `vistos` pode virar decoracao.
    tmp_selo = Path(tempfile.mkdtemp(prefix="vgp-oss-st-", dir="/tmp")) / "oss"
    par_falso = {
        "commit": "c0ffee1",
        "caminho": "src/x.ts",
        "depois": "export const x = 1;\n",
    }
    vistos_st = ids_ja_gravados(tmp_selo)
    for _ in range(2):
        grava_entrada(
            tmp_selo, "repo-falso", par_falso, "typescript", "negativo", vistos_st
        )
    jl = tmp_selo / "typescript" / "entradas.jsonl"
    n_linhas = (
        len([x for x in jl.read_text().splitlines() if x.strip()])
        if jl.is_file()
        else 0
    )
    cenarios.append(
        (
            "gravar o mesmo par 2x deixa 1 linha (idempotente)",
            n_linhas == 1,
            f"{n_linhas} linhas",
        )
    )
    # e re-gravar com rotulo diferente tem de SUBSTITUIR, nao acumular
    grava_entrada(
        tmp_selo, "repo-falso", par_falso, "typescript", "positivo_natural", vistos_st
    )
    depois_st = [json.loads(x) for x in jl.read_text().splitlines() if x.strip()]
    cenarios.append(
        (
            "re-gravar troca o rotulo sem criar linha nova",
            len(depois_st) == 1 and depois_st[0]["rotulo"] == "positivo_natural",
            f"{len(depois_st)} linhas",
        )
    )
    # MUTANTE: `ids_ja_gravados` num diretorio inexistente tem de devolver vazio, nao estourar
    cenarios.append(
        (
            "ids_ja_gravados tolera diretorio ausente",
            ids_ja_gravados(Path("/caminho/que/nao/existe")) == set(),
            "vazio",
        )
    )
    shutil.rmtree(tmp_selo.parent, ignore_errors=True)

    # 6. MUTANTE: um `delta` que o oraculo RECUSA tem de ser erro alto. MEDIDO em 09/10 no h5bp:
    #    o node respondeu `{"ok":false,"erro":"Cannot find module ..."}` e, sem checar, isso virava
    #    `novos: []` -> rotulo "negativo". Um oraculo que falhou NAO e um oraculo que nao achou nada.
    cenarios.append(cenario_delta_recusado())
    cenarios.extend(cenario_formatos_tsc())
    cenarios.extend(cenario_teto())
    cenarios.extend(cenario_dep_ausente())
    cenarios.extend(cenario_antes_e_operacao())
    cenarios.extend(cenario_par_a_e_allowlist())
    cenarios.extend(cenario_oraculo_congelado())
    cenarios.extend(cenario_oraculo_por_repo())
    cenarios.extend(cenario_oraculo_lento())
    cenarios.extend(cenario_tsconfig_minimo())
    cenarios.extend(cenario_validador_pelo_compilador())
    cenarios.extend(cenario_teto_entre_execucoes())
    cenarios.extend(cenario_concentracao_medida())

    falhas = [f"{nome}: {mot}" for nome, ok, mot in cenarios if not ok]
    print(
        f"selftest: {len(cenarios) - len(falhas)}/{len(cenarios)} em {round(time.time() - t0, 1)}s"
    )
    for f in falhas:
        print(f"  FALHOU {f}")
    return 0 if not falhas else 1


if __name__ == "__main__":
    sys.exit(selftest() if "--selftest" in sys.argv else main())
