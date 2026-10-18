"""O executor de oraculo: uma porta so por onde passa TODO subprocesso de oraculo.

MEDIDO em 10/10, sete vezes, em sete camadas diferentes, com a MESMA assinatura — `rc=0` e nenhum
sinal produzido:

| # | ferramenta | como se manifestou |
|---|---|---|
| 1 | `cargo` | diagnostico no **stderr**; quem lia o stdout via «delta de 0» |
| 2 | `tsc` | **dois** formatos de saida; 598 erros do zod lidos como ZERO |
| 3 | LS (`delta`) | `ok` nao checado: oraculo RECUSADO lido como «nenhum erro novo» |
| 4 | protocolo do LS | resposta de 65.536 bytes truncada sem newline |
| 5 | denominadores do teto | teto medido contra o n errado, quatro vezes |
| 6 | `oraculo_ls.js` | trocado no meio do lote; 2 lotes parados por divergencia que nao reproduz |
| 7 | `parse_tsconfig` | `subprocess.run(input=...)` **sem newline**: o `node` saiu com rc=0 e stdout vazio |

Nenhum deles era o oraculo errado. Todos sao **um programa que reporta sucesso sem ter produzido
sinal**. E o defeito nao e de uma camada: e de qualquer subprocesso que alguem escrever amanha.
Por isso isto e uma FUNCAO, e nao uma regra — a regra eu quebraria na proxima urgencia.

O contrato por ferramenta diz ONDE o sinal tem de aparecer. Uma ferramenta cujo sinal vai no
`stderr` lida pelo `stdout` devolve `rc=0` com nada, e e exatamente o caso 1.

`vazio_valido` existe para o `tsc` de um projeto genuinely limpo: ali o sinal LEGITIMO e a
ausencia.-nesse caso a ausencia so e aceito com um **controle positivo da mesma sessao**, porque
«o `tsc` nao achou nada» e «o `tsc` nao rodou» produzem o mesmo byte — a distincao so existe com
uma prova de que o mesmo caminho responde quando ha algo para responder.
"""

from __future__ import annotations

import json
import re
import subprocess
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path

# ---------------------------------------------------------------------- contratos


@dataclass(frozen=True)
class Contrato:
    """ONDE o sinal de uma ferramenta aparece, e quando "vazio" e uma resposta legitima."""

    nome: str
    # o stream em que o sinal tem de aparecer: "stdout", "stderr", "ambos" ou "nenhum"
    stream: str = "ambos"
    # a ferramenta exige framing `#<n bytes>` na resposta?
    framing: bool = False
    # "vazio" (nenhum sinal no stream contratado) e uma resposta legitima?
    vazio_valido: bool = False
    # o que uma linha de sinal tem de conter para contar como sinal? (regex)
    sinal_re: str = ""


CONTRATOS: dict[str, Contrato] = {
    # MEDIDO em 09/10 (zod): o `cargo` escreve o diagnostico no stderr e o stdout fica VAZIO com
    # `--message-format=short`. Quem lia o stdout via "delta de 0".
    "cargo": Contrato("cargo", stream="stderr", sinal_re=r"error(\[[A-Z]?\d+\])?"),
    # MEDIDO em 10/10: o `tsc` tem DOIS formatos (`arquivo(l,c):` e `arquivo:l:c - `) e o
    # sinal pode estar em qualquer um dos dois streams.
    "tsc": Contrato("tsc", stream="ambos", sinal_re=r"error TS\d+"),
    # MEDIDO em 10/10: `subprocess.run(input=...)` nao acrescenta o newline, o `node` le por
    # linha, `data` nunca disparou, `rc=0` e stdout vazio.
    "node_ls": Contrato("node_ls", stream="stdout", framing=True, sinal_re=r"^\s*[{#]"),
    "node_parse": Contrato(
        "node_parse", stream="stdout", framing=True, sinal_re=r"^#\d+"
    ),
    # o `git show` e silencioso por design quando o blob nao existe: `rc!=0` e o sinal.
    "git_show": Contrato("git_show", stream="nenhum"),
    # `git status --porcelain` e vazio num repo limpo — e isso e o que o chamador quer.
    "git_status": Contrato("git_status", stream="stdout", vazio_valido=True),
    # MEDIDO em 10/10: `tsc --showConfig` num tsconfig que o compilador nao aceita responde com
    # `error TS18003: ...` no **stdout** e rc=1.
    "show_config": Contrato("show_config", stream="ambos", sinal_re=r"(error TS|\{)"),
}


class SinalVazio(RuntimeError):
    """`rc=0` sem sinal no stream contratado. O nome carrega a ferramenta e o stream."""


class RespostaMalformada(RuntimeError):
    """O processo respondeu, mas no framing declarado nao."""


# ---------------------------------------------------------------------- o executor


def extrai_sinal(contrato: Contrato, out: str, err: str) -> bool:
    """O sinal tem de aparecer no stream CONTRATADO, e no formato declarado.

    Extrair o contrato para uma funcao e o que mantem `chama_oraculo` legivel: aqui mora a
    pergunta ("este resultado tem sinal?"), la mora a politica ("o que fazemos sem sinal?").
    """
    contratado = {
        "stdout": [out],
        "stderr": [err],
        "ambos": [out, err],
        "nenhum": [],
    }[contrato.stream]
    if not any(s.strip() for s in contratado):
        return False
    if not contrato.sinal_re:
        return True
    return any(re.search(contrato.sinal_re, s, re.MULTILINE) for s in contratado)


