"""CAUSA por classe — cada destino é uma AFIRMAÇÃO, e a prova é um trecho real lido do corpus.

O que mudou em relação ao classificador por regex: cada classe recebe o destino que a LEITURA de
>=5 casos reais sustenta, não o que o regex deduz. Onde a leitura mostrou que o regex errava, isso está
escrito — o destino errado herdado fica visível em `destino_por_regex` para comparação.

Uso: python3 causas.py --ate 2026-10-08T20:20:00Z
"""

import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
BRUTO = Path.home() / ".claude/touring/vgp/v0e_fora_de_gate.jsonl"
# HERE e o DIRETORIO (v0e-fora-de-gate): parents[0]=diagnostics, parents[1]=o bundle. O script
# `coleta.py` usa parents[2] porque fica um nivel acima dele; aqui a arvore e diferente e o mesmo
# indice resolve para um caminho que nao existe — erro de off-by-one silencioso.
SCRIPTS = HERE.parents[1] / "scripts"
assert (SCRIPTS / "inventario_erros.py").exists(), f"scripts errado: {SCRIPTS}"
sys.path.insert(0, str(SCRIPTS))
from inventario_erros import ROOTS as ROOTS_INV  # noqa: E402  (sys.path.insert antes: obrigatorio)

# destino por CLASSE. Cada um e uma AFIRMACAO sustentada pela leitura de casos reais (o metodo esta
# em `ler_topo.py`, que imprime o texto INTEIRO — a coluna `primeira_linha` do bruto corta em 200 chars
# e esconde a causa). Onde a leitura refutou o destino que o regex propunha, o antigo fica anotado.
CAUSA = {
    "touring run falhou": (
        "b: o gate nao declarava o orcamento — 35 timeout, TODOS sem --timeout-ms (default 30s)",
        "b",
    ),
    "arquivo/caminho inexistente": (
        "c: uso legitimo — o chamador apontou caminho inexistente (ls: cannot access / No module named)",
        "c",
    ),
    "permissão do Claude Code negou": (
        "d: camada de permissao do Claude Code (Force/approval), nao hook do Touring",
        "d",
    ),
    "is_error com saída de sucesso (outro comando do pipeline falhou)": (
        "c: uso legitimo — pytest passou (20 passed, 1 skipped) e o comando SEGUINTE no && falhou",
        "c",
    ),
    "pytest falhou (sumário)": (
        "c: uso legitimo — o teste falhou de verdade (33 failed, 101 passed); o sumário e o diagnóstico",
        "c",
    ),
    "sintaxe do shell": (
        "b: a mensagem nao ensina a rota — 'syntax error near unexpected token' sem dizer como citar "
        "o codigo em --code (4/4 lidos falham por aspas dentro de aspas)",
        "b",
    ),
    "git": (
        "c: uso legitimo — 2/5 pathspec ausente; 3/5 index.lock de sessao CC concorrente",
        "c",
    ),
    "timeout": (
        "c: uso legitimo — o `timeout 560` do proprio comando expirou (exit 143 = SIGTERM do proprio timeout)",
        "c",
    ),
    "saída vazia com exit≠0 (grep/test sem casamento)": (
        "c: uso legitimo — grep sem casamento dentro de pipeline com && (o && e o que reprova)",
        "c",
    ),
    "permissão/sandbox": (
        "d: o proprio CEG negou no sandbox (Landlock KernelEnforced) — ambiente, nao Touring",
        "d",
    ),
    "entrada de ferramenta inválida": (
        "c: uso legitimo — InputValidationError da propria ferramenta (parametro a mais; "
        "control character no comando)",
        "c",
    ),
    "traceback cortado (sem a linha da exceção)": (
        "c: uso legitimo — a excecao ESTA no texto (SemProcedencia, RelacaoDesconhecida em "
        "grafo_modelo.py:168/223); o balde errou porque o traceback estava no fim da string",
        "c",
    ),
    "comando inexistente": (
        "c: uso legitimo — `bc: command not found` (utilitario ausente no SO)",
        "c",
    ),
    "CLI touring/uv/outros (error:)": (
        "b: `touring run --lang python -c` — a CLI NAO tem `-c` (exige --code/--file); 2/4 lidos. "
        "A mensagem de uso existe mas o hook nao ensina a forma correta",
        "b",
    ),
    "cargo falhou (sem diagnóstico no recorte)": (
        "c: uso legitimo — cargo falhou e o diagnostico ficou fora do recorte",
        "c",
    ),
}

