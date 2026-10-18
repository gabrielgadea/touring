#!/usr/bin/env python3
"""Retention for the runtime journals under ~/.claude/touring, driven by a MANIFEST.

Origin (17/10/2026, ECC analysis, decision N6): 1.208 files / 290,8 MB of journals, the biggest
(`hook_trace.jsonl`, 191 MB in one file) read by a handful of programs nobody had listed, and
`memwatch.log` written with ZERO readers. Rotation by size or deleting by age could erase what a
KPI still needs, so retention is declared per journal in `docs/kpi/journals.toml` — path, writer,
every reader with the window it needs, retention, rollup — and this script only ever prunes what
that manifest proves nobody needs.

Laws it keeps:

* **Dry-run is the default.** `--apply` is explicit; pruning is the one irreversible step.
* **A reader that reads the whole file blocks pruning** (`janela_dias = "toda"`, also the default
  for a reader whose window nobody verified): fail-closed, and the dry-run names the blocker.
* **Roll up before you prune.** Expired lines are aggregated into `rollup/<id>.jsonl` (per UTC day
  and group: count and sums) and, with `expira = "arquivar"`, kept compressed in `arquivo/`.
  `--totais` answers a KPI from the rollup; the test suite proves it equals the raw computation.
* **Idempotent and crash-safe.** The rollup records a digest of every pruned batch; a retry after
  a crash between "rollup committed" and "raw swapped" skips the add instead of double counting.
* **Never prune the undatable.** A line whose timestamp cannot be read is kept (absent signal is
  not "old"); a journal with no datable line at all is reported `sem_data`.
* **A writer without a reader is named** (`escritor_sem_leitor`), and a file in the census
  directories that no journal claims is named (`sem_manifesto`).

Concurrency: the swap reads the journal up to its last complete line, re-reads whatever was
appended meanwhile and verifies the size before `os.replace`; a journal held open for WRITING by
another process (checked through /proc) is skipped. The residual window is microseconds between
the final size check and the rename: run `--apply` while the writers are quiet.

Days are UTC. Timestamps without an offset bucket by their literal date. Exit: 0 ok, 2 manifest
errors (apply refuses), 1 a journal failed during apply, 64 usage.

    python3 scripts/journal_gc.py                       # dry-run against ~/.claude/touring
    python3 scripts/journal_gc.py --verificar           # manifest vs code, nothing else
    python3 scripts/journal_gc.py --totais hook_trace --de 2026-09-01 --ate 2026-09-30
"""
from __future__ import annotations

import argparse
import datetime as dt
import gzip
import hashlib
import json
import os
import re
import shutil
import sys
from collections.abc import Callable
from pathlib import Path
from typing import Any

import tomllib

ROOT = Path(__file__).resolve().parent.parent
MANIFESTO = ROOT / "docs" / "kpi" / "journals.toml"
UTC = dt.timezone.utc

TIPOS = ("jsonl", "texto", "arquivos")
EXPIRA = ("arquivar", "apagar")
UNIDADES = {"epoch_s": 1, "epoch_ms": 1_000, "epoch_ns": 1_000_000_000}
OBRIGATORIOS = ("id", "caminho", "tipo", "retencao_dias", "expira", "escritores")
LOTES_LEMBRADOS = 200
TENTATIVAS_CAUDA = 3


class ManifestoInvalido(Exception):
    """The manifest has error-level problems; applying on top of it is refused."""


class _Abort(Exception):
    """A journal could not be swapped safely; the raw file is untouched."""


# --- timestamps -----------------------------------------------------------------------------------

def parse_dia(valor: Any, unidade: str) -> str | None:
    """The UTC day (YYYY-MM-DD) of a timestamp value, or None when it cannot be read."""
    if valor is None or isinstance(valor, bool):
        return None
    try:
        if unidade in UNIDADES:
            numero = float(valor)  # nan/inf raise ValueError/OverflowError below
            if numero < 0:
                return None
            return dt.datetime.fromtimestamp(numero / UNIDADES[unidade], UTC).date().isoformat()
        if unidade == "iso" and isinstance(valor, str):
            texto = valor.strip()
            data = dt.datetime.fromisoformat(texto)  # py>=3.11: accepts Z, +HHMM, space separator, date only
            return (data.astimezone(UTC) if data.tzinfo else data).date().isoformat()
    except (TypeError, ValueError, OverflowError, OSError):
        return None
    return None


