#!/usr/bin/env python3
"""O framing do adjudicador: o corpo e lido em BYTES, e o frame INTEIRO chega ao chamador.

MEDIDO em 01/11 (V-8): o `oraculo_ls.js` declara o comprimento do corpo com
`Buffer.byteLength(corpo, "utf8")` — BYTES — e escreve o corpo em UTF-8. O rotulador lia com
`text=True` e `stdout.read(n)`, que pede n **CARACTERES**. Num corpo com acento (o `zod` tem),
os caracteres sao MENOS que os bytes declarados: o pipe esvaziava e o `read` bloqueava PARA
SEMPRE, com o node VIVO e ocioso, sem erro e sem saida. Era o run do V-8 anterior, que voltou
vazio.

O selftest nao testa a happy path: testa que cada GARANTIA some quando ela e removida. O
MUTANTE aqui e o leitor em modo TEXTO, que precisa TRAVAR para provar que o teste distingue.

Tres clausulas:
  (a) o corpo com caractere multibyte CHEGA INTEIRO (o `read` nao bloqueia);
  (b) dois comandos SEGUIDOS funcionam — o `\n` que o oraculo escreve depois do corpo nao pode
      sobrar e virar o cabecalho do proximo frame;
  (c) o `ler_framing` compara o comprimento em BYTES: um frame inteiro com acento nao pode ser
      acusado de "truncado".

Uso: python3 -I test_rotular_oss_framing.py   (exit 0 = tudo verde)
"""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import rotular_oss as ro
import chama_oraculo as co

ORACULO_FALSO = """
let buffer = "";
process.stdin.on("data", (d) => {
  buffer += d.toString();
  let i;
  while ((i = buffer.indexOf("\\n")) >= 0) {
    const linha = buffer.slice(0, i);
    buffer = buffer.slice(i + 1);
    if (!linha.trim()) continue;
    const corpo = JSON.stringify({ ok: true, msg: "acentuação: çãõ é" });
    process.stdout.write(`#${Buffer.byteLength(corpo, "utf8")}\\n${corpo}\\n`);
  }
});
"""

MSG_ESPERADO = "acentuação: çãõ é"


def escreve_oraculo_falso(tmp: Path) -> Path:
    p = tmp / "oraculo_falso.js"
    p.write_text(ORACULO_FALSO, encoding="utf-8")
    return p