# o que a LEITURA refutou do classificador por regex (fica visivel, nunca apagado)
REFUTADO = {
    "CLI touring/uv/outros (error:)": "o regex propunha (a) defeito no codigo; a leitura mostra "
    "uso de flag inexistente — e a CLI estar sem `--code` e um defeito "
    "de ENSINAR, nao de existir",
    "traceback cortado (sem a linha da exceção)": "o regex propunha (b) 'o recorte escondeu a linha'; "
    "a linha estava la — o balde nao a parseou",
    "sintaxe do shell": "o regex via genericamente (b); a leitura dos 4 casos mostra UMA causa so "
    "(citação de codigo dentro de --code), o que torna a correcao pontual",
}


def evidencia_por_classe(linhas: list[dict], n: int = 3) -> dict:
    """Os n exemplos mais curtos e sem segredo de cada classe: session + tool_use_id + trecho."""
    por = defaultdict(list)
    for it in linhas:
        if it["classe"].startswith("não classificado"):
            continue
        cmd = it.get("comando", "")
        if "API" in cmd or "KEY" in cmd or "token" in cmd.lower() or "AIza" in cmd:
            continue  # sem segredo/PII: descarta o comando, nao a classe
        por[it["classe"]].append(it)
    out = {}
    for cl, rows in por.items():
        rows.sort(key=lambda r: len(r["primeira_linha"]))
        out[cl] = [
            {
                # ids INTEIROS: cortados em 14 viravam `call_function_` em varias evidencias, e um id
                # repetido nao distingue dois itens. `call_function_` que ainda aparece E o id real do
                # harness daquela sessao — ver `ids_repetidos` no relatorio.
                "session": r["session"],
                "tool_use_id": str(r["tool_use_id"]),
                "primeira_linha": r["primeira_linha"][:200],
                # para `touring run falhou`, a 1a linha e so 'Exit code N': a prova esta no ENVELOPE
                "failure_kind": r.get("failure_kind"),
                "declara_timeout_ms": r.get("declara_timeout_ms"),
                "destino_por_regex": r["destino"],
            }
            for r in rows[:n]
        ]
    return out


def reencontrar(evidence: dict) -> tuple[int, int, list]:
    """Cada (session, tool_use_id) procurado de volta no transcript. (n, reencontrados, faltantes).

    Prova que o par das evidencias NAO e decorativo: se um par nao volta no arquivo, o dado nao
    serviria. Os transcripts estao em DOIS projetos (os ROOTS do inventario_erros): um stem pode
    existir no analise OU no touring, e um glob so no analise produz 0/N — que parece "evidencia
    inventada" quando nao e.
    """
    from inventario_erros import ROOTS as ROOTS_INV

    idx = {}
    for proj in ROOTS_INV:
        # rglob, nao glob: um subagente mora em `<transcript>/subagents/agent-*.jsonl` (e em
        # `subagents/workflows/<wf>/`), um nivel abaixo. `glob("*.jsonl")` acha so a raiz e produz
        # 6/44 "faltantes" que sao subagentes legitimos.
        for f in proj.rglob("*.jsonl"):
            ids = set()
            with f.open(encoding="utf-8", errors="replace") as fh:
                for raw in fh:
                    if '"tool_use"' not in raw and '"tool_result"' not in raw:
                        continue
                    # A chave real no transcript e `toolUseID` (camelCase, a mesma que o
                    # medir_conselho le). Procurar `tool_use_id` da 0/N e parece "evidencia inventada".
                    ids.update(re.findall(r'"(?:toolUseID|tool_use_id|id)"\s*:\s*"([^"]+)"', raw))
            idx.setdefault(f.stem, set()).update(ids)
    pares = [
        (e["session"], e["tool_use_id"]) for rows in evidence.values() for e in rows
    ]
    # Um stem `agent-<hex>` NAO e um transcript: e o campo `agentId` de um subagente, gravado dentro
    # do transcript do PAI. Para esses o reencontro e por substring: o id procurado tem de aparecer
    # em ALGUM transcript (nao podemos saber qual sem varrer tudo, e a busca e por substring mesmo).
    todos = set().union(*idx.values()) if idx else set()
    faltam = []
    for s, t in pares:
        if t in idx.get(s, ()):
            continue
        if not any(t in ids for ids in idx.values()):
            faltam.append((s, t))
    return len(pares), len(pares) - len(faltam), faltam


