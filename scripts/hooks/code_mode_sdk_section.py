#!/usr/bin/env python3
"""SessionStart — injeta o contrato do code mode UMA VEZ por sessão.

Por que uma vez, e não por chamada
----------------------------------
Medido na sessão de 25/08/2026: **1.196 injeções de nudge, ~1,12 MB de contexto**,
pagas por todas as ferramentas — Bash em 66% das chamadas, Edit em 170%, e o
próprio `touring run` em 94%. A adoção que essa persuasão induziu foi de 28%.
Persuasão repetida é o modo caro de ensinar uma regra que cabe numa seção.

O harness do DeepSeek resolve isso pela apresentação, não pela repetição: o SDK
gerado entra no system prompt e o README do `dsh-agent-tool-presentation` registra
a razão — *"the presentation is fixed when the agent is composed, so its request
prefix is stable for the session's life"*. Prefixo estável é prefixo cacheável.
Aqui vale o mesmo: o stub é ordenado lexicograficamente e byte-estável, então
duas sessões no mesmo projeto emitem exatamente os mesmos bytes.

O que esta seção NÃO faz
------------------------
Não impõe nada. Anúncio ensina a regra; quem a aplica é o executor (os gates
G1/G6/G8 no `PreToolUse`) — a lição D8 de `rules/touring-4-pillars.md`, e o
postmortem do DeepSeek de 07/08 diz o mesmo com todas as letras: *"schema
omission is not enforcement when a direct caller can bypass it"*.

Kill switch humano: TOURING_SDK_SECTION_DISABLED=1
"""

from __future__ import annotations

import json
import os
import subprocess
import sys

# Teto do que esta seção pode custar. O stub real mede ~1,3 KB; um stub que
# cresça além disto deixou de ser uma seção e virou um dump — e a regra de
# densidade vale para ela como vale para qualquer injeção.
MAX_BYTES = 4096


def sdk_stub() -> str | None:
    """O contrato tipado, direto do binário que o executa.

    Gerado pelo MESMO binário que atende as sub-chamadas, e não transcrito à
    mão: um contrato copiado envelhece em silêncio, e o modelo passa a escrever
    contra métodos que não existem mais.
    """
    try:
        out = subprocess.run(
            ["touring", "run", "--lang", "python", "--sdk-stub"],
            capture_output=True,
            text=True,
            timeout=6,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    if out.returncode != 0 or not out.stdout.strip():
        return None
    return out.stdout.strip()


def presentation(project_root: str) -> tuple[str, str]:
    """A apresentação do escopo: sessão → alias → projeto → default.

    Espelha `code_mode_presentation` em cli_suggester.rs — as duas leituras
    respondem a MESMA pergunta ("como este escopo apresenta o code mode?") e
    divergir seria ensinar uma regra que o executor não aplica (a lição D8:
    o texto declarado e o predicado do executor derivam da mesma fonte).
    Devolve (modo, origem) — a origem viaja para a seção poder dizer de onde
    a regra veio.
    """
    env = os.environ.get("TOURING_CODE_MODE", "").strip()
    if env in ("native", "code", "both"):
        return env, "TOURING_CODE_MODE (sessão)"
    if os.environ.get("TOURING_CODE_ONLY") == "1":
        return "code", "TOURING_CODE_ONLY=1 (alias)"
    toml = os.path.join(project_root, ".touring", "touring.toml")
    try:
        na_secao = False
        with open(toml, encoding="utf-8") as fh:
            for linha in fh:
                l = linha.strip()
                if l.startswith("["):
                    na_secao = l == "[code_mode]"
                    continue
                if not na_secao or "=" not in l:
                    continue
                chave, valor = l.split("=", 1)
                if chave.strip() != "mode":
                    continue
                v = valor.strip().strip('"')
                if v in ("native", "code", "both"):
                    return v, f"{toml} (projeto)"
                break
    except OSError:
        pass
    return "both", "default"


def section(stub: str, modo: str, origem: str) -> str:
    """A seção completa: apresentação do escopo, transporte, contrato, e a regra."""
    efeito = {
        "code": (
            "inspeção ISOLADA PASSA — de qualquer classe. O que colapsa é a "
            "RAJADA: a 2ª chamada de `grep`/`cat`/`find`/`ls`/`wc`/`sed-n` "
            "dentro de 300s volta NEGADA com as duas fundidas em 1 programa "
            "(a 1ª já executou intacta). Calibrado por medição em 115 "
            "transcripts: 77,5% do volume de inspeção está em rajadas ≥2, e "
            "os 22,5% isolados não pagam mais pedágio. Um `touring run` na "
            "janela zera a contagem."
        ),
        "native": "indução desligada neste escopo — nenhum nudge de code mode.",
        "both": (
            "rajada de inspeção da mesma classe vira advisory e depois deny "
            "progressivo (G1) — o laço acumulado é entregue como 1 programa."
        ),
    }[modo]
    return (
        f"[CODE MODE — contrato desta sessão · apresentação `{modo}` ({origem})]\n"
        f"Efeito: {efeito}\n"
        "Transporte: `touring run --lang <python|bash> --code '<programa>'`. "
        "Uma chamada executa o programa inteiro no sandbox e devolve só o que ele "
        "imprime — N round-trips viram 1 (Anthropic CodeAct / programmatic tool "
        "calling; Cloudflare Code Mode).\n"
        "Quando: a pergunta toca ≥3 arquivos/fatos, ou a mesma classe de inspeção "
        "se repete. Para 1 arquivo já conhecido, a chamada atômica é melhor — "
        "forçar programa no caso simples taxa o caso comum.\n"
        "`--orchestrate` liga o SDK abaixo, com o qual o programa consulta o daemon "
        "de dentro do sandbox. `--brief` devolve o digest e DECLARA o que elidiu "
        "(`elided_lines`). `--harvest <slug>` persiste o programa como snippet.\n\n"
        f"{stub}\n"
    )


def main() -> int:
    """Emite a seção; qualquer erro é silêncio, jamais um bloqueio."""
    if os.environ.get("TOURING_SDK_SECTION_DISABLED") == "1":
        return 0
    stub = sdk_stub()
    if not stub:
        # Sem transporte não há contrato a anunciar. Silêncio é a resposta
        # honesta — anunciar um SDK indisponível ensinaria uma rota morta.
        return 0
    modo, origem = presentation(os.getcwd())
    body = section(stub, modo, origem)
    if len(body.encode()) > MAX_BYTES:
        body = body.encode()[:MAX_BYTES].decode(errors="ignore")
    print(
        json.dumps(
            {
                "hookSpecificOutput": {
                    "hookEventName": "SessionStart",
                    "additionalContext": body,
                }
            }
        )
    )
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception:  # fail-open é invariante de hook (CEG)
        sys.exit(0)