def _leitor_em_texto_mutante(caminho: Path) -> tuple[bool, str]:
    """O leitor ANTIGO: pipe em modo TEXTO e `read(n)` pedindo n caracteres.

    Devolve (terminou, motivo). Com um corpo multibyte, NAO termina — e e isso que prova que o
    defeito existia e que o teste novo distingue.
    """
    p = subprocess.Popen(
        ["node", str(caminho)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )
    estado = {"terminou": False, "motivo": ""}

    def roda():
        try:
            p.stdin.write(json.dumps({"op": "abrir"}) + "\n")
            p.stdin.flush()
            cabecalho = p.stdout.readline()
            n = int(cabecalho[1:].strip())
            corpo = p.stdout.read(n)
            estado["motivo"] = f"leu {len(corpo)} chars de {n} bytes declarados"
            estado["terminou"] = True
        except Exception as e:  # noqa: BLE001
            estado["motivo"] = f"excecao: {e}"

    t = threading.Thread(target=roda, daemon=True)
    t.start()
    t.join(5)
    if t.is_alive():
        p.kill()
        estado["motivo"] = "TRAVOU: o read(n) pediu caracteres que o frame nao tem"
    p.kill()
    return estado["terminou"], estado["motivo"]


def cenario_leitor_em_bytes() -> list:
    """(a) e (b): o corpo multibyte chega inteiro, e dois comandos seguidos funcionam."""
    cen = []
    with tempfile.TemporaryDirectory() as tmp:
        falso = escreve_oraculo_falso(Path(tmp))
        original = ro.ORACULO_LS
        ro.ORACULO_LS = falso
        try:
            lsc = ro.LS(falso)
            respostas = []
            erro = []

            def roda():
                try:
                    for _ in range(2):
                        respostas.append(lsc.cmd({"op": "abrir"}))
                except Exception as e:  # noqa: BLE001
                    erro.append(str(e))

            t = threading.Thread(target=roda, daemon=True)
            t.start()
            t.join(30)
            travou = t.is_alive()
            if travou:
                lsc.proc.kill()
            cen.append(
                (
                    "o corpo com acento CHEGA INTEIRO (o leitor nao trava)",
                    not travou and not erro,
                    (erro[0][:120] if erro else ("TRAVOU" if travou else f"{len(respostas)} respostas")),
                )
            )
            cen.append(
                (
                    "o texto multibyte volta EXATO",
                    bool(respostas) and respostas[0].get("msg") == MSG_ESPERADO,
                    repr(respostas[0].get("msg")) if respostas else "(sem resposta)",
                )
            )
            cen.append(
                (
                    "dois comandos SEGUIDOS funcionam (o \\n do corpo nao vira cabecilho)",
                    len(respostas) == 2 and not erro,
                    f"{len(respostas)} resposta(s) erro={erro[:1]}",
                )
            )
        finally:
            ro.ORACULO_LS = original
    return cen


def cenario_mutante_em_texto_trava() -> list:
    """O MUTANTE: o leitor em modo TEXTO TRAVA num corpo multibyte."""
    cen = []
    with tempfile.TemporaryDirectory() as tmp:
        falso = escreve_oraculo_falso(Path(tmp))
        terminou, motivo = _leitor_em_texto_mutante(falso)
        cen.append(
            (
                "MUTANTE: o leitor em modo TEXTO TRAVA (precisa reprovar aqui)",
                not terminou,
                motivo[:120],
            )
        )
    return cen


def cenario_ler_framing_compara_bytes() -> list:
    """(c) o `ler_framing` compara o comprimento em BYTES."""
    cen = []
    corpo = json.dumps({"ok": True, "msg": MSG_ESPERADO}, ensure_ascii=False)
    n_bytes = len(corpo.encode("utf-8"))
    saida = f"#{n_bytes}\n{corpo}\n"
    try:
        obj = co.ler_framing("node_ls", saida)
        ok = obj.get("msg") == MSG_ESPERADO
        motivo = repr(obj.get("msg"))[:80]
    except Exception as e:  # noqa: BLE001
        ok = False
        motivo = str(e)[:100]
    cen.append(
        (
            "o frame INTEIRO com acento nao e acusado de truncado",
            ok,
            f"n_bytes={n_bytes} n_chars={len(corpo)} {motivo}",
        )
    )
    # e um frame realmente truncado ainda reprova
    saida_truncada = f"#{n_bytes}\n{corpo[:10]}\n"
    try:
        co.ler_framing("node_ls", saida_truncada)
        reprovou = False
        motivo = "aceitou"
    except co.RespostaMalformada as e:
        reprovou = True
        motivo = str(e)[:80]
    cen.append(
        (
            "MUTANTE: um frame truncado ainda REPROVA (precisa reprovar aqui)",
            reprovou,
            motivo,
        )
    )
    return cen


def main() -> int:
    cen = []
    cen.extend(cenario_leitor_em_bytes())
    cen.extend(cenario_mutante_em_texto_trava())
    cen.extend(cenario_ler_framing_compara_bytes())
    falhas = [f"{n}: {m}" for n, ok, m in cen if not ok]
    print(f"selftest do framing do oraculo: {len(cen) - len(falhas)}/{len(cen)}")
    for n, ok, m in cen:
        print(f"  {'ok  ' if ok else 'FALHOU'} {n} -- {m}")
    return 0 if not falhas else 1


def test_selftest_do_framing():
    """O selftest como o pytest o ve (exit 5 = 'nada a fazer' lido como verde)."""
    assert main() == 0, (
        "o selftest do framing reprovou: rode `python3 scripts/test_rotular_oss_framing.py`"
    )


if __name__ == "__main__":
    raise SystemExit(main())
