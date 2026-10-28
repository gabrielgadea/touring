#!/usr/bin/env python3
# #tags: kind:script purpose:measure domain:code-mode process:measure lang:python
"""gerar_vetores_rust.py — reference vectors for the SimHash port, computed by the production binary.

Each body is wrapped as a no-op bash heredoc (``: <<'ZZZ' … ZZZ``), run through
``touring run --lang bash --allow-repeat --code <body>``, and the ``code_simhash`` the binary
journaled for the returned ``run_id`` is the expected value. The wrapper lines are part of the
hashed code, so the vector is exactly (code string → Rust simhash).

The bodies are synthetic (no session content) and aim at the places a port diverges: comments,
blank lines, CRLF, form feed / vertical tab / U+2028 mid-line, NBSP, accents (NFC and NFD), digits
and superscripts, underscores, punctuation runs, fewer than 3 tokens, long realistic code.

Writes ``vetores_rust.json`` beside this file; refuses to overwrite unless ``--sobrescrever``.
Non-Latin scripts are stored as ``\\uXXXX`` escapes (lossless) so no raw foreign character lands in
versioned prose.
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
import time
from pathlib import Path

AQUI = Path(__file__).resolve().parent
SAIDA = AQUI / "vetores_rust.json"
JOURNAL = Path.home() / ".claude" / "touring" / "run_journal.jsonl"
_NAO_LATINO = re.compile(r"[\u0370-\u03ff\u0400-\u04ff\u0590-\u06ff\u3040-\u30ff\u3400-\u9fff\uac00-\ud7af]")

CORPOS: dict[str, str] = {
    "python_simples": "def total(xs):\n    return sum(x for x in xs if x > 0)\nprint(total([1, -2, 3]))",
    "comentarios_e_vazias": "# cabecalho\n\nimport json\n\n// estilo c\nx = json.loads('{}')\n   # recuado\n",
    "crlf": "a = 1\r\nb = a + 2\r\nprint(b)\r\n",
    "pontuacao_colada": "obj.metodo(arg)[0]->campo::x != y && z <= 10",
    "underscore_e_digitos": "valor_total_2026 = calc_x2(base_10) * 3_000",
    "acentos_nfc": "situação = 'ação concluída'; função(variável)",
    "acentos_nfd": "situac\u0327a\u0303o = 'ac\u0327a\u0303o'",
    "sobrescrito_e_numerais": "area = lado\u00b2 + \u2167 + \u00bd",
    "nbsp_meio_da_linha": "alfa\u00a0beta\u00a0gama delta",
    "form_feed_e_vt": "um\x0cdois\x0btres quatro",
    "separador_de_linha_unicode": "linha um\u2028linha dois tres",
    "tabs": "\tif x:\n\t\ty = x\t+ 1",
    "dois_tokens": "ok",
    "um_token_so": "x",
    "so_comentarios": "# nada\n// nada",
    "bash_loop": 'for f in *.py; do\n  wc -l "$f"\ndone | sort -n | tail -3',
    "sql_like": "SELECT a, b FROM t WHERE c = 'x' AND d IN (1, 2, 3);",
    "json_like": '{"chave": [1, 2, {"sub": null}], "outra": true}',
    "escrita_cjk": "nome = '\u7b80\u4f53' + '\u4e2d\u6587'",
    "emoji": "status = 'ok \u2705' if feito else 'falha \u274c'",
    "longo_realista": (
        "import argparse\nimport json\nfrom pathlib import Path\n\n"
        "def carregar(p: Path) -> list[dict]:\n    return [json.loads(l) for l in p.read_text().splitlines() if l]\n\n"
        "def main() -> int:\n    ap = argparse.ArgumentParser()\n    ap.add_argument('arquivo', type=Path)\n"
        "    args = ap.parse_args()\n    linhas = carregar(args.arquivo)\n"
        "    print(json.dumps({'n': len(linhas)}))\n    return 0\n"
    ),
    "longo_realista_renomeado": (
        "import argparse\nimport json\nfrom pathlib import Path\n\n"
        "def ler(p: Path) -> list[dict]:\n    return [json.loads(l) for l in p.read_text().splitlines() if l]\n\n"
        "def main() -> int:\n    ap = argparse.ArgumentParser()\n    ap.add_argument('arquivo', type=Path)\n"
        "    args = ap.parse_args()\n    registros = ler(args.arquivo)\n"
        "    print(json.dumps({'n': len(registros)}))\n    return 0\n"
    ),
}


def _codigo(corpo: str) -> str:
    return f": <<'ZZZ'\n{corpo}\nZZZ\n"


def _simhash_do_binario(codigo: str) -> str:
    r = subprocess.run(
        ["touring", "run", "--lang", "bash", "--allow-repeat", "--code", codigo],
        capture_output=True, text=True, timeout=60, check=False,
    )
    inicio = r.stdout.find("{")
    if inicio < 0:
        raise RuntimeError(f"touring run sem JSON (rc={r.returncode}): {r.stderr[-300:]}")
    run_id = json.loads(r.stdout[inicio:])["run_id"]
    for _ in range(20):  # the journal append can trail the process exit by a few ms
        for linha in reversed(JOURNAL.read_text(encoding="utf-8").splitlines()[-200:]):
            if f'"{run_id}"' in linha:
                valor = json.loads(linha).get("code_simhash")
                if not valor:
                    raise RuntimeError(f"{run_id} sem code_simhash no journal")
                return valor
        time.sleep(0.05)
    raise RuntimeError(f"{run_id} não apareceu no journal")


def main() -> int:
    if SAIDA.exists() and "--sobrescrever" not in sys.argv:
        print(json.dumps({"erro": f"{SAIDA.name} já existe; use --sobrescrever"}))
        return 2
    vetores = []
    for nome, corpo in CORPOS.items():
        codigo = _codigo(corpo)
        vetores.append({"nome": nome, "codigo": codigo, "simhash_rust": _simhash_do_binario(codigo)})
    versao = subprocess.run(["touring", "--version"], capture_output=True, text=True, check=False)
    dados = {
        "origem": "touring run --allow-repeat → run_journal.jsonl code_simhash",
        "binario": (versao.stdout or versao.stderr).strip().splitlines()[0],
        "gerado_em": time.strftime("%Y-%m-%d"),
        "vetores": vetores,
    }
    texto = json.dumps(dados, ensure_ascii=False, indent=2)
    SAIDA.write_text(_NAO_LATINO.sub(lambda m: f"\\u{ord(m.group()):04x}", texto) + "\n", encoding="utf-8")
    print(json.dumps({"vetores": len(vetores), "saida": str(SAIDA)}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