_PREFIXO_DATA = re.compile(r"\[?(\d{4}-\d{2}-\d{2}(?:[T ]\d{2}:\d{2}:\d{2}(?:[.,]\d+)?(?:Z|[+-]\d{2}:?\d{2})?)?)")  # used with .match(): anchored


def dia_do_texto(linha: str) -> str | None:
    """The day of a text-log line that STARTS with a date/datetime; any other line is undatable."""
    achado = _PREFIXO_DATA.match(linha)
    return parse_dia(achado.group(1), "iso") if achado else None


# --- manifest -------------------------------------------------------------------------------------

def carregar(caminho: Path) -> dict[str, Any]:
    with open(caminho, "rb") as fh:
        m = tomllib.load(fh)
    m.setdefault("journal", [])
    m.setdefault("censo", {"diretorios": ["."], "extensoes": [".jsonl", ".log"]})
    return m


def _janela(leitor: dict[str, Any]) -> Any:
    return leitor.get("janela_dias", "toda")


def _bloqueio(j: dict[str, Any]) -> list[str]:
    return [leitor.get("arquivo", "?") for leitor in j.get("leitores") or [] if _janela(leitor) == "toda"]


def _janela_maxima(j: dict[str, Any]) -> int:
    return max((w for w in map(_janela, j.get("leitores") or []) if isinstance(w, int) and not isinstance(w, bool)), default=0)


def _problema(nivel: str, codigo: str, journal: str, msg: str) -> dict[str, str]:
    return {"nivel": nivel, "codigo": codigo, "journal": journal, "msg": msg}


def _token(j: dict[str, Any]) -> str:
    """What a writer/reader file must literally mention to count as touching this journal."""
    caminho = str(j.get("caminho", ""))
    return caminho.split("*")[0] if "*" in caminho else Path(caminho).name


def _checar_arquivo(repo: Path, arquivo: str, j: dict[str, Any], papel: str) -> list[dict[str, str]]:
    jid = str(j.get("id", "?"))
    if arquivo.startswith("ad-hoc:"):
        return [_problema("info", "escritor_ad_hoc", jid, f"{papel} sem arquivo: {arquivo[7:].strip()}")]
    externo = arquivo.startswith("~")
    destino = Path(arquivo).expanduser() if externo else repo / arquivo
    if not destino.is_file():
        if externo:
            return [_problema("info", "externo_nao_verificado", jid, f"{papel} {arquivo}: fora do repo e ausente nesta máquina; não verificável")]
        return [_problema("erro", "arquivo_inexistente", jid, f"{papel} {arquivo} não existe no repo")]
    try:
        texto = destino.read_text(encoding="utf-8", errors="replace")
    except OSError as exc:
        return [_problema("erro", "arquivo_inexistente", jid, f"{papel} {arquivo} ilegível: {exc}")]
    if _token(j) not in texto:
        return [_problema("erro", "arquivo_nao_cita_o_journal", jid, f"{papel} {arquivo} não menciona `{_token(j)}`: o manifesto envelheceu ou o arquivo errado foi listado")]
    return []


def _exigencias_de_ts(j: dict[str, Any]) -> list[dict[str, str]]:
    jid, ts, tipo = str(j.get("id", "?")), j.get("ts") or {}, j.get("tipo")
    if tipo == "jsonl" and not (ts.get("campo") and ts.get("unidade") in (*UNIDADES, "iso")):
        return [_problema("erro", "campo_ausente", jid, "tipo jsonl exige ts.campo e ts.unidade (epoch_s|epoch_ms|epoch_ns|iso)")]
    if tipo == "texto" and ts.get("formato") not in ("prefixo-data", "nenhum"):
        return [_problema("erro", "campo_ausente", jid, "tipo texto exige ts.formato (prefixo-data|nenhum)")]
    return []


