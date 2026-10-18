#!/usr/bin/env python3
"""Juiz do motor VGP (V2/V5 da proposta) contra o corpus V0d — exit 0 = aceite.

Escrito pela coordenadora ANTES do motor (o aceite precede a execução). Contrato que o motor cumpre,
sem daemon (puro, como `touring ast polyglot-semantic`):

    touring vgp check --path <caminho virtual> --content-file <arquivo com o conteúdo proposto>
                      --project-root <raiz> [--disk-file <conteúdo em disco>] [--overlay <orig>=<arquivo>]...
                      [--lang python|typescript|rust] -j

    -> {"verdict": "deny"|"warn"|"allow"|"unknown", "layer": 1, "lang": "python", "elapsed_ms": 3.2,
        "findings": [{"classe": "nome"|"atributo"|"import"|"kwarg"|"membro", "nome": "x", "linha": 12,
                      "severidade": "deny"|"warn", "sugestao": "...", "reais": ["..."]}]}

Régua sem deriva (09/10/2026): com `estado_no_instante/manifesto.json` no corpus, cada negativo vai com
`--disk-file` (o arquivo 1 ms antes da edição) e cada entrada com `--overlay` dos módulos do projeto no instante
dela. Sem o manifesto, a régua é a estrita (conteúdo inteiro contra o projeto de hoje).
`path` `<string>`/`<stdin>` = código inline (V5): os nomes se resolvem contra `--project-root`. `--overlay` sobrepõe
um arquivo ao disco na resolução de módulos (os irmãos em /tmp que o script importa, campo `contexto` do corpus).

Métricas (só entradas cujo `reproduz` é verdadeiro contam no recall — spec V0d §4):
  recall     = positivos de SÍMBOLO com um achado (deny OU warn) cujo `nome` == `nome_esperado` / positivos de símbolo
  falso_deny = negativos com QUALQUER achado `deny` / negativos, POR BALDE (arquivo · inline) que tenha negativos
  p95        = percentil 95 de `elapsed_ms` do motor (o hook roda no daemon; o spawn do processo não conta)
Aceite V2 (proposta §6): recall ≥ 0,70 nos positivos de símbolo de arquivo (origem atribuido|tmp|cache) · falso_deny
≤ 1 % em cada balde · p95 ≤ 150 ms. V5: recall no inline MEDIDO e reportado (`--v5-estrito` exige ≥ 0,70 também).
Falha fechada: saída que não é JSON, ou sem `verdict`, conta como falha do motor (rc 1), nunca como "allow".

Revisão v2 (09/10/2026, medida — `diagnostics/sonda_camada2_pyright_analise_v2_{instante,estrita}.json`):
  (1) SÍMBOLO × FORMA pela mensagem do runtime, predicado positivo por classe: `'list' object has no attribute 'get'`
      é a FORMA de um dado (24 dos 65 positivos reproduzidos; o pyright pega 1 de 21 no analise — `json.load` devolve
      `Any`), não um símbolo inventado: sai do recall e é reportada à parte, com o número. Sem classe → nomeado.
  (2) recall de DETECÇÃO (deny ou warn): na V2/V3 o motor está em sombra, e quem nega é a política da V4. Medido: com
      o estado do instante e o delta, sobram 10 de 259 negativos com achado real — 9 corrigidos numa edição seguinte
      (consumidor antes do provedor; 3 testes na fase RED) e 1 arquivo apagado. Negar ali é decisão de política,
      não erro do verificador; contar deny no recall puniria o motor por uma escolha que ainda não é dele.
  (3) falso deny POR BALDE: os negativos inline/tmp da V5 não se diluem nos 300 de arquivo.

V3 (proposta §6 — comparativo): `--arg-motor=--camadas --arg-motor=1,2 --p95-ms 1000 --base <json da V2>` reprova se o
recall de símbolo de QUALQUER balde não superar o da base, se o falso deny de algum balde piorar, ou p95 > 1 s.

Revisão v3 (09/10/2026, held-out disjunto 20261010 — `diagnostics/holdout/RELATORIO.md` §0, §3):
  o e935 deu 5/600 (arquivo) e 2/600 (inline) falsos deny: a taxa pontual (0,83 % e 0,33 %) passa de 1 %, o limite
  superior unilateral de Clopper-Pearson a 95 % (1,74 % e 1,05 %) não. Taxa pontual não prova "falso deny ≤ 1 %".
  (1) O limite superior 95 % de cada balde vai SEMPRE ao JSON (`falso_deny_por_balde.<balde>.limite_superior_95`) e à tabela.
  (2) `--exigir-limite-superior`: balde com limite > 1 % vira pendência (k, n, limite e quantos falsos o n permitiria).
      Sem a flag o veredito é o de antes, byte a byte (prova: `diagnostics/holdout-20261011/`).

Uso: python3 juiz_motor_vgp.py [--touring <bin>] [--corpus <dir>] [--v5-estrito] [--arg-motor A]... [--p95-ms N]
                               [--base <json>] [--json <saida>] [--exigir-limite-superior] | --selftest
"""

from __future__ import annotations

import argparse
import contextlib
import io
import json
import math
import re
import shutil
import subprocess
import sys
import tempfile
import warnings
from pathlib import Path

CORPUS = Path.home() / ".claude/touring/vgp/corpus_v0d"
LIMIAR_RECALL = 0.70
LIMIAR_FALSO_DENY = 0.01
CONF_LIMITE = (
    0.95  # confiança do limite superior UNILATERAL do falso deny (Clopper-Pearson)
)
LIMIAR_P95_MS = 150.0
ORIGENS_ARQUIVO = (
    "atribuido",
    "tmp",
    "cache",
    "semeado",
)  # semeado = arquivo real mutado (VL0 §7)
# Atributo de um VALOR builtin = forma do dado; o resto do que o runtime nomeia é símbolo. Predicado positivo para cada.
FORMA = re.compile(
    r"'(str|list|dict|int|float|tuple|bytes|set|frozenset|bool|NoneType)' object has no attribute"
)
SIMBOLO = re.compile(
    r"module '[^']+' has no attribute|cannot import name|No module named|is not defined"
    r"|unexpected keyword argument|object has no attribute|type object '[^']+' has no attribute"
)