def exige_sinal(
    ferramenta: str,
    contrato: Contrato,
    tem: bool,
    rc: int,
    controle_positivo: bool,
    out: str,
    err: str,
) -> None:
    """ERRO NOMEADO quando o sinal falta. A garantia (b) do executor.

    `vazio_valido` existe para o `tsc` de um projeto genuinely limpo: ali o sinal LEGITIMO e a
    ausencia. E mesmo assim so com um CONTROLE POSITIVO da mesma sessao, porque «o `tsc` nao achou
    nada» e «o `tsc` nao rodou» produzem o mesmo byte — a distincao so existe com uma prova de que
    o mesmo caminho responde quando ha algo para responder.
    """
    if tem:
        return
    if rc != 0:
        return  # o rc carries o sinal; quem chama decide
    if contrato.vazio_valido and controle_positivo:
        return
    if contrato.vazio_valido:
        raise SinalVazio(
            f"sinal vazio: {ferramenta} rc=0 sem saida no {contrato.stream}, e o contrato so "
            f"aceita vazio COM um controle positivo da mesma sessao"
        )
    raise SinalVazio(
        f"sinal vazio: {ferramenta} rc=0 sem saida no {contrato.stream} "
        f"({len(out)} bytes em stdout, {len(err)} em stderr)"
    )


def chama_oraculo(
    ferramenta: str,
    cmd: Sequence[str],
    *,
    entrada: str | None = None,
    cwd: Path | str | None = None,
    timeout: int = 1800,
    env: dict | None = None,
    controle_positivo: bool = False,
) -> dict:
    """Roda uma ferramenta de oraculo e **recusa** um resultado sem sinal.

    Devolve `{rc, stdout, stderr, sinal}` — `sinal` e o objeto do framing, quando a ferramenta
    declara framing.

    As tres garantias:

    (a) `entrada` ganha o `\n` final. MEDIDO em 10/10: `subprocess.run(input=...)` nao acrescenta
        o delimitador, o Node le por linha, `data` nunca dispara, o processo sai com `rc=0` e
        stdout vazio — um zero sem aviso, e desta vez dentro do validador que eu acabara de
        escrever. Uma ferramenta que le entrada por linha recebe a entrada TERMINADA, sempre.

    POR QUE O EXECUTOR NAO PRECISA DA ESPERA QUE A CLASSE `LS` PRECISA: `subprocess.run` espera o
    processo TERMINAR, entao a leitura do stdout so acontece depois que ha algo la. A classe `LS` e
    um `Popen` **persistente** (um `node` de vida longa, um comando por vez), e nesse caso o
    `readline()` em modo texto devolve `''` quando o buffer ainda nao tem linha — com o processo
    VIVO. MEDIDO em 10/10 (lote `v8`): 6 de 12 leituras vazias com `rc=None`, e a resposta chegava
    segundos depois. `run` espera o fim; `Popen` le enquanto o processo vive, e cada um precisa
    da garantia que o outro nao tem.

    (b) `rc=0` com o stream contratado vazio e ERRO NOMEADO (ver `exige_sinal`).
    (c) o framing `#<n bytes>` e conferido: o comprimento declarado e comparado com o recebido, e
        um comprimento maior vira `truncado: esperava N, veio M`, nunca lista vazia.
    """
    contrato = CONTRATOS.get(ferramenta)
    if contrato is None:
        raise ValueError(
            f"a ferramenta {ferramenta!r} nao tem contrato declarado. "
            f"Conhecidas: {sorted(CONTRATOS)}"
        )
    if entrada is not None and not entrada.endswith("\n"):
        entrada += "\n"

    p = subprocess.run(
        list(cmd),
        input=entrada,
        capture_output=True,
        text=True,
        timeout=timeout,
        check=False,
        cwd=cwd,
        env=env,
    )
    out, err = p.stdout or "", p.stderr or ""
    tem = extrai_sinal(contrato, out, err)
    exige_sinal(ferramenta, contrato, tem, p.returncode, controle_positivo, out, err)
    return {
        "rc": p.returncode,
        "stdout": out,
        "stderr": err,
        "sinal": ler_framing(ferramenta, out) if contrato.framing else None,
    }


def ler_framing(ferramenta: str, saida: str) -> dict:
    """Le a resposta `#<n bytes>\\n<corpo>\\n` e devolve o objeto JSON.

    MEDIDO em 10/10: com `readline()` a resposta truncava em **65.536 bytes** e o `json.loads`
    estourava «Unterminated string starting at line 1 column 65455» — lido como «o oraculo nao
    acusou nada». Aqui o comprimento e DECLARADO e conferido, e um comprimento maior que o enviado
    vira erro NOMEADO, nunca lista vazia.
    """
    linhas = saida.splitlines()
    if not linhas or not linhas[0].startswith("#"):
        raise RespostaMalformada(
            f"{ferramenta}: a resposta nao veio no framing '#<n bytes>' "
            f"(primeira linha: {linhas[0][:60]!r})"
        )
    try:
        n = int(linhas[0][1:].strip())
    except ValueError as e:
        raise RespostaMalformada(
            f"{ferramenta}: cabecalho malformado {linhas[0][:40]!r}"
        ) from e
    corpo = linhas[1] if len(linhas) > 1 else ""
    if len(corpo) != n:
        raise RespostaMalformada(
            f"truncado: esperava {n}, veio {len(corpo)} bytes de {ferramenta}"
        )
    try:
        return json.loads(corpo)
    except json.JSONDecodeError as e:
        raise RespostaMalformada(
            f"{ferramenta}: o corpo declarado ({n} bytes) nao e JSON: {e}"
        ) from e
