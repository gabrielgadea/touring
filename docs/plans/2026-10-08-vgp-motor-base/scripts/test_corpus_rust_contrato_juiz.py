#!/usr/bin/env python3
"""TESTE RED: o gerador `corpus_rust.py` escreve o que o juiz consegue ler.

ACHADO (10/10, coordinatora + agente da VRS, confirmado por leitura dos dois lados):
`corpus_rust.py` grava `contexto` como STRING — `json.dumps(motivo)` na L265 e o literal `"[]"`
na L304 — mas `juiz_motor_vgp.py` L179 faz `for c in entrada.get("contexto") or []` e indexa
`c["path"]` / `c["snap"]`. Iterar uma string devolve CARACTERES, e indexar um caractere por
`"path"` e TypeError. Ou seja: NENHUMA medicao roda sobre `~/.claude/touring/vgp/corpus_rust/`,
porque o erro aparece dentro do juiz, e nao como veredito.

Este teste existe porque o `--selftest` do gerador (13/13) NAO pega: ele roda a receita e o
`aceita()`, nunca o juiz. Um guard que so exercita o proprio produtor nao prova o contrato com o
consumidor. Aqui o CONSUMIDOR e o juiz, de verdade.

Nao faz mock: chama `juiz_motor_vgp.py` por `subprocess`, como o juiz seria chamado de verdade.
"""
import json
import subprocess
import sys
import tempfile
from pathlib import Path

AQUI = Path(__file__).resolve().parent
GERADOR = AQUI / "corpus_rust.py"
JUIZ = AQUI / "juiz_motor_vgp.py"
MOTOR = Path.home() / ".claude/touring/vgp/binarios/f55ec4b5/touring"


def falha(msg: str) -> None:
    print(f"FALHOU: {msg}")
    sys.exit(1)


def gera_entradas(tmp: Path) -> tuple:
    """Roda o gerador de verdade e devolve (entradas, caminho do corpus)."""
    if not GERADOR.is_file():
        falha(f"nao achei o gerador em {GERADOR}")
    saida = tmp / "corpus"
    print(f"=== 1) gerando 3 entradas com {GERADOR.name} (n=1 por classe) ===")
    r = subprocess.run(
        [sys.executable, str(GERADOR), "--n", "1", "--saida", str(saida)],
        capture_output=True,
        text=True,
        timeout=1800,
    )
    arq = saida / "entradas.jsonl"
    if r.returncode != 0 or not arq.is_file():
        falha(f"o gerador nao produziu corpus (rc={r.returncode}): {r.stderr[-400:]}")
    linhas = [json.loads(x) for x in arq.read_text().splitlines() if x.strip()]
    print(f"    {len(linhas)} entradas")
    return linhas, saida


def confere_contrato(entradas: list) -> None:
    """O que o juiz ITERA: `contexto` tem de ser LISTA de dicts com `path` e `snap`."""
    print("=== 2) o contrato do campo `contexto` (o que o juiz itera) ===")
    for e in entradas:
        ctx = e.get("contexto")
        if not isinstance(ctx, list):
            falha(
                f"entrada {e['id']}: `contexto` e {type(ctx).__name__}, e o juiz faz "
                f"`for c in entrada['contexto']` e espera LISTA. Valor: {ctx!r}"
            )
        for c in ctx:
            if not isinstance(c, dict) or "path" not in c or "snap" not in c:
                falha(f"entrada {e['id']}: item de `contexto` sem `path`/`snap`: {c!r}")
    print(f"    ok: todas as {len(entradas)} entradas com `contexto` = LISTA")


def roda_juiz(tmp: Path, corpus: Path) -> None:
    """O juiz de verdade. O sintoma publicado e um Traceback dentro dele."""
    print("=== 3) o juiz de verdade, sobre esse corpus ===")
    if not JUIZ.is_file():
        falha(f"nao achei o juiz em {JUIZ}")
    cmd = [
        sys.executable,
        str(JUIZ),
        "--corpus",
        str(corpus),
        "--json",
        str(tmp / "v.json"),
    ]
    if MOTOR.is_file():
        cmd += ["--touring", str(MOTOR)]
    else:
        print(f"    (aviso: motor congelado ausente em {MOTOR}; uso o do PATH)")
    j = subprocess.run(cmd, capture_output=True, text=True, timeout=1800)
    txt = (j.stdout or "") + (j.stderr or "")
    if "Traceback" in txt:
        for ln in txt.splitlines():
            if "Error" in ln or ln.strip().startswith("File "):
                print(f"    {ln.strip()[:120]}")
        falha("o juiz levantou excecao sobre o corpus que o gerador produziu")
    arq = tmp / "v.json"
    if not arq.is_file():
        falha(f"o juiz nao escreveu o JSON do veredito: {txt[-400:]}")
    v = json.loads(arq.read_text())
    print(
        f"    veredito: pronto={v.get('pronto')}  "
        f"positivos_simbolo={v.get('positivos_simbolo')}  chamadas={v.get('chamadas')}"
    )
    if v.get("chamadas", 0) <= 0:
        falha("o juiz rodou 0 chamadas — medicao nenhuma aconteceu")


def main() -> int:
    tmp = Path(tempfile.mkdtemp(prefix="vgp-rust-red-"))
    entradas, corpus = gera_entradas(tmp)
    confere_contrato(entradas)
    roda_juiz(tmp, corpus)
    print()
    print("PASSOU: o gerador escreve o que o juiz le.")
    return 0


def test_contrato_do_gerador_com_o_juiz():
    """O contrato RED como o pytest o ve.

    MEDIDO em 10/10 (achado da coordenadora, ver `test_todo_teste_e_coletavel.py`): sem esta funcao
    o pytest responde `no tests ran` com exit 5 — e exit 5 e lido por varios CI como "nada a fazer",
    entao o pipeline fica VERDE sem rodar o juiz de verdade, que e exatamente a garantia que este
    arquivo existe para provar.

    Este teste e o mais caro do bundle (gera o corpus com `cargo check` e chama o juiz) e nao e
    mutavel por `pytest -k`: ele LEVA o mutante dentro do `main`, e um mutante assim exige reescrever
    o gerador. A prova de mutante dele esta em `scripts/_prova_assert_rust.py`.
    """
    assert main() == 0, (
        "o gerador nao escreve o que o juiz le: rode "
        "`python3 scripts/test_corpus_rust_contrato_juiz.py`"
    )


if __name__ == "__main__":
    sys.exit(main())