# O oráculo de cada linguagem fala a própria língua: códigos do rustc e do tsc são o predicado positivo de símbolo.
SIMBOLO_POR_LANG = {
    "rust": re.compile(
        r"error\[E0(?:425|432|433|412|599|609|559|560|061|063|026)\]|cannot find (?:value|function|type|struct|macro"
        r"|module|trait|crate)|unresolved import|no (?:method|field|function|associated item|variant) named|has no field"
    ),
    "ts": re.compile(
        r"\bTS(?:2304|2305|2307|2322|2339|2353|2551|2552|2554|2561|2614|2724|2741)\b|Cannot find (?:name|module)"
        r"|has no exported member|does not exist on type"
    ),
}
FAMILIA = {"rust": "rust", "typescript": "ts", "tsx": "ts", "javascript": "ts"}
# Classes web (VL0 §4) não têm compilador: o rótulo vem do oráculo do corpus, que exige a mensagem dele.
CLASSES_WEB = frozenset({"recurso", "ancora", "cdn", "cdn_pin", "classe_css"})


def classe_erro(mensagem: str, lang: str = "python", classe: str = "") -> str:
    """forma | simbolo | nao_classificado, pela régua DA LINGUAGEM da entrada.

    Python: forma é o caso particular de `object has no attribute` (a ordem importa). Rust/TS/TSX/JS: o código do
    compilador na mensagem. Classes web: a mensagem do oráculo presente. Mensagem de uma linguagem lida sob a régua de
    outra é `nao_classificado` — nunca símbolo por coincidência."""
    msg = mensagem or ""
    if lang in ("", "python"):
        if FORMA.search(msg):
            return "forma"
        return "simbolo" if SIMBOLO.search(msg) else "nao_classificado"
    if classe in CLASSES_WEB:
        return "simbolo" if msg.strip() else "nao_classificado"
    rx = SIMBOLO_POR_LANG.get(FAMILIA.get(lang, ""))
    return "simbolo" if rx and rx.search(msg) else "nao_classificado"


def balde(entrada: dict) -> str:
    return "arquivo" if entrada.get("origem") in ORIGENS_ARQUIVO else "inline"


def p95(valores: list[float]) -> float:
    if not valores:
        return math.inf
    ordenados = sorted(valores)
    return ordenados[min(len(ordenados) - 1, math.ceil(0.95 * len(ordenados)) - 1)]


_ESTADO: dict[str, dict] = {}


def carregar_estado(corpus: Path) -> dict:
    """`estado_no_instante/manifesto.json` (scripts/estado_no_instante_v0d.py), se existir: o `--disk-file` de cada
    negativo (delta: o hook só nega o que a edição INTRODUZ) e os módulos do projeto NO INSTANTE da entrada (sem
    deriva: um nome que existia no dia da edição e saiu depois não é falso deny do motor). Ausente → régua estrita."""
    chave = str(corpus)
    if chave not in _ESTADO:
        m = corpus / "estado_no_instante" / "manifesto.json"
        _ESTADO[chave] = json.loads(m.read_text()) if m.is_file() else {}
    return _ESTADO[chave]


ARGS_MOTOR: list[
    str
] = []  # extras do motor (V3: ["--camadas", "1,2"]); preenchido pelo main


def rodar_motor(touring: list[str], entrada: dict, corpus: Path) -> dict:
    """Uma chamada do motor; devolve o JSON ou {"_erro": motivo} (falha fechada)."""
    cmd = [
        *touring,
        "vgp",
        "check",
        "--path",
        entrada["path"],
        "--content-file",
        str(corpus / entrada["snap"]),
        "--project-root",
        entrada["project_root"],
        "--lang",
        entrada.get("lang", "python"),
        *ARGS_MOTOR,
        "-j",
    ]
    estado = carregar_estado(corpus).get(entrada["id"], {})
    if estado.get("disk_file"):
        cmd += ["--disk-file", str(corpus / estado["disk_file"])]
    elif entrada.get("snap_antes"):
        # par natural M (held-out web): o rótulo é por DELTA, então o motor julga a edição contra o ANTES
        cmd += ["--disk-file", str(corpus / entrada["snap_antes"])]
    # os módulos no instante da edição primeiro; os `contexto` do corpus (irmãos em /tmp) por cima
    sobre = {o["path"]: corpus / o["file"] for o in estado.get("overlays", [])}
    sobre |= {c["path"]: corpus / c["snap"] for c in entrada.get("contexto") or []}
    for orig, arq in sobre.items():
        cmd += ["--overlay", f"{orig}={arq}"]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=60, check=False)
    except (OSError, subprocess.TimeoutExpired) as e:
        return {"_erro": f"{type(e).__name__}: {e}"}
    try:
        saida = json.loads(r.stdout)
    except json.JSONDecodeError:
        return {
            "_erro": f"rc {r.returncode}, stdout não-JSON: {r.stdout[:120]!r} · stderr: {r.stderr[:200]!r}"
        }
    if not isinstance(saida, dict) or "verdict" not in saida:
        return {"_erro": f"JSON sem `verdict`: {str(saida)[:120]}"}
    return saida


def detectou(saida: dict, nome: str, so_deny: bool = False) -> bool:
    return any(
        f.get("nome") == nome
        and f.get("severidade") in (("deny",) if so_deny else ("deny", "warn"))
        for f in saida.get("findings", [])
    )


def nega(saida: dict) -> bool:
    return any(f.get("severidade") == "deny" for f in saida.get("findings", []))


def carregar(corpus: Path) -> tuple[list[dict], list[dict]]:
    entradas = [
        json.loads(linha)
        for linha in (corpus / "entradas.jsonl").read_text().splitlines()
        if linha.strip()
    ]
    pos = [
        e
        for e in entradas
        if e["rotulo"] == "positivo" and str(e.get("reproduz")).lower() == "true"
    ]
    neg = [e for e in entradas if e["rotulo"] == "negativo"]
    # V5: negativos inline/tmp (execuções BEM-SUCEDIDAS) num arquivo à parte — o `entradas.jsonl` da V0d fica intacto
    # (o juiz da V0d conta 391 · 91 · 300 exatos).
    extra = corpus / "entradas_inline_neg.jsonl"
    if extra.is_file():
        neg += [json.loads(x) for x in extra.read_text().splitlines() if x.strip()]
    return pos, neg


