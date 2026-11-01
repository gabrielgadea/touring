"""HORUST-REAL — o oráculo é o `rustc`, lido pelo `cargo check --message-format=json`.

Uma RODADA aplica juntas várias injeções (no máximo 1 por arquivo) num pacote, roda um `cargo check` e atribui cada diagnóstico ao arquivo
que o produziu. Válido = o arquivo injetado tem EXATAMENTE 1 diagnóstico, do código esperado, na linha da mutação, nomeando o nome novo
(E0063: o tipo; até 60 linhas acima). Erro num arquivo sem injeção não tem dono: a rodada é partida ao meio até isolar o culpado.

O sinal do `cargo` em JSON mora no STDOUT e o `rc=0` sem a mensagem `build-finished` é erro nomeado, nunca «limpo» (contrato `cargo_json`
registrado no `chama_oraculo` sem editá-lo). Restauração por SHA dos bytes originais; nunca `git checkout`.
"""

from __future__ import annotations

import contextlib
import dataclasses
import hashlib
import json
import os
import re
import sys
from collections.abc import Callable, Iterator
from pathlib import Path

_SCRIPTS = Path(os.environ.get("HORUST_SCRIPTS_DIR") or Path(__file__).resolve().parents[2] / "scripts")
if str(_SCRIPTS) not in sys.path:
    sys.path.insert(0, str(_SCRIPTS))
import chama_oraculo  # noqa: E402

chama_oraculo.CONTRATOS["cargo_json"] = chama_oraculo.Contrato(
    "cargo_json", stream="stdout", sinal_re=r'"reason"\s*:\s*"build-finished"'
)
HWSLOT = "/home/gabrielgadea/projects/touring/scripts/hw-slot"
RC_MACHINE_CHECK = 70


class RestauracaoFalhou(RuntimeError):
    """O arquivo não era o esperado antes da injeção, ou não voltou ao sha original depois."""


class OraculoFalhou(RuntimeError):
    """O cargo terminou sem diagnóstico e sem sucesso: não é «limpo», é «não rodou»."""


@dataclasses.dataclass(frozen=True)
class Diag:
    arq: str
    linha: int
    coluna: int
    codigo: str
    msg: str


@dataclasses.dataclass(frozen=True)
class Saida:
    diags: list
    terminou: bool
    sucesso: bool | None
    ruins: int


@dataclasses.dataclass(frozen=True)
class Injecao:
    id: str
    arq: str  # relativo à raiz do worktree
    classe: str
    operador: str
    codigo: str
    linha: int
    nome: str  # o que a mensagem tem de nomear (o nome novo; E0063: o tipo)
    texto_original: str
    texto_novo: str
    pkg: str = ""
    aceitos: tuple = ()  # códigos que valem (vazio = só `codigo`)


def _norm(p: str) -> str:
    return p[2:] if p.startswith("./") else p


def parse_saida(stdout: str) -> Saida:
    """Os diagnósticos `error` com span primário (dedupe: o mesmo erro sai no alvo lib e no de teste), se o build terminou e se teve sucesso."""
    vistos, diags, ruins, terminou, sucesso = set(), [], 0, False, None
    for linha in stdout.splitlines():
        try:
            o = json.loads(linha)
        except json.JSONDecodeError:
            ruins += 1
            continue
        if not isinstance(o, dict):
            continue
        if o.get("reason") == "build-finished":
            terminou, sucesso = True, bool(o.get("success"))
        elif o.get("reason") == "compiler-message":
            m = o.get("message") or {}
            primarios = [s for s in m.get("spans", []) if s.get("is_primary")]
            if m.get("level") != "error" or not primarios:
                continue
            s = primarios[0]
            d = Diag(_norm(s["file_name"]), s["line_start"], s.get("column_start", 0), (m.get("code") or {}).get("code") or "", m.get("message", ""))
            if d not in vistos:
                vistos.add(d)
                diags.append(d)
    return Saida(diags, terminou, sucesso, ruins)


def _nomeia(msg: str, nome: str) -> bool:
    return re.search(r"(?<![\w])" + re.escape(nome) + r"(?![\w])", msg) is not None


def validar(inj: Injecao, diags_do_arquivo: list[Diag]) -> str:
    """'' se a injeção deu exatamente o erro esperado; senão o motivo do descarte."""
    if not diags_do_arquivo:
        return "sem_erro"
    if len(diags_do_arquivo) > 1:
        return "outro_erro_no_arquivo"
    d = diags_do_arquivo[0]
    if d.codigo not in (inj.aceitos or (inj.codigo,)):
        return "codigo_inesperado"
    if inj.codigo == "E0063":
        if not inj.linha - 60 <= d.linha <= inj.linha:
            return "linha_diferente"
    elif d.linha != inj.linha:
        return "linha_diferente"
    if not _nomeia(d.msg, inj.nome):
        return "nome_fora_da_mensagem"
    return ""


