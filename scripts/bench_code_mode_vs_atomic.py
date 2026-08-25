#!/usr/bin/env python3
"""Prova controlada: code mode substitui N tool calls com que eficiência?

Três perguntas, três medições — nenhuma estimada onde dava para medir:

  1. EQUIVALÊNCIA  — o resultado dos dois braços é byte-a-byte o mesmo?
                     Sem isso, "mais barato" não significa nada.
  2. CONTEXTO      — quantos bytes entram na janela em cada braço? O custo
                     POR TOOL CALL é medido no transcript REAL da sessão
                     (comando ecoado + resultado + additionalContext dos
                     hooks), não arbitrado.
  3. LATÊNCIA      — tempo de parede de cada braço, medido na execução.

O que NÃO é medido aqui, e por isso não é reivindicado: o round-trip de
inferência do modelo entre uma tool call e a seguinte. Ele existe e favorece
o code mode (N round-trips viram 1), mas medi-lo exigiria instrumentar o
harness. As economias reportadas são, portanto, um PISO.

Uso:
    python3 scripts/bench_code_mode_vs_atomic.py --transcript <jsonl> [--json]
"""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import sys
import time
from pathlib import Path

REPO = Path("/home/gabrielgadea/projects/touring")

# A tarefa real do benchmark: para cada crate, contar arquivos .rs e LOC.
# É exatamente o tipo de varredura que esta sessão fez dezenas de vezes.
CRATES = [
    "touring-cli",
    "touring-foundation",
    "touring-generator",
    "touring-analysis",
    "touring-dispatch",
    "touring-hooks-core",
    "touring-intelligence",
    "touring-server",
]


def measure_transcript_cost(path: Path) -> dict[str, float]:
    """Custo REAL, em bytes de contexto, de uma tool call Bash nesta sessão.

    Soma três parcelas por chamada: o comando ecoado de volta, o resultado, e
    o `additionalContext` que os hooks injetam em CADA PreToolUse — a parcela
    que costuma ser esquecida e que não escala com o valor da resposta.
    """
    cmd_bytes: list[int] = []
    result_bytes: list[int] = []
    hook_bytes: list[int] = []
    by_id: dict[str, int] = {}

    with path.open(encoding="utf-8", errors="ignore") as handle:
        for line in handle:
            try:
                rec = json.loads(line)
            except json.JSONDecodeError:
                continue
            # A medição do hook vem ANTES de qualquer `continue`: os registros
            # que carregam o nudge não têm `message` no formato de mensagem, e
            # a versão anterior os descartava no filtro acima — reportando "0
            # injeções" numa sessão em que o nudge aparece em toda chamada.
            for field in ("hookAdditionalContext", "attachment"):
                value = rec.get(field)
                if not value:
                    continue
                text = value if isinstance(value, str) else json.dumps(value, ensure_ascii=False)
                if "SUGGEST" in text or "MUST " in text:
                    hook_bytes.append(len(text.encode()))
            msg = rec.get("message")
            if not isinstance(msg, dict):
                continue
            for block in msg.get("content") or []:
                if not isinstance(block, dict):
                    continue
                if block.get("type") == "tool_use" and block.get("name") == "Bash":
                    cmd = (block.get("input") or {}).get("command", "")
                    cmd_bytes.append(len(cmd.encode()))
                    by_id[str(block.get("id"))] = len(cmd.encode())
                elif block.get("type") == "tool_result":
                    content = block.get("content")
                    text = (
                        content
                        if isinstance(content, str)
                        else json.dumps(content, ensure_ascii=False)
                    )
                    if str(block.get("tool_use_id")) in by_id:
                        result_bytes.append(len(text.encode()))

    def mean(xs: list[int]) -> float:
        return statistics.mean(xs) if xs else 0.0

    return {
        "calls_measured": len(cmd_bytes),
        "mean_cmd_bytes": mean(cmd_bytes),
        "mean_result_bytes": mean(result_bytes),
        "median_result_bytes": statistics.median(result_bytes) if result_bytes else 0.0,
        "mean_hook_bytes": mean(hook_bytes),
        "hook_injections": len(hook_bytes),
    }


def run(cmd: str) -> tuple[str, float]:
    """Executa no shell e devolve (stdout, segundos)."""
    start = time.perf_counter()
    out = subprocess.run(
        ["bash", "-c", cmd], capture_output=True, text=True, cwd=REPO, timeout=300
    )
    return out.stdout, time.perf_counter() - start