def novo_acumulador() -> dict:
    return {
        "tempos": [],
        "erros": [],
        "falsos": [],
        "perdidos": [],
        "simbolo": {"arquivo": [0, 0], "inline": [0, 0]},
        "deny": {"arquivo": [0, 0], "inline": [0, 0]},
        "forma": {"arquivo": [0, 0], "inline": [0, 0]},
        "nao_classificado": [],
        "negativos": {"arquivo": 0, "inline": 0},
        "avisos_em_negativos": 0,
        "defeito_de_corpus": [],
    }


def contar_negativo(m: dict, e: dict, saida: dict) -> None:
    m["negativos"][balde(e)] += 1
    achados = saida.get("findings", [])
    if any(f.get("severidade") == "warn" for f in achados):
        m["avisos_em_negativos"] += 1
    if nega(saida):
        negs = [f for f in achados if f.get("severidade") == "deny"][:3]
        m["falsos"].append(
            {"id": e["id"], "balde": balde(e), "path": e["path"], "achados": negs}
        )


def contar_positivo(m: dict, e: dict, saida: dict) -> None:
    classe = classe_erro(
        e.get("mensagem") or "", e.get("lang") or "python", e.get("classe") or ""
    )
    b, nome = balde(e), e["nome_esperado"]
    if classe == "nao_classificado":
        m["nao_classificado"].append(e["id"])
        return
    alvo = m[classe][b]
    alvo[1] += 1
    acertou = detectou(saida, nome)
    alvo[0] += acertou
    if classe != "simbolo":
        return
    m["deny"][b][1] += 1
    m["deny"][b][0] += detectou(saida, nome, so_deny=True)
    if not acertou:
        m["perdidos"].append(
            {
                "id": e["id"],
                "origem": e.get("origem"),
                "classe": e.get("classe"),
                "esperado": nome,
                "verdict": saida.get("verdict"),
            }
        )


def snap_compila(corpus: Path, e: dict) -> bool:
    """Um snap Python que não compila NÃO é o programa que rodou: o NameError/AttributeError do positivo e o sucesso
    do negativo só acontecem depois do parse. Medido em 09/10: 3 dos 50 positivos inline do V0d (`bash -c '…'`
    aninhado) e 10 dos 300 negativos inline (`-c` sem o desescape do shell). Sai de todo denominador, nomeado."""
    if e.get("lang", "python") != "python":
        return True
    texto = (corpus / e["snap"]).read_text(errors="ignore")
    try:
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            compile(texto, e["id"], "exec")
    except (SyntaxError, ValueError):
        return False
    return True


def medir(touring: list[str], corpus: Path, pos: list[dict], neg: list[dict]) -> dict:
    """Roda o motor em cada entrada e acumula; nenhuma decisão de aceite aqui."""
    m = novo_acumulador()
    for e in pos + neg:
        if not snap_compila(corpus, e):
            m["defeito_de_corpus"].append(e["id"])
            continue
        saida = rodar_motor(touring, e, corpus)
        if "_erro" in saida:
            m["erros"].append({"id": e["id"], "erro": saida["_erro"]})
            continue
        m["tempos"].append(float(saida.get("elapsed_ms", math.inf)))
        (contar_negativo if e["rotulo"] == "negativo" else contar_positivo)(m, e, saida)
    return m


def razao(par: list[int]) -> float:
    return par[0] / par[1] if par[1] else 0.0


# Cópia de `diagnostics/instrumentos-2026-10-09/limites.py` (o juiz é autocontido: não importa nada do bundle).
def cdf_binomial(k: int, n: int, p: float) -> float:
    """P(X <= k) para X ~ Binomial(n, p), somando em escala log (estável para n grande)."""
    if p <= 0.0:
        return 1.0
    if p >= 1.0:
        return 1.0 if k >= n else 0.0
    termos = [
        math.lgamma(n + 1)
        - math.lgamma(i + 1)
        - math.lgamma(n - i + 1)
        + i * math.log(p)
        + (n - i) * math.log1p(-p)
        for i in range(k + 1)
    ]
    m = max(termos)
    return math.exp(m) * sum(math.exp(t - m) for t in termos)


def limite_superior(k: int, n: int, conf: float = CONF_LIMITE) -> float:
    """Limite superior UNILATERAL de Clopper-Pearson (exato) para k falhas em n: k = 0 → 1 − (1−conf)^(1/n); k > 0 → o p
    que resolve P(X <= k | n, p) = 1 − conf, por bisseção (sem scipy)."""
    if n <= 0:
        raise ValueError("n deve ser positivo")
    if k >= n:
        return 1.0
    if k == 0:
        return 1.0 - (1.0 - conf) ** (1.0 / n)
    lo, hi = k / n, 1.0
    for _ in range(80):
        meio = (lo + hi) / 2
        if cdf_binomial(k, n, meio) > 1.0 - conf:
            lo = meio
        else:
            hi = meio
    return (lo + hi) / 2


def limite_do_balde(falsos: int, n: int) -> float:
    """O limite superior 95 % do falso deny de um balde: é ele, não a taxa pontual, que prova `falso deny ≤ teto`."""
    return limite_superior(falsos, n, CONF_LIMITE)


def falsos_permitidos(n: int, teto: float = LIMIAR_FALSO_DENY) -> int:
    """Maior k com limite_do_balde(k, n) <= teto (monótono em k → bisseção); −1 se nem k = 0 cabe (n pequeno demais).
    O limite é maior que k/n, então k < teto·n: o intervalo de busca é [−1, teto·n]."""
    lo, hi = -1, int(teto * n)
    while lo < hi:
        meio = (lo + hi + 1) // 2
        if limite_do_balde(meio, n) <= teto:
            lo = meio
        else:
            hi = meio - 1
    return lo


