#!/usr/bin/env python3
"""Paired, interleaved per-id timing of TWO binaries under the SAME judge run (protocol C', pre-registered 17/10/2026).

Why: the machine is shared; between two rounds a minute apart the whole latency distribution rescales 0.44–2.16× (CPU
contention, measured on the P0B A/B), so comparing a round of A with a round of B measures the machine. Here the judge's
`main()` runs ONCE; its `rodar_motor` is wrapped so that, for every id, the motor of A (reference) and B (candidate) is
called in the balanced order ABBABAAB (4 calls each; mean position 4.5 for both) on the identical input — both binaries
see the same machine state within milliseconds. Nothing in the judge changes; the wrapper only multiplies the call.

A/A calibration = the same binary passed as A and B. Analysis and the decision rules: analisa_pareado.py.

PHASE D (PREREGISTRO-FASE-D-2026-10-18.md, the default): every motor call goes through the launcher conta_custo.py,
which measures the retired user-space instructions and the tree CPU time from outside. Per id, next to the wall times
(`A`, `B`) and the verdicts, the file stores `instr_A/B` and `cpu_A/B` (one value per call, null = the launcher left no
reading: absence is never zero). Order, judge and the rest of the output are the C' ones. `--sem-custo` (first argument)
runs the C' protocol without the launcher.

Usage: python3 -I tempos_pareados.py [--sem-custo] <juiz.py> <bin_A_ref> <bin_B_novo> <saida.json> [args do juiz ...]
       (the judge args must NOT include --touring: it is injected as A)
"""
import hashlib
import importlib.util
import json
import sys
import tempfile
from pathlib import Path

ORDEM = "ABBABAAB"
LANCADOR = Path(__file__).resolve().with_name("conta_custo.py")


def sha16(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()[:16]


def le_custo(arq: Path) -> dict:
    """The launcher's reading of one call; a missing or broken file is ABSENT (nulls), never zero."""
    try:
        d = json.loads(arq.read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        return {"instr": None, "cpu_ms": None, "erro": f"sem leitura do lancador: {type(e).__name__}"}
    finally:
        arq.unlink(missing_ok=True)
    return d if isinstance(d, dict) else {"instr": None, "cpu_ms": None, "erro": "JSON do lancador nao e objeto"}


def main() -> int:
    argv = sys.argv[1:]
    com_custo = "--sem-custo" not in argv[:1]
    if not com_custo:
        argv = argv[1:]
    juiz_py, bin_a, bin_b, saida = (Path(argv[0]).resolve(), Path(argv[1]).resolve(),
                                    Path(argv[2]).resolve(), Path(argv[3]))
    args = argv[4:]
    if "--touring" in args:
        sys.exit("tempos_pareados: não passe --touring; o binário A é injetado")
    spec = importlib.util.spec_from_file_location("juiz_pareado", juiz_py)
    juiz = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(juiz)
    original = juiz.rodar_motor
    por_id: dict[str, dict] = {}
    pasta = tempfile.TemporaryDirectory(prefix="conta-custo-")
    chamada = [0]

    def rodar_pareado(touring, entrada, corpus):
        # `touring` is the judge's command LIST ([binary] — motor args travel in its ARGS_MOTOR global): swap only
        # the binary, keep anything after it
        reg = {"A": [], "B": [], "verdict_A": [], "verdict_B": []}
        if com_custo:
            reg |= {"instr_A": [], "instr_B": [], "cpu_A": [], "cpu_B": []}
        ultima = None
        for lado in ORDEM:
            binario = str(bin_a if lado == "A" else bin_b)
            if com_custo:  # the launcher is the command prefix: forks the motor, counts it and its descendants
                chamada[0] += 1
                arq = Path(pasta.name) / f"{chamada[0]}.json"
                prefixo = [sys.executable, "-I", str(LANCADOR), "--out", str(arq), "--", binario]
            else:
                prefixo = [binario]
            out = original([*prefixo, *list(touring)[1:]], entrada, corpus)
            reg[lado].append(out.get("elapsed_ms"))
            reg[f"verdict_{lado}"].append(out.get("verdict"))
            if com_custo:
                custo = le_custo(arq)
                reg[f"instr_{lado}"].append(custo.get("instr"))
                reg[f"cpu_{lado}"].append(custo.get("cpu_ms"))
            if lado == "B":
                ultima = out
        por_id[entrada["id"]] = reg
        return ultima

    juiz.rodar_motor = rodar_pareado
    sys.argv = [str(juiz_py), "--touring", str(bin_a), *args]
    try:
        rc = juiz.main()
    finally:
        pasta.cleanup()
    meta = {"protocolo": "D" if com_custo else "C'"} | ({"lancador": sha16(LANCADOR)} if com_custo else {})
    saida.write_text(json.dumps({**meta, "ordem": ORDEM, "A": str(bin_a), "B": str(bin_b),
                                 "sha_A": sha16(bin_a), "sha_B": sha16(bin_b), "juiz": str(juiz_py), "args": args,
                                 "por_id": por_id}, ensure_ascii=False, sort_keys=True), encoding="utf-8")
    print(f"[pareado] {len(por_id)} ids × {len(ORDEM)} chamadas em {saida} (rc do juiz {rc})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