def arm_atomic() -> tuple[str, float, int, int]:
    """Braço A: uma chamada por crate — o padrão atômico."""
    parts: list[str] = []
    total = 0.0
    for crate in CRATES:
        cmd = (
            f"n=$(find crates/{crate}/src -name '*.rs' | wc -l); "
            f"l=$(find crates/{crate}/src -name '*.rs' -exec cat {{}} + | wc -l); "
            f'echo "{crate} {{files:$n, loc:$l}}"'
        )
        out, secs = run(cmd)
        parts.append(out)
        total += secs
    payload = "".join(parts)
    return payload, total, len(CRATES), len(payload.encode())


def arm_code_mode() -> tuple[str, float, int, int]:
    """Braço B: uma varredura só, no sandbox — o code mode."""
    inner = (
        "for c in " + " ".join(CRATES) + "; do "
        "n=$(find crates/$c/src -name '*.rs' | wc -l); "
        "l=$(find crates/$c/src -name '*.rs' -exec cat {} + | wc -l); "
        'echo "$c {files:$n, loc:$l}"; done'
    )
    escaped = inner.replace("'", r"'\''")
    out, secs = run(f"touring run --lang bash --code '{escaped}' 2>/dev/null")
    try:
        payload = json.loads(out).get("stdout", "")
    except json.JSONDecodeError:
        payload = out
    # O que entra no contexto é o envelope inteiro, não só o stdout.
    return payload, secs, 1, len(out.encode())


def main() -> int:
    """Roda os dois braços, compara e imprime o agregado."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--transcript", type=Path, required=True)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    cost = measure_transcript_cost(args.transcript)
    a_out, a_secs, a_calls, a_bytes = arm_atomic()
    b_out, b_secs, b_calls, b_bytes = arm_code_mode()

    equivalent = a_out.strip() == b_out.strip()

    # Contexto total: payload + (comando + hook) × número de chamadas.
    per_call_overhead = cost["mean_cmd_bytes"] + cost["mean_hook_bytes"]
    a_context = a_bytes + per_call_overhead * a_calls
    b_context = b_bytes + per_call_overhead * b_calls

    report = {
        "task": f"contar arquivos .rs e LOC em {len(CRATES)} crates",
        "equivalent_output": equivalent,
        "atomic": {
            "tool_calls": a_calls,
            "payload_bytes": a_bytes,
            "context_bytes": round(a_context),
            "seconds": round(a_secs, 2),
        },
        "code_mode": {
            "tool_calls": b_calls,
            "payload_bytes": b_bytes,
            "context_bytes": round(b_context),
            "seconds": round(b_secs, 2),
        },
        "measured_per_call_overhead_bytes": round(per_call_overhead),
        "transcript_sample": cost,
        "context_reduction": round(1 - b_context / a_context, 3) if a_context else 0,
        "roundtrip_reduction": round(1 - b_calls / a_calls, 3) if a_calls else 0,
    }

    if args.json:
        json.dump(report, sys.stdout, indent=2, ensure_ascii=False)
        print()
        return 0

    print("=" * 64)
    print("CODE MODE vs TOOL CALLS ATÔMICAS — experimento controlado")
    print("=" * 64)
    print(f"  tarefa: {report['task']}")
    print(f"  saída idêntica nos dois braços: {'SIM' if equivalent else 'NÃO'}")
    print()
    print(f"  custo POR TOOL CALL medido no transcript real desta sessão:")
    print(f"    chamadas Bash medidas      {cost['calls_measured']}")
    print(f"    comando (média)            {cost['mean_cmd_bytes']:.0f} B")
    print(f"    resultado (mediana)        {cost['median_result_bytes']:.0f} B")
    print(f"    hook additionalContext     {cost['mean_hook_bytes']:.0f} B"
          f"  ({cost['hook_injections']} injeções)")
    print(f"    → overhead fixo por call   {per_call_overhead:.0f} B")
    print()
    print(f"  {'':22} {'ATÔMICO':>12} {'CODE MODE':>12}")
    print(f"  {'tool calls':22} {a_calls:>12} {b_calls:>12}")
    print(f"  {'payload (B)':22} {a_bytes:>12} {b_bytes:>12}")
    print(f"  {'contexto total (B)':22} {round(a_context):>12} {round(b_context):>12}")
    print(f"  {'latência (s)':22} {a_secs:>12.2f} {b_secs:>12.2f}")
    print()
    print(f"  redução de contexto     {report['context_reduction']:.1%}")
    print(f"  redução de round-trips  {report['roundtrip_reduction']:.1%}")
    print()
    print("  NÃO reivindicado (não medido): o round-trip de inferência entre")
    print("  chamadas. Ele favorece o code mode, então isto é um PISO.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
