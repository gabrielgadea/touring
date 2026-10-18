"""PROBE — o retrato de 606 reproduz?

Antes de construir o entregavel, a pergunta de risco: a tabela que a coordenadora mediu em
08/10 ~17:20 BRT (= 20:20 UTC) reproduz sobre o MESMO universo hoje? Se nao reproduzir, o trabalho
arquitetural e pointless e o brief manda PARAR.

Reproduz a logica de inventario_erros.Inventory._one EXATAMENTE (filtro de arquivo por mtime, `names`
so marca Bash, residue_class so quando nao ha diagnostico de linguagem), acrescentando SO o corte
superior de timestamp que o original nao tem.

Uso: python3 probe_606.py
"""

import json
import sys
from collections import Counter
from datetime import datetime
from pathlib import Path

SCRIPTS = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"
sys.path.insert(0, str(SCRIPTS))
from inventario_erros import (  # noqa: E402
    ROOTS,
    SINCE,
    _text,
    parse_errors,
    residue_class,
    unwrap_run,
)  # noqa: E402

ATE = "2026-10-08T20:20:00Z"
ESPERADO = {
    "touring run falhou": 220,
    "arquivo/caminho inexistente": 55,
    "permissão do Claude Code negou": 53,
    "is_error com saída de sucesso (outro comando do pipeline falhou)": 34,
    "pytest falhou (sumário)": 22,
    "sintaxe do shell": 15,
    "git": 13,
    "timeout": 11,
    "saída vazia com exit≠0 (grep/test sem casamento)": 8,
    "permissão/sandbox": 6,
    "entrada de ferramenta inválida": 6,
    "traceback cortado (sem a linha da exceção)": 5,
    "comando inexistente": 4,
    "CLI touring/uv/outros (error:)": 4,
    "cargo falhou (sem diagnóstico no recorte)": 2,
}


def main():
    since_ts = datetime.fromisoformat(SINCE).timestamp()
    res, total_bash, transcripts = Counter(), 0, 0
    for root in ROOTS:
        for f in root.rglob("*.jsonl"):
            if f.stat().st_mtime < since_ts:
                continue
            transcripts += 1
            names = {}
            for line in f.open(encoding="utf-8", errors="replace"):
                if '"tool_use"' not in line and '"tool_result"' not in line:
                    continue
                rec = json.loads(line)
                if (
                    str(rec.get("timestamp", "")) < SINCE
                    or str(rec.get("timestamp", "")) > ATE
                ):
                    continue
                for c in (rec.get("message") or {}).get("content") or []:
                    if isinstance(c, dict):
                        if c.get("type") == "tool_use":
                            names[c.get("id")] = c.get("name")
                        elif (
                            c.get("type") == "tool_result"
                            and names.get(c.get("tool_use_id")) == "Bash"
                        ):
                            total_bash += 1
                            if c.get("is_error") and not parse_errors(
                                unwrap_run(_text(c))
                            ):
                                res[residue_class(_text(c))] += 1

    nomeado = {k: v for k, v in res.items() if not k.startswith("não classificado")}
    nc = sum(v for k, v in res.items() if k.startswith("não classificado"))
    fora_de_gate = sum(nomeado.values()) - nomeado.get("hook/guard negou", 0)
    print(
        f"transcripts varridos: {transcripts} · resultados de Bash (com corte): {total_bash}"
    )
    print(
        f"resíduo TOTAL: {sum(res.values())}  · 'hook/guard negou': {nomeado.get('hook/guard negou', 0)}"
    )
    print(f"==> RESÍDUO FORA DE GATE: {fora_de_gate}  (a tabela diz 606)")
    print(f"    não classificado: {nc}  (a tabela diz 148)\n")
    print(f"  {'classe':<62} {'medido':>7} {'tabela':>7} {'dif':>5}")
    for k, v in sorted(nomeado.items(), key=lambda kv: -kv[1]):
        if k == "hook/guard negou":
            continue
        esp = ESPERADO.get(k)
        print(
            f"  {k[:62]:<62} {v:>7} {esp if esp is not None else '-':>7} "
            f"{(v - esp) if esp is not None else '-':>5}"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
