#!/usr/bin/env python3
"""P2.4 — As 5 classes têm efeito mensurável (simulação por braços sobre corpus real).

A pergunta (RETOMAR-AQUI §P2.4, eixo *varia*): o roteamento baseado na
classificação T0-T4 MUDA os números — ou as classes são decoração? Simulamos
3 braços sobre as MESMAS 1.312 tool calls reais (20 prompts, 8 sessões) com
bytes de payload REAIS dos transcripts:

| braço | regra | nudges |
|---|---|---|
| native | tudo passa | 0 |
| both (baseline) | G1: 3ª advisory, 4ª+ da mesma classe/180s negada → 1 programa por rajada | advisory + média medida do cli_suggester |
| code | 1ª chamada de classe colapsada nega → 1 programa por turno cobrindo as colapsáveis | 0 (o deny É a rota; a seção é 1×/sessão) |

Bytes de payload do programa fundido NÃO estão no transcript — reportamos dois
bounds REAIS: **inferior** = digest no teto declarado da prática M5 (~200 tok ≈
800 B por programa, `rules/touring-4-pillars.md` D5); **superior** = a soma dos
resultados reais das chamadas substituídas (zero compressão). Se até o bound
superior mostra efeito, a prova não depende de nenhuma estimativa de compressão.

Média de nudge por chamada no braço `both`: 940 B — medida na sessão de origem
(1.196 injeções ≈ 1,12 MB, memória `nudge-entrega-o-programa`).

Saída: data/effect_p24.json + agregado em stdout.
"""

from __future__ import annotations

import json
import os
import sys
from collections import defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from calibration_p23 import classify_bash  # noqa: E402 — mesmo bundle

TRANSCRIPTS = os.path.expanduser("~/.claude/projects/-home-gabrielgadea-projects-touring/*.jsonl")
OUT = "docs/plans/2026-08-25-code-mode-afordancia/data/effect_p24.json"

COLLAPSED = {"grep": ("grep", "rg"), "cat": ("cat", "head", "tail", "less"), "find": ("find", "fd")}
NUDGE_BYTES_BOTH = 940      # média medida 25/08 (1.196 injeções ≈ 1,12 MB)
ADVISORY_BYTES = 1100       # advisory G1 da 3ª chamada (medido na prática)
DIGEST_BYTES = 800          # teto M5 do agregado (~200 tok)
WINDOW_S = 180              # janela do G1
G1_THRESHOLD = 3            # 3ª advisory, 4ª+ nega


def scan_class_of(cmd: str) -> str | None:
    """Espelho do Rust (com o fix P2.3 do redirect)."""
    toks = cmd.split()
    i = 0
    while i < len(toks) and ("=" in toks[i] and (toks[i][0].isalpha() or toks[i][0] == "_")):
        i += 1
    if i >= len(toks):
        return None
    first = os.path.basename(toks[i])
    for cls, bins in COLLAPSED.items():
        if first in bins:
            if cls == "cat":
                j = i + 1
                while j < len(toks):
                    t = toks[j]
                    if t in ("-n", "-c"):
                        j += 2
                        continue
                    if t.startswith("-"):
                        j += 1
                        continue
                    if t.startswith(">"):
                        return None  # escrita, não inspeção (fix P2.3)
                    break
            return cls
    if first == "ls":
        return "ls"
    if first == "wc":
        return "wc"
    if first == "sed" and "-n" in toks:
        return "sed-n"
    return None


