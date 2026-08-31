#!/usr/bin/env python3
"""O alfabeto de um predicado de caminho é propriedade do acervo, não da intuição.

Regra (30/08/2026, dois casos medidos no mesmo dia): o `_PARECE_CAMINHO` do
analise rejeitava `[ ]` e reconhecia 0 de 505 caminhos reais; os detectores da
taco-planning usavam `[a-z_0-9/]` e ou não viam (`_RE_FILE_LINE`: 0 match em
`crates/touring-ceg/…rs:530`) ou capturavam MUTILADO (`ceg/src/…`) — um caminho
bem-formado que não existe, que reprova a jusante um arquivo que está lá. O
modo de falha é silencioso por construção: o detector não erra, ele nunca vê.

Por isso este teste não usa exemplos — ele varre o UNIVERSO (`git ls-files`) e
exige que cada caminho real seja reconhecido INTEIRO (predicado positivo,
nunca "não levantou exceção"). Uma convenção nova de nome falha aqui no dia em
que entra no acervo, não meses depois.

Importa os predicados do espelho `client/` — que o guard de sync
(`test_sync_client_skills.py`) mantém byte-igual ao lado vivo.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
MIRROR = REPO / "client" / "skills" / "taco-planning" / "scripts"
sys.path.insert(0, str(MIRROR))

from confidence_tagger import _RE_FILE_LINE  # noqa: E402
from gap_detector import _RE_FILE_CITATION  # noqa: E402
from lib import _PATH_RE  # noqa: E402

EXTS_PATH = (".rs", ".py", ".ts", ".tsx", ".js", ".go", ".toml", ".yaml", ".json", ".md")
EXTS_CODE = (".rs", ".py", ".ts", ".tsx", ".js", ".go")

# O domínio do _PATH_RE é "caminho citável em prosa livre": nomes com espaço,
# ':', '(' etc. são inambiguáveis sem delimitador — limite do domínio, não do
# alfabeto. Esses nomes (títulos de fase virando arquivo) só existem sob docs/.
ALFABETO_CITAVEL = set("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_./-")


def _universo() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files"], capture_output=True, text=True, cwd=REPO, check=True
    )
    return out.stdout.splitlines()


def _citavel(p: str) -> bool:
    return set(p) <= ALFABETO_CITAVEL


def test_todo_caminho_citavel_do_acervo_e_reconhecido_inteiro():
    univ = [p for p in _universo() if p.endswith(EXTS_PATH)]
    assert len(univ) > 1000, "universo suspeito de vazio — o teste rodou fora do repo?"
    falhas = []
    for p in univ:
        if not _citavel(p):
            continue
        m = _PATH_RE.search(f"veja {p} aqui")
        if not m or m.group(0) != p:
            falhas.append((p, m.group(0) if m else None))
    assert not falhas, (
        f"_PATH_RE não reconhece INTEIROS {len(falhas)} caminho(s) reais do acervo "
        f"(primeiro: {falhas[0]}) — o alfabeto envelheceu contra a convenção; "
        "derive-o do acervo, nunca da intuição"
    )


def test_caminho_nao_citavel_so_existe_como_artefato_de_prosa_sob_docs():
    univ = [p for p in _universo() if p.endswith(EXTS_PATH)]
    fora_de_docs = [p for p in univ if not _citavel(p) and not p.startswith("docs/")]
    assert not fora_de_docs, (
        f"{len(fora_de_docs)} caminho(s) de CÓDIGO com nome fora do alfabeto citável "
        f"(primeiro: {fora_de_docs[0]!r}) — ou o nome é um erro, ou o alfabeto do "
        "_PATH_RE precisa crescer JUNTO com a convenção nova"
    )


def test_toda_citacao_de_codigo_e_reconhecida_com_e_sem_linha():
    univ = [p for p in _universo() if p.endswith(EXTS_CODE) and _citavel(p)]
    assert len(univ) > 500, "universo de código suspeito de vazio"
    falhas_cit, falhas_line = [], []
    for p in univ:
        m = _RE_FILE_CITATION.search(f"veja `{p}` aqui")
        if not m or m.group(1) != p:
            falhas_cit.append(p)
        if not _RE_FILE_LINE.search(f"veja `{p}:42` aqui"):
            falhas_line.append(p)
    assert not falhas_cit, f"_RE_FILE_CITATION mutila/perde {len(falhas_cit)}: {falhas_cit[:3]}"
    assert not falhas_line, f"_RE_FILE_LINE não vê {len(falhas_line)}: {falhas_line[:3]}"


def test_prosa_continua_sem_casar():
    for texto in ("isto é só prosa comum", "a versão 1.2 saiu ontem", "use e.g. 2>&1 sempre"):
        assert not _PATH_RE.search(texto), texto
        assert not _RE_FILE_CITATION.search(texto), texto


def test_nome_que_estende_um_fonte_vira_ausencia_nunca_outro_arquivo():
    """Classe medida pela peer (analise, 30/08): backup cujo prefixo é o
    arquivo vivo. No acervo daqui: `.cargo/config.toml.bak.p4` estende o
    `.cargo/config.toml` vivo (censo: 15 nomes com extensão coberta no meio,
    1 colisão). Citar um desses nomes tem de extrair NADA (ausência honesta)
    ou o nome INTEIRO — jamais um prefixo que nomeia outro objeto real."""
    import re as _re

    meio = _re.compile(r"\.(rs|py|ts|tsx|js|go|toml|yaml|json|md)\.[A-Za-z0-9]")
    extensores = [p for p in _universo() if meio.search(p)]
    assert extensores, "o censo tinha 15 — universo suspeito se zerou de repente"
    errados = []
    for p in extensores:
        m = _PATH_RE.search(f"veja {p} aqui")
        if m and m.group(0) != p:
            errados.append((p, m.group(0)))
    assert not errados, (
        f"captura de PREFIXO em nome extensor — nomeia o objeto errado: {errados[:3]}"
    )
