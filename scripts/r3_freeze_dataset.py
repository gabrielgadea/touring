#!/usr/bin/env python3
"""r3_freeze_dataset — S-4: congela datasets e splits, e PROVA que seguem congelados.

Plano: docs/plans/2026-10-02-jev-harness-r3/plan.md:90-96
Critérios: CRITERIOS-ACEITACAO.md §1 (Split, Audit sample) + **Errata 3** (E3.1)

## E3.1 — o que este script congela, e o que mudou desde a E1.3

A E1.3 pedia "n = 240 sorteado, estratificado 120 followed / 120 not-followed".
A E3.1 (registrada 02/10 13:06:55, **antes de existir qualquer rótulo**) substituiu
isso por: **as 198 ofertas INTEIROS são adjudicadas** — sem amostragem, logo sem
variância de amostragem — e a estratificação por decisão de B0 passou a valer só
para a DIVISÃO `audit-val` / `audit-test` (40/60, seed 20261002, por sessão),
garantindo `not_followed` nas duas metades.

**Nenhum limiar muda aqui.** O script implementa a E3.1; se algum dia o n medido
cair abaixo do que a E3.1 supõe, o script DIZ e sai ≠ 0 — ele não corrige o dado.

## "Gravar não é congelar" (lição 4 do §0 dos critérios)

O `--check` relê os arquivos do disco e confere o sha256 contra o manifesto. É
inter-processos: um manifesto reescrito junto com o dataset passa, um byte
alterado no dataset não.

## O que entra no manifesto

Os artefatos de DADOS + os de CÓDIGO que definem o que eles significam
(`r3_use_cases.py`, `CRITERIOS-ACEITACAO.md`): um congelamento que não fixa a
regra que produziu o dado congela o número, não o significado.
"""
from __future__ import annotations

import argparse
import hashlib
import os
import json
import random
import sys
from collections import Counter, defaultdict
from pathlib import Path

SEED = 20261002                 # igual a r3_use_cases.SEED
SPLIT_AUDIT = (0.4, 0.6)        # E3.1: audit-val / audit-test
SPLIT_DATASET = (0.6, 0.2, 0.2)  # r3_use_cases.SPLIT: train/val/test temporal por sessão

# As raízes vêm do ambiente quando `R3_FAKE_REPO` está setado — é o que torna a
# suíte HERMÉTICA: o teste aponta para um tmp e o processo filho (o `--check` roda
# como subprocesso, senão o monkeypatch do pytest não alcança) grava lá, não no
# corpus vivo. Sem a variável, o comportamento é o de produção — o freeze continua
# apontando para o repo e para `~/.claude/touring/r3-data`.
_FAKE = os.environ.get("R3_FAKE_REPO")
if _FAKE:
    REPO = Path(_FAKE)
    R3 = REPO / "scripts"
    DATA_DIR = REPO / "docs/plans/2026-10-02-jev-harness-r3/data"
    CRITERIOS = DATA_DIR.parent / "CRITERIOS-ACEITACAO.md"
    USE_CASES = R3 / "r3_use_cases.py"
    EXTERNAL = Path(os.environ.get("R3_FAKE_EXTERNAL", REPO / "externo"))
else:
    R3 = Path(__file__).resolve().parent
    REPO = R3.parent
    DATA_DIR = REPO / "docs/plans/2026-10-02-jev-harness-r3/data"
    CRITERIOS = DATA_DIR.parent / "CRITERIOS-ACEITACAO.md"
    USE_CASES = R3 / "r3_use_cases.py"
    EXTERNAL = Path("~/.claude/touring/r3-data").expanduser()

# Artefatos de DADOS, gravados em disco FORA do repo (texto de comando/prompt).
DATASETS = ("a2_offers.jsonl", "a1_blocks.jsonl")
# Artefatos que definem o SIGNIFICADO do dado — vão no manifesto junto.
CODE_ARTIFACTS = ("r3_use_cases.py", "CRITERIOS-ACEITACAO.md")
MANIFEST = DATA_DIR / "MANIFEST.sha256"
SPLITS = DATA_DIR / "splits.json"
# PRIVACIDADE (regra do lote r3): o repo é PÚBLICO e `audit_sample.jsonl` carrega
# o texto COMPLETO dos comandos (sem truncagem — Errata 4, E4.2): caminho absoluto,
# código, prompt. Ele
# vai para FORA do repo, junto com os datasets. No repo ficam só o hash (que é o
# que congela) e o relatório agregado. A 1ª versão o gravou em `data/` e o
# `git status` expôs `cmds` com `/tmp/claude-1000/...` — visível para qualquer
# leitor do repositório.
AUDIT = EXTERNAL / "audit_sample.jsonl"
EXEMPT_MARKERS_OUT = DATA_DIR / "exempt_markers.json"


