"""T4 — prompt intent: Touring's keyword classifier (prompt-enhance hook) vs Laya zero-shot.

Gold set = the 25 golden prompts from prompt_enhance_tests.rs (the keyword classifier passes
them by construction) + 12 texts the UserPromptSubmit hook really receives in a session:
automated notifications and short replies (gold: general) and two real research prompts.
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import time
from pathlib import Path

import laya

HOOK = os.path.expanduser("~/.claude/hooks/touring-hook")
OUT = Path(__file__).with_name("results_intent.json")

GOLDEN = [
    ("Create a FastAPI endpoint for user registration", "code"),
    ("Implement a binary search function in Python", "code"),
    ("Crie um módulo de autenticação com JWT", "code"),
    ("Write a React component for a data table", "code"),
    ("Build a CLI tool that parses CSV files", "code"),
    ("Fix the KeyError on line 42 of parser.py", "debug"),
    ("This function crashes with a TypeError when input is None", "debug"),
    ("Corrija o erro de conexão no módulo de banco de dados", "debug"),
    ("Track down the memory leak in this service", "debug"),
    ("Debug why the API returns 500 on POST requests", "debug"),
    ("Refactor the user service to follow SOLID principles", "refactor"),
    ("Simplify the nested if-else chain in process_order", "refactor"),
    ("Limpe e otimize a classe DatabaseManager", "refactor"),
    ("Write unit tests for the payment processor module", "test"),
    ("Add pytest fixtures for the database connection", "test"),
    ("Crie testes de cobertura para o serviço de autenticação", "test"),
    ("Explain how the event loop works in asyncio", "analysis"),
    ("Review the architecture of the relay module", "analysis"),
    ("Analise por que a latência aumentou após o deploy", "analysis"),
    ("Suggest alternative approaches for caching user sessions", "creative"),
    ("Proponha uma estratégia de migração para microserviços", "creative"),
    ("Plan the migration from monolith to microservices", "plan"),
    ("Planeje as etapas para implementar CI/CD completo", "plan"),
    ("Hello", "general"),
    ("What time is it?", "general"),
]
REAL = [
    ("<task-notification> Agent \"Digerir cookbooks do Jev\" finished. O digest está gravado. Ele cobre os 18 cookbooks, não 19. Na fonte há uma inconsistência no autoformat.", "general"),
    ("<task-notification> Agent \"Ecossistema e recepção do Jev\" finished. O relatório está gravado; os arquivos brutos ficam em scratchpad/jev/mine; nada foi alterado no repositório.", "general"),
    ("<task-notification> Background command \"Roda o ADW strategy-loop (recall, diagnóstico, explore) no bundle da pesquisa\" completed (exit code 0)", "general"),
    ("Stop hook feedback: Loop Engineering: not converged (3/30). Unmet clauses: ['dag_done']. Next action: execute pending subtask(s): R3", "general"),
    ("[SYSTEM NOTIFICATION - NOT USER INPUT] This is an automated background-task event, NOT a message from the user.", "general"),
    ("Tool loaded.", "general"),
    ("Publique", "general"),
    ("ok, obrigado", "general"),
    ("faça uma exploração exaustiva, uma pesquisa profunda e uma análise completa e detalhada a respeito do Jev, as melhores práticas no context7 e como podemos utilizar", "analysis"),
    ("faça mais uma rodada de exploração exaustiva, pesquisa profunda e análise completa e detalhada do Jev, do touring e do laya-mlx e complemente a estratégia", "analysis"),
    ("corrija o defeito do hook OUTER que anuncia artefatos que não disparou", "debug"),
    ("escreva testes para o roteador do factory", "test"),
]
INTENTS = {
    "debug": "Find or fix a bug, error, crash or failure",
    "code": "Write or implement new code or functionality",
    "test": "Write or run tests",
    "refactor": "Restructure or clean up existing code without changing behavior",
    "analysis": "Explain, review, research or analyze something",
    "plan": "Plan steps, phases or a roadmap",
    "creative": "Propose ideas, alternatives or strategies",
    "general": "Anything else: greetings, short replies, automated system notifications",
}
MODE_RE = re.compile(r"PROMPT ENHANCEMENT -- (\w+) MODE")


def keyword_mode(text: str) -> str | None:
    """Ask the real prompt-enhance hook and read the mode it injects."""
    payload = json.dumps({"prompt": text, "hook_event_name": "UserPromptSubmit"})
    proc = subprocess.run([HOOK, "prompt-enhance"], input=payload, capture_output=True, text=True, timeout=30)
    m = MODE_RE.search(proc.stdout + proc.stderr)
    return m.group(1).lower() if m else None


def main() -> None:
    router = laya.Router(device="cuda", max_loaded=2, preload=True)
    q = {"intent": {"type": "choice", "instructions": "What is the person asking for?", "criteria": INTENTS}}
    rows = []
    for text, gold in GOLDEN + REAL:
        kw = keyword_mode(text)
        t0 = time.perf_counter()
        res = router.predict({"message": text}, q)
        ms = (time.perf_counter() - t0) * 1000
        a = res["answers"]["intent"]
        rows.append({"text": text[:90], "gold": gold, "keyword": kw, "laya": a["choice"],
                     "conf": a["confidence"], "route": res["routing"]["model"], "ms": round(ms, 1),
                     "set": "golden" if (text, gold) in GOLDEN else "real"})
    summary = {}
    for s in ("golden", "real"):
        sub = [r for r in rows if r["set"] == s]
        summary[s] = {"n": len(sub),
                      "keyword_acc": round(sum(r["keyword"] == r["gold"] for r in sub) / len(sub), 3),
                      "laya_acc": round(sum(r["laya"] == r["gold"] for r in sub) / len(sub), 3),
                      "laya_acc_conf_ge_0.8": [sum(r["laya"] == r["gold"] for r in sub if r["conf"] >= 0.8),
                                               sum(1 for r in sub if r["conf"] >= 0.8)]}
    OUT.write_text(json.dumps({"summary": summary, "rows": rows}, ensure_ascii=False, indent=1))
    print(json.dumps(summary, ensure_ascii=False))
    for r in rows:
        mark = lambda x: "✓" if x == r["gold"] else "✗"
        print(f"{r['set']:6} gold={r['gold']:9} kw={str(r['keyword']):9}{mark(r['keyword'])} laya={r['laya']:9}{mark(r['laya'])} "
              f"c={r['conf']:.2f} {r['route']:12} | {r['text'][:60]}")


if __name__ == "__main__":
    main()