def _campos_e_valores(j: dict[str, Any]) -> list[dict[str, str]]:
    jid = str(j.get("id", "?"))
    out = [_problema("erro", "campo_ausente", jid, f"campo obrigatório `{c}` ausente") for c in OBRIGATORIOS if c not in j]
    for campo, permitidos in (("tipo", TIPOS), ("expira", EXPIRA)):
        if j.get(campo) not in (*permitidos, None):
            out.append(_problema("erro", "valor_invalido", jid, f"{campo} `{j[campo]}` não está em {permitidos}"))
    retencao = j.get("retencao_dias")
    if "retencao_dias" in j and (not isinstance(retencao, int) or isinstance(retencao, bool) or retencao < 1):
        out.append(_problema("erro", "valor_invalido", jid, "retencao_dias precisa ser inteiro >= 1"))
    caminho = Path(str(j.get("caminho", "")))
    if caminho.is_absolute() or ".." in caminho.parts:
        out.append(_problema("erro", "caminho_fora_da_base", jid, f"`{j['caminho']}` sai da base do runtime"))
    return out + _exigencias_de_ts(j)


def _leitores_validos(j: dict[str, Any]) -> list[dict[str, str]]:
    """A reader window is "toda" or an int >= 0; anything else would silently constrain nothing."""
    jid, out = str(j.get("id", "?")), []
    for leitor in j.get("leitores") or []:
        w = _janela(leitor)
        if w != "toda" and not (isinstance(w, int) and not isinstance(w, bool) and w >= 0):
            out.append(_problema("erro", "valor_invalido", jid, f"leitor {leitor.get('arquivo', '?')}: janela_dias `{w}` precisa ser inteiro >= 0 ou \"toda\""))
    return out


def _regras_de_retencao(j: dict[str, Any]) -> list[dict[str, str]]:
    jid = str(j.get("id", "?"))
    out = []
    retencao = j.get("retencao_dias")
    if isinstance(retencao, int) and retencao < _janela_maxima(j):
        out.append(_problema("erro", "retencao_menor_que_janela", jid, f"retenção {retencao} d < janela {_janela_maxima(j)} d de um leitor: podaria o que ele lê"))
    if j.get("expira") == "apagar" and j.get("leitores") and not j.get("rollup"):
        out.append(_problema("erro", "poda_sem_rollup", jid, "apagar com leitores e sem rollup tira dado de quem lê: use expira=\"arquivar\" ou declare rollup"))
    if "escritores" in j and not j["escritores"]:
        out.append(_problema("erro", "sem_escritor", jid, "nenhum escritor declarado: journal órfão ou manifesto incompleto"))
    if j.get("escritores") and not j.get("leitores"):
        out.append(_problema("aviso", "escritor_sem_leitor", jid, "escritor sem leitor (REGRA #0): ligar a um consumidor ou aposentar o escritor"))
    return out


def _problemas_do_journal(j: dict[str, Any], repo: Path) -> list[dict[str, str]]:
    jid = str(j.get("id", "?"))
    out = _campos_e_valores(j) + _leitores_validos(j) + _regras_de_retencao(j)
    for arquivo in j.get("escritores") or []:
        out += _checar_arquivo(repo, arquivo, j, "escritor")
    for leitor in j.get("leitores") or []:
        out += _checar_arquivo(repo, str(leitor.get("arquivo", "?")), j, "leitor")
        if leitor.get("verificada") is not True:
            out.append(_problema("info", "janela_nao_verificada", jid, f"leitor {leitor.get('arquivo', '?')}: janela não verificada no código; tratada como `{_janela(leitor)}`"))
    return out


def _coberto(base: Path, alvo: Path, journals: list[dict[str, Any]]) -> bool:
    rel = alvo.relative_to(base).as_posix()
    return any(
        (Path(str(j.get("caminho", ""))).as_posix() == rel) or (j.get("tipo") == "arquivos" and alvo in set(base.glob(str(j.get("caminho", "")))))
        for j in journals
    )


def _censo(m: dict[str, Any], base: Path) -> list[dict[str, str]]:
    censo, journals = m.get("censo") or {}, m.get("journal") or []
    out = []
    for d in censo.get("diretorios") or ["."]:
        pasta = base / d
        if not pasta.is_dir():
            continue
        for f in sorted(p for p in pasta.iterdir() if p.is_file() and p.name.endswith(tuple(censo.get("extensoes") or []))):
            if not _coberto(base, f, journals):
                out.append(_problema("aviso", "sem_manifesto", "-", f"{f.relative_to(base).as_posix()} está no censo e nenhum journal o reivindica"))
    return out