def _caminho_no_manifesto(p: Path) -> str:
    """O caminho que o `--check` vai resolver.

    Em produção (`EXTERNAL` = `~/.claude/touring/r3-data`) o caminho é relativo ao
    repo, e o `--check` o resolve por `REPO / caminho`. No modo teste
    (`R3_FAKE_REPO`) o corpus externo está no tmp e NÃO é relativo ao repo — então
    o caminho vai absoluto, senão o `--check` pointed para o corpus real e acusaria
    violação de um congelamento que não mudou nada (foi o que aconteceu na 1ª
    versão hermética).
    """
    try:
        return str(p.relative_to(REPO))
    except ValueError:
        return str(p)


def sha256_de(caminho: Path) -> str:
    h = hashlib.sha256()
    with open(caminho, "rb") as f:
        for pedaco in iter(lambda: f.read(1 << 16), b""):
            h.update(pedaco)
    return h.hexdigest()


def ler_jsonl(p: Path) -> list[dict]:
    return [json.loads(l) for l in p.read_text(encoding="utf-8").splitlines() if l.strip()]


def _composicao(itens: list[dict]) -> Counter:
    return Counter(i["b0_decisao"] for i in itens)


def dividir_audit(ofertas: list[dict], seed: int = SEED) -> tuple[list[dict], list[dict], dict]:
    """E3.1: divide TODAS as ofertas em audit-val (40%) / audit-test (60%), com a
    sessão como UNIDADE indivisível e as duas metades carregando os dois estratos.

    **Por que a sessão é indivisível**: o §1 dos critérios diz que "nenhum evento de
    uma sessão cruza partições", e a chave do runtime é a sessão
    (`cli_suggester.rs:4535`). A primeira versão deste script estratifikava por B0
    DENTRO de cada sessão e partiu 12 sessões entre as metades; o guard
    `sessoes_cruzadas` pegou antes de gravar qualquer coisa. As duas exigências — não
    cruzar sessão e equilibrar estrato — não são incompatíveis: o equilíbrio é
    buscado ENTRE sessões.

    **O método**: as maiores sessões primeiro (é elas que pesam na proporção), e
    cada uma vai para a metade cujo estrato DOMINANTE está mais atrás da meta. O
    desempate é determinístico pelo nome da sessão — a seed entra só no empate que
    a composição não resolve, para que a divisão não dependa da ordem do dict.
    """
    por_sessao: dict[str, list[dict]] = defaultdict(list)
    for o in ofertas:
        por_sessao[o["session"]].append(o)

    unidades = []
    for sessao in sorted(por_sessao):
        itens = sorted(por_sessao[sessao], key=lambda r: r["ts"])
        unidades.append({"sessao": sessao, "itens": itens, "n": len(itens),
                         "comp": _composicao(itens)})

    total = _composicao(ofertas)
    meta = {k: total.get(k, 0) * SPLIT_AUDIT[0] for k in ("followed", "not_followed")}

    # Balanceamento por SORT DECRESCENTE de tamanho, com a sessão mais pesada
    # indo para a metade mais pobre do seu estrato DOMINANTE. A 1ª versão usava
    # guloso puro e mandava as 5 maiores para `test`; o resultado tinha
    # audit-test sem nenhum `not_followed` e o guard recusou gravar.
    #
    # O desempate de tamanho é determinístico pelo nome da sessão — a seed entra
    # só para desempatar sessões do MESMO tamanho e mesma composição, o que
    # mantém a divisão reprodutível sem depender da ordem de iteração.
    rng = random.Random(seed)
    unidades.sort(key=lambda u: (-u["n"], u["comp"].get("not_followed", 0) * -1,
                                 rng.random(), u["sessao"]))
    accum = {"val": Counter(), "test": Counter()}
    for u in unidades:
        nf = u["comp"].get("not_followed", 0)
        f = u["comp"].get("followed", 0)
        dominante = "not_followed" if nf >= f else "followed"
        # A sessão vai para a metade que está MAIS ATRÁS da meta do seu estrato.
        manque_val = meta[dominante] - accum["val"][dominante]
        manque_test = meta[dominante] - accum["test"][dominante]
        # Uma sessão que carrega os DOIS estratos não pode "vazar" um deles para a
        # metade errada: ela compõe as duas, e a escolha é pelo dominante.
        metade = "val" if manque_val >= manque_test else "test"
        u["metade"] = metade
        accum[metade]["followed"] += f
        accum[metade]["not_followed"] += nf

    val = [i for u in unidades if u["metade"] == "val" for i in u["itens"]]
    test = [i for u in unidades if u["metade"] == "test" for i in u["itens"]]
    s_val = sorted(u["sessao"] for u in unidades if u["metade"] == "val")
    s_test = sorted(u["sessao"] for u in unidades if u["metade"] == "test")

    return val, test, {
        "n_total": len(ofertas),
        "n_val": len(val),
        "n_test": len(test),
        "val_por_b0": dict(_composicao(val)),
        "test_por_b0": dict(_composicao(test)),
        "sessoes_val": s_val,
        "sessoes_test": s_test,
        "sessoes_cruzadas": sorted(set(s_val) & set(s_test)),
        "seed": seed,
        "proporcao": {"audit_val": SPLIT_AUDIT[0], "audit_test": SPLIT_AUDIT[1]},
        "unidade": "sessão inteira (o §1 dos critérios proíbe cruzar partições)",
        "sessoes": len(unidades),
    }


