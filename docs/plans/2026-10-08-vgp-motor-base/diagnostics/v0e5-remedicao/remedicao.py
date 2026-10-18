"""V0e5-remedicao — os gates depois dos deploys, com exposicao, IC e os AVISOS que nao sao erro.

Duas correcoes que a entrega anterior provou necessarias:

1. **AVISO nao e erro de ferramenta.** O S3 passou a avisar no 30.4.95: o aviso vive num registro
   `attachment` (`hook_additional_context` / `hook_success`), NAO num `tool_result` is_error. Ler so
   tool_result da zero por construcao — era o que fez a entrega anterior reportar "0 avisos".
   `content` desses anexos e uma LISTA DE STRINGS (nao string, nao lista de dict): o `_text` do
   inventario_erros, que so junta `x.get("text")`, devolve VAZIO nela. Por isso `texto_de_anexo` aqui.

2. **A ORDEM de GATES rouba o G10.** O texto do exec-burst ("... a rajada desenrolada ...") casa o
   padrao do G1 (`RAJADA`), e o G1 e o PRIMEIRO da lista: `gate_of` credita ao G1 e o G10 nunca
   aparece. E o mesmo defeito que a V0e5 mediu no balde G1/S3/G10 — aqui no par G1/G10.

A regua (`v0e_negacoes`) e IMPORTADA e nunca editada. `--ate` e OBRIGATORIO: sem um instante congelado
a janela cresce com a sessao e o numero nao reproduz.

Uso: python3 remedicao.py --ate <ISO> [--selftest]
"""

import json
import math
import re
import sys
from collections import Counter
from datetime import datetime
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCRIPTS = HERE.parents[1] / "scripts"
assert (SCRIPTS / "v0e_negacoes.py").exists(), f"scripts errado: {SCRIPTS}"
sys.path.insert(0, str(SCRIPTS))
from inventario_erros import ROOTS, SINCE  # noqa: E402
from v0e_negacoes import COMPILED, HOOK_DENIAL, gate_of  # noqa: E402

DEPLOY_94 = "2026-10-09T14:16:43Z"
DEPLOY_95 = "2026-10-09T16:32:42Z"

# O AVISO do S3: ele nao nega, ele ensina. Casa pelo prefixo literal do texto do anexo.
AVISO_S3 = re.compile(r"^\[CODE MODE · rajada\] aviso", re.I)
# O G10, escolhido pelo SEU prefixo — nao pelo padrao do G1, que lhe rouba a precedencia.
# O prefixo do hook ("PreToolUse:Bash hook error: ") VARIA (a ferramenta pode ser outra), entao o
# padrao casa o G10 em qualquer posicao e depois ancora no texto. A versao com `^\[(?:PreToolUse...)`
# — prefixo opcional DEPOIS do ^ — nunca casava nada, e o selftest nao pegou porque testava o texto
# sem o prefixo do hook. Por isso o controle abaixo usa o texto REAL, com o prefixo.
G10_PROPRIO = re.compile(r"\[G10 (?:exec-burst|write→run)\]")
G10_ROTA = re.compile(r"touring run", re.I)


def janelas(ate: str) -> dict:
    return {
        "A": ("2026-09-17T00:00:00Z", DEPLOY_94),
        "B": (DEPLOY_94, DEPLOY_95),
        "C": (DEPLOY_95, ate),
    }


def janela_de(ts: str, jn: dict) -> str | None:
    for nome, (ini, fim) in jn.items():
        if ini <= ts < fim:
            return nome
    return None


def texto_de_anexo(att: dict) -> str:
    """O texto de um attachment. `content` pode ser str OU lista de str — a lista de str e o caso do
    aviso do S3, e trata-la como lista de dict devolve vazio (o erro da 1a versao). Uma lista de dict
    tambem pode aparecer (chave 'text'), e ai o texto e o do dict."""
    c = att.get("content")
    if isinstance(c, str):
        return c
    if isinstance(c, list):
        return "\n".join(x for x in c if isinstance(x, str)) or " ".join(
            x.get("text", "") for x in c if isinstance(x, dict)
        )
    return json.dumps(c, ensure_ascii=False) if c is not None else ""


def texto_de_result(c: dict) -> str:
    """O texto de um tool_result (str ou lista de dict com 'text')."""
    b = c.get("content")
    if isinstance(b, str):
        return b
    if isinstance(b, list):
        return " ".join(x.get("text", "") for x in b if isinstance(x, dict))
    return ""


def wilson(k: int, n: int, z: float = 1.96) -> tuple[float, float]:
    """IC de Wilson 95%. n=0 devolve (0.0, 1.0): ausencia de dado, nunca 0%."""
    if n == 0:
        return 0.0, 1.0
    p = k / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return max(0.0, c - h), min(1.0, c + h)


