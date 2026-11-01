"""Linha de base da dívida do `programa_inline` (corpus_v0d): o que ele devolve e se aquilo é o Python que rodou.

Universo: todo `tool_use` Bash dos transcripts do analise (dedupe por id). Para cada comando em que o extrator
devolve texto, a VIA e o CONSUMIDOR são classificados por predicado positivo; o resto vai para "não classificado",
visível, nunca somado a uma classe. Uso: python3 medir_programa_inline.py [--extrator antigo|novo] [--json saida]
"""

from __future__ import annotations

import argparse
import collections
import json
import re
import shlex
import sys
from pathlib import Path

BUNDLE = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"
TRANSCRIPTS = Path.home() / ".claude/projects/-home-gabrielgadea-projects-analise"
sys.path.insert(0, str(BUNDLE))

HEREDOC = re.compile(r"<<-?\s*['\"]?(\w+)['\"]?")
SEGMENTO_ANTES = re.compile(r"(?:^|&&|\|\||;|\||\n)\s*([^\n&|;]*?)\s*<<-?\s*['\"]?\w+", re.M)


def _bash_do_registro(linha: str) -> list[tuple[str, str]]:
    """(tool_use id, comando) de cada bloco Bash de uma linha de transcript."""
    if '"tool_use"' not in linha or '"Bash"' not in linha:
        return []
    try:
        rec = json.loads(linha)
    except json.JSONDecodeError:
        return []
    blocos = (rec.get("message") or {}).get("content") or []
    return [
        (b.get("id", ""), b["input"]["command"])
        for b in blocos
        if isinstance(b, dict) and b.get("type") == "tool_use" and b.get("name") == "Bash"
        and isinstance((b.get("input") or {}).get("command"), str)
    ]


def comandos() -> dict[str, str]:
    """tool_use id → comando, de todos os transcripts (dedupe pelo id)."""
    if not TRANSCRIPTS.is_dir():
        raise SystemExit(f"transcripts inacessíveis: {TRANSCRIPTS} (no sandbox o Landlock nega — rode nativo)")
    out: dict[str, str] = {}
    for f in sorted(TRANSCRIPTS.glob("*.jsonl")):
        with f.open(errors="replace") as fh:
            for linha in fh:
                for tid, cmd in _bash_do_registro(linha):
                    out[tid or f"{f.name}:{len(out)}"] = cmd
    if not out:
        raise SystemExit("0 comandos Bash: universo degenerado (glob vazio sob Landlock devolve 0 em silêncio)")
    return out


def lang_do_run(cmd: str) -> str:
    try:
        t = shlex.split(cmd, posix=True)
    except ValueError:
        m = re.search(r"--lang\s+(\w+)", cmd)
        return m.group(1) if m else "?"
    return t[t.index("--lang") + 1] if "--lang" in t and t.index("--lang") + 1 < len(t) else "(ausente)"


def consumidor(cmd: str) -> str:
    """O 1º verbo do segmento imediatamente antes do PRIMEIRO `<<` (o que HEREDOC_PROG extrai)."""
    m = SEGMENTO_ANTES.search(cmd)
    if not m:
        return "?"
    palavras = m.group(1).split()
    for w in palavras:
        if "=" in w.split("/")[-1] and not w.startswith("-"):
            continue
        base = w.split("/")[-1]
        if base in {"env", "time", "nice", "timeout", "sudo"} or re.fullmatch(r"\d+[smh]?", base):
            continue
        return "python" if re.fullmatch(r"python(3(\.\d+)?)?", base) else base
    return "?"


def classifica(cmd: str) -> tuple[str, str]:
    """(via, detalhe) na MESMA ordem de decisão do extrator antigo."""
    if "touring run" in cmd:
        return "touring_run", f"--lang {lang_do_run(cmd)}"
    if re.search(r"<<-?\s*['\"]?(\w+)['\"]?[^\n]*\n", cmd):
        return "heredoc", consumidor(cmd)
    if re.search(r"python3?\s+-c\s", cmd):
        aspas = "duplas" if re.search(r"-c\s+\"", cmd) else ("simples+'\\''" if "'\\''" in cmd else "simples")
        return "-c", aspas
    return "nao_classificado", ""


def compila(texto: str) -> bool:
    import warnings

    with warnings.catch_warnings():
        warnings.simplefilter("ignore")  # SyntaxWarning de texto que nem é Python não é sinal
        try:
            compile(texto, "<x>", "exec")
        except (SyntaxError, ValueError):
            return False
    return True


def comparar(cmds: dict[str, str]) -> dict:
    """Antigo × novo por classe do antigo: mantido igual · mantido diferente · recusado (por motivo)."""
    from corpus_v0d import programa_inline as antigo
    from shell_inline import analisar

    tab: dict = collections.defaultdict(collections.Counter)
    comp_novo: collections.Counter = collections.Counter()
    so_novo = 0
    for cmd in cmds.values():
        a, n = antigo(cmd), analisar(cmd)
        if a is None and n.texto is None:
            continue
        if a is None:
            so_novo += 1
            tab[("(antigo nada)", n.via)]["novo extrai"] += 1
            comp_novo[("(antigo nada)", n.via)] += compila(n.texto or "")
            continue
        chave = classifica(cmd)
        if n.texto is None:
            tab[chave][f"recusa: {n.motivo[:70]}"] += 1
        else:
            tab[chave]["igual" if n.texto == a else "diferente"] += 1
            comp_novo[chave] += compila(n.texto)
    return {"tab": tab, "comp_novo": comp_novo, "so_novo": so_novo}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--extrator", choices=("antigo", "novo"), default="antigo")
    ap.add_argument("--json")
    ap.add_argument("--comparar", action="store_true")
    a = ap.parse_args()
    if a.comparar:
        r = comparar(comandos())
        for chave, cont in sorted(r["tab"].items(), key=lambda kv: -sum(kv[1].values())):
            total = sum(cont.values())
            print(f"{chave[0]:14} {chave[1][:26]:26} total {total:5} · compila(novo) {r['comp_novo'][chave]:5}")
            for k, n in cont.most_common(5):
                print(f"      {n:5}  {k}")
        print(f"novo extrai onde o antigo não extraía: {r['so_novo']}")
        return 0
    if a.extrator == "antigo":
        from corpus_v0d import programa_inline as extrai
    else:
        from shell_inline import programa_python as extrai  # o extrator novo (fonte única)
    cmds = comandos()
    tab: collections.Counter = collections.Counter()
    comp: collections.Counter = collections.Counter()
    devolveu = 0
    for cmd in cmds.values():
        prog = extrai(cmd)
        if prog is None:
            continue
        devolveu += 1
        chave = classifica(cmd)
        tab[chave] += 1
        comp[chave] += compila(prog)
    print(f"universo: {len(cmds)} comandos Bash · o extrator ({a.extrator}) devolveu texto em {devolveu}")
    for (via, det), n in tab.most_common():
        print(f"  {via:16} {det:28} {n:6}  compila {comp[(via, det)]:6}")
    if a.json:
        Path(a.json).write_text(json.dumps({"universo": len(cmds), "devolveu": devolveu,
                                            "classes": [[v, d, n, comp[(v, d)]] for (v, d), n in tab.most_common()]},
                                           ensure_ascii=False, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