def atribuir(injecoes: list[Injecao], diags: list[Diag]) -> tuple[dict, list[Diag]]:
    """(id → motivo ou '', diagnósticos em arquivo SEM injeção na rodada)."""
    por_arq: dict[str, list[Diag]] = {}
    for d in diags:
        por_arq.setdefault(d.arq, []).append(d)
    donos = {i.arq for i in injecoes}
    return (
        {i.id: validar(i, por_arq.get(i.arq, [])) for i in injecoes},
        [d for d in diags if d.arq not in donos],
    )


def rodada(injecoes: list[Injecao], runner: Callable[[list[Injecao]], list[Diag]]) -> dict:
    """Roda o lote; se algum erro não tem dono, parte ao meio até isolar o culpado (uma injeção só que ainda cascateia é descartada)."""
    if not injecoes:
        return {}
    resultado, sem_dono = atribuir(injecoes, runner(injecoes))
    if not sem_dono:
        return resultado
    if len(injecoes) == 1:
        return {injecoes[0].id: "cascata_cruzada"}
    meio = len(injecoes) // 2
    return {**rodada(injecoes[:meio], runner), **rodada(injecoes[meio:], runner)}


def _sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


@contextlib.contextmanager
def aplicado(raiz: Path, injecoes: list[Injecao]) -> Iterator[None]:
    """Escreve os textos novos; ao sair, restaura os bytes originais e CONFERE o sha. Arquivo que não é o original esperado → recusa."""
    originais: dict[Path, bytes] = {}
    for i in injecoes:
        alvo = raiz / i.arq
        atual = alvo.read_bytes()
        if _sha(atual) != _sha(i.texto_original.encode("utf-8")):
            raise RestauracaoFalhou(f"{alvo} não é o original esperado (sha {_sha(atual)[:12]})")
        originais[alvo] = atual
    try:
        for i in injecoes:
            (raiz / i.arq).write_bytes(i.texto_novo.encode("utf-8"))
        yield
    finally:
        ruins = []
        for alvo, b in originais.items():
            alvo.write_bytes(b)
            if _sha(alvo.read_bytes()) != _sha(b):
                ruins.append(str(alvo))
        if ruins:
            raise RestauracaoFalhou(f"a restauração por sha FALHOU em {ruins}")


def montar_comando(pacotes: str | list[str], alvo: str, rotulo: str, via_slot: bool = True, offline: bool = False) -> list[str]:
    """`cargo check -p A -p B --lib|--bins` — pelo hw-slot (um por rodada: o slot é solto entre rodadas). Sem slot só no teste sintético."""
    lista = [pacotes] if isinstance(pacotes, str) else list(pacotes)
    cargo = ["cargo", "check", *[x for p in lista for x in ("-p", p)], f"--{alvo}", "--keep-going", "--message-format=json", "--locked", *(["--offline"] if offline else [])]
    if not via_slot:
        return cargo
    nome = lista[0] + (f"+{len(lista) - 1}" if len(lista) > 1 else "")
    return [HWSLOT, "--class", "rust", "--wait", "7200", "--label", f"{rotulo}-{nome}", "--", *cargo]


def cargo_check(
    raiz: Path, target: Path, pacote: str | list[str], alvo: str, rotulo: str = "horust-real", timeout: int = 7200, via_slot: bool = True, offline: bool = False
) -> Saida:
    cmd = montar_comando(pacote, alvo, rotulo, via_slot, offline)
    env = {**os.environ, "CARGO_TARGET_DIR": str(target)}
    for tentativa in (1, 2):
        r = chama_oraculo.chama_oraculo("cargo_json", cmd, cwd=raiz, timeout=timeout, env=env)
        if r["rc"] != RC_MACHINE_CHECK or tentativa == 2:
            break
    s = parse_saida(r["stdout"])
    if r["rc"] != 0 and not s.diags:
        raise OraculoFalhou(f"{pacote}/{alvo}: rc={r['rc']} sem diagnóstico: {r['stderr'][-300:]!r}")
    return s


def runner_cargo(raiz: Path, target: Path, alvo: str, via_slot: bool = True, offline: bool = False) -> Callable[[list[Injecao]], list[Diag]]:
    """O runner da rodada: aplica o lote, roda UM `cargo check` com os pacotes do lote (`Injecao.pkg`), restaura por sha."""

    def run(injs: list[Injecao]) -> list[Diag]:
        pacotes = sorted({i.pkg for i in injs})
        if not pacotes or "" in pacotes:
            raise ValueError("toda injeção do lote precisa do pacote (Injecao.pkg)")
        with aplicado(raiz, injs):
            return cargo_check(raiz, target, pacotes, alvo, via_slot=via_slot, offline=offline).diags

    return run