def classifica(texto: str) -> tuple[str, object] | None:
    """(gate, rota) pela regua — mas o G10 e decidido pelo SEU prefixo antes, para nao ser roubado
    pelo padrao do G1 (ver docstring)."""
    if G10_PROPRIO.search(texto):
        # O nome vem do SUB-GATE que o proprio texto nomeia (`[G10 exec-burst]` vs `[G10 write→run]`),
        # nao do primeiro nome que startswith — exec-burst vem antes na lista e roubava os dois.
        sub = "write→run" if "write→run" in texto else "exec-burst"
        for nome, pat, rota in COMPILED:
            if nome.startswith(sub):
                return nome, rota
        return sub, G10_ROTA
    for nome, pat, rota in COMPILED:
        if pat.search(texto):
            return nome, rota
    return None


def varre(ate: str) -> tuple[dict, list]:
    """(acumulador por janela, lista de TODOS os eventos de C para conferencia)."""
    jn = janelas(ate)
    acc = {
        j: {
            "neg": Counter(),
            "exp": Counter(),
            "avisos": 0,
            "av_rota": 0,
            "av_jud": 0,
            "rota_ok": Counter(),
            "rota_jud": Counter(),
        }
        for j in jn
    }
    eventos = []
    desde = datetime.fromisoformat(SINCE).timestamp()
    for root in ROOTS:
        for f in root.rglob("*.jsonl"):
            if f.stat().st_mtime < desde:
                continue
            nomes, prox = {}, None
            for raw in f.open(encoding="utf-8", errors="replace"):
                if (
                    '"tool_use"' not in raw
                    and '"tool_result"' not in raw
                    and '"attachment"' not in raw
                ):
                    continue
                try:
                    rec = json.loads(raw)
                except json.JSONDecodeError:
                    continue
                ts = str(rec.get("timestamp", ""))
                j = janela_de(ts, jn)
                if j is None:
                    continue
                att = rec.get("attachment") or {}
                if att.get("type") in ("hook_additional_context", "hook_success"):
                    t = texto_de_anexo(att)
                    if AVISO_S3.search(t.strip()):
                        acc[j]["avisos"] += 1
                        if prox is not None:
                            acc[j]["av_jud"] += 1
                            acc[j]["av_rota"] += bool(
                                G10_ROTA.search(
                                    json.dumps(
                                        {
                                            "name": prox.get("name"),
                                            **(prox.get("input") or {}),
                                        },
                                        ensure_ascii=False,
                                    )
                                )
                            )
                        if j == "C":
                            eventos.append(
                                {
                                    "ts": ts,
                                    "sessao": f.stem,
                                    "tipo": "AVISO S3",
                                    "gate": "S3 (aviso)",
                                    "toolUseID": att.get("toolUseID"),
                                    "texto": t[:120],
                                }
                            )
                    continue
                for c in (rec.get("message") or {}).get("content") or []:
                    if not isinstance(c, dict):
                        continue
                    if c.get("type") == "tool_use":
                        nomes[c["id"]] = c.get("name")
                        prox = c
                        acc[j]["exp"][c.get("name")] += 1
                    elif c.get("type") == "tool_result":
                        if not c.get("is_error") or not HOOK_DENIAL.search(
                            texto_de_result(c)
                        ):
                            continue
                        g = classifica(texto_de_result(c))
                        if g is None:
                            continue
                        nome, rota = g
                        acc[j]["neg"][nome] += 1
                        if prox is not None and rota is not None:
                            acc[j]["rota_jud"][nome] += 1
                            acc[j]["rota_ok"][nome] += bool(
                                rota.search(
                                    json.dumps(
                                        {
                                            "name": prox.get("name"),
                                            **(prox.get("input") or {}),
                                        },
                                        ensure_ascii=False,
                                    )
                                )
                            )
                        if j == "C":
                            eventos.append(
                                {
                                    "ts": ts,
                                    "sessao": f.stem,
                                    "tipo": "negacao",
                                    "gate": nome,
                                    "toolUseID": c.get("tool_use_id"),
                                    "texto": texto_de_result(c)[:120],
                                }
                            )
                        prox = None
    return acc, eventos


# gate -> ferramenta exposta. O write→run dispara no RUN do script, nao no Write: expoe Bash.
GATES = {
    "G1 rajada-de-inspeção": "Bash",
    "exec-burst (python-inline seriado)": "Bash",
    "write→run (script escrito-e-executado)": "Bash",
    "G11 orçamento de bypass": "Bash",
    "rm guard": "Bash",
    "G7 re-inspeção (3ª leitura)": "Read",
    "G3 edit sem Read recente": "Edit",
    "python_pre_gate (F82 etc.)": "Edit",
}

# Os 4 controles POSITIVOS NOMEADOS, com o instante e o texto real lidos do transcript e7c33fba.
CONTROLES = [
    ("2026-10-09T16:33:28.421Z", "AVISO S3", "S3 (aviso)", "grep"),
    ("2026-10-09T17:30:30.875Z", "AVISO S3", "S3 (aviso)", "python-inline"),
    ("2026-10-09T17:31:31.293Z", "negacao", "write→run", "escrito-e-executado"),
    ("2026-10-09T17:35:24.110Z", "negacao", "exec-burst", "chamada seriada"),
]