def dividir_dataset(blocos: list[dict], seed: int = SEED) -> dict[str, list[str]]:
    """Split temporal 60/20/20 POR SESSÃO (r3_use_cases.SPLIT, §1 dos critérios).
    Temporal de verdade: ordena por timestamp e corta — não sorteia."""
    rng = random.Random(seed)
    por_sessao: dict[str, list[dict]] = defaultdict(list)
    for b in blocos:
        por_sessao[b["session"]].append(b)
    out = {"train": [], "val": [], "test": []}
    for sessao in sorted(por_sessao):
        itens = sorted(por_sessao[sessao], key=lambda r: (r["ts"] if "ts" in r else r.get("turno", 0)))
        n = len(itens)
        a = int(round(n * SPLIT_DATASET[0]))
        b = a + int(round(n * SPLIT_DATASET[1]))
        out["train"] += [f"{sessao}:{i}" for i in range(a)]
        out["val"] += [f"{sessao}:{i}" for i in range(a, b)]
        out["test"] += [f"{sessao}:{i}" for i in range(b, n)]
    return out


def _id_para(ofertas: list[dict]) -> dict[str, int]:
    """`chave -> índice` estável dentro desta execução (session + ts)."""
    return {o["ts"] + "|" + o["session"]: i for i, o in enumerate(ofertas)}


def _grava_audit_sample(ofertas: list[dict], val: list[dict], id_para: dict) -> None:
    """TODAS as ofertas (E3.1: sem amostragem), com a partição — o `pack` do S-5
    consome isto e embaralha por `item_id`."""
    ids_val = {id(o) for o in val}
    with open(AUDIT, "w", encoding="utf-8") as f:
        for o in ofertas:
            iid = id_para[o["ts"] + "|" + o["session"]]
            f.write(json.dumps({
                "item_id": f"a2-{iid:05d}",
                "session": o["session"],
                "ts": o["ts"],
                "b0_decisao": o["b0_decisao"],
                "b0_valor": o["b0_valor"],
                "split": "audit-val" if id(o) in ids_val else "audit-test",
                "n_cmds": o["n_cmds"],
                "rota_proposta": o.get("rota_proposta"),
                "cmds": o["cmds"],
            }, ensure_ascii=False) + "\n")


def _exempt_markers_json() -> str:
    import r3_extract_a1_blocks as a1
    return json.dumps({
        "marcadores": list(a1.EXEMPT_MARKERS),
        "familias_exempt": list(a1.EXEMPT_FAMILIES),
        "janela_duplicata_turnos": a1.DUPLICATE_WINDOW_TURNS,
        "observacao": "congelado com o dataset: mudar esta lista depois de ver o "
                      "resultado invalida o braço (CRITERIOS §1, regra do τ)",
    }, ensure_ascii=False, indent=1)


def _splits_json(ofertas: list[dict], blocos: list[dict], diag: dict) -> str:
    return json.dumps({
        "seed": SEED,
        "audit": diag,
        "a1_dataset": {k: len(v) for k, v in dividir_dataset(blocos).items()},
        "a1_split_por": "sessão, temporal (60/20/20)",
        "audit_split_por": "sessão, 40/60, estratificado por decisão de B0 (E3.1)",
        "fonte": {"a2": "a2_offers.jsonl", "a1": "a1_blocks.jsonl"},
    }, ensure_ascii=False, indent=1)


