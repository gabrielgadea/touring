"""V0e-resto — o destino das falhas de Bash FORA de gate.

Grava, POR ITEM, o resíduo fora de gate em ~/.claude/touring/vgp/v0e_fora_de_gate.jsonl (fora do
repositorio: as mensagens podem conter dado de processo). O bruto fica em disco; so agregados saem
no relatorio.

A logica de varredura e a de `inventario_erros.Inventory._one`, IMPORTADA (nunca copiada): `residue_class`
decide a classe, `parse_errors` decide se ha diagnostico de linguagem. O que este script acrescenta e o
CORTE SUPERIOR de timestamp (o retrato de 08/10 17:20 BRT = 20:20 UTC) e o destino.

Destino: (a) defeito no codigo do Touring · (b) mensagem devia ensinar a rota · (c) uso legitimo
· (d) ambiente. Cada um por PREDICADO POSITIVO sobre o texto real do erro, nunca por eliminacao.
`nao classificado` nunca e somado a nenhuma classe.

Uso: python3 v0e_fora_de_gate.py [--ate 2026-10-08T20:20:00Z] [--selftest]
"""

import json
import re
import sys
from collections import Counter
from datetime import datetime
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[2] / "scripts"
assert (SCRIPTS / "inventario_erros.py").exists(), (
    f"caminho de scripts errado: {SCRIPTS}"
)
sys.path.insert(0, str(SCRIPTS))
from inventario_erros import (  # noqa: E402
    ROOTS,
    SINCE,
    _text,
    parse_errors,  # noqa: E402
    residue_class,
    unwrap_run,
)

OUT = Path.home() / ".claude/touring/vgp/v0e_fora_de_gate.jsonl"
ATE_DEFAULT = "2026-10-08T20:20:00Z"
LIMITE = 200

# ---------------------------------------------------------------- destino: predicados POSITIVOS
# ORDEM = especificidade. O erro de 1a versao: um `^Exit code [123]$` com re.M casa a linha em
# QUALQUER lugar do texto, e 133 `touring run falhou` (cuja 1a linha e so "Exit code 1", com o
# envelope mais abaixo) cairam em "uso legitimo". Aqui o destino do `touring run` e decidido pelo
# ENVELOPE (falha_kind), nao pela linha de saida.
DESTINO = [
    (
        "d",
        "ambiente (rede)",
        re.compile(
            r"Temporary failure in name resolution|Connection refused|Network is unreachable|"
            r"ETIMEDOUT|ECONNRESET|could not resolve host"
        ),
    ),
    (
        "d",
        "permissao do SO / sandbox",
        re.compile(
            r"Permission denied|Operation not permitted|landlock|EACCES|Read-only file system"
        ),
    ),
    (
        "d",
        "permissao do Claude Code (negativa do usuario)",
        re.compile(
            r"Permission to use Bash with command|has been denied|doesn't want to proceed",
            re.I,
        ),
    ),
    (
        "d",
        "guard de seguranca do Claude Code",
        re.compile(
            r"Recursive force delete detected|Override safety checks|Recursive deletion",
            re.I,
        ),
    ),
    (
        "d",
        "camada de permissao do Claude Code (Force/approval)",
        re.compile(
            r"^\s*(Force operation|Force without confirmation|This Bash command contains multiple"
            r" operations|This command requires approval|Local branch deletion)",
            re.M,
        ),
    ),
    (
        "a",
        "defeito no codigo do Touring",
        re.compile(
            r"panicked at|unwrap\(\) on a None|error\[E\d{4}\]|cargo test failed", re.I
        ),
    ),
    (
        "b",
        "rota nao ensinada (o gate devia dizer)",
        re.compile(r"similarity=\d+%|redundant-exact-call|repeti[çc][ãa]o", re.I),
    ),
    (
        "c",
        "uso legitimo (grep/test sem casamento, sem envelope)",
        re.compile(r"\A(?:INFO[^\n]*\n)*\s*Exit code [12]\s*\Z"),
    ),
]

# predicados NOVOS para reduzir o 'nao classificado' (item 4 do brief), cada um com controle no selftest.
NOVOS = [
    (
        "saida vazia + exit!=0 (exit code nu)",
        re.compile(r"^Exit code [1-9]\d*\s*$", re.M),
    ),
    ("saida vazia + exit==0 mas is_error", re.compile(r"^Exit code 0\s*$", re.M)),
    (
        "arquivo inexistente nomeado",
        re.compile(r"cannot stat|No such file or directory|does not exist", re.I),
    ),
    (
        "comando inexistente nomeado",
        re.compile(r"command not found|not recognized|No module named", re.I),
    ),
    (
        "git nomeado (fatal/usage)",
        re.compile(r"^(fatal|error):|^usage:|^git:", re.M | re.I),
    ),
    ("timeout nomeado", re.compile(r"timed? ?out|Timeout|exit code 124", re.I)),
    (
        "pytest falhou nomeado",
        re.compile(r"^(FAILED|ERROR) |=+ .*\d+ (failed|error)", re.M),
    ),
]