def selftest(ate: str | None) -> int:
    """Os 4 controles NOMEADOS + o negativo (n=0) + o par texto_de_anexo."""
    checks = {
        "wilson n=0 e (0,1)": wilson(0, 0) == (0.0, 1.0),
        "wilson 0/10": abs(wilson(0, 10)[1] - 0.2775) < 0.001,
        "texto_de_anexo: str": texto_de_anexo({"content": "x"}) == "x",
        "texto_de_anexo: LISTA DE STRINGS (o caso do S3)": texto_de_anexo(
            {"content": ["a", "b"]}
        )
        == "a\nb",
        "texto_de_anexo: lista de dict": texto_de_anexo({"content": [{"text": "a"}]})
        == "a",
        "aviso S3 casa": bool(
            AVISO_S3.search("[CODE MODE · rajada] aviso — 2ª inspeção")
        ),
        "negacao do S3 NAO e aviso": not AVISO_S3.search(
            "[CODE MODE · rajada] 2ª inspeção `grep` em 300s — a 1ª já executou"
        ),
        "G10 pelo prefixo proprio, COM o prefixo do hook (texto real)": classifica(
            "PreToolUse:Bash hook error: [G10 exec-burst] 5ª chamada seriada do executor"
        )[0].startswith("exec-burst"),
        "G10 exec-burst NAO e creditado ao G1": not classifica(
            "PreToolUse:Bash hook error: [G10 exec-burst] 5ª chamada seriada — a rajada desenrolada"
        )[0].startswith("G1"),
        "write->run pelo prefixo, COM o prefixo do hook": classifica(
            "PreToolUse:Bash hook error: [G10 write→run] 2º script escrito-e-executado"
        )[0].startswith("write→run"),
    }
    if ate:
        acc, ev = varre(ate)
        for ts, tipo, gate, marca in CONTROLES:
            achou = [
                e for e in ev if e["ts"].startswith(ts[:19]) and marca in e["texto"]
            ]
            # o gate precisa ser o ESPERADO: achar o texto e classificar em outro gate e o defeito
            # exato que a V0e5 mediu (o padrao do G1 roubando o G10).
            checks[f"CONTROLE {ts[11:19]} {tipo} {marca}"] = any(
                e["gate"].startswith(gate.split(" ")[0]) for e in achou
            )
    else:
        checks["CONTROLES (rode com --ate para checa-los)"] = False
    for k, v in checks.items():
        print(f"  {'ok ' if v else 'FALHOU'} {k}")
    return 0 if all(checks.values()) else 1


def main() -> int:
    if "--selftest" in sys.argv:
        return selftest(
            sys.argv[sys.argv.index("--ate") + 1] if "--ate" in sys.argv else None
        )
    if "--ate" not in sys.argv:
        print(
            "--ate OBRIGATORIO: sem um instante congelado a janela cresce com a sessao "
            "e o numero nao reproduz."
        )
        return 2
    ate = sys.argv[sys.argv.index("--ate") + 1]
    acc, ev = varre(ate)
    jn = janelas(ate)
    out = {"ate": ate, "since": SINCE, "janelas": jn, "gates": {}, "eventos_C": ev}
    for g, ferr in GATES.items():
        lin = {"ferramenta_exposta": ferr, "janelas": {}}
        for j in jn:
            n, k = acc[j]["exp"][ferr], acc[j]["neg"].get(g, 0)
            lo, hi = wilson(k, n)
            ju = acc[j]["rota_jud"].get(g, 0)
            lin["janelas"][j] = {
                "exposicao": n,
                "negacoes": k,
                "taxa_por_100": round(k / n * 100, 2) if n else "sem exposicao",
                "ic95": [round(lo * 100, 2), round(hi * 100, 2)] if n else None,
                "rota_seguida": round(acc[j]["rota_ok"].get(g, 0) / ju * 100, 1)
                if ju
                else None,
                "rota_julgada": ju,
            }
        out["gates"][g] = lin
    a = acc["C"]
    n_av, k_av = a["av_jud"], a["av_rota"]
    lo, hi = wilson(k_av, n_av)
    out["s3_avisos"] = {
        "C": {
            "avisos": a["avisos"],
            "julgados": n_av,
            "seguiram_rota": k_av,
            "taxa": round(k_av / n_av * 100, 1) if n_av else "sem dado",
            "ic95": [round(lo * 100, 2), round(hi * 100, 2)] if n_av else None,
        }
    }
    (HERE / "remedicao.json").write_text(json.dumps(out, ensure_ascii=False, indent=1))
    print(f"ate={ate}  eventos em C: {len(ev)}")
    for e in sorted(ev, key=lambda x: x["ts"]):
        print(
            f"  {e['ts'][:19]}  {e['tipo']:<10} {e['gate'][:34]:<34} {e['texto'][:70]!r}"
        )
    print(f"\nS3 avisos C: {out['s3_avisos']['C']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