def extract_calls_with_results() -> list[dict]:
    """Cada Bash tool call com seu resultado real, por sessão, na ordem."""
    import glob
    calls = []
    files = sorted(glob.glob(TRANSCRIPTS), key=os.path.getmtime)
    for f in files[-10:]:
        sid = os.path.basename(f).replace(".jsonl", "")[:8]
        pending: dict[str, dict] = {}
        order = []
        for line in open(f, encoding="utf-8", errors="ignore"):
            try:
                d = json.loads(line)
            except json.JSONDecodeError:
                continue
            if d.get("type") == "assistant":
                for c in d.get("message", {}).get("content", []):
                    if isinstance(c, dict) and c.get("type") == "tool_use" and c.get("name") == "Bash":
                        pending[c.get("id")] = {"cmd": str(c.get("input", {}).get("command", "")), "session": sid, "ts": d.get("timestamp", "")}
                        order.append(c.get("id"))
            elif d.get("type") == "user":
                c = d.get("message", {}).get("content")
                if isinstance(c, list):
                    for item in c:
                        if isinstance(item, dict) and item.get("type") == "tool_result":
                            tid = item.get("tool_use_id")
                            if tid in pending:
                                content = item.get("content")
                                if isinstance(content, list):
                                    text = "".join(str(x.get("text", "")) for x in content if isinstance(x, dict))
                                else:
                                    text = str(content or "")
                                pending[tid]["result_bytes"] = len(text.encode())
        for tid in order:
            if tid in pending and "result_bytes" in pending[tid]:
                calls.append(pending[tid])
    return calls


