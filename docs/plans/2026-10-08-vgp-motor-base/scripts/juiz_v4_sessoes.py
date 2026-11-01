"""Juiz da V4 — "5 sessões reais com deny correto; zero falso deny reportado" (proposta §6).

Escrito ANTES do dado real: o motor da Kimi vai emitir o registro de sombra/ativo em
`~/.claude/touring/vgp/sombra.jsonl`, e este juiz e escrito CONTRA O CONTRATO de entrada — nao contra
o dado que vier. Por isso os testes usam fixtures SINTETICAS: um juiz ajustado ao dado real deixa de
medir o motor.

O deny tem dois lugares no transcript: o registro diz que o gate DENY e traz os `reais`; o transcript
mostra o que aconteceu depois (a recusa ativa vem como `PreToolUse:<Tool> hook error: [VGP · deny]`).

Veredito por deny:
  DENY CORRETO       a chamada seguinte da mesma sessao, sobre o MESMO path (ou o mesmo comando, se
                     inline), nao contem mais o nome negado — OU usa um dos `reais`.
  FALSO DENY         depois do deny: (a) bypass, (b) o MESMO conteudo reenviado, ou (c) texto do
                     assistente OU do usuario dizendo que o simbolo existe / falso positivo.
  NAO CLASSIFICADO   deny sem chamada seguinte em 10 min — visivel, nunca somado aos corretos.

Exit 0 sse >= 5 sessoes DISTINTAS com >= 1 deny correto E 0 falso reportado.
Exit 3 = dado insuficiente (nunca "0 falsos" sem nenhum deny). Exit 1 = falso reportado.

Uso: python3 juiz_v4_sessoes.py --desde <ISO-Z> --ate <ISO-Z> [--sombra <caminho>]
       python3 juiz_v4_sessoes.py --selftest [--fixtures <dir>]
"""

import json
import re
import sys
from collections import Counter
from datetime import datetime, timedelta
from pathlib import Path

PROJ = Path.home() / ".claude/projects"
SOMBRA_PADRAO = Path.home() / ".claude/touring/vgp/sombra.jsonl"
JANELA_PAR_S = 600  # 10 min sem chamada seguinte -> nao classificado

# ---------------------------------------------------------------- predicados POSITIVOS
DENY_NO_TRANSCRIPT = re.compile(
    r"(?:PreToolUse|PostToolUse):?\S*\s*hook error:\s*\[VGP · deny\]"
)
AVISO_NO_TRANSCRIPT = re.compile(r"\[VGP · aviso\]")

# (a) bypass: a variavel de ambiente ou o kill switch do VGP na chamada seguinte.
BYPASS = re.compile(
    r"TOURING_GATE_OK\s*=\s*1|TOURING_VGP_DISABLED\s*=\s*1|"
    r"TOURING_CODE_GATES_DISABLED\s*=\s*1"
)

# (c) o simbolo existe / falso positivo. Frases POSITIVAS (listadas no relatorio).
FALSO_POSITIVO = re.compile(
    r"falso positivo|false positive|o simbolo (existe|n[aã]o existe)|"
    r"symbol (does not )?exist|n[aã]o existe (esse|este) simbolo|"
    r"inexistente|n[aã]o existe na base|"
    r"o (VGP|gate) err(ou|a)|deny errado|deny incorreto",
    re.I,
)


def iso(ts: str) -> datetime:
    return datetime.fromisoformat(ts.replace("Z", "+00:00"))


def z(dt: datetime) -> str:
    return (
        dt.astimezone()
        .astimezone(__import__("datetime").timezone.utc)
        .strftime("%Y-%m-%dT%H:%M:%SZ")
    )


# ---------------------------------------------------------------- registro
def le_sombra(caminho: Path) -> list[dict]:
    """As linhas do registro. Linha malformada entra marcada, nunca descartada em silencio."""
    if not caminho.exists():
        return []
    out = []
    for raw in caminho.read_text(encoding="utf-8", errors="replace").splitlines():
        raw = raw.strip()
        if not raw:
            continue
        try:
            out.append(json.loads(raw))
        except json.JSONDecodeError:
            out.append({"ts": "", "malformada": True, "_linha": raw[:200]})
    return out


def denies(registros: list[dict], desde: str, ate: str) -> tuple[list[dict], dict]:
    """(os denies que contam, os ignorados e POR QUE).

    So conta modo=ativo E verdict=deny dentro da janela. `modo=sombra` nunca conta: um gate em modo
    sombra nao nega ninguem, e conta-lo inflaria o numero de sessoes com evidencia inexistente.
    """
    dentro, ign = [], Counter()
    for r in registros:
        if r.get("malformada"):
            ign["linha malformada"] += 1
        elif str(r.get("ts", "")) < desde:
            ign["antes de --desde"] += 1
        elif str(r.get("ts", "")) > ate:
            ign["depois de --ate"] += 1
        elif r.get("modo") != "ativo":
            ign["modo != ativo (sombra)"] += 1
        elif r.get("verdict") != "deny":
            ign["verdict != deny"] += 1
        else:
            dentro.append(r)
    return dentro, dict(ign)