def verificar(m: dict[str, Any], repo: Path, base: Path | None = None) -> list[dict[str, str]]:
    """Manifest vs itself and vs the code it names (and, with `base`, vs the files on disk)."""
    out, vistos = [], set()
    for j in m.get("journal") or []:
        jid = str(j.get("id", "?"))
        if jid in vistos:
            out.append(_problema("erro", "id_duplicado", jid, "id repetido no manifesto"))
        vistos.add(jid)
        out += _problemas_do_journal(j, repo)
    if base is not None and base.is_dir():
        out += _censo(m, base)
    return out


# --- scanning -------------------------------------------------------------------------------------

def _corte(agora: dt.datetime, retencao_dias: int) -> str:
    return (agora.astimezone(UTC).date() - dt.timedelta(days=retencao_dias)).isoformat()


def _classificador(j: dict[str, Any]) -> Callable[[bytes], tuple[str | None, Any]]:
    """line bytes -> (UTC day | None, parsed record | None)."""
    if j["tipo"] == "texto":
        if (j.get("ts") or {}).get("formato") == "nenhum":
            return lambda raw: (None, None)
        return lambda raw: (dia_do_texto(raw.decode("utf-8", "replace")), None)
    campo, unidade = j["ts"]["campo"], j["ts"]["unidade"]

    def classificar(raw: bytes) -> tuple[str | None, Any]:
        try:
            rec = json.loads(raw)
        except ValueError:
            return None, None
        return (parse_dia(rec.get(campo), unidade), rec) if isinstance(rec, dict) else (None, None)

    return classificar


def _linhas_completas(path: Path, limite: int):
    """Yield (raw, end_offset) for every COMPLETE line that ends at or before `limite`."""
    fim = 0
    with open(path, "rb") as fh:
        while fim < limite:
            raw = fh.readline()
            if not raw or not raw.endswith(b"\n") or fim + len(raw) > limite:
                return
            fim += len(raw)
            yield raw, fim


def _plano_base(j: dict[str, Any], retencao: int, agora: dt.datetime) -> dict[str, Any]:
    return {
        "id": j["id"], "tipo": j["tipo"], "caminho": j["caminho"], "acao": j["expira"], "retencao_dias": retencao,
        "corte": _corte(agora, retencao), "bloqueio": _bloqueio(j), "sem_leitor": not j.get("leitores"),
        "linhas": 0, "expiram": 0, "mantem": 0, "sem_data": 0, "bytes_total": 0, "bytes_expiram": 0, "estado": "ok",
    }


def _plano_arquivos(j: dict[str, Any], base: Path, agora: dt.datetime, plano: dict[str, Any]) -> dict[str, Any]:
    limite = (agora - dt.timedelta(days=plano["retencao_dias"])).timestamp()
    alvos = sorted(p for p in base.glob(j["caminho"]) if p.is_file())
    velhos = [p for p in alvos if p.stat().st_mtime < limite]
    plano.update(linhas=len(alvos), expiram=len(velhos), mantem=len(alvos) - len(velhos),
                 bytes_total=sum(p.stat().st_size for p in alvos), bytes_expiram=sum(p.stat().st_size for p in velhos))
    plano["estado"] = "ausente" if not alvos else "bloqueado" if plano["bloqueio"] else "ok"
    return plano


def _plano_log(j: dict[str, Any], base: Path, plano: dict[str, Any]) -> dict[str, Any]:
    path = base / j["caminho"]
    if not path.is_file():
        plano["estado"] = "ausente"
        return plano
    size = path.stat().st_size
    plano["bytes_total"] = size
    classificar = _classificador(j)
    for raw, _fim in _linhas_completas(path, size):
        plano["linhas"] += 1
        d = classificar(raw)[0]
        if d is None:
            plano["sem_data"] += 1
        elif d < plano["corte"]:
            plano["expiram"] += 1
            plano["bytes_expiram"] += len(raw)
        else:
            plano["mantem"] += 1
    datavel = plano["expiram"] + plano["mantem"]
    plano["estado"] = "bloqueado" if plano["bloqueio"] else "sem_data" if plano["linhas"] and not datavel else "ok"
    return plano


