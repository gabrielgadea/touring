"""MEDICAO: os caminhos `.html` que as edicoes do transcript PROCESSARAM ainda existem?

O oraculo por instancia decide se uma referencia estava quebrada NO INSTANTE, reconstruindo o
estado do disco. Se o arquivo base ja nao esta no disco, esse estado nao pode ser reconstructions
— e o n cai a zero sem que ninguem perceba o porque.

O risco e concreto: quando meci o held-out web, 5 de 10 caminhos `.html` ainda existiam (os outros
eram scratchpad de /tmp de outra sessao, ja apagados). Se a maioria das edicoes que introduzem
referencia e em /tmp, o oraculo nao tem sobre o que decidir.

Este script mede POR CLASSE: quantos caminhos distintos aparecem em cada classe e quantos ainda
existem. Ele nao decide nada — e a medicao que vem antes de construir.
"""
import json
import re
from collections import Counter, defaultdict
from datetime import datetime, timezone
from pathlib import Path

CORTE = "2026-10-10T00:00:00Z"
PROJETOS = ("analise", "touring")
EXCLUIR_MARCADOR = "vgp-motor-base"

TAGS_RECURSO = {"script", "img", "link", "iframe", "source", "video", "audio"}
TAGS_REF = {"a", "area", "use", "form", "label", "input"}
ATRS_CARREGA = {"src", "href", "action", "poster", "data"}
ATRS_REF = {"id", "for", "aria-labelledby", "aria-describedby", "aria-controls", "class"}

TAG = re.compile(r"<(?P<tag>[A-Za-z][\w:-]*)(?P<atrs>[^>]*)>")
ATTR = re.compile(r"""\s(?P<nome>[\w:-]+)\s*=\s*(?P<asp>["'])(?P<val>[^"']*)(?P=asp)""")
VERSAO = re.compile(r"^\d+\.\d+\.\d+")

TEMP_PREFIX = ("/tmp/",)


def classe_de(valor: str, tag: str, atr: str) -> str:
    if atr == "class":
        return "classe_css"
    if atr in ATRS_REF:
        return "id" if atr == "id" else "ancora"
    if valor.startswith("#"):
        return "ancora"
    if valor.startswith("http"):
        return "cdn" if VERSAO.match(valor.rsplit("@", 1)[-1]) else "cdn_pin"
    return "recurso"


def classes_no_texto(texto: str) -> Counter:
    c = Counter()
    for m in TAG.finditer(texto):
        tag = m.group("tag").lower()
        for a in ATTR.finditer(m.group("atrs") or ""):
            atr, val = a.group("nome").lower(), a.group("val").strip()
            if not val or atr not in ATRS_CARREGA | ATRS_REF:
                continue
            if atr in ("src", "poster", "data") and tag not in TAGS_RECURSO:
                continue
            if atr == "href" and tag not in TAGS_RECURSO | TAGS_REF:
                continue
            if atr == "action" and tag != "form":
                continue
            if atr == "class":
                for cl in val.split():
                    c["classe_css"] += 1
                continue
            c[classe_de(val, tag, atr)] += 1
    return c


def classifica_transcript(arq: Path) -> tuple:
    """(caminhos por classe, ja editou, tem referencia)."""
    por_classe = defaultdict(set)
    for ln in arq.read_text(errors="replace").splitlines():
        if '"tool_use"' not in ln:
            continue
        try:
            rec = json.loads(ln)
        except json.JSONDecodeError:
            continue
        if (rec.get("timestamp") or "") > CORTE:
            continue
        conteudo = rec.get("message", {}).get("content")
        if not isinstance(conteudo, list):
            continue
        for b in conteudo:
            if not isinstance(b, dict) or b.get("type") != "tool_use":
                continue
            if b.get("name") not in ("Edit", "Write", "MultiEdit"):
                continue
            inp = b.get("input") or {}
            caminho = str(inp.get("file_path") or "")
            if not caminho.endswith(".html"):
                continue
            novo = inp.get("new_string") or inp.get("content") or ""
            if not isinstance(novo, str) or not novo:
                continue
            for k in classes_no_texto(novo):
                por_classe[k].add(caminho)
    return por_classe