def congelar(args) -> int:
    ofertas = ler_jsonl(EXTERNAL / "a2_offers.jsonl")
    blocos = ler_jsonl(EXTERNAL / "a1_blocks.jsonl")

    val, test, diag = dividir_audit(ofertas)
    id_para = _id_para(ofertas)
    if diag["sessoes_cruzadas"]:
        print(f"ERRO: sessão em val e test ao mesmo tempo: {diag['sessoes_cruzadas']}")
        return 2
    for metade, nome in ((diag["val_por_b0"], "audit-val"), (diag["test_por_b0"], "audit-test")):
        if not metade.get("not_followed"):
            print(f"ERRO: {nome} sem `not_followed` — E3.1 exige os dois estratos nas duas metades")
            return 2

    DATA_DIR.mkdir(parents=True, exist_ok=True)
    _grava_audit_sample(ofertas, val, id_para)
    EXEMPT_MARKERS_OUT.write_text(_exempt_markers_json(), encoding="utf-8")
    SPLITS.write_text(_splits_json(ofertas, blocos, diag), encoding="utf-8")

    # manifesto: DADOS (fora do repo) + CÓDIGO que os define + o que fica no repo.
    linhas = []
    for nome in DATASETS:
        p_ = EXTERNAL / nome
        if p_.exists():
            linhas.append(f"{sha256_de(p_)}  {_caminho_no_manifesto(p_)}")
    for nome in CODE_ARTIFACTS:
        p_ = (USE_CASES.parent / nome) if nome.endswith(".py") else CRITERIOS
        if p_.exists():
            linhas.append(f"{sha256_de(p_)}  {p_.relative_to(REPO)}")
    if AUDIT.exists():
        linhas.append(f"{sha256_de(AUDIT)}  {_caminho_no_manifesto(AUDIT)}")
    for nome in ("splits.json", "exempt_markers.json"):
        p_ = DATA_DIR / nome
        if p_.exists():
            linhas.append(f"{sha256_de(p_)}  {p_.relative_to(REPO)}")
    MANIFEST.write_text("\n".join(sorted(linhas)) + "\n", encoding="utf-8")

    a1_split = {k: len(v) for k, v in dividir_dataset(blocos).items()}
    print(f"audit_sample.jsonl : {len(ofertas)} ofertas (TODAS — E3.1, sem amostragem)")
    print(f"  audit-val  : {diag['n_val']:4d}  por B0: {diag['val_por_b0']}")
    print(f"  audit-test : {diag['n_test']:4d}  por B0: {diag['test_por_b0']}")
    print(f"  sessões cruzadas entre metades: {len(diag['sessoes_cruzadas'])} (tem de ser 0)")
    print(f"  sessões: {diag['sessoes']} (val {len(diag['sessoes_val'])} / "
          f"test {len(diag['sessoes_test'])})")
    print(f"splits.json        : a1 blocos por particao {a1_split}")
    print(f"MANIFEST.sha256    : {len(linhas)} artefatos")
    return 0


def conferir(args) -> int:
    """O `--check` da lição 4: relê do disco e confere. Gravar não é congelar."""
    if not MANIFEST.exists():
        print(f"ERRO: sem manifesto em {MANIFEST}")
        return 2
    ruins, n = [], 0
    for linha in MANIFEST.read_text(encoding="utf-8").splitlines():
        if not linha.strip():
            continue
        esperado, caminho = linha.split("  ", 1)
        p = Path(caminho) if caminho.startswith("/") else (
            Path("~").expanduser() / caminho[2:] if caminho.startswith("~/") else REPO / caminho)
        n += 1
        if not p.exists():
            ruins.append(f"AUSENTE  {caminho}")
            continue
        got = sha256_de(p)
        if got != esperado:
            ruins.append(f"MUDOU    {caminho}\n          esperado {esperado[:16]}…\n          obtido   {got[:16]}…")
    if ruins:
        print(f"CONGELAMENTO VIOLADO ({len(ruins)}/{n}):")
        for r in ruins:
            print("  " + r)
        return 1
    print(f"OK: {n} artefatos conferem com o manifesto.")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--check", action="store_true",
                    help="relê os artefatos e confere o sha256 contra o manifesto")
    args = ap.parse_args()
    return conferir(args) if args.check else congelar(args)


if __name__ == "__main__":
    sys.exit(main())