def classificar_novo(texto: str, ordem=NOVOS) -> str:
    """Classe NOVA para um 'nao classificado', por predicado positivo. Vazio = continua visivel."""
    for nome, pat in ordem:
        if pat.search(texto):
            return nome
    return ""


def destino_de(texto: str, ordem=DESTINO) -> str:
    """Destino por predicado POSITIVO. O resto e nomeado 'c: uso legitimo', nunca reclassificado."""
    for letra, nome, pat in ordem:
        if pat.search(texto):
            return f"{letra}: {nome}"
    return "c: uso legitimo (sem sinal de defeito)"


# destino do `touring run`: decidido pelo ENVELOPE, nunca pela linha de saida (que e so "Exit code N"
# em 133 dos 220 casos e nao distingue nada). failure.kind e o que o sandbox contou.
DESTINO_RUN = {
    "timeout": "b: o gate nao declava o orcamento (default 30s) — ja corrigido no release 09/10",
    "exception": "a: excecao no programa do sandbox — depende do programa, nao do gate",
    "proc-exit": "c: o programa saiu !=0 (uso legitimo do exit como veredito)",
    "invalid-output": "a: defeito no envelope que o `touring run` devolve",
    "output-limit": "b: o orcamento de saida cortou o programa — mensagem nao ensina a dividir",
    "forbidden": "d: capacidade negada pelo X6 (o advisory waived, o comando rodou)",
}


def destino_run(texto: str) -> str:
    """Destino de um `touring run falhou`. Sem envelope no transcript, o destino e declarado, não inferido."""
    k = falha_kind(texto)
    return DESTINO_RUN.get(
        k, "c: uso legitimo (sem envelope: so 'Exit code N' no transcript)"
    )


def falha_kind(texto: str) -> str:
    """failure.kind do envelope do `touring run`; '' quando o envelope nao chegou ao transcript."""
    i = texto.find("{")
    if i < 0 or '"duration_ms"' not in texto:
        return ""
    try:
        env, _ = json.JSONDecoder().raw_decode(texto[i:])
    except json.JSONDecodeError:
        return ""
    if not isinstance(env, dict):
        return ""
    return str((env.get("failure") or {}).get("kind") or "")


def item(c: dict, ts: str, session: str, project: str, cmd: str) -> dict | None:
    """Um residuo fora de gate como linha, ou None se nao for residuo fora de gate."""
    t = _text(c)
    if parse_errors(unwrap_run(t)) or not c.get("is_error"):
        return None
    classe = residue_class(t)
    if classe == "hook/guard negou":
        return None  # ja tratado na V0e
    e_run = classe == "touring run falhou"
    linha = {
        "session": session,
        "tool_use_id": c.get("tool_use_id"),
        "ts": ts,
        "project": project,
        "classe": classe,
        "destino": destino_run(t) if e_run else destino_de(t),
        "primeira_linha": next((x.strip() for x in t.splitlines() if x.strip()), "")[
            :LIMITE
        ],
        "comando": cmd[:LIMITE],
    }
    if classe.startswith("não classificado"):
        linha["classe_nova"] = classificar_novo(t) or "nao classificado (sem predicado)"
    if e_run:
        linha["failure_kind"] = falha_kind(t) or "sem envelope no transcript"
        linha["declara_timeout_ms"] = "--timeout-ms" in cmd
    return linha


def eventos(f: Path, ate: str) -> list[tuple[str, dict]]:
    """(timestamp, bloco) de cada tool_use/tool_result do transcript dentro da janela."""
    out = []
    for raw in f.open(encoding="utf-8", errors="replace"):
        if '"tool_use"' not in raw and '"tool_result"' not in raw:
            continue
        try:
            rec = json.loads(raw)
        except json.JSONDecodeError:
            continue
        ts = str(rec.get("timestamp", ""))
        if ts < SINCE or ts > ate:
            continue
        out += [
            (ts, c)
            for c in (rec.get("message") or {}).get("content") or []
            if isinstance(c, dict)
        ]
    return out


def varre(ate: str) -> list[dict]:
    """Todo residuo fora de gate ate `ate`. Uma linha por ITEM."""
    linhas = []
    for root in ROOTS:
        project = root.name.rsplit("-", 1)[-1]
        for f in root.rglob("*.jsonl"):
            if f.stat().st_mtime < datetime.fromisoformat(SINCE).timestamp():
                continue
            nomes, cmds = {}, {}
            for ts, c in eventos(f, ate):
                if c.get("type") == "tool_use":
                    nomes[c["id"]] = c.get("name")
                    cmds[c["id"]] = str((c.get("input") or {}).get("command", ""))
                elif nomes.get(c.get("tool_use_id")) == "Bash":
                    li = item(
                        c, ts, f.stem, project, cmds.get(c.get("tool_use_id"), "")
                    )
                    if li:
                        linhas.append(li)
    return linhas