def main() -> int:
    raiz = Path.home() / ".claude/projects"
    dirs = [d for d in sorted(raiz.iterdir()) if d.is_dir() and any(p in d.name for p in PROJETOS)]
    print(f"corte={CORTE}  projetos={[d.name for d in dirs]}")

    caminhos_por_classe = defaultdict(set)
    excluidos = Counter()
    total_arqs = 0
    for d in dirs:
        for arq in sorted(d.rglob("*.jsonl")):
            sid = arq.name.removesuffix(".jsonl")
            txt = arq.read_text(errors="replace")
            total_arqs += 1
            # exclusao das sessoes do plano VGP: marcadores no transcript
            if EXCLUIR_MARCADOR in txt or sid.startswith(("751b3a8c", "d75671a4")):
                excluidos[sid] += 1
                continue
            pc = classifica_transcript(arq)
            for k, v in pc.items():
                caminhos_por_classe[k] |= v

    print(f"\ntranscripts lidos: {total_arqs}  | excluidos do plano VGP: {len(excluidos)}")
    if excluidos:
        print(f"  {list(excluidos)[:6]}")

    print("\n=== SOBREVIVENCIA POR CLASSE (caminhos distintos que a edicao tocou) ===")
    print(f"  {'classe':12s} {'caminhos':>9s} {'vivem':>7s} {'morrtos':>8s} {'em /tmp':>8s}")
    for k in ("recurso", "ancora", "cdn", "cdn_pin", "classe_css", "id"):
        cs = caminhos_por_classe.get(k, set())
        if not cs:
            print(f"  {k:12s} {0:>9d} {'-':>7s} {'-':>8s} {'-':>8s}")
            continue
        vivem = sum(1 for c in cs if Path(c).is_file())
        tmp = sum(1 for c in cs if c.startswith(TEMP_PREFIX))
        print(f"  {k:12s} {len(cs):>9d} {vivem:>7d} {len(cs) - vivem:>8d} {tmp:>8d}")

    # REGRA DA COORDENADORA (10/10): o oraculo NAO pode depender do disco de HOJE para caminhos de
    # /tmp ou scratchpad. A existencia de um alvo no instante vem de (a) uma dobra do transcript,
    # (b) `git show <rev>:<rel>`, ou (c) o disco de hoje SO quando mtime e historia provam que o
    # alvo ja existia e nao mudou. Fora disso: «desconhecido no instante» -> INCOMPLETA, contada
    # e nunca positiva.
    #
    # Esta tabela e o TEOR: o maximo de candidatos que a regra permite. Ela ainda nao aplica (a)/(b)/
    # (c) — e o que falta para reduzir «desconhecido» a zero no que der.
    print("\n=== TETO APOS A REGRA: candidatos potencialmente PROVAVEIS por classe ===")
    print("   (so o que tem base de instancia: versado em git OU arquivo de projeto persistente)")
    print(f"  {'classe':12s} {'potencial':>10s} {'so /tmp':>8s} {'versado':>9s}")
    for k in ("recurso", "ancora", "cdn", "cdn_pin", "classe_css", "id"):
        cs = caminhos_por_classe.get(k, set())
        so_tmp = {c for c in cs if c.startswith(TEMP_PREFIX)}
        versado = {c for c in cs - so_tmp if (Path(c).parent / ".git").exists() or Path(c).is_file()}
        provavel = len(versado)
        print(f"  {k:12s} {provavel:>10d} {len(so_tmp):>8d} {provavel:>9d}")
    print(
        "\n  NOTA: «so /tmp» NAO vira positivo — vira «desconhecido no instante» (INCOMPLETA),\n"
        "  salvo se a dobra do transcript provar o que existia. O oraculo ainda nao foi construido;\n"
        "  esta e a contagem do universo, antes dele."
    )
    return 0


main()