def n_minimo_para_zero_falsos(
    teto: float = LIMIAR_FALSO_DENY, conf: float = CONF_LIMITE
) -> int:
    """Menor n em que 0 falsos já dá limite superior <= teto (1 − (1−conf)^(1/n) <= teto)."""
    return math.ceil(math.log(1.0 - conf) / math.log(1.0 - teto))


def falsos_do_balde(m: dict, b: str) -> int:
    return sum(f["balde"] == b for f in m["falsos"])


def tabela_falso_deny(m: dict) -> dict:
    """Por balde com negativos: falsos, n, taxa pontual, limite superior 95 % e quantos falsos o n permitiria."""
    saida = {}
    for b, n in m["negativos"].items():
        if not n:
            continue
        k = falsos_do_balde(m, b)
        lim = limite_do_balde(k, n)
        saida[b] = {
            "falsos": k,
            "n": n,
            "taxa": k / n,
            "limite_superior_95": lim,
            "teto": LIMIAR_FALSO_DENY,
            "limite_ate_o_teto": lim <= LIMIAR_FALSO_DENY,
            "falsos_permitidos_pelo_teto": falsos_permitidos(n),
        }
    return saida


def pendencias_limite(m: dict) -> list[str]:
    """`--exigir-limite-superior`: o balde cujo limite superior 95 % passa do teto é pendência, mesmo que a taxa pontual
    passe. A mensagem diz k, n, o limite e quantos falsos esse n permitiria (ou o n necessário, se nem 0 falsos basta)."""
    pend = []
    for b, t in tabela_falso_deny(m).items():
        if t["limite_ate_o_teto"]:
            continue
        k, n, perm = t["falsos"], t["n"], t["falsos_permitidos_pelo_teto"]
        prefixo = f"falso deny ({b}) limite superior 95% {t['limite_superior_95']:.2%} > {LIMIAR_FALSO_DENY:.0%} ({k}/{n}, taxa {t['taxa']:.3%})"
        if perm >= 0:
            pend.append(
                f"{prefixo} — com n={n} o teto admite no máximo {perm} falso(s)"
            )
        else:
            pend.append(
                f"{prefixo} — n={n} não prova ≤ {LIMIAR_FALSO_DENY:.0%} nem com 0 falsos; exige n ≥ {n_minimo_para_zero_falsos()}"
            )
    return pend


def pendencias(m: dict, v5_estrito: bool, exigir_limite: bool = False) -> list[str]:
    pend = []
    if m["erros"]:
        pend.append(
            f"{len(m['erros'])} chamada(s) do motor falharam (falha fechada) — 1ª: {m['erros'][0]}"
        )
    recall = {
        b: m["simbolo"][b]
        for b in (["arquivo", "inline"] if v5_estrito else ["arquivo"])
    }
    for b, par in recall.items():
        if razao(par) < LIMIAR_RECALL:
            pend.append(
                f"recall de símbolo ({b}) {razao(par):.3f} < {LIMIAR_RECALL} ({par[0]}/{par[1]})"
            )
    for b, n in m["negativos"].items():
        falsos = falsos_do_balde(m, b)
        if n and falsos / n > LIMIAR_FALSO_DENY:
            pend.append(
                f"falso deny ({b}) {falsos / n:.3%} > {LIMIAR_FALSO_DENY:.0%} ({falsos}/{n})"
            )
    if exigir_limite:
        pend += pendencias_limite(m)
    if p95(m["tempos"]) > LIMIAR_P95["ms"]:
        pend.append(f"p95 {p95(m['tempos']):.1f} ms > {LIMIAR_P95['ms']} ms")
    return pend


LIMIAR_P95 = {"ms": LIMIAR_P95_MS}  # a V3 sobe para 1000 ms (`--p95-ms`)


def contra_a_base(v: dict, base: dict) -> list[str]:
    """V3: o recall de símbolo tem de SUPERAR o da base em algum balde sem cair em nenhum, e nenhum balde pode ter mais
    falsos deny que a base."""
    pend, ganhou = [], False
    for b, par in v["positivos_simbolo"].items():
        antes = base["positivos_simbolo"][b]
        if razao(par) < razao(antes):
            pend.append(
                f"recall de símbolo ({b}) caiu: {par[0]}/{par[1]} < base {antes[0]}/{antes[1]}"
            )
        ganhou |= razao(par) > razao(antes)
    if not ganhou:
        pend.append(
            "recall de símbolo não superou a base em nenhum balde (a camada nova não acrescenta)"
        )
    for b in v["negativos"]:
        agora = sum(f["balde"] == b for f in v["falsos"])
        antes = sum(f["balde"] == b for f in base.get("falsos", []))
        if agora > antes:
            pend.append(f"falso deny ({b}) piorou: {agora} > base {antes}")
    return pend


def julgar(
    touring: list[str],
    corpus: Path,
    v5_estrito: bool,
    exigir_limite: bool = False,
) -> dict:
    pos, neg = carregar(corpus)
    if not pos or not neg:
        return {
            "pronto": False,
            "pendencias": [
                f"corpus degenerado: {len(pos)} positivos reproduzidos, {len(neg)} negativos"
            ],
        }
    m = medir(touring, corpus, pos, neg)
    if not m["simbolo"]["arquivo"][1] and not m["erros"]:
        return {
            "pronto": False,
            "pendencias": [
                "corpus sem positivo de SÍMBOLO de arquivo — o recall não é medível"
            ],
        }
    pend = pendencias(m, v5_estrito, exigir_limite)
    return {
        "pronto": not pend,
        "pendencias": pend,
        "exigir_limite_superior": exigir_limite,
        "falso_deny_por_balde": tabela_falso_deny(m),
        "recall_simbolo": {b: razao(par) for b, par in m["simbolo"].items()},
        "positivos_simbolo": m["simbolo"],
        "recall_deny_informativo": m["deny"],
        "forma_fora_do_recall": m["forma"],
        "nao_classificado": m["nao_classificado"],
        "falsos": m["falsos"],
        "negativos": m["negativos"],
        "avisos_em_negativos": m["avisos_em_negativos"],
        "defeito_de_corpus": m["defeito_de_corpus"],
        "p95_ms": p95(m["tempos"]),
        "chamadas": len(m["tempos"]),
        "erros": m["erros"][:10],
        "perdidos": m["perdidos"],
    }