# ---------------------------------------------------------------- transcript
def _chamadas(sessao: str, ts_deny: datetime, janela_s: int) -> list[dict]:
    """tool_use da sessao dentro da janela (exclusive ate ts_deny, exclusive ate o limite)."""
    arqs = list(PROJ.rglob(f"{sessao}.jsonl"))
    if not arqs:
        return []
    ini, fim = z(ts_deny), z(ts_deny + timedelta(seconds=janela_s))
    out = []
    for raw in arqs[0].open(encoding="utf-8", errors="replace"):
        if '"tool_use"' not in raw:
            continue
        try:
            rec = json.loads(raw)
        except json.JSONDecodeError:
            continue
        ts = str(rec.get("timestamp", ""))
        if ts <= ini or ts > fim:
            continue
        for c in (rec.get("message") or {}).get("content") or []:
            if isinstance(c, dict) and c.get("type") == "tool_use":
                inp = c.get("input") or {}
                out.append(
                    {
                        "ts": ts,
                        "tool": c.get("name"),
                        "inp": inp,
                        "alvo": str(
                            inp.get("file_path")
                            or inp.get("notebook_path")
                            or inp.get("command")
                            or ""
                        ),
                        "blob": json.dumps(inp, ensure_ascii=False),
                    }
                )
    return out


def par_para_o_deny(r: dict, ts: datetime) -> dict | None:
    """A PRIMEIRA chamada da mesma sessao sobre o MESMO alvo (path, ou comando quando inline)."""
    alvo_deny = None if r.get("balde") == "inline" else r.get("path")
    if r.get("balde") == "inline":
        alvo_deny = r.get("_cmd") or r.get("comando")
    for ch in _chamadas(str(r.get("session_id", "")), ts, JANELA_PAR_S):
        if alvo_deny and ch["alvo"] == str(alvo_deny):
            return ch
    return None


def nomes_negados(r: dict) -> list[str]:
    return [str(f["nome"]) for f in (r.get("findings") or []) if f.get("nome")]


def reals_de(r: dict) -> list[str]:
    return [str(x) for f in (r.get("findings") or []) for x in (f.get("reais") or [])]


def classifica(r: dict) -> dict:
    """O veredito de UM deny. Cada ramo e um PREDICADO POSITIVO, nunca uma ausencia."""
    ts = iso(str(r["ts"]))
    nomes, reals = nomes_negados(r), reals_de(r)
    par = par_para_o_deny(r, ts)
    base = {
        "ts": r["ts"],
        "session_id": r.get("session_id"),
        "hook": r.get("hook"),
        "nomes_negados": nomes,
        "reais": reals,
    }
    if par is None:
        return {
            **base,
            "veredito": "nao classificado",
            "porque": f"sem chamada seguinte sobre o mesmo alvo em {JANELA_PAR_S // 60} min",
        }
    if BYPASS.search(par["blob"]):
        return {
            **base,
            "veredito": "falso deny",
            "motivo": "bypass",
            "porque": "a chamada seguinte carrega bypass (TOURING_GATE_OK=1 ou kill switch)",
        }
    if any(x in par["blob"] for x in reals):
        return {
            **base,
            "veredito": "deny correto",
            "porque": "a chamada seguinte usa um dos `reais`",
        }
    if nomes and all(n in par["blob"] for n in nomes):
        return {
            **base,
            "veredito": "falso deny",
            "motivo": "reenvio identico",
            "porque": f"a chamada seguinte repete {nomes}",
        }
    if nomes:
        return {
            **base,
            "veredito": "deny correto",
            "porque": "a chamada seguinte nao contem nenhum dos nomes negados",
        }
    return {
        **base,
        "veredito": "nao classificado",
        "porque": "a chamada seguinte existe, mas o deny nao nomeou simbolo nenhum",
    }


def fala_falso_denovo(sessao: str, ts: datetime, janela_s: int = 1800) -> str | None:
    """(c) texto do assistente OU do usuario dizendo que o simbolo existe / falso positivo."""
    ini, fim = z(ts), z(ts + timedelta(seconds=janela_s))
    for f in PROJ.rglob(f"{sessao}.jsonl"):
        for raw in f.open(encoding="utf-8", errors="replace"):
            try:
                rec = json.loads(raw)
            except json.JSONDecodeError:
                continue
            ts2 = str(rec.get("timestamp", ""))
            if ts2 < ini or ts2 > fim:
                continue
            b = (rec.get("message") or {}).get("content")
            txt = (
                b
                if isinstance(b, str)
                else " ".join(
                    str(x.get("text", "")) for x in (b or []) if isinstance(x, dict)
                )
            )
            m = FALSO_POSITIVO.search(txt or "")
            if m:
                return m.group(0)
    return None


