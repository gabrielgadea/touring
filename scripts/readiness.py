#!/usr/bin/env python3
"""One answer to "is it ready?": an ATTENTION LIST with a verdict, not a fifth health number.

Origin (17/10/2026, ECC analysis, decision N5): four measures answer "is it healthy?" with the
same word and four different grandezas — `touring status` composite_health_score (0.4939),
`touring e2e` overall_score (0.8593), `touring harness-metric` composite (0.536) and
`touring kpi` (26 PASS / 13 ADVISORY / 7 STUB / 0 FAIL of 46). Averaging them would mint a sixth
number; electing one would throw information away. This script instead joins the SIGNALS behind
them into a list of attention items — each with its source, value, threshold and the command
that resolves it — and a verdict, `pronto` or `atencao`.

Laws it keeps:

* **Absent signal is not zero (L2, fail-closed).** A source that fails, times out, prints
  something that is not JSON or prints an error object becomes an item `fonte.<nome>`; it is
  never omitted and never read as healthy. (`status` itself is guilty of the opposite: its
  composite substitutes a neutral 0.5 for a missing dimension.)
* **Accepted is not silent.** An item may only be waived by an entry in
  `docs/kpi/readiness-dispensas.yaml` that names the item, the reason, the owner and an expiry
  (at most `MAX_VALIDADE_DIAS` ahead). An expired waiver stops waiving and says so; a malformed
  one does not waive and becomes an item of its own.
* **The old measures survive as detail** under distinct names, each with what it measures and
  its denominator, so nobody has to guess which "health" a number is.

Exit code: 0 `pronto`, 2 `atencao`, 64 usage error (argparse's default 2 would collide with the
verdict, so usage errors are remapped).

    python3 scripts/readiness.py --text            # live: runs the five touring commands
    python3 scripts/readiness.py --json --from-dir DIR   # offline: DIR/{status,e2e,kpi,doctor,harness-metric}.json
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
DISPENSAS = ROOT / "docs" / "kpi" / "readiness-dispensas.yaml"

# name -> touring arguments. The first four decide the verdict; `harness-metric` only feeds the
# detail section, but a failure of it is still an item (a missing signal is never silent).
SOURCES: dict[str, list[str]] = {
    "status": ["status", "-j"],
    "e2e": ["e2e", "-j"],
    "kpi": ["kpi", "-j"],
    "doctor": ["doctor", "-j"],
    "harness-metric": ["harness-metric", "-j"],
}
CORE = ("status", "e2e", "kpi", "doctor")

# `touring status` took 45 s on 18/10/2026 under load (a daemon handler blew its 15 s budget);
# the default leaves room for that without hanging a gate forever.
TIMEOUT_S = 120
MAX_VALIDADE_DIAS = 90
SEVERIDADES = ("bloqueia", "sem_medida", "aviso")  # display order; all of them keep the verdict at `atencao`


@dataclass
class Fonte:
    nome: str
    ok: bool
    data: Any = None
    erro: str | None = None
    rc: int | None = None
    duracao_s: float | None = None


def _json_from(text: str) -> Any:
    """Parse JSON from command output, tolerating log noise before the first `{`/`[`."""
    try:
        return json.loads(text)
    except ValueError:
        starts = [i for i in (text.find("{"), text.find("[")) if i >= 0]
        if not starts:
            raise
        return json.loads(text[min(starts):])


def _error_shaped(data: Any) -> str | None:
    """The daemon reports a blown budget as a bare error object on stdout — a failure, not data."""
    if isinstance(data, dict) and ("error_kind" in data or (data and set(data) <= {"error"})):
        return str(data.get("error") or data.get("error_kind"))[:200]
    return None


def classify(nome: str, rc: int | None, stdout: str, duracao_s: float | None = None) -> Fonte:
    try:
        data = _json_from(stdout)
    except ValueError:
        trecho = " ".join(stdout.split())[:120] or "(saída vazia)"
        return Fonte(nome, False, None, f"saída não é JSON (rc={rc}): {trecho}", rc, duracao_s)
    erro = _error_shaped(data)
    if erro:
        return Fonte(nome, False, None, erro, rc, duracao_s)
    return Fonte(nome, True, data, None, rc, duracao_s)


def fetch_live(nome: str, root: Path, touring: str, timeout: int) -> Fonte:
    inicio = time.monotonic()
    try:
        proc = subprocess.run(
            [touring, *SOURCES[nome]], cwd=root, capture_output=True, text=True, timeout=timeout, check=False
        )
    except subprocess.TimeoutExpired:
        return Fonte(nome, False, None, f"excedeu {timeout}s", None, float(timeout))
    except OSError as exc:
        return Fonte(nome, False, None, f"não executou: {exc}", None, None)
    return classify(nome, proc.returncode, proc.stdout, round(time.monotonic() - inicio, 2))


def fetch_dir(nome: str, pasta: Path) -> Fonte:
    arquivo = pasta / f"{nome}.json"
    try:
        texto = arquivo.read_text(encoding="utf-8")
    except OSError as exc:
        return Fonte(nome, False, None, f"arquivo ausente: {exc.filename or arquivo}", None, None)
    return classify(nome, None, texto)


def _r(valor: Any) -> Any:
    """Floats read as 0.7738, not 0.7737999999999999."""
    return round(valor, 4) if isinstance(valor, float) else valor


def _item(id_: str, fonte: str, severidade: str, valor: Any, limiar: str, rota: str, detalhe: str = "") -> dict[str, Any]:
    valor = "ausente" if valor is None else _r(valor)  # an item always says what was seen, even if that is nothing
    item = {"id": id_, "fonte": fonte, "severidade": severidade, "valor": valor, "limiar": limiar, "rota": rota}
    if detalhe:
        item["detalhe"] = detalhe
    return item


def itens_da_fonte_ausente(fontes: dict[str, Fonte]) -> list[dict[str, Any]]:
    out = []
    for nome, f in fontes.items():
        if f.ok:
            continue
        out.append(_item(
            f"fonte.{nome}", nome, "bloqueia" if nome in CORE else "aviso", f.erro,
            "fonte responde com JSON de dados",
            f"touring {' '.join(SOURCES[nome])}  # repita; se persistir: touring doctor -j / touring daemon-ctl status",
            "sinal ausente não é zero: nada abaixo desta fonte foi avaliado",
        ))
    return out


def itens_status(d: dict[str, Any]) -> list[dict[str, Any]]:
    out = []
    dh = d.get("daemon_health")
    if not isinstance(dh, dict) or "error" in dh or "healthy_count" not in dh:
        erro = dh.get("error") if isinstance(dh, dict) else None
        out.append(_item(
            "status.daemon_health", "status", "bloqueia", (erro or "ausente")[:160],
            "healthy_count/total_count presentes",
            "touring daemon-ctl status && touring doctor -j",
            "o composite_health_score troca a dimensão ausente por 0.5 neutro (30% do peso)",
        ))
    gen = ((d.get("index") or {}).get("index_generation") or {})
    if gen.get("state") != "complete":
        out.append(_item(
            "status.index_generation", "status", "bloqueia", gen.get("state", "ausente"),
            "state == complete", "touring index status && touring index rebuild --dir $PWD",
        ))
    streak = (d.get("health_delta") or {}).get("streak_alert_count")
    if isinstance(streak, int | float) and streak > 0:
        out.append(_item(
            "status.regressao_em_serie", "status", "aviso", streak, "streak_alert_count == 0",
            "touring health-delta status",
        ))
    return out


def itens_e2e(d: dict[str, Any]) -> list[dict[str, Any]]:
    out = []
    if d.get("overall_status") != "pass":
        out.append(_item(
            "e2e.geral", "e2e", "bloqueia", d.get("overall_status", "ausente"), "overall_status == pass",
            "touring e2e -j  # ver phases[] com status != pass",
        ))
    for fase in d.get("phases") or []:
        nome = fase.get("phase", "?")
        run, ok = fase.get("tests_run"), fase.get("tests_passed")
        testes_falhos = isinstance(run, int) and isinstance(ok, int) and ok < run
        issues = fase.get("issues") or []
        if fase.get("status") == "pass" and not issues and not testes_falhos:
            continue
        sev = "bloqueia" if fase.get("status") not in ("pass", "warn") else "aviso"
        valor = f"{fase.get('status')} score={_r(fase.get('score'))}"
        if testes_falhos:
            valor += f" testes={ok}/{run}"
        out.append(_item(
            f"e2e.fase.{nome}", "e2e", sev, valor, "status == pass, 0 issues, testes_passed == tests_run",
            f"touring e2e -j | jq '.phases[] | select(.phase==\"{nome}\")'", "; ".join(map(str, issues))[:200],
        ))
    return out


def itens_kpi(d: dict[str, Any]) -> list[dict[str, Any]]:
    out = []
    regressoes = (d.get("summary") or {}).get("regressions")
    if isinstance(regressoes, int | float) and regressoes > 0:
        out.append(_item("kpi.regressoes", "kpi", "bloqueia", regressoes, "regressions == 0", "touring kpi -j | jq '.summary'"))
    for c in d.get("checks") or []:
        status, cid = c.get("status"), c.get("id", "?")
        if status == "PASS":
            continue
        limiar = f"{c.get('direction')} {c.get('threshold')}"
        rota = f"touring kpi -j | jq '.checks[] | select(.id==\"{cid}\")'"
        if c.get("source_kind") == "external":
            rota += f"  # medição externa: docs/kpi/external/{cid}.json (docs/kpi/README.md)"
        if status == "FAIL":
            sev, valor = "bloqueia", c.get("actual")
        elif status == "STUB":
            sev, valor = "sem_medida", c.get("stub_reason") or "sem medição (actual=null)"
        else:  # ADVISORY and anything unknown: measured out of threshold, or a status this script never saw
            sev, valor = ("aviso", c.get("actual")) if status == "ADVISORY" else ("bloqueia", f"status desconhecido: {status}")
        out.append(_item(f"kpi.{cid}", "kpi", sev, valor, limiar, rota, (c.get("name") or "")[:100]))
    return out


def itens_doctor(d: Any) -> list[dict[str, Any]]:
    out = []
    linhas = d if isinstance(d, list) else []
    if not isinstance(d, list):
        out.append(_item("doctor.formato", "doctor", "bloqueia", type(d).__name__, "lista de {name,status,detail}", "touring doctor -j"))
    for c in linhas:
        status, nome = c.get("status"), c.get("name", "?")
        if status == "ok":
            continue
        sev = {"warn": "aviso", "missing": "sem_medida"}.get(status, "bloqueia")
        out.append(_item(
            f"doctor.{nome}", "doctor", sev, f"{status}: {str(c.get('detail', ''))[:140]}", "status == ok",
            "touring doctor -j  # rode com o cwd na raiz do projeto: o doctor resolve o projeto pelo cwd",
        ))
    return out


def coletar_itens(fontes: dict[str, Fonte]) -> list[dict[str, Any]]:
    itens = itens_da_fonte_ausente(fontes)
    for nome, fn in (("status", itens_status), ("e2e", itens_e2e), ("kpi", itens_kpi), ("doctor", itens_doctor)):
        if fontes[nome].ok:
            itens += fn(fontes[nome].data)
    return itens


# --- detail: the old measures, under distinct names, with their definition ---------------------

def _num(d: Any, *caminho: str) -> Any:
    for k in caminho:
        if not isinstance(d, dict):
            return None
        d = d.get(k)
    return d


def medidas(fontes: dict[str, Fonte]) -> list[dict[str, Any]]:
    """The measures that used to be called "health", each named for what it actually measures."""
    def dado(nome: str) -> Any:
        return fontes[nome].data if fontes[nome].ok else None

    status, e2e, kpi, harness = dado("status"), dado("e2e"), dado("kpi"), dado("harness-metric")
    pesos = sum(float(f.get("weight", 0)) for f in (e2e or {}).get("phases") or [])
    sumario = (kpi or {}).get("summary") or {}
    wiring = next((f for f in (e2e or {}).get("phases") or [] if f.get("phase") == "wiring"), None)
    return [
        {"nome": "saude_composta_daemon", "valor": _num(status, "composite_health_score"), "fonte": "touring status -j",
         "mede": "média ponderada fixa de 5 dimensões: daemon_health 30% (healthy/total), órfãos 20% (1 - órfãos/símbolos públicos), "
                 "regressão em aberto 20% (1/(1+outstanding)), cache hit 15%, EMA do reward 15%",
         "denominador": "soma dos pesos = 1,0; dimensão sem dado entra como 0,5 neutro (nunca como ausente)"},
        {"nome": "e2e_nota_ponderada", "valor": _num(e2e, "overall_score"), "fonte": "touring e2e -j",
         "mede": "média das notas das fases (index, wiring, knowledge, ast, quality, learning) ponderada pelo peso de cada fase",
         "denominador": f"soma dos pesos das fases executadas = {pesos:g}" if e2e else "soma dos pesos das fases executadas"},
        {"nome": "harness_composto", "valor": _num(harness, "composite"), "fonte": "touring harness-metric -j",
         "mede": "média aritmética de 6 dimensões de liveness do gateway (executable, inspectable, stateful, governed, performant, evolving)",
         "denominador": "6 dimensões em 0..1, cada uma ancorada em contadores do gateway; harness frio lê 0,5 neutro"},
        {"nome": "kpi_compromissos", "valor": (
            {"pass": sumario.get("passed"), "advisory": sumario.get("advisory"), "stub": sumario.get("stub"),
             "fail": sumario.get("failed"), "total": sumario.get("total")} if kpi else None),
         "fonte": "touring kpi -j",
         "mede": "contagem de compromissos falsificáveis por veredito (não é nota)",
         "denominador": "compromissos declarados em docs/kpi/commitments.yaml; STUB conta no total como não medido"},
        {"nome": "nota_fase_wiring", "valor": wiring.get("score") if wiring else None, "fonte": "touring e2e -j (phases[wiring])",
         "mede": "integração dos símbolos públicos (consumidores por símbolo) — o 'health: healthy 0.81 [wiring:0.81]' do cabeçalho "
                 "dos hooks coincide em valor com esta nota em 18/10/2026",
         "denominador": "símbolos públicos indexados (métrica total_pub_symbols da fase)"},
    ]


# --- waivers ------------------------------------------------------------------------------------

def _data(valor: Any) -> dt.date | None:
    if isinstance(valor, dt.datetime):
        return valor.date()
    if isinstance(valor, dt.date):
        return valor
    try:
        return dt.date.fromisoformat(str(valor))
    except ValueError:
        return None


def _problema(id_: str, valor: str, limiar: str, rota: str) -> dict[str, Any]:
    return _item(id_, "dispensas", "bloqueia", valor, limiar, rota)


def _ler_dispensas(caminho: Path) -> tuple[list[Any], list[dict[str, Any]]]:
    """-> (raw waiver list, problems). A waiver file that cannot be read waives nothing, loudly."""
    rota = f"corrigir {caminho.name}"
    if not caminho.exists():
        return [], [_problema("dispensas.arquivo", "ausente", "arquivo versionado presente", f"criar {caminho.name} (dispensas: [])")]
    try:
        import yaml  # PyYAML: required here — its absence is an item, not a silent "no waivers"
    except ImportError:
        return [], [_problema("dispensas.arquivo", "PyYAML ausente", "arquivo legível", "python3 -m pip install pyyaml")]
    try:
        doc = yaml.safe_load(caminho.read_text(encoding="utf-8")) or {}
    except (OSError, yaml.YAMLError) as exc:
        return [], [_problema("dispensas.arquivo", str(exc)[:160], "YAML válido", rota)]
    lista = doc.get("dispensas") if isinstance(doc, dict) else None
    if not isinstance(lista, list):
        return [], [_problema("dispensas.arquivo", "chave `dispensas` ausente ou não é lista", "dispensas: [...]", rota)]
    return lista, []


def _validar_dispensa(n: int, d: Any, hoje: dt.date, arquivo: str) -> tuple[dict[str, Any] | None, dict[str, Any] | None]:
    """-> (waiver, None) when valid, (None, problem) otherwise."""
    if not isinstance(d, dict):
        return None, _problema(f"dispensas.invalida.{n}", repr(d)[:80], "mapa {item,motivo,dono,valido_ate}", f"corrigir {arquivo}")
    ate = _data(d.get("valido_ate"))
    falta = [k for k in ("item", "motivo", "dono") if not str(d.get(k) or "").strip()]
    if ate is None:
        falta.append("valido_ate (AAAA-MM-DD)")
    elif (ate - hoje).days > MAX_VALIDADE_DIAS:
        falta.append(f"valido_ate a no máximo {MAX_VALIDADE_DIAS} dias de hoje")
    if falta:
        return None, _problema(
            f"dispensas.invalida.{d.get('item') or n}", f"falta/errado: {', '.join(falta)}",
            "item, motivo, dono e valido_ate válidos", f"corrigir {arquivo}; a dispensa NÃO vale enquanto estiver inválida",
        )
    return {**d, "valido_ate": ate.isoformat()}, None


def carregar_dispensas(caminho: Path, hoje: dt.date) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    """-> (valid waivers, problems). A problem is never a silent skip: it comes back as an item."""
    lista, problemas = _ler_dispensas(caminho)
    validas = []
    for n, d in enumerate(lista):
        ok, problema = _validar_dispensa(n, d, hoje, caminho.name)
        if ok:
            validas.append(ok)
        else:
            problemas.append(problema)
    return validas, problemas


def aplicar_dispensas(itens: list[dict[str, Any]], dispensas: list[dict[str, Any]], hoje: dt.date) -> tuple[list[dict[str, Any]], list[dict[str, Any]], list[str]]:
    por_item = {d["item"]: d for d in dispensas}
    ativos, dispensados = [], []
    for it in itens:
        d = por_item.get(it["id"])
        if d is None:
            ativos.append(it)
        elif dt.date.fromisoformat(d["valido_ate"]) < hoje:
            ativos.append({**it, "dispensa_expirada": d["valido_ate"]})
        else:
            dispensados.append({**it, "dispensa": {k: d[k] for k in ("motivo", "dono", "valido_ate")}})
    orfas = sorted(set(por_item) - {it["id"] for it in itens})
    return ativos, dispensados, orfas


def avaliar(fontes: dict[str, Fonte], dispensas_path: Path, hoje: dt.date) -> dict[str, Any]:
    itens = coletar_itens(fontes)
    dispensas, problemas = carregar_dispensas(dispensas_path, hoje)
    ativos, dispensados, orfas = aplicar_dispensas(itens, dispensas, hoje)
    ativos += problemas
    ordem = {s: i for i, s in enumerate(SEVERIDADES)}
    ativos.sort(key=lambda i: (ordem.get(i["severidade"], 0), i["id"]))
    return {
        "veredito": "atencao" if ativos else "pronto",
        "itens": ativos,
        "dispensados": dispensados,
        "dispensas_sem_item": orfas,
        "fontes": {n: {"ok": f.ok, "erro": f.erro, "rc": f.rc, "duracao_s": f.duracao_s} for n, f in fontes.items()},
        "medidas": medidas(fontes),
        "hoje": hoje.isoformat(),
    }


def texto(r: dict[str, Any]) -> str:
    linhas = [f"PRONTIDÃO: {r['veredito'].upper()}  ({len(r['itens'])} itens de atenção, {len(r['dispensados'])} dispensados)"]
    for sev in SEVERIDADES:
        grupo = [i for i in r["itens"] if i["severidade"] == sev]
        if grupo:
            linhas.append(f"\n[{sev}] {len(grupo)}")
        for i in grupo:
            extra = f"  (dispensa expirou em {i['dispensa_expirada']})" if "dispensa_expirada" in i else ""
            linhas.append(f"  - {i['id']}: {i['valor']}  | limiar: {i['limiar']}{extra}")
            if i.get("detalhe"):
                linhas.append(f"      {i['detalhe']}")
            linhas.append(f"      rota: {i['rota']}")
    if r["dispensas_sem_item"]:
        linhas.append("\nDispensas sem item correspondente (podem sair do arquivo): " + ", ".join(r["dispensas_sem_item"]))
    linhas.append("\nMedidas antigas (detalhe, nomes distintos):")
    for m in r["medidas"]:
        linhas.append(f"  - {m['nome']} = {m['valor']}  [{m['fonte']}]\n      mede: {m['mede']}\n      denominador: {m['denominador']}")
    return "\n".join(linhas)


class _Parser(argparse.ArgumentParser):
    def error(self, message: str) -> None:  # exit 2 is the verdict `atencao`; usage errors must not alias it
        self.print_usage(sys.stderr)
        self.exit(64, f"{self.prog}: erro: {message}\n")


def main(argv: list[str] | None = None) -> int:
    ap = _Parser(description=__doc__.splitlines()[0])
    fmt = ap.add_mutually_exclusive_group()
    fmt.add_argument("--json", action="store_true", help="saída JSON (padrão: --text)")
    fmt.add_argument("--text", action="store_true")
    ap.add_argument("--root", default=str(ROOT), help="raiz do projeto: cwd dos comandos touring (o doctor resolve o projeto pelo cwd)")
    ap.add_argument("--touring", default="touring", help="binário touring")
    ap.add_argument("--timeout", type=int, default=TIMEOUT_S, help="segundos por comando")
    ap.add_argument("--from-dir", help="lê DIR/<fonte>.json em vez de rodar os comandos (offline)")
    ap.add_argument("--dispensas", default=str(DISPENSAS))
    ap.add_argument("--hoje", help="AAAA-MM-DD (testes)")
    args = ap.parse_args(argv)
    hoje = _data(args.hoje) if args.hoje else dt.datetime.now().astimezone().date()
    if hoje is None:
        ap.error(f"--hoje inválido: {args.hoje!r}")
    root = Path(args.root)
    if args.from_dir:
        fontes = {n: fetch_dir(n, Path(args.from_dir)) for n in SOURCES}
    else:
        fontes = {n: fetch_live(n, root, args.touring, args.timeout) for n in SOURCES}
    r = avaliar(fontes, Path(args.dispensas), hoje)
    print(json.dumps(r, ensure_ascii=False, indent=2) if args.json else texto(r))
    return 0 if r["veredito"] == "pronto" else 2


if __name__ == "__main__":
    sys.exit(main())