def motor_ausente(touring: list[str]) -> str | None:
    try:
        r = subprocess.run(
            [*touring, "vgp", "check", "--help"],
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
    except OSError as e:
        return f"binário do touring não executa: {e}"
    if r.returncode != 0:
        return f"`touring vgp check` não existe neste binário (rc {r.returncode}): {(r.stderr or r.stdout)[:160]!r}"
    return None


# ---------- selftest: um motor falso por cenário prova que cada reprovação dispara ----------

MOTOR_FALSO = r"""#!/usr/bin/env python3
import json, sys
a = sys.argv[1:]
if a[:3] == ["vgp", "check", "--help"]:
    sys.exit(0)
modo = "{modo}"
conteudo = open(a[a.index("--content-file") + 1]).read()
nome = conteudo.split("#ESPERADO=")[1].split()[0] if "#ESPERADO=" in conteudo else None
if modo == "lixo":
    print("isto não é json"); sys.exit(0)
achados = []
if nome and modo != "cego":
    sev = "warn" if modo in ("so_avisa", "avisa_negativos") else "deny"
    achados.append({{"classe": "atributo", "nome": nome, "linha": 1, "severidade": sev}})
if modo in ("paranoico", "avisa_negativos") and not nome:
    sev = "deny" if modo == "paranoico" else "warn"
    achados.append({{"classe": "nome", "nome": "x", "linha": 1, "severidade": sev}})
if modo == "nega_inline" and "#INLINE" in conteudo:
    achados.append({{"classe": "nome", "nome": "z", "linha": 1, "severidade": "deny"}})
if modo == "pega_forma" and "#FORMA=" in conteudo:
    achados.append({{"classe": "atributo", "nome": "get", "linha": 1, "severidade": "deny"}})
print(json.dumps({{"verdict": "deny" if achados else "allow", "findings": achados,
                  "elapsed_ms": 500.0 if modo == "lento" else 2.0}}))
"""


def corpus_sintetico(
    raiz: Path,
    n_arquivo: int = 120,
    n_inline: int = 10,
    nega_arquivo: int = 0,
    nega_inline: int = 0,
) -> Path:
    """10 positivos de símbolo (7 de arquivo, 3 inline) + 4 de FORMA (que nenhum motor estático vê) e 120 negativos
    (+ 10 inline); o nome esperado viaja no conteúdo dos de símbolo. `nega_*` marca (`#NEGA`) os primeiros negativos do
    balde — o `_motor_em_processo` nega só esses (k falsos deny exatos em n)."""
    corpus = raiz / "corpus"
    (corpus / "snap").mkdir(parents=True)
    linhas = []
    for i in range(10):
        (corpus / "snap" / f"p{i}.py").write_text(f"x = 1  #ESPERADO=inventado{i}\n")
        arquivo = i < 7
        linhas.append(
            {
                "id": f"p{i}",
                "rotulo": "positivo",
                "reproduz": "True",
                "origem": "atribuido" if arquivo else "inline",
                "path": f"/proj/a{i}.py" if arquivo else "<string>",
                "project_root": "/proj",
                "snap": f"snap/p{i}.py",
                "nome_esperado": f"inventado{i}",
                "classe": "atributo",
                "mensagem": f"AttributeError: module 'm' has no attribute 'inventado{i}'",
                "lang": "python",
            }
        )
    for i in range(4):
        (corpus / "snap" / f"q{i}.py").write_text("def (  # não compila\n")
        linhas.append(
            {
                "id": f"q{i}",
                "rotulo": "positivo",
                "reproduz": "True",
                "origem": "atribuido",
                "path": f"/proj/q{i}.py",
                "project_root": "/proj",
                "snap": f"snap/q{i}.py",
                "nome_esperado": f"oculto{i}",
                "classe": "nome",
                "mensagem": f"NameError: name 'oculto{i}' is not defined",
                "lang": "python",
            }
        )
    for i in range(4):
        (corpus / "snap" / f"f{i}.py").write_text("d = carregar()  #FORMA=get\n")
        linhas.append(
            {
                "id": f"f{i}",
                "rotulo": "positivo",
                "reproduz": "True",
                "origem": "tmp",
                "path": f"/tmp/f{i}.py",
                "project_root": "/proj",
                "snap": f"snap/f{i}.py",
                "nome_esperado": "get",
                "classe": "atributo",
                "mensagem": "AttributeError: 'list' object has no attribute 'get'",
                "lang": "python",
            }
        )
    for i in range(n_arquivo):
        (corpus / "snap" / f"n{i}.py").write_text(
            "y = 2  #NEGA\n" if i < nega_arquivo else "y = 2\n"
        )
        linhas.append(
            {
                "id": f"n{i}",
                "rotulo": "negativo",
                "origem": "atribuido",
                "path": f"/proj/b{i}.py",
                "project_root": "/proj",
                "snap": f"snap/n{i}.py",
                "lang": "python",
            }
        )
    (corpus / "entradas.jsonl").write_text(
        "\n".join(json.dumps(x) for x in linhas) + "\n"
    )
    # V5: 10 negativos INLINE no arquivo à parte (execuções bem-sucedidas) — o balde inline é julgado sozinho
    inline = []
    for i in range(n_inline):
        (corpus / "snap" / f"i{i}.py").write_text(
            "z = 3  #INLINE #NEGA\n" if i < nega_inline else "z = 3  #INLINE\n"
        )
        inline.append(
            {
                "id": f"i{i}",
                "rotulo": "negativo",
                "origem": "inline",
                "path": "<stdin>",
                "project_root": "/proj",
                "snap": f"snap/i{i}.py",
                "lang": "python",
            }
        )
    (corpus / "entradas_inline_neg.jsonl").write_text(
        "\n".join(json.dumps(x) for x in inline) + "\n"
    )
    return corpus


# (mensagem, lang, classe, esperado) — o oráculo de cada linguagem fala a SUA língua; a régua do Python não serve às outras.
CASOS_CLASSE_ERRO = (
    ("NameError: name 'x' is not defined", "python", "nome", "simbolo"),
    (
        "AttributeError: 'list' object has no attribute 'get'",
        "python",
        "atributo",
        "forma",
    ),
    ("error[E0425]: cannot find value `x` in this scope", "rust", "nome", "simbolo"),
    (
        "error[E0599]: no method named `pusha` found for struct `Vec`",
        "rust",
        "atributo",
        "simbolo",
    ),
    (
        "error TS2339: Property 'z' does not exist on type 'Ponto'.",
        "typescript",
        "atributo",
        "simbolo",
    ),
    (
        "error TS2305: Module './m' has no exported member 'X'.",
        "tsx",
        "import",
        "simbolo",
    ),
    (
        "HEAD https://cdn.jsdelivr.net/npm/react@18.3.99/umd/react.production.min.js -> 404",
        "html",
        "cdn",
        "simbolo",
    ),
    (
        "classe 'btn-primry' não definida em nenhuma folha carregada",
        "tsx",
        "classe_css",
        "simbolo",
    ),
    ("", "html", "recurso", "nao_classificado"),
    (
        "error TS2339: Property 'z' does not exist on type 'Ponto'.",
        "python",
        "atributo",
        "nao_classificado",
    ),
    (
        "error[E0425]: cannot find value `x` in this scope",
        "typescript",
        "nome",
        "nao_classificado",
    ),
)


def _falhas_classe_erro() -> list[str]:
    """A tabela acima + o balde do corpus semeado: devolve as falhas (vazia = ok)."""
    falhas = [
        f"classe_erro({m[:40]!r}, {lang}, {c}) = {obtido} ≠ {esperado}"
        for m, lang, c, esperado in CASOS_CLASSE_ERRO
        if (obtido := classe_erro(m, lang, c)) != esperado
    ]
    if balde({"origem": "semeado"}) != "arquivo":
        falhas.append("balde(origem=semeado) ≠ arquivo")
    return falhas


# (falsos, n, limite superior 95 % em pontos percentuais): os valores de `diagnostics/holdout/RELATORIO.md` §3 — escritos à
# mão, de propósito: a conferência não pode derivar da função que confere.
REFERENCIAS_LIMITE = (
    (0, 600, 0.50),
    (1, 600, 0.79),
    (3, 1200, 0.64),
    (5, 600, 1.74),
    (2, 600, 1.05),
)
TOLERANCIA_PP = 0.01
# (n, falsos que o n permite sob o teto de 1 %): 298 não prova nem com 0 falsos; 299 prova com 0; 600 admite 1.
PERMITIDOS_REFERENCIA = ((100, -1), (298, -1), (299, 0), (600, 1))
# (nome, falsos arquivo, falsos inline, n arquivo, n inline, --exigir-limite-superior, pronto, trechos presentes, ausentes)
CENARIOS_LIMITE = (
    ("0/600 sem a flag", 0, 0, 600, 600, False, True, (), ()),
    ("0/600 com a flag (0,50%)", 0, 0, 600, 600, True, True, (), ()),
    ("1/600 com a flag (0,79%)", 1, 1, 600, 600, True, True, (), ()),
    (
        "5/600 e 2/600 sem a flag: a taxa pontual passa",
        5,
        2,
        600,
        600,
        False,
        True,
        (),
        ("limite superior",),
    ),
    (
        "5/600 e 2/600 com a flag: o limite reprova",
        5,
        2,
        600,
        600,
        True,
        False,
        (
            "falso deny (arquivo) limite superior 95% 1.74% > 1% (5/600",
            "com n=600 o teto admite no máximo 1 falso(s)",
            "falso deny (inline) limite superior 95% 1.05% > 1% (2/600",
        ),
        (),
    ),
    (
        "2/600 só no arquivo com a flag: só esse balde reprova",
        2,
        0,
        600,
        600,
        True,
        False,
        ("falso deny (arquivo) limite superior 95% 1.05%",),
        ("falso deny (inline)",),
    ),
    ("n pequeno (120 e 10), 0 falsos, sem a flag", 0, 0, 120, 10, False, True, (), ()),
    (
        "n pequeno (120 e 10), 0 falsos, com a flag",
        0,
        0,
        120,
        10,
        True,
        False,
        ("n=120 não prova ≤ 1% nem com 0 falsos; exige n ≥ 299", "n=10 não prova"),
        (),
    ),
)


def _motor_em_processo(touring: list[str], entrada: dict, corpus: Path) -> dict:
    """Motor falso SEM subprocesso (centenas de chamadas em milissegundos): acerta o `#ESPERADO=` e nega o que traz
    `#NEGA` — k falsos deny exatos em n, para provar a aritmética do limite sem o custo de 1.200 spawns."""
    texto = (corpus / entrada["snap"]).read_text()
    achados = []
    if "#ESPERADO=" in texto:
        nome = texto.split("#ESPERADO=")[1].split()[0]
        achados.append(
            {"classe": "atributo", "nome": nome, "linha": 1, "severidade": "deny"}
        )
    if "#NEGA" in texto:
        achados.append(
            {"classe": "nome", "nome": "x", "linha": 1, "severidade": "deny"}
        )
    return {
        "verdict": "deny" if achados else "allow",
        "findings": achados,
        "elapsed_ms": 2.0,
    }


def _falhas_limite_superior() -> list[str]:
    """Valores de referência do limite, `falsos_permitidos` e os cenários da flag `--exigir-limite-superior`."""
    falhas = []
    for k, n, ref in REFERENCIAS_LIMITE:
        pct = 100 * limite_do_balde(k, n)
        if abs(pct - ref) > TOLERANCIA_PP:
            falhas.append(
                f"limite superior 95% de {k}/{n} = {pct:.4f}% ≠ {ref}% (±{TOLERANCIA_PP} pp)"
            )
    for n, esperado in PERMITIDOS_REFERENCIA:
        if (obtido := falsos_permitidos(n)) != esperado:
            falhas.append(f"falsos_permitidos({n}) = {obtido} ≠ {esperado}")
    if (nmin := n_minimo_para_zero_falsos()) != 299:
        falhas.append(f"n_minimo_para_zero_falsos() = {nmin} ≠ 299")
    # The table line must never print the -1 sentinel (P2 on 09/10: "o n=274 admite até -1 falso(s)").
    for n, balde, esperado in (
        (274, "inline", "nem com 0 falsos; exige n ≥ 299"),
        (600, "arquivo", "admite até 1 falso"),
    ):
        lim = limite_do_balde(0, n)
        t = {
            "falsos": 0,
            "n": n,
            "taxa": 0.0,
            "limite_superior_95": lim,
            "teto": LIMIAR_FALSO_DENY,
            "limite_ate_o_teto": lim <= LIMIAR_FALSO_DENY,
            "falsos_permitidos_pelo_teto": falsos_permitidos(n),
        }
        linha = linha_falso_deny(balde, t, exigido=False)
        if esperado not in linha or "-1" in linha:
            falhas.append(
                f"linha da tabela com n={n}: falta «{esperado}» ou sobrou «-1» — {linha}"
            )
    return falhas + _falhas_cenarios_limite()


def _falhas_cenarios_limite() -> list[str]:
    """Cada cenário de `CENARIOS_LIMITE` com o motor em processo (a troca de `rodar_motor` é desfeita no `finally`)."""
    falhas = []
    original = globals()["rodar_motor"]
    globals()["rodar_motor"] = _motor_em_processo
    try:
        for (
            nome,
            k_arq,
            k_inl,
            n_arq,
            n_inl,
            exigir,
            pronto,
            presentes,
            ausentes,
        ) in CENARIOS_LIMITE:
            with tempfile.TemporaryDirectory() as tmp:
                corpus = corpus_sintetico(Path(tmp), n_arq, n_inl, k_arq, k_inl)
                v = julgar(
                    ["motor-em-processo"], corpus, v5_estrito=True, exigir_limite=exigir
                )
            pend = " | ".join(v["pendencias"])
            if v["pronto"] != pronto:
                falhas.append(
                    f"cenário «{nome}»: pronto = {v['pronto']} ≠ {pronto} — {pend or 'sem pendências'}"
                )
            falhas += [
                f"cenário «{nome}»: falta «{t}» nas pendências — {pend}"
                for t in presentes
                if t not in pend
            ]
            falhas += [
                f"cenário «{nome}»: sobrou «{t}» nas pendências — {pend}"
                for t in ausentes
                if t in pend
            ]
    finally:
        globals()["rodar_motor"] = original
    return falhas


def _falhas_flag_na_cli(motor: Path, corpus: Path, saida: Path) -> list[str]:
    """`main()` de ponta a ponta com o motor `bom` no corpus pequeno (n = 120 e 10): o limite de 0 falsos em 120 é 2,47 %.
    Sem a flag o veredito é PRONTO (rc 0); com a flag, NÃO PRONTO (rc 1) e o limite vai à tabela e ao JSON."""
    falhas = []
    for exigir, rc_esperado in ((False, 0), (True, 1)):
        argv = [
            "--touring",
            str(motor),
            "--corpus",
            str(corpus),
            "--v5-estrito",
            "--json",
            str(saida),
        ]
        argv += ["--exigir-limite-superior"] if exigir else []
        with contextlib.redirect_stdout(io.StringIO()) as buf:
            rc = main(argv)
        tabela, v = buf.getvalue(), json.loads(saida.read_text())
        lim = (
            v.get("falso_deny_por_balde", {})
            .get("arquivo", {})
            .get("limite_superior_95", -1.0)
        )
        rotulo = "com a flag" if exigir else "sem a flag"
        if rc != rc_esperado:
            falhas.append(f"CLI {rotulo}: rc {rc} ≠ {rc_esperado}")
        if abs(lim - 0.02466) > 5e-4:
            falhas.append(
                f"CLI {rotulo}: limite_superior_95 do arquivo no JSON = {lim} ≠ 0.0247 (0 em 120)"
            )
        if "limite_superior_95 2.47%" not in tabela:
            falhas.append(
                f"CLI {rotulo}: a tabela não traz o limite superior 95% (2.47%)"
            )
        if exigir != (
            "PENDENTE: falso deny (arquivo) limite superior 95% 2.47%" in tabela
        ):
            falhas.append(
                f"CLI {rotulo}: a pendência do limite {'faltou' if exigir else 'apareceu'}"
            )
    return falhas


MOTOR_ECO = r"""#!/usr/bin/env python3
import json, sys
print(json.dumps({"verdict": "allow", "findings": [], "elapsed_ms": 1.0, "argv": sys.argv[1:]}))
"""


def _falhas_disk_file_do_par() -> list[str]:
    """Par natural M (held-out web, 09/10): o rótulo é por DELTA, então o motor recebe o ANTES como `--disk-file`.

    Sem isto todo erro PRÉ-EXISTENTE do arquivo vira falso deny contra um rótulo que nunca o contou. Par A (criação)
    não tem antes e não recebe `--disk-file`.
    """
    falhas = []
    with tempfile.TemporaryDirectory() as tmp:
        raiz = Path(tmp)
        motor = raiz / "motor_eco"
        motor.write_text(MOTOR_ECO)
        motor.chmod(0o755)
        (raiz / "snap").mkdir()
        (raiz / "snap" / "depois.ts").write_text("export const b = 2\n")
        (raiz / "snap" / "antes.ts").write_text("export const a = 1\n")
        base = {
            "path": "src/x.ts",
            "project_root": tmp,
            "lang": "typescript",
            "snap": "snap/depois.ts",
        }
        com = rodar_motor(
            [str(motor)], {**base, "id": "m", "snap_antes": "snap/antes.ts"}, raiz
        )
        sem = rodar_motor([str(motor)], {**base, "id": "a"}, raiz)
    argv_com, argv_sem = com.get("argv", []), sem.get("argv", [])
    antes = str(raiz / "snap" / "antes.ts")
    if (
        "--disk-file" not in argv_com
        or argv_com[argv_com.index("--disk-file") + 1] != antes
    ):
        falhas.append(
            f"par M com `snap_antes`: o motor não recebeu `--disk-file <antes>` — argv {argv_com or com}"
        )
    if "--disk-file" in argv_sem:
        falhas.append(
            f"par A sem `snap_antes`: o motor recebeu `--disk-file` — argv {argv_sem}"
        )
    return falhas


def selftest() -> int:
    """Cada motor falso tem de produzir o veredito esperado — senão o juiz não discrimina."""
    # `bom` passa SEM ver a forma (se a forma voltasse ao recall, ele reprovaria); `so_avisa` passa porque o recall é de
    # detecção; `avisa_negativos` passa (aviso não é deny, só é reportado); `pega_forma` não muda nada.
    cenarios = (
        ("bom", True),
        ("so_avisa", True),
        ("avisa_negativos", True),
        ("pega_forma", True),
        ("cego", False),
        ("paranoico", False),
        ("nega_inline", False),
        ("lento", False),
        ("lixo", False),
    )
    resultados = {}
    with tempfile.TemporaryDirectory() as tmp:
        corpus = corpus_sintetico(Path(tmp))
        for modo, esperado in cenarios:
            motor = Path(tmp) / f"motor_{modo}"
            motor.write_text(MOTOR_FALSO.format(modo=modo))
            motor.chmod(0o755)
            v = julgar([str(motor)], corpus, v5_estrito=True)
            resultados[modo] = (v["pronto"] == esperado, v["pendencias"][:2])
        falhas_cli = _falhas_flag_na_cli(
            Path(tmp) / "motor_bom", corpus, Path(tmp) / "veredito.json"
        )
    for modo, (certo, pend) in resultados.items():
        print(f"  {'OK ' if certo else 'FALHA'} motor {modo}: {pend or 'pronto'}")
    falhas = _falhas_classe_erro()
    falhas += _falhas_limite_superior()
    falhas += falhas_cli
    falhas += _falhas_disk_file_do_par()
    for f in falhas:
        print(f"  FALHA {f}")
    ok = all(r[0] for r in resultados.values()) and not falhas
    limite = f"{len(REFERENCIAS_LIMITE)} valores de referência + {len(CENARIOS_LIMITE)} cenários do limite superior + 2 da CLI + 2 do `--disk-file` do par"
    print(
        f"selftest: {f'{len(cenarios)}/{len(cenarios)} cenários + {len(CASOS_CLASSE_ERRO)} casos de classe_erro + {limite}' if ok else 'REPROVOU'}"
    )
    return 0 if ok else 1


def linha_falso_deny(b: str, t: dict, exigido: bool) -> str:
    """Uma linha da tabela de falso deny: taxa pontual ao lado do limite superior 95 %."""
    perm = t["falsos_permitidos_pelo_teto"]
    admite = (
        f"o n={t['n']} admite até {perm} falso(s)"
        if perm >= 0
        else f"o n={t['n']} não prova o teto nem com 0 falsos; exige n ≥ {n_minimo_para_zero_falsos()}"
    )
    linha = (
        f"  falso deny ({b}): {t['falsos']}/{t['n']} · taxa {t['taxa']:.3%} · limite_superior_95 {t['limite_superior_95']:.2%} "
        f"(teto {t['teto']:.0%}; {admite})"
    )
    if t["limite_ate_o_teto"]:
        return linha
    return linha + (
        " · LIMITE ACIMA DO TETO"
        if exigido
        else " · limite acima do teto (só reprova com --exigir-limite-superior)"
    )


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="Juiz do motor VGP contra o corpus V0d")
    ap.add_argument("--touring", default=shutil.which("touring") or "touring")
    ap.add_argument("--corpus", type=Path, default=CORPUS)
    ap.add_argument("--v5-estrito", action="store_true")
    ap.add_argument("--json", type=Path)
    ap.add_argument("--arg-motor", action="append", default=[])
    ap.add_argument("--p95-ms", type=float, default=LIMIAR_P95_MS)
    ap.add_argument(
        "--base",
        type=Path,
        help="JSON do juiz para a fase anterior (V3 compara com a V2)",
    )
    ap.add_argument(
        "--exigir-limite-superior",
        action="store_true",
        help="balde cujo limite superior 95%% (Clopper-Pearson unilateral) de falso deny passa do teto vira pendência",
    )
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args(argv)
    if a.selftest:
        return selftest()
    ARGS_MOTOR[:] = a.arg_motor
    LIMIAR_P95["ms"] = a.p95_ms
    touring = [a.touring]
    if ausente := motor_ausente(touring):
        print(f"veredito: MOTOR AUSENTE — {ausente}")
        return 2
    v = julgar(touring, a.corpus, a.v5_estrito, a.exigir_limite_superior)
    if a.base and "positivos_simbolo" in v:
        v["pendencias"] += contra_a_base(v, json.loads(a.base.read_text()))
        v["pronto"] = not v["pendencias"]
    if a.json:
        a.json.write_text(json.dumps(v, ensure_ascii=False, indent=1))
    ps = v.get("positivos_simbolo", {"arquivo": [0, 0], "inline": [0, 0]})
    fm = v.get("forma_fora_do_recall", {})
    print(
        f"recall de símbolo · arquivo {ps['arquivo'][0]}/{ps['arquivo'][1]} · inline {ps['inline'][0]}/{ps['inline'][1]} "
        f"| forma (fora) {fm} | falsos deny {len(v.get('falsos', []))} em {v.get('negativos', {})} "
        f"| avisos em negativos {v.get('avisos_em_negativos', 0)} | p95 {v.get('p95_ms', math.inf):.1f} ms "
        f"| chamadas {v.get('chamadas', 0)} | não classificados {len(v.get('nao_classificado', []))} "
        f"| defeito de corpus (snap não compila) {len(v.get('defeito_de_corpus', []))}"
    )
    for b, t in v.get("falso_deny_por_balde", {}).items():
        print(linha_falso_deny(b, t, a.exigir_limite_superior))
    for p in v["pendencias"]:
        print(f"  PENDENTE: {p}")
    print(f"veredito: {'PRONTO' if v['pronto'] else 'NÃO PRONTO'}")
    return 0 if v["pronto"] else 1


if __name__ == "__main__":
    sys.exit(main())
