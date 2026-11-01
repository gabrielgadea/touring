"""Exclusao mutua para quem escreve nos clones OSS.

MEDIDO em 10/10, duas vezes e de formas diferentes:

1. O `rotulador_v3` parou no retestemunho com `LS: [TS2345 119]` e `tsc: []` num par cujo delta
   medido e ZERO -- os dois oraculos concordam. A diferenca real: o `oraculo_ls.js` foi reescrito
   (framing `#<n bytes>` + campo `base`) enquanto o lote rodava, e o `node` le o script UMA vez no
   boot, entao o processo respondeu com a versao antiga. Isso e um escritor trocado no meio.

2. O `rotulador_v5` parou em `linearElementEditor.ts` com tres `TS2554` a mais no LS. Reproduzi o
   par exato: os dois oraculos concordam EXATAMENTE (6 linhas antes, 6 depois, as mesmas), e o
   `TS2305` da linha 52 e acusado pelo `tsc` tambem. Causa: eu estava rodando os probes, que
   reescrevem o mesmo arquivo do clone, AO MESMO TEMPO que o rotulador. E o espelho: a prova de
   equivalencia JS devolveu `0 arquivo(s)` para express e lodash com o `tsc` respondendo `rc=1` e
   UMA linha — o `rotular_oss.py` estava escrevendo o `tsconfig.json` do clone enquanto a prova o
   lia.

Nenhum dos dois e um bug do oraculo. Os dois sao DOIS ESCRITORES no mesmo arquivo, e nenhum deles
diz que o outro esta la. «Nao roda junto» e disciplina, e disciplina nao sobrevive a sessao longa.

`O_CLOEXEC` e obrigatorio, e nao por cleanup: o `oraculo_ls.js` e um processo `node` de vida longa
DENTRO do rotulador, e um fd de lock herdado por ele continuaria segurando o exclusive apos o
rotulador morrer — o proximo escritor esperaria por um processo que ja saiu.

O lock e BLOQUEANTE e o dono e o `fd`, nao o `pid`: se o dono morrer, o kernel fecha o descritor e
libera, e o proximo entra. Nao ha timeout porque um timeout aqui seria exatamente o «zero sem
avisar»: o escritor mais lento seria acusa de nao ter escrito.
"""

from __future__ import annotations

import contextlib
import fcntl
import os
from collections.abc import Iterator
from pathlib import Path

LOCK = Path.home() / ".claude/touring/vgp/oss/.lock"


def adquire(caminho: Path | str | None = None) -> int:
    """Toma o lock exclusivo e BLOQUEANTE, e devolve o fd NAO INERDÁVEL.

    `O_CLOEXEC` e o ponto: o `node` do `oraculo_ls.js` roda por dentro do rotulador e nao pode
    herdar este descritor, ou o lock vive mais que o processo que o pediu.
    """
    alvo = Path(caminho) if caminho else LOCK
    alvo.parent.mkdir(parents=True, exist_ok=True)
    fd = os.open(str(alvo), os.O_CREAT | os.O_RDWR | os.O_CLOEXEC, 0o644)
    fcntl.flock(fd, fcntl.LOCK_EX)  # bloqueante: quem espera, espera
    return fd


def devolve(fd: int) -> None:
    """Libera e fecha. `finally` de quem acquisitions, nunca caminho feliz."""
    with contextlib.suppress(OSError):
        fcntl.flock(fd, fcntl.LOCK_UN)
    with contextlib.suppress(OSError):
        os.close(fd)


@contextlib.contextmanager
def escreve_exclusivo(caminho: Path | str | None = None) -> Iterator[None]:
    """`with escreve_exclusivo():` — todo escritor dos clones entra por aqui.

    TODO escritor usa ESTA funcao: o rotulador, a prova de equivalencia, o controle positivo e os
    probes. Um escritor que abre o arquivo sem o lock reintroduz a corrida que custou dois lotes.
    """
    fd = adquire(caminho)
    try:
        yield
    finally:
        devolve(fd)


def esta_travado(caminho: Path | str | None = None) -> bool:
    """Algum esta escrevendo AGORA? Teste nao-bloqueante, so para diagnostico.

    Nao use isto para decidir se pode escrever: a checagem e o `flock`, e entre as duas ha uma
    janela. Quem vai escrever usa `escreve_exclusivo`.
    """
    alvo = Path(caminho) if caminho else LOCK
    if not alvo.is_file():
        return False
    fd = os.open(str(alvo), os.O_RDWR | os.O_CLOEXEC)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        fcntl.flock(fd, fcntl.LOCK_UN)
        return False
    except BlockingIOError:
        return True
    finally:
        os.close(fd)