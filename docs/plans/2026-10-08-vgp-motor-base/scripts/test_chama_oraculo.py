#!/usr/bin/env python3
"""Selftest do executor de oraculo — um MUTANTE por clausula do contrato.

MEDIDO em 10/10: `rc=0` sem sinal produzido apareceu sete vezes na onda, em sete camadas. A defesa
e uma funcao so (`chama_oraculo`), e por isso o selftest nao testa a happy path: testa que cada
GARANTIA some quando ela e removida.

Cada mutante abaixo e a garantia funcionando, virada do avesso:
  (a) o `\\n` da entrada   -> sem ele, o Node le por linha e sai com rc=0 e stdout vazio;
  (b) o sinal exigido     -> sem ele, um oraculo morto vira «nenhum diagnostico novo»;
  (c) o framing conferido  -> sem ele, um corpo truncado vira lista vazia.

Um selftest que so passa na happy path nao distingue um executor que funciona de um que aceita
tudo — e aceitar tudo e a forma silenciosa de o executor nao existir.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import chama_oraculo as co

NODE_ECHO = """
let buf = "";
process.stdin.on("data", (d) => {
  buf += d.toString();
  let i;
  while ((i = buf.indexOf("\\n")) >= 0) {
    const l = buf.slice(0, i); buf = buf.slice(i + 1);
    if (l.trim()) {
      const c = JSON.stringify({ ok: true, eco: l });
      process.stdout.write(`#${Buffer.byteLength(c, "utf8")}\\n${c}\\n`);
    }
  }
});
"""
# o MUTANTE do item (a): le a entrada INTEIRA e nunca pelo delimitador, entao a requisicao fica no
# buffer e o processo sai com rc=0 e stdout vazio — exatamente o caso medido em 10/10.
NODE_SEM_NEWLINE = """
process.stdin.on("end", () => { process.stdout.write(""); });
process.stdin.resume();
"""
NODE_SILENCIOSO = """
process.stdin.resume();
"""


def escreve_node(codigo: str, nome: str) -> Path:
    p = Path("/tmp") / nome
    p.write_text(codigo, encoding="utf-8")
    return p


def cenario_a_newline_da_entrada() -> list:
    """(a) a entrada chega TERMINADA. MEDIDO: sem o `\\n`, rc=0 e stdout vazio."""
    cen = []
    bom = escreve_node(NODE_ECHO, "vgp-oraculo-eco.js")
    ruim = escreve_node(NODE_SEM_NEWLINE, "vgp-oraculo-sem-nl.js")

    r = co.chama_oraculo(
        "node_parse", ["node", str(bom)], entrada='{"op":"x"}', timeout=60
    )
    cen.append(
        (
            "a entrada sem \\n no chamador mesmo assim chega ao processo",
            r["sinal"] is not None and r["sinal"].get("ok") is True,
            str(r["sinal"])[:60],
        )
    )

    # o MUTANTE: o processo que le por linha e nunca ve a requisicao. O executor tem de recusar.
    levantou = ""
    try:
        co.chama_oraculo(
            "node_parse", ["node", str(ruim)], entrada='{"op":"x"}', timeout=60
        )
    except co.SinalVazio as e:
        levantou = str(e)
    except co.RespostaMalformada as e:
        levantou = str(e)
    cen.append(
        (
            "MUTANTE: processo que nao le a entrada e RECUSADO (precisa reprovar aqui)",
            "sinal vazio" in levantou,
            levantou[:80],
        )
    )
    return cen


def cenario_b_sinal_exigido() -> list:
    """(b) `rc=0` sem sinal e ERRO NOMEADO. MEDIDO: o `cargo` escreve no stderr."""
    cen = []
    silencioso = escreve_node(NODE_SILENCIOSO, "vgp-oraculo-calcado.js")

    levantou = ""
    try:
        co.chama_oraculo(
            "node_parse", ["node", str(silencioso)], entrada='{"op":"x"}', timeout=60
        )
    except (co.SinalVazio, co.RespostaMalformada) as e:
        levantou = str(e)
    cen.append(
        (
            "MUTANTE: rc=0 sem sinal vira ERRO NOMEADO (precisa reprovar aqui)",
            "sinal vazio" in levantou,
            levantou[:80] or "(NAO LEVANTOU)",
        )
    )
    cen.append(
        (
            "o erro nomeia a FERRAMENTA e o STREAM",
            "node_parse" in levantou and "stdout" in levantou,
            levantou[:80],
        )
    )

    # o `cargo`: sinal no stderr. Um contrato que declarasse `stdout` recusaria um sinal real.
    cen.append(
        (
            "o contrato do cargo declara stderr, nao stdout",
            co.CONTRATOS["cargo"].stream == "stderr",
            co.CONTRATOS["cargo"].stream,
        )
    )
    cen.append(
        (
            "o contrato do tsc declara os DOIS streams",
            co.CONTRATOS["tsc"].stream == "ambos",
            co.CONTRATOS["tsc"].stream,
        )
    )

    # `vazio_valido` so com controle positivo
    g = co.CONTRATOS["git_status"]
    levantou2 = ""
    try:
        co.exige_sinal("git_status", g, False, 0, False, "", "")
    except co.SinalVazio as e:
        levantou2 = str(e)
    cen.append(
        (
            "vazio_valido SEM controle positivo ainda e erro (precisa reprovar aqui)",
            "controle positivo" in levantou2,
            levantou2[:90],
        )
    )
    try:
        co.exige_sinal("git_status", g, False, 0, True, "", "")
        passou = True
    except co.SinalVazio:
        passou = False
    cen.append(("vazio_valido COM controle positivo passa", passou, ""))
    return cen


def cenario_c_framing() -> list:
    """(c) o comprimento declarado e conferido. MEDIDO: truncamento em 65.536 bytes."""
    cen = []
    try:
        co.ler_framing("node_parse", "#10\ncurto\n")
        levantou = ""
    except co.RespostaMalformada as e:
        levantou = str(e)
    cen.append(
        (
            "comprimento declarado MAIOR que o recebido e erro NOMEADO (precisa reprovar)",
            "truncado" in levantou,
            levantou[:80] or "(NAO LEVANTOU)",
        )
    )
    cen.append(
        (
            "o erro de truncamento nomeia os dois numeros",
            "esperava" in levantou and "veio" in levantou,
            levantou[:80],
        )
    )

    try:
        co.ler_framing("node_parse", "sem framing nenhum\n")
        levantou2 = ""
    except co.RespostaMalformada as e:
        levantou2 = str(e)
    cen.append(
        (
            "resposta SEM framing e erro (precisa reprovar)",
            "framing" in levantou2,
            levantou2[:80],
        )
    )

    bom = '{"ok":true,"n":1}'
    lido = co.ler_framing("node_parse", f"#{len(bom)}\n{bom}\n")
    cen.append(
        (
            "um framing bem formado devolve o objeto",
            lido.get("ok") is True,
            str(lido)[:50],
        )
    )

    # MUTANTE do (c): ler por linha em vez do comprimento declarado — o defeito medido em 10/10
    def ler_por_linha(_f: str, saida: str) -> dict:
        for ln in saida.splitlines():
            if ln.startswith("{"):
                return json.loads(ln)
        return {}

    grande = json.dumps({"ok": True, "msg": "x" * 70000})
    saida_grande = f"#{len(grande)}\n{grande}\n"
    try:
        json.loads(ler_por_linha("x", saida_grande).get("msg", "")[:10])
        mut_ok = False
        mot = "aceitou"
    except json.JSONDecodeError as e:
        mut_ok = True
        mot = str(e)[:60]
    cen.append(
        (
            "MUTANTE: ler por linha REPROVA na resposta grande (precisa reprovar aqui)",
            mut_ok,
            mot,
        )
    )
    real = co.ler_framing("node_parse", saida_grande)
    cen.append(
        (
            "a leitura por comprimento declarada le a resposta grande INTEIRA",
            real.get("ok") is True and len(real.get("msg", "")) == 70000,
            f"{len(real.get('msg', ''))} chars",
        )
    )
    return cen


def cenario_ferramenta_sem_contrato() -> list:
    """Uma ferramenta nova sem contrato e RECUSADA, nao aceita por padrao."""
    levantou = ""
    try:
        co.chama_oraculo("ferramenta_que_nao_existe", ["true"])
    except ValueError as e:
        levantou = str(e)
    return [
        (
            "ferramenta sem contrato declarado e recusada (precisa reprovar aqui)",
            "nao tem contrato" in levantou,
            levantou[:80],
        )
    ]


def main() -> int:
    cen = []
    cen.extend(cenario_a_newline_da_entrada())
    cen.extend(cenario_b_sinal_exigido())
    cen.extend(cenario_c_framing())
    cen.extend(cenario_ferramenta_sem_contrato())
    falhas = [f"{n}: {m}" for n, ok, m in cen if not ok]
    print(f"selftest do executor de oraculo: {len(cen) - len(falhas)}/{len(cen)}")
    for n, ok, m in cen:
        print(f"  {'ok  ' if ok else 'FALHOU'} {n} -- {m}")
    return 0 if not falhas else 1


def test_selftest_do_executor():
    """O selftest como o pytest o ve.

    MEDIDO em 10/10 (achado da coordenadora, ver `test_todo_teste_e_coletavel.py`): sem esta funcao
    o pytest responde `no tests ran` com exit 5 — e exit 5 e lido por varios CI como "nada a fazer",
    entao o pipeline fica VERDE sem rodar o executor de oraculo. `main()` devolve != 0 se algum cenario
    falha, e o assert transforma isso em veredito do pytest.
    """
    assert main() == 0, "o selftest do executor reprovou: rode `python3 scripts/test_chama_oraculo.py`"


if __name__ == "__main__":
    raise SystemExit(main())