def main() -> int:
    calls = extract_calls_with_results()
    for c in calls:
        c["class"] = scan_class_of(c["cmd"])
        c["cmd_bytes"] = len(c["cmd"].encode())

    arms: dict[str, dict] = {}
    n = len(calls)

    # ── native: tudo passa ────────────────────────────────────────────────────
    arms["native"] = {
        "chamadas": n,
        "cmd_bytes": sum(c["cmd_bytes"] for c in calls),
        "payload_bytes": sum(c["result_bytes"] for c in calls),
        "nudge_bytes": 0,
        "programas": 0,
    }

    # ── both: G1 por (sessão, classe), janela 180s (aprox.: sequência por sessão) ──
    both = {"chamadas": 0, "cmd_bytes": 0, "payload_bytes": 0, "nudge_bytes": 0, "programas": 0}
    by_session = defaultdict(list)
    for c in calls:
        by_session[c["session"]].append(c)
    for sid, sc in by_session.items():
        burst = defaultdict(list)
        for c in sc:
            cls = c["class"]
            if cls in ("grep", "cat", "find"):
                burst[cls].append(c)
                pos = len(burst[cls])
                if pos < G1_THRESHOLD:
                    both["chamadas"] += 1
                    both["cmd_bytes"] += c["cmd_bytes"]
                    both["payload_bytes"] += c["result_bytes"]
                elif pos == G1_THRESHOLD:
                    both["chamadas"] += 1
                    both["cmd_bytes"] += c["cmd_bytes"]
                    both["payload_bytes"] += c["result_bytes"]
                    both["nudge_bytes"] += ADVISORY_BYTES
                else:
                    # negada — acumulada na rajada; o programa é cobrado ao fim
                    pass
            else:
                both["chamadas"] += 1
                both["cmd_bytes"] += c["cmd_bytes"]
                both["payload_bytes"] += c["result_bytes"]
            both["nudge_bytes"] += NUDGE_BYTES_BOTH
        for cls, cs in burst.items():
            negadas = cs[G1_THRESHOLD:]
            if negadas:
                both["programas"] += 1
                both["chamadas"] += 1
                both["cmd_bytes"] += sum(x["cmd_bytes"] for x in negadas) + 40  # `touring run --code '<...>'`
                both["payload_bytes"] += min(DIGEST_BYTES, sum(x["result_bytes"] for x in negadas))
    arms["both"] = both

    # ── code: 1ª colapsada nega; 1 programa por (sessão, turno aprox. rajada contígua) ──
    code = {"chamadas": 0, "cmd_bytes": 0, "payload_bytes": 0, "nudge_bytes": 0, "programas": 0}
    payload_subst_lower = 0
    payload_subst_upper = 0
    for sid, sc in by_session.items():
        run = 0
        buffer: list[dict] = []

        def flush():
            nonlocal run
            if buffer:
                code["programas"] += 1
                code["chamadas"] += 1
                code["cmd_bytes"] += sum(x["cmd_bytes"] for x in buffer) + 40
                upper = sum(x["result_bytes"] for x in buffer)
                code["payload_bytes"] += min(DIGEST_BYTES, upper)
        for c in sc:
            if c["class"] in ("grep", "cat", "find"):
                buffer.append(c)
            else:
                flush()
                buffer = []
                code["chamadas"] += 1
                code["cmd_bytes"] += c["cmd_bytes"]
                code["payload_bytes"] += c["result_bytes"]
        flush()
        buffer = []
    # bounds para o payload substituído (só informativo — já aplicado min() acima)
    for c in calls:
        if c["class"] in ("grep", "cat", "find"):
            payload_subst_upper += c["result_bytes"]
            payload_subst_lower += min(DIGEST_BYTES, c["result_bytes"])
    # a seção SDK de sessão (P1.2): 1× por sessão, medida em 2.324 B
    code["nudge_bytes"] += 2324 * len(by_session)
    arms["code"] = code

    # ── cenário inspeção-pesada: os 25% de sessões com mais chamadas colapsáveis ──
    heavy_sids = sorted(by_session, key=lambda s: -sum(1 for c in by_session[s] if c["class"] in ("grep", "cat", "find")))[: max(1, len(by_session) // 4)]
    heavy = [c for s in heavy_sids for c in by_session[s]]
    heavy_collapsed = [c for c in heavy if c["class"] in ("grep", "cat", "find")]
    heavy_payload_collapsed = sum(c["result_bytes"] for c in heavy_collapsed)
    heavy_payload_total = sum(c["result_bytes"] for c in heavy)
    scenario_heavy = {
        "sessões": heavy_sids,
        "chamadas": len(heavy),
        "colapsáveis": len(heavy_collapsed),
        "payload_colapsável": heavy_payload_collapsed,
        "payload_total": heavy_payload_total,
        "economia_payload_code": f"{100 * (heavy_payload_collapsed - min(heavy_payload_collapsed, DIGEST_BYTES * max(1, len(heavy_collapsed) // 3))) // max(1, heavy_payload_total)}% (digest≈800B por 3 chamadas)",
    }

    native = arms["native"]
    report = {
        "corpus": {"chamadas_bash": n, "sessões": len(by_session)},
        "arms": arms,
        "efeito": {},
        "bounds_payload_substituído": {"inferior_digest": payload_subst_lower, "superior_sem_compressão": payload_subst_upper},
        "cenário_inspeção_pesada": scenario_heavy,
    }
    for arm in ("both", "code"):
        a = arms[arm]
        tot_a = a["cmd_bytes"] + a["payload_bytes"] + a["nudge_bytes"]
        tot_n = native["cmd_bytes"] + native["payload_bytes"] + native["nudge_bytes"]
        report["efeito"][arm] = {
            "chamadas": f"{n} → {a['chamadas']} ({100*(n-a['chamadas'])//n}%)",
            "bytes_totais": f"{tot_n} → {tot_a} ({100*(tot_n-tot_a)//tot_n}%)",
            "programas_fundidos": a["programas"],
        }

    with open(OUT, "w") as fh:
        json.dump(report, fh, indent=1, ensure_ascii=False)

    print(f"corpus: {n} chamadas Bash reais, {len(by_session)} sessões")
    for arm, a in arms.items():
        tot = a["cmd_bytes"] + a["payload_bytes"] + a["nudge_bytes"]
        print(f"{arm:>6}: {a['chamadas']:>4} chamadas | cmd {a['cmd_bytes']:>8,} B | payload {a['payload_bytes']:>9,} B | nudge {a['nudge_bytes']:>9,} B | total {tot:>9,} B | {a['programas']} programas")
    print("efeito:")
    for arm, e in report["efeito"].items():
        print(f"  {arm}: chamadas {e['chamadas']} | bytes {e['bytes_totais']}")
    print(f"  bound payload substituído: {payload_subst_lower:,} ≤ x ≤ {payload_subst_upper:,}")
    print("cenário inspeção-pesada:", json.dumps(scenario_heavy, ensure_ascii=False))
    print(f"→ {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
