"""Mutação do shell_inline (conjunto do SUBAGENTE; o da coordenadora é o mutar_shell_inline.py, não mexer nele).

Cada mutante troca UM predicado e o selftest tem de falhar em ≥1 caso. Os 6 mutantes ANTIGOS são reconstruídos.
Controle: o mutante neutro TEM de sobreviver. Cada mutante vira um arquivo e roda como subprocesso (sem exec).
Uso: python3 mutar_shell_inline_sub.py [caminho do shell_inline.py]
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ALVO = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts/shell_inline.py"
SC = Path(__file__).parent
DIR = SC / "mutantes_shell_inline"

# (grupo, nome, trecho original — único no fonte, trecho mutante)
MUTANTES: list[tuple[str, str, str, str]] = [
    ("neutro", "controle: troca por texto idêntico", "_PROF_MAX = 3", "_PROF_MAX = 3"),
    ("antigo", "O1 \\$ deixa de ser desescapado em aspas duplas", '_DUPLAS_ESC = {"$", "`", \'"\', "\\\\"}', '_DUPLAS_ESC = {"`", \'"\', "\\\\"}'),
    ("antigo", "O2 python -m vira programa inline", 'if re.fullmatch(r"-[A-Za-z]*m.*", v):', 'if re.fullmatch(r"-[A-Za-z]*Z.*", v):'),
    ("antigo", "O3 script no argv vira stdin", 'return "script", i', 'return "stdin", -1'),
    ("antigo", "O4 ambíguo só com 3+ programas", "if len(validos) > 1:", "if len(validos) > 2:"),
    ("antigo", "O5 <<- não tira os tabs", "if tira_tab:", "if False:"),
    ("antigo", "O6 comentário # não é ignorado", 'elif c == "#":', 'elif c == "#" and False:'),
    ("L1", "N1a opção curta nunca toma valor", "return 1 if k + 1 < len(v) else 2", "return 1"),
    ("L1", "N1b opção longa `--opt v` nunca toma valor", "n += 2 if v in longas else 1", "n += 1"),
    ("L1", "N1c env: -C/--chdir/-S deixam de tomar valor", '"env": ("uCS", frozenset({"--unset", "--chdir", "--split-string"}), 0),', '"env": ("u", frozenset({"--unset"}), 0),'),
    ("L1", "N1d timeout: a duração deixa de ser operando", '"timeout": ("sk", frozenset({"--signal", "--kill-after"}), 1),', '"timeout": ("sk", frozenset({"--signal", "--kill-after"}), 0),'),
    ("L1", "N1e setsid some da tabela", '"setsid": ("", frozenset(), 0),', ""),
    ("L1", "N1f `--` do embrulho não é consumido (= M7 da coordenadora)", 'if v == "--":\n            return n + 1', 'if v == "--":\n            return n'),
    ("L2", "N2a o aninhado não é analisado", "for a in _varrer(texto, ctx.dentro())", "for a in []"),
    ("L2", "N2b profundidade sem limite", "if ctx.prof > _PROF_MAX:", "if ctx.prof > 99:"),
    ("L2", "N2c bash -c com expansão é analisado mesmo assim", "None if w.expande else w.valor,", "w.valor,"),
    ("L2", "N2d só `-c` puro (não -lc)", 'tem_c = tem_c or (v[0] == "-" and "c" in v[1:])', 'tem_c = tem_c or v == "-c"'),
    ("L2", "N2e `-o pipefail` não consome o valor", 'i += 2 if v[-1] in "oO" else 1', "i += 1"),
    ("L2", "N2f bash com script no argv lê o heredoc como programa", "r = None if (i < len(argv) and not tem_s) else _redir_stdin(s)", "r = _redir_stdin(s)"),
    ("L3", "N3a barra+quebra não é removida", 'if par == "\\\\\\n":', "if False:"),
    ("L3", "N3b $5 (dígito) deixa de expandir", "[A-Za-z_{(\\[0-9@*#?$!-]", "[A-Za-z_{(\\[@*#?$!-]"),
    ("L3", "N3c <<\\EOF não cita o delimitador", "p.citada = True  # `<<\\EOF`", "pass  # `<<\\EOF`"),
    ("L3", "N3d barra no fim engolindo o delimitador é aceita", "if engoliu:", "if False:"),
    ("L3", "N3e \\\\ não vira \\", 'nxt in "$`\\\\":', 'nxt in "$`":'),
    ("L3", "N3f $[…] deixa de expandir", "[A-Za-z_{(\\[0-9@*#?$!-]", "[A-Za-z_{(0-9@*#?$!-]"),
    ("L4", "N4a touring run --file deixa de ser candidato", 'return a.texto is not None or a.via == "touring_run_file"', "return a.texto is not None"),
    ("L4", "N4b candidatos devolve só o primeiro", "return [a for a in achados(comando, substituicoes) if _valido(a)]", "return [a for a in achados(comando, substituicoes) if _valido(a)][:1]"),
    ("L4", "N4c recusa principal volta a ser a 1ª", "return next((a for a in todos if a.via != \"nenhuma\"), todos[0])", "return todos[0]"),
    ("S", "N5a substituições sempre ligadas", "if not ctx.subst:", "if False:"),
    ("S", "N5b `)` entre aspas duplas fecha a substituição", "if ch == '\"':  # um `)` entre aspas duplas não fecha a substituição", "if False:  # um"),
    ("S", "N5c crase deixa de ser registrada", 'if bruto.startswith("`"):', "if False:"),
    ("S", "N5d o corpo de heredoc dentro de $(…) passa a contar aspas", 'return self._fim_heredoc(j) if ch == "<" else j', "return j"),
    ("E", "N6a comando em posição de expansão vira 'nenhum interpretador'", "if argv and argv[0].expande and _MENCIONA_PY.search(texto):", "if False:"),
    ("D", "D1 candidatos passa a ter default False", "def candidatos(comando: str, substituicoes: bool = True)", "def candidatos(comando: str, substituicoes: bool = False)"),
    ("D", "D2 achados passa a ter default False", "def achados(comando: str, substituicoes: bool = True)", "def achados(comando: str, substituicoes: bool = False)"),
    ("D", "D3 analisar passa a ter default True", "def analisar(comando: str, substituicoes: bool = False)", "def analisar(comando: str, substituicoes: bool = True)"),
    ("D", "D4 programa_python passa a ter default True", "def programa_python(comando: str, substituicoes: bool = False)", "def programa_python(comando: str, substituicoes: bool = True)"),
]


def rodar(caminho: Path) -> tuple[bool, str, list[str]]:
    """(selftest verde?, linha-resumo, primeiros casos que caíram). Exceção/timeout contam como 'quebrou'."""
    try:
        r = subprocess.run([sys.executable, "-I", str(caminho), "--selftest"], capture_output=True, text=True, timeout=120)
    except subprocess.TimeoutExpired:
        return False, "TIMEOUT", []
    saida = r.stdout.splitlines()
    caidos = [ln.strip()[:90] for ln in saida if ln.startswith("ERRADO")]
    resumo = saida[-1] if saida else f"sem saída (rc={r.returncode}) {r.stderr.strip().splitlines()[-1][:60] if r.stderr.strip() else ''}"
    return r.returncode == 0, resumo, caidos


def main() -> int:
    alvo = Path(sys.argv[1]) if len(sys.argv) > 1 else ALVO
    src = alvo.read_text()
    DIR.mkdir(exist_ok=True)
    base = DIR / "m_base.py"
    base.write_text(src)
    verde, resumo, _ = rodar(base)
    print(f"base sem mutação: {resumo} · verde={verde}")
    if not verde:
        return 2
    mortos = 0
    por_grupo: dict[str, list[int]] = {}
    for k, (grupo, nome, velho, novo) in enumerate(MUTANTES):
        n = src.count(velho)
        if n != 1:
            print(f"  [SUBSTRING {n}x] {nome}: {velho[:70]!r}")
            continue
        arq = DIR / f"m_{k:02d}.py"
        arq.write_text(src.replace(velho, novo, 1))
        verde, resumo, caidos = rodar(arq)
        morto = not verde
        if grupo != "neutro":
            por_grupo.setdefault(grupo, [0, 0])
            por_grupo[grupo][1] += 1
            por_grupo[grupo][0] += morto
            mortos += morto
        print(f"  {'MORTO ' if morto else 'VIVO  '} {nome:62} {resumo}" + (f"  ← {caidos[0]}" if caidos else ""))
    total = sum(v[1] for v in por_grupo.values())
    print(f"\nmortos {mortos}/{total} " + " · ".join(f"{g} {m}/{t}" for g, (m, t) in por_grupo.items()))
    return 0 if mortos == total and total == len([m for m in MUTANTES if m[0] != "neutro"]) else 1


if __name__ == "__main__":
    raise SystemExit(main())