def selftest() -> int:
    """Controle POSITIVO e NEGATIVO por predicado novo; e um mutante que troca a ordem e pego."""
    checks = {
        "nc: exit code nu": classificar_novo("Exit code 1")
        == "saida vazia + exit!=0 (exit code nu)",
        "nc: exit code 0": classificar_novo("Exit code 0")
        == "saida vazia + exit==0 mas is_error",
        "nc: arquivo": bool(
            classificar_novo("cp: cannot stat '/x': No such file or directory")
        ),
        "nc: comando": bool(classificar_novo("bash: foo: command not found")),
        "nc: git": bool(classificar_novo("fatal: not a git repository")),
        "nc: timeout": bool(classificar_novo("command timed out after 30s")),
        "nc: pytest": bool(classificar_novo("FAILED tests/test_a.py::test_b")),
        "neg: prosa": classificar_novo("the error: word in prose\nall good\n") == "",
        "neg: eco sem erro": classificar_novo("ok") == "",
        "dest: ambiente": destino_de("Temporary failure in name resolution").startswith(
            "d:"
        ),
        "dest: permissao": destino_de("Permission denied").startswith("d:"),
        "dest: legitimo": destino_de("Exit code 1").startswith("c:"),
        "kind: timeout": falha_kind(
            json.dumps({"duration_ms": 1, "failure": {"kind": "timeout"}})
        )
        == "timeout",
        "kind: ausente": falha_kind("Exit code 1") == "",
        # o destino do touring run vem do ENVELOPE, nunca da linha de saida
        "dest_run: timeout": destino_run(
            json.dumps({"duration_ms": 1, "failure": {"kind": "timeout"}})
        ).startswith("b:"),
        "dest_run: exception": destino_run(
            json.dumps({"duration_ms": 1, "failure": {"kind": "exception"}})
        ).startswith("a:"),
        "dest_run: sem envelope": destino_run("Exit code 1").startswith("c:"),
        # e o texto SO com 'Exit code 1' NAO pode virar 'uso legitimo' quando ha envelope embaixo
        "dest_run: exit code 1 + envelope = decidedo pelo kind": not destino_run(
            "Exit code 1\\n"
            + json.dumps({"duration_ms": 1, "failure": {"kind": "timeout"}})
        ).startswith("c:"),
    }
    # MUTANTE que troca a ordem dos regex: o teste so vale se a entrada for AMBIGUA — uma que case
    # mais de um predicado, onde a ordem decide o rotulo. Inverter a lista tem de mudar a resposta,
    # senao o teste nao prova nada (uma entrada de predicado unico passa nas duas ordens).
    ambigua = (
        "fatal: could not read from remote repository\ncommand timed out after 30s"
    )
    original = classificar_novo(ambigua)
    mutada = classificar_novo(ambigua, list(reversed(NOVOS)))
    checks["ambigua casa 2+"] = sum(1 for _, p in NOVOS if p.search(ambigua)) >= 2
    checks["mutante pego"] = original != mutada
    for k, v in checks.items():
        print(f"  {'ok ' if v else 'FALHOU'} {k}")
    return 0 if all(checks.values()) else 1


def main() -> int:
    if "--selftest" in sys.argv:
        return selftest()
    ate = sys.argv[sys.argv.index("--ate") + 1] if "--ate" in sys.argv else ATE_DEFAULT
    linhas = varre(ate)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("w", encoding="utf-8") as fh:
        for li in linhas:
            fh.write(json.dumps(li, ensure_ascii=False) + "\n")

    print(f"itens: {len(linhas)} · bruto: {OUT}\n")
    print(f"  {'classe':<58} {'n':>4}")
    for k, v in Counter(li["classe"] for li in linhas).most_common():
        print(f"  {k[:58]:<58} {v:>4}")
    print(f"\ndestino: {dict(Counter(li['destino'] for li in linhas).most_common())}")
    tr = [li for li in linhas if li["classe"] == "touring run falhou"]
    print(
        f"\ntouring run falhou ({len(tr)}): "
        f"{dict(Counter(li.get('failure_kind') for li in tr).most_common())}"
    )
    sem = sum(
        1
        for li in tr
        if li.get("failure_kind") == "timeout" and not li.get("declara_timeout_ms")
    )
    com = sum(
        1
        for li in tr
        if li.get("failure_kind") == "timeout" and li.get("declara_timeout_ms")
    )
    print(f"  timeout SEM --timeout-ms (default 30s): {sem} · COM: {com}")
    nc = Counter(li.get("classe_nova") for li in linhas if li.get("classe_nova"))
    if nc:
        print(
            f"\n'não classificado' reduzido por predicado positivo: {dict(nc.most_common())}"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