# ---------------------------------------------------------------- veredito
def julga(registros: list[dict], desde: str, ate: str) -> dict:
    ds, ign = denies(registros, desde, ate)
    vs = []
    for r in ds:
        v = classifica(r)
        fala = fala_falso_denovo(str(r.get("session_id", "")), iso(str(r["ts"])))
        if fala and v["veredito"] == "deny correto":
            v = {
                **v,
                "veredito": "falso deny",
                "motivo": "falso positivo reportado",
                "porque": f"a sessao disse: {fala!r}",
            }
        vs.append(v)
    ok = [v for v in vs if v["veredito"] == "deny correto"]
    sessoes = {v["session_id"] for v in ok}
    return {
        "denys": len(vs),
        "corretos": len(ok),
        "falsos": sum(1 for v in vs if v["veredito"] == "falso deny"),
        "nao_classificados": sum(1 for v in vs if v["veredito"] == "nao classificado"),
        "sessoes_com_deny_correto": len(sessoes),
        "ignorados": ign,
        "sessoes": sorted(sessoes),
        "detalhe": vs,
    }


def exit_code(j: dict) -> int:
    if j["falsos"]:
        return 1
    if j["denys"] == 0:
        return 3  # dado insuficiente: NUNCA "0 falsos" sem nenhum deny
    return 0 if j["sessoes_com_deny_correto"] >= 5 else 3


# ---------------------------------------------------------------- selftest (fixtures sinteticas)
def selftest(dir_fx: Path) -> int:
    checks = {}
    larg = ("1970-01-01T00:00:00Z", "9999-12-31T23:59:59Z")

    def roda(nome):
        return julga(le_sombra(dir_fx / nome), *larg)

    j = roda("vazio.jsonl")
    checks["negativo: sombra vazio -> exit 3 (nunca '0 falsos')"] = (
        j["denys"] == 0 and exit_code(j) == 3
    )
    for nome, esp in [
        ("cinco_sessoes.jsonl", 0),
        ("quatro_sessoes.jsonl", 3),
        ("um_bypass.jsonl", 1),
        ("reenvio.jsonl", 1),
    ]:
        j = roda(nome)
        checks[f"{nome} -> exit {esp}"] = exit_code(j) == esp
    j = roda("sombra_modo.jsonl")
    checks["modo=sombra nunca conta (denys=0)"] = j["denys"] == 0 and exit_code(j) == 3
    j = roda("sem_par.jsonl")
    checks["deny sem par -> nao classificado, fora dos corretos"] = (
        j["nao_classificados"] == 1 and j["corretos"] == 0
    )
    # (c) o par estava PERFEITO (o nome sumiu) mas a sessao reportou falso positivo: o veredito tem
    # de virar falso deny. Sem este teste, um juiz que so olha o par classifica isto como correto.
    j = roda("fala_falso.jsonl")
    checks["falso positivo reportado vira falso deny (predicado (c))"] = (
        j["falsos"] == 1 and j["corretos"] == 0
    )
    # as frases do predicado (c), testadas uma a uma — o que nao casa tambem e um controle
    for frase, deve in [
        ("falso positivo: o simbolo existe", True),
        ("o simbolo nao existe mesmo", True),
        ("isso e um false positive do VGP", True),
        ("o gate errou, deny incorreto", True),
        ("o simbolo foi definido la em cima", False),
        ("corrigir o nome e rerodar", False),
    ]:
        checks[f"frase(c) {'casa' if deve else 'nao casa'}: {frase[:34]!r}"] = (
            bool(FALSO_POSITIVO.search(frase)) is deve
        )
    # (a) as tres formas de bypass
    for b, deve in [
        ("TOURING_GATE_OK=1 ls", True),
        ("TOURING_VGP_DISABLED=1 cargo x", True),
        ("TOURING_CODE_GATES_DISABLED=1 p", True),
        ("ls -la", False),
    ]:
        checks[f"bypass {'casa' if deve else 'nao casa'}: {b[:30]!r}"] = (
            bool(BYPASS.search(b)) is deve
        )
    for k, v in checks.items():
        print(f"  {'ok ' if v else 'FALHOU'} {k}")
    return 0 if all(checks.values()) else 1


def main() -> int:
    if "--selftest" in sys.argv:
        d = (
            Path(sys.argv[sys.argv.index("--fixtures") + 1])
            if "--fixtures" in sys.argv
            else Path(__file__).resolve().parent.parent / "fixtures_v4"
        )
        return selftest(d)
    for flag in ("--desde", "--ate"):
        if flag not in sys.argv:
            print(f"{flag} OBRIGATORIO: sem instante congelado o numero nao reproduz.")
            return 2
    desde, ate = (sys.argv[sys.argv.index(f) + 1] for f in ("--desde", "--ate"))
    cam = (
        Path(sys.argv[sys.argv.index("--sombra") + 1])
        if "--sombra" in sys.argv
        else SOMBRA_PADRAO
    )
    j = julga(le_sombra(cam), desde, ate)
    print(
        json.dumps(
            {k: v for k, v in j.items() if k != "detalhe"}, ensure_ascii=False, indent=1
        )
    )
    for v in j["detalhe"]:
        print(
            f"  {v['ts']} {str(v['session_id'])[:12]:<12} {v['veredito']:<18} {v['porque'][:66]}"
        )
    rc = exit_code(j)
    print(f"exit={rc}")
    return rc


if __name__ == "__main__":
    sys.exit(main())