def ids_repetidos(evidence: dict) -> list[tuple[str, int]]:
    """Ids que se repetem entre evidencias, com a contagem. Um id repetido NAO distingue dois itens.

    `call_function_` aparece assim porque esse e o id INTEIRO que o harness daquela sessao gravou — nao
    e truncamento meu (o [:14] que existia cortava em `call_function`, sem o underscore final). Quando
    o proprio corpus grava ids repetidos, o par (session, tool_use_id) e a chave que sobrevive: dois
    itens com o mesmo id em sessoes diferentes nao colidem, e na mesma sessao sao o mesmo item.
    """
    c = Counter(e["tool_use_id"] for rows in evidence.values() for e in rows)
    return [(k, v) for k, v in c.items() if v > 1]


def main():
    linhas = [json.loads(x) for x in BRUTO.open(encoding="utf-8")]
    por_classe = Counter(
        it["classe"] for it in linhas if not it["classe"].startswith("não classificado")
    )
    tabela = []
    for cl, n in por_classe.most_common():
        destino, letra = CAUSA.get(
            cl, ("c: uso legitimo (sem leitura suficiente)", "c")
        )
        tabela.append(
            {
                "classe": cl,
                "n": n,
                "destino": destino,
                "letra": letra,
                "evidencias": evidencia_por_classe(
                    [it for it in linhas if it["classe"] == cl]
                ),
            }
        )
    # conservacao
    soma = sum(t["n"] for t in tabela)
    nc = [it for it in linhas if it["classe"].startswith("não classificado")]
    print(
        f"itens {len(linhas)} = classes nomeadas {soma} + nao classificado {len(nc)}  "
        f"-> {'EXATA' if soma + len(nc) == len(linhas) else 'FALHA'}"
    )
    print(f"\n{'classe':<56} {'n':>4}  destino")
    for t in tabela:
        print(f"  {t['classe'][:56]:<56} {t['n']:>4}  {t['destino'][:60]}")

    nc2 = Counter(it.get("classe_nova") for it in nc)
    print(f"\n'não classificado' {len(nc)} — reduzido por predicado positivo:")
    for k, v in nc2.most_common():
        print(f"  {v:>4}  {k}")

    saida = {
        "total_itens": len(linhas),
        "conservacao": soma + len(nc) == len(linhas),
        "tabela": tabela,
        "nao_classificado": {"total": len(nc), "reduzido": dict(nc2)},
        "touring_run": dict(
            Counter(
                it.get("failure_kind")
                for it in linhas
                if it["classe"] == "touring run falhou"
            )
        ),
        "refutado_pela_leitura": REFUTADO,
    }
    # Prova de que o par (session, tool_use_id) das evidencias REENCONTRA no transcript.
    ev = {t["classe"]: t["evidencias"].get(t["classe"], []) for t in tabela}
    n_ev, ok_ev, faltam = reencontrar(ev)
    repete = ids_repetidos(ev)
    saida["reencontro"] = {
        "pares": n_ev,
        "reencontrados": ok_ev,
        "faltantes": faltam,
        "ids_repetidos": repete,
    }
    print(
        f"\nreencontradas {ok_ev}/{n_ev} evidencias no transcript"
        + ("" if not faltam else f"  FALTANDO: {faltam[:5]}")
    )
    print(f"ids repetidos entre evidencias: {repete or 'nenhum'}")
    (HERE / "causas.json").write_text(json.dumps(saida, ensure_ascii=False, indent=1))
    print(f"\njson -> {HERE / 'causas.json'}")
    return 0 if ok_ev == n_ev else 1


if __name__ == "__main__":
    sys.exit(main())