def planejar(m: dict[str, Any], base: Path, agora: dt.datetime) -> list[dict[str, Any]]:
    """Read-only: what each journal would lose. Never writes."""
    out = []
    for j in m.get("journal") or []:
        plano = _plano_base(j, j["retencao_dias"], agora)
        out.append(_plano_arquivos(j, base, agora, plano) if j["tipo"] == "arquivos" else _plano_log(j, base, plano))
    return out


# --- rollup ---------------------------------------------------------------------------------------

def caminho_rollup(base: Path, jid: str) -> Path:
    return base / "rollup" / f"{jid}.jsonl"


def _ler_rollup_completo(path: Path) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    meta: dict[str, Any] = {"batches": []}
    recs: list[dict[str, Any]] = []
    if path.is_file():
        for linha in path.read_text(encoding="utf-8").splitlines():
            reg = json.loads(linha)
            if "_meta" in reg:
                meta = {"batches": [], **reg["_meta"]}
            else:
                recs.append(reg)
    return meta, recs


def ler_rollup(path: Path) -> list[dict[str, Any]]:
    return _ler_rollup_completo(path)[1]


def _gravar_rollup(path: Path, jid: str, meta: dict[str, Any], recs: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    ordenados = sorted(recs, key=lambda r: (r["dia"], json.dumps(r["chave"], sort_keys=True)))
    corpo = [json.dumps({"_meta": {"journal": jid, "batches": meta["batches"][-LOTES_LEMBRADOS:]}}, sort_keys=True)]
    corpo += [json.dumps(r, sort_keys=True, ensure_ascii=False) for r in ordenados]
    tmp = path.with_name(path.name + ".gc-tmp")
    with open(tmp, "w", encoding="utf-8") as fh:
        fh.write("\n".join(corpo) + "\n")
        fh.flush()
        os.fsync(fh.fileno())
    os.replace(tmp, path)


def _numero(v: Any) -> bool:
    return isinstance(v, int | float) and not isinstance(v, bool)


def _acumular(agg: dict[tuple[str, str], dict[str, Any]], dia: str, rec: Any, cfg: dict[str, Any]) -> None:
    chave = {c: "" if not isinstance(rec, dict) or rec.get(c) is None else str(rec.get(c)) for c in cfg.get("agrupar_por") or []}
    celula = agg.setdefault((dia, json.dumps(chave, sort_keys=True)), {"dia": dia, "chave": chave, "n": 0, "soma": {}})
    celula["n"] += 1
    for campo in cfg.get("somar") or []:
        valor = rec.get(campo) if isinstance(rec, dict) else None
        if _numero(valor):
            celula["soma"][campo] = celula["soma"].get(campo, 0) + valor


def _fundir(recs: list[dict[str, Any]], agg: dict[tuple[str, str], dict[str, Any]]) -> list[dict[str, Any]]:
    por = {(r["dia"], json.dumps(r["chave"], sort_keys=True)): r for r in recs}
    for k, novo in agg.items():
        atual = por.get(k)
        if atual is None:
            por[k] = novo
            continue
        atual["n"] += novo["n"]
        for campo, v in novo["soma"].items():
            atual["soma"][campo] = atual["soma"].get(campo, 0) + v
    return list(por.values())


def totais(base: Path, jid: str, dia_ini: str, dia_fim: str, onde: dict[str, str] | None = None) -> dict[str, Any]:
    """Count and sums from the rollup for days in [dia_ini, dia_fim], optionally filtered by group value."""
    n, soma = 0, {}
    for r in ler_rollup(caminho_rollup(base, jid)):
        if not (dia_ini <= r["dia"] <= dia_fim) or any(r["chave"].get(k) != v for k, v in (onde or {}).items()):
            continue
        n += r["n"]
        for campo, v in r["soma"].items():
            soma[campo] = soma.get(campo, 0) + v
    return {"n": n, "soma": soma}


# --- apply ----------------------------------------------------------------------------------------

def _abertos_para_escrita(path: Path) -> list[tuple[int, str]]:
    """Other processes holding `path` open for writing (Linux /proc; elsewhere: unknown -> none)."""
    proc, alvo, eu, achados = Path("/proc"), os.path.realpath(path), os.getpid(), []
    if not proc.is_dir():
        return achados
    for pasta in proc.iterdir():
        if not pasta.name.isdigit() or int(pasta.name) == eu:
            continue
        try:
            for fd in (pasta / "fd").iterdir():
                if os.path.realpath(os.readlink(fd)) != alvo:
                    continue
                flags = next((ln.split()[1] for ln in (pasta / "fdinfo" / fd.name).read_text().splitlines() if ln.startswith("flags:")), "0")
                if int(flags, 8) & 3:
                    achados.append((int(pasta.name), (pasta / "comm").read_text().strip()))
                    break
        except OSError:
            continue
    return achados


class _Arquivador:
    """Expired lines -> one gzip temp per month, appended to `arquivo/<name>.<YYYY-MM>.gz` at commit."""

    def __init__(self, path: Path):
        self.pasta, self.nome, self.tmps = path.parent / "arquivo", path.name, {}

    def escrever(self, dia: str, raw: bytes) -> None:
        mes = dia[:7]
        if mes not in self.tmps:
            self.pasta.mkdir(parents=True, exist_ok=True)
            tmp = self.pasta / f".{self.nome}.{mes}.gc-tmp"
            # The handle must outlive this call: one stream per month, closed in fechar/descartar.
            self.tmps[mes] = (tmp, gzip.open(tmp, "wb"))  # noqa: SIM115
        self.tmps[mes][1].write(raw)

    def fechar(self) -> None:
        for _tmp, gz in self.tmps.values():
            gz.close()

    def efetivar(self) -> None:
        self.fechar()
        for mes, (tmp, _gz) in self.tmps.items():
            with open(self.pasta / f"{self.nome}.{mes}.gz", "ab") as destino, open(tmp, "rb") as origem:
                shutil.copyfileobj(origem, destino)
                destino.flush()
                os.fsync(destino.fileno())
        self.descartar()

    def descartar(self) -> None:
        for tmp, gz in self.tmps.values():
            gz.close()
            tmp.unlink(missing_ok=True)
        self.tmps = {}


def _fsync_dir(pasta: Path) -> None:
    fd = os.open(pasta, os.O_RDONLY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def _tem_ledger(j: dict[str, Any]) -> bool:
    return bool(j.get("rollup")) or j["expira"] == "arquivar"


def _varrer_e_separar(j, path, size0, corte, tmp, arq):
    """Stream the complete lines: kept ones to `tmp`, expired ones to the aggregate/archive/digest."""
    classificar, cfg = _classificador(j), j.get("rollup") or {}
    agg, digest, contagem, fim, mantidos = {}, hashlib.sha256(), {"podadas": 0, "mantidas": 0}, 0, 0
    with open(tmp, "wb") as dst:
        for raw, fim in _linhas_completas(path, size0):
            dia, rec = classificar(raw)
            if dia is not None and dia < corte:
                contagem["podadas"] += 1
                digest.update(raw)
                if j.get("rollup"):
                    _acumular(agg, dia, rec, cfg)
                if j["expira"] == "arquivar":
                    arq.escrever(dia, raw)
            else:
                contagem["mantidas"] += 1
                mantidos += dst.write(raw)
    return agg, digest.hexdigest(), contagem, fim, mantidos


def _anexar_cauda(path: Path, tmp: Path, fim: int, mantidos: int, apos_ler_cauda: Callable[[], None] | None) -> None:
    """Copy whatever is after the processed range (incomplete line, appends) and verify the size."""
    with open(path, "rb") as src, open(tmp, "r+b") as dst:
        for _ in range(TENTATIVAS_CAUDA):
            src.seek(fim)
            cauda = src.read()
            dst.seek(mantidos)
            dst.truncate()
            dst.write(cauda)
            dst.flush()
            if apos_ler_cauda:
                apos_ler_cauda()
            if os.stat(path).st_size == fim + len(cauda):
                os.fsync(dst.fileno())
                return
    raise _Abort("escritor ativo: o arquivo continua mudando durante a troca; nada foi alterado")


def _efetivar(j, base, path, tmp, arq, agg, digest, antes_de_trocar_o_bruto) -> None:
    if _tem_ledger(j):
        ledger = caminho_rollup(base, j["id"])
        meta, recs = _ler_rollup_completo(ledger)
        if digest not in meta["batches"]:
            arq.efetivar()
            meta["batches"].append(digest)
            _gravar_rollup(ledger, j["id"], meta, _fundir(recs, agg))
    if antes_de_trocar_o_bruto:
        antes_de_trocar_o_bruto()
    shutil.copymode(path, tmp)
    os.replace(tmp, path)
    _fsync_dir(path.parent)


def _aplicar_log(j, base, agora, apos_ler_cauda, antes_de_trocar_o_bruto) -> dict[str, Any]:
    path = base / j["caminho"]
    tmp = path.with_name(path.name + ".gc-tmp")
    arq = _Arquivador(path)
    corte = _corte(agora, j["retencao_dias"])
    try:
        agg, digest, contagem, fim, mantidos = _varrer_e_separar(j, path, path.stat().st_size, corte, tmp, arq)
        arq.fechar()
        _anexar_cauda(path, tmp, fim, mantidos, apos_ler_cauda)
        _efetivar(j, base, path, tmp, arq, agg, digest, antes_de_trocar_o_bruto)
    except _Abort as exc:
        return {"id": j["id"], "estado": "erro", "podadas": 0, "mantidas": 0, "motivo": str(exc)}
    finally:
        arq.descartar()
        tmp.unlink(missing_ok=True)
    return {"id": j["id"], "estado": "podado", **contagem}


def _aplicar_arquivos(j, base, agora) -> dict[str, Any]:
    limite = (agora - dt.timedelta(days=j["retencao_dias"])).timestamp()
    apagados, pulados = 0, []
    for p in sorted(base.glob(j["caminho"])):
        if not p.is_file() or p.stat().st_mtime >= limite:
            continue
        if _abertos_para_escrita(p):
            pulados.append(p.name)
            continue
        p.unlink()
        apagados += 1
    res = {"id": j["id"], "estado": "podado" if apagados else "nada", "podadas": apagados, "mantidas": 0}
    if pulados:
        res["motivo"] = f"pulados (abertos para escrita): {', '.join(pulados)}"
    return res


def aplicar(
    m: dict[str, Any], base: Path, agora: dt.datetime, *, repo: Path | None = None,
    apos_ler_cauda: Callable[[], None] | None = None, antes_de_trocar_o_bruto: Callable[[], None] | None = None,
) -> list[dict[str, Any]]:
    """Prune what the plan allows. With `repo`, refuses to run on a manifest that has errors."""
    if repo is not None:
        erros = [p for p in verificar(m, repo, base) if p["nivel"] == "erro"]
        if erros:
            raise ManifestoInvalido("; ".join(f"{p['codigo']}({p['journal']})" for p in erros))
    out = []
    for j, plano in zip(m.get("journal") or [], planejar(m, base, agora), strict=True):
        estado = plano["estado"]
        if estado != "ok":
            out.append({"id": j["id"], "estado": estado, "podadas": 0, "mantidas": 0, **({"bloqueio": plano["bloqueio"]} if plano["bloqueio"] else {})})
        elif plano["expiram"] == 0:
            out.append({"id": j["id"], "estado": "nada", "podadas": 0, "mantidas": plano["mantem"]})
        elif j["tipo"] == "arquivos":
            out.append(_aplicar_arquivos(j, base, agora))
        elif quem := _abertos_para_escrita(base / j["caminho"]):
            pid, comm = quem[0]
            out.append({"id": j["id"], "estado": "pulado", "podadas": 0, "mantidas": 0, "motivo": f"aberto para escrita por pid {pid} ({comm})"})
        else:
            out.append(_aplicar_log(j, base, agora, apos_ler_cauda, antes_de_trocar_o_bruto))
    return out


# --- CLI ------------------------------------------------------------------------------------------

def _mb(n: int) -> str:
    return f"{n / 1e6:.1f} MB"


def texto(modo: str, base: Path, problemas: list[dict[str, str]], plano: list[dict[str, Any]], resultado: list[dict[str, Any]]) -> str:
    out = [f"JOURNAL GC — {modo.upper()} (base {base})"]
    for nivel in ("erro", "aviso", "info"):
        grupo = [p for p in problemas if p["nivel"] == nivel]
        if grupo:
            out.append(f"\n[{nivel}] {len(grupo)}")
            out += [f"  - {p['codigo']} ({p['journal']}): {p['msg']}" for p in grupo]
    if plano:
        out.append("\nPlano por journal:")
        for p in plano:
            extra = f" bloqueado por {', '.join(p['bloqueio'])}" if p["estado"] == "bloqueado" else ""
            out.append(
                f"  {p['id']:<22} {p['estado']:<10} linhas={p['linhas']:<8} expiram={p['expiram']:<8} sem_data={p['sem_data']:<6} "
                f"liberaria={_mb(p['bytes_expiram'])} de {_mb(p['bytes_total'])} (retenção {p['retencao_dias']} d, corte {p['corte']}){extra}"
            )
        livre = sum(p["bytes_expiram"] for p in plano if p["estado"] == "ok")
        bloqueado = sum(p["bytes_expiram"] for p in plano if p["estado"] == "bloqueado")
        out.append(f"\nLiberável agora: {_mb(livre)}; retido por leitores de janela total: {_mb(bloqueado)}")
    if resultado:
        out.append("\nResultado:")
        out += [f"  {r['id']:<22} {r['estado']:<10} podadas={r['podadas']} mantidas={r['mantidas']} {r.get('motivo', '')}" for r in resultado]
    return "\n".join(out)


class _Parser(argparse.ArgumentParser):
    def error(self, message: str) -> None:
        self.print_usage(sys.stderr)
        self.exit(64, f"{self.prog}: erro: {message}\n")


def _agora(valor: str | None) -> dt.datetime:
    if not valor:
        return dt.datetime.now(UTC)
    d = dt.datetime.fromisoformat(valor)
    return d if d.tzinfo else d.replace(tzinfo=UTC)


def _argumentos(argv: list[str] | None) -> tuple[_Parser, argparse.Namespace]:
    ap = _Parser(description=__doc__.splitlines()[0])
    ap.add_argument("--manifesto", default=str(MANIFESTO))
    ap.add_argument("--base", help="padrão: `base` do manifesto")
    ap.add_argument("--repo", default=str(ROOT))
    ap.add_argument("--agora", help="ISO 8601 (testes)")
    ap.add_argument("--json", action="store_true")
    modo = ap.add_mutually_exclusive_group()
    modo.add_argument("--apply", action="store_true", help="poda de verdade (padrão: dry-run)")
    modo.add_argument("--verificar", action="store_true", help="só manifesto x código")
    ap.add_argument("--totais", metavar="ID", help="totais do rollup do journal")
    ap.add_argument("--de")
    ap.add_argument("--ate")
    ap.add_argument("--onde", action="append", default=[], metavar="CAMPO=VALOR")
    args = ap.parse_args(argv)
    if args.totais and not (args.de and args.ate):
        ap.error("--totais exige --de e --ate (AAAA-MM-DD)")
    return ap, args


def _executar(args: argparse.Namespace, m: dict[str, Any], base: Path, agora: dt.datetime) -> int:
    problemas = verificar(m, Path(args.repo), base)
    com_erro = any(p["nivel"] == "erro" for p in problemas)
    rotulo = "apply" if args.apply else "verificar" if args.verificar else "dry-run"
    plano: list[dict[str, Any]] = []
    resultado: list[dict[str, Any]] = []
    if not args.verificar and not com_erro:  # a manifest with errors is never planned or applied
        if args.apply:
            resultado = aplicar(m, base, agora)
        else:
            plano = planejar(m, base, agora)
    saida = {"modo": rotulo, "base": str(base), "problemas": problemas, "plano": plano, "resultado": resultado}
    print(json.dumps(saida, ensure_ascii=False, indent=2) if args.json else texto(rotulo, base, problemas, plano, resultado))
    if com_erro:
        return 2
    return 1 if any(r["estado"] == "erro" for r in resultado) else 0


def main(argv: list[str] | None = None) -> int:
    ap, args = _argumentos(argv)
    m = carregar(Path(args.manifesto))
    base = Path(args.base or m.get("base", "~/.claude/touring")).expanduser()
    if args.totais:
        onde = dict(kv.split("=", 1) for kv in args.onde)
        print(json.dumps(totais(base, args.totais, args.de, args.ate, onde), ensure_ascii=False))
        return 0
    try:
        agora = _agora(args.agora)
    except ValueError:
        ap.error(f"--agora inválido: {args.agora!r}")
    return _executar(args, m, base, agora)


if __name__ == "__main__":
    sys.exit(main())
