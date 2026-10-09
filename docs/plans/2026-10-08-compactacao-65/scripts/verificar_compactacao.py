"""The one judge of the compaction plan (08/10/2026): one clause per item, each decided by execution.

Exit 0 only when every clause holds. Each clause returns (ok, evidence); the evidence is what was
measured, never a restatement of the intent. Usage: python3 verificar_compactacao.py [--only C1,C4]
"""

import json
import os
import subprocess
import sys
from pathlib import Path

HOME = Path.home()
BUNDLE = Path(__file__).resolve().parents[1]
TOURING = HOME / "projects/touring"
ANALISE = HOME / "projects/analise"
LOOP_HOOKS = HOME / ".claude/skills/loop-engineering/scripts/hooks"
WARMUPS = ("touring-hook session-start", "rl_warmup", "session_startup_intelligence", "investigate_on_start",
           "scout_perpetuo_reminder", "painel_emanacao", "guidance_injection_hook", "warmup_index",
           "gpu_service_activator")
PLUGIN_MCP = ("plugin:brand-voice:gong", "plugin:marketing:canva", "plugin:design:notion", "plugin:legal:egnyte")


def run(cmd, cwd=None, timeout=900, stdin=None):
    env = {**os.environ, "TOURING_CODE_MODE": "native"}
    p = subprocess.run(cmd, cwd=cwd, input=stdin, capture_output=True, text=True, timeout=timeout, env=env)
    return p.returncode, (p.stdout + p.stderr)


def c65():
    cfg = json.loads((HOME / ".claude/settings.json").read_text())
    env = cfg.get("env") or {}
    stop = json.dumps(cfg["hooks"].get("Stop"))
    checks = {"pct=65": env.get("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE") == "65",
              "sem COMPACT_FOCUS/CONTEXT_THRESHOLD": not {"COMPACT_FOCUS", "CONTEXT_THRESHOLD"} & env.keys(),
              "Stop sem auto_compact": "auto_compact" not in stop,
              "symlink removido": not (HOME / ".claude/hooks/auto_compact.sh").exists(),
              "flag global ausente": not (HOME / ".claude/context_alert").exists()}
    return all(checks.values()), checks


# C1 baseline (Gabriel, 09/10/2026: "troque a base do C1"): measured by THIS judge's own counter
# (_record/_count) over the BASELINE_HOURS before deploy_utc — never a constant from another instrument.
# The 898 it replaces came from medir_conselho.py (three attachment types, raw len) while _count reads
# hook_additional_context with len(json.dumps): a ratio with no common denominator. Frozen below is what
# this counter read on 09/10 for 2026-10-08T01:11:50Z → 2026-10-09T01:11:50Z (1.034,1 chars/call); a
# recount that differs — transcripts pruned or rewritten — fails the clause instead of moving the bar.
BASELINE_HOURS = 24
BASELINE_FROZEN = {"calls": 3133, "chars": 3239902}
PROJECT_TRANSCRIPTS = HOME / ".claude/projects/-home-gabrielgadea-projects-analise"


def _record(line: str, since_utc: str) -> dict | None:
    """The record a line holds, if it can matter and is newer than `since_utc`."""
    if '"tool_use"' not in line and '"hook_additional_context"' not in line:
        return None
    try:
        rec = json.loads(line)
    except json.JSONDecodeError:
        return None
    return rec if str(rec.get("timestamp", "")) >= since_utc else None


def _count(rec: dict) -> tuple[int, int]:
    att = rec.get("attachment") or {}
    if att.get("type") == "hook_additional_context":
        return 0, len(json.dumps(att.get("content"), ensure_ascii=False))
    if rec.get("type") != "assistant":
        return 0, 0
    content = (rec.get("message") or {}).get("content") or []
    return sum(isinstance(c, dict) and c.get("type") == "tool_use" for c in content), 0


def _shift(utc: str, hours: float) -> str:
    from datetime import datetime, timedelta

    t = datetime.strptime(utc[:19], "%Y-%m-%dT%H:%M:%S") + timedelta(hours=hours)
    return t.strftime("%Y-%m-%dT%H:%M:%SZ")


def _advice_between(ini_utc: str, fim_utc: str | None = None, root: Path | None = None) -> tuple[int, int]:
    """(tool calls, advice chars) of the records with ini_utc <= timestamp < fim_utc; fim None = open.

    The end compares to the second (`[:19]`): as strings, "13:00:00.250Z" sorts BEFORE "13:00:00Z"."""
    calls = chars = 0
    for f in (root or PROJECT_TRANSCRIPTS).glob("*.jsonl"):
        for line in f.open(encoding="utf-8", errors="replace"):
            rec = _record(line, ini_utc)
            if rec is None or (fim_utc and str(rec.get("timestamp", ""))[:19] >= fim_utc[:19]):
                continue
            n_calls, n_chars = _count(rec)
            calls, chars = calls + n_calls, chars + n_chars
    return calls, chars


def _advice_since(since_utc: str) -> tuple[int, int]:
    """(tool calls, hook additionalContext chars) in analise transcripts after `since_utc` (ISO, UTC)."""
    return _advice_between(since_utc)


def _c1_amostras(cfg: dict, root: Path | None = None) -> dict:
    """The two samples C1 compares, read by the SAME counter: the BASELINE_HOURS before the deploy, and the
    window after it — closed at `pos_fim_utc` (the next release went live) when the config names one."""
    deploy, fim = cfg["deploy_utc"], cfg.get("pos_fim_utc")
    base_ini = _shift(deploy, -BASELINE_HOURS)
    return {"base": _advice_between(base_ini, deploy, root=root), "base_janela": [base_ini, deploy],
            "pos": _advice_between(deploy, fim, root=root), "pos_janela": [deploy, fim or "aberta"]}


def _c1_checks(amostras: dict, cargo_rc: int) -> tuple[bool, dict]:
    (b_calls, b_chars), (calls, chars) = amostras["base"], amostras["pos"]
    antes = b_chars / b_calls if b_calls else None
    per_call = chars / calls if calls else None
    checks = {"testes de regressão nomeados": cargo_rc == 0,
              "linha de base reproduzível": (b_calls, b_chars) == (BASELINE_FROZEN["calls"], BASELINE_FROZEN["chars"]),
              "amostra ≥ 100 chamadas": calls >= 100,
              "queda ≥ 40%": antes is not None and per_call is not None and per_call <= antes * 0.6}
    return all(checks.values()), {**checks, "calls": calls, "chars_por_chamada": per_call, "antes": antes,
                                  "base_calls": b_calls, "base_janela": amostras["base_janela"],
                                  "pos_janela": amostras["pos_janela"]}


def c1():
    """Item 1 by its own two kinds of evidence: the named Rust regression tests pass, and the advice
    actually injected per tool call in real sessions after the deploy fell ≥ 40% with ≥ 100 calls —
    against the 24 h before the deploy, measured by the same counter (see BASELINE_FROZEN).

    A black-box call to the hook from outside Claude Code was tried and rejected: the suggester is
    stateful (the same fresh session answered exec-gate-advisory once, then nothing), and without
    CLAUDE_PROJECT_DIR the daemon answers with an error that a negative check reads as a pass."""
    cfg_path = BUNDLE / "diagnostics/item1.json"
    if not cfg_path.exists():
        return False, "aguardando a entrega da Kimi (diagnostics/item1.json: cargo_cmd + deploy_utc)"
    cfg = json.loads(cfg_path.read_text())
    rc, out = run(["bash", "-c", cfg["cargo_cmd"]], cwd=TOURING, timeout=1800)
    ok, evidencia = _c1_checks(_c1_amostras(cfg), rc)
    return ok, {**evidencia, "cargo_tail": out.strip().splitlines()[-1:]}


def c2():
    rc1, out1 = run([sys.executable, str(TOURING / "scripts/test_skill_size_guard.py")])
    cons = BUNDLE / "scripts/conservacao.py"
    rc2, out2 = run([sys.executable, str(cons)]) if cons.exists() else (1, "conservacao.py ausente")
    return rc1 == 0 and rc2 == 0, {"guarda": (rc1, out1.strip()[-160:]), "conservação": (rc2, out2.strip()[-160:])}


def c3():
    g = (HOME / ".claude/CLAUDE.md").read_text()
    sec = g[g.find("## Compact Instructions"):][:2500]
    a = (ANALISE / ".claude/CLAUDE.md").read_text()
    checks = {"seção global": "## Compact Instructions" in g,
              "literal": "literalmente" in sec, "palavras do Gabriel": "palavras dele" in sec,
              "arquivos e testes": "arquivo criado ou alterado" in sec and "teste" in sec,
              "formato antigo removido do analise": "FORMAT the compacted summary" not in a}
    return all(checks.values()), checks


def c4():
    rc1, out1 = run([sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", "."], cwd=LOOP_HOOKS)
    rc2, out2 = run([sys.executable, str(BUNDLE / "scripts/mutar_registrador.py")])
    return rc1 == 0 and rc2 == 0, {"suíte": out1.strip().splitlines()[-1:], "mutação": out2.strip().splitlines()[-1:]}


def c5():
    # -rf: a transient failure the judge cannot NAME cannot be attributed (3 failed in 146 s on 09/10 01:15Z, then 178
    # passed in every rerun — with the daemon unreachable and under 2x CPU contention — and the tail kept no name)
    rc, out = run([sys.executable, "-m", "pytest", "-q", "-rf", "-p", "no:cacheprovider", "tests"],
                  cwd=TOURING / "client/hooks")
    src = (TOURING / "client/hooks/check_context.sh").read_text()
    checks = {"suíte client/hooks": rc == 0, "registro, não existência": "settings.local.json" in src,
              "flag por sessão": ".pct" in src and ".delivered" in src}
    failed = [ln for ln in out.splitlines() if ln.startswith(("FAILED", "ERROR"))][:10]
    return all(checks.values()), {**checks, "tail": out.strip().splitlines()[-1:], "falhas": failed}


def _warmups_outside_fresh(path):
    cfg = json.loads(path.read_text())
    hooks = cfg.get("hooks", cfg) if "hooks" in cfg else cfg
    bad = []
    for grp in hooks.get("SessionStart") or []:
        if grp.get("matcher") == "startup|resume|clear|fork":
            continue
        bad += [h["command"] for h in grp.get("hooks") or [] if any(w in h.get("command", "") for w in WARMUPS)]
    return bad


def c6():
    paths = [HOME / ".claude/settings.json", TOURING / "client/claude/hooks.settings.json",
             ANALISE / ".claude/settings.local.json"]
    bad = {p.name: _warmups_outside_fresh(p) for p in paths}
    return not any(bad.values()), bad


def c7():
    g = (HOME / ".claude/CLAUDE.md").read_text()
    a = (ANALISE / "CLAUDE.md").read_text()
    checks = {"global": "`/clear`, não `/compact`" in g, "analise": "tarefa distinta → `/clear`" in a}
    return all(checks.values()), checks


def c8():
    before = json.loads((BUNDLE / "diagnostics/piso-antes.json").read_text())
    after_path = BUNDLE / "diagnostics/piso-depois.json"
    run([sys.executable, str(BUNDLE / "scripts/medir_piso.py"), "--json", str(after_path)])
    after = json.loads(after_path.read_text())
    disabled = set((json.loads((HOME / ".claude.json").read_text())["projects"].get(str(ANALISE)) or {})
                   .get("disabledMcpServers") or [])
    cons = BUNDLE / "scripts/conservacao.py"
    rc_r, _ = run([sys.executable, str(cons), "--rules"]) if cons.exists() else (1, "")
    rc_a, _ = run([sys.executable, str(cons), "--agents"]) if cons.exists() else (1, "")
    checks = {"instruções menores": after["instruction_chars"] < before["instruction_chars"],
              "descrições menores": after["agent_description_chars"] < before["agent_description_chars"],
              "MCP de plugin desabilitados": all(s in disabled for s in PLUGIN_MCP),
              "conservação regras": rc_r == 0, "conservação agentes": rc_a == 0}
    nums = {k: [before[k], after[k]] for k in ("instruction_chars", "agent_description_chars")}
    return all(checks.values()), {**checks, **nums}


def c9():
    rep = (BUNDLE / "diagnostics/sondas-mod-2026-10-08.md").read_text()
    rc, out = run(["claude", "plugin", "validate", str(BUNDLE / "scripts/probe-mod")], timeout=120)
    checks = {"(a) registrada": "(a) O mod enxerga o conselho dos hooks? — NÃO" in rep,
              "(b) registrada": "sobrevive ao `--resume`? — NÃO" in rep,
              "sonda reproduzível": rc == 0 and "session.compact" in out}
    return all(checks.values()), checks


CLAUSES = {"C65": c65, "C1": c1, "C2": c2, "C3": c3, "C4": c4, "C5": c5, "C6": c6, "C7": c7, "C8": c8, "C9": c9}


def main():
    if {"-h", "--help"} & set(sys.argv):  # --help used to fall through and run every clause
        print(__doc__)
        return 0
    only = sys.argv[sys.argv.index("--only") + 1].split(",") if "--only" in sys.argv else list(CLAUSES)
    failed = []
    for name in only:
        try:
            ok, evidence = CLAUSES[name]()
        except Exception as exc:  # a clause that cannot be evaluated is a failed clause, never a pass
            ok, evidence = False, f"{type(exc).__name__}: {exc}"
        print(f"{'PASS' if ok else 'FAIL'} {name}: {json.dumps(evidence, ensure_ascii=False)[:600]}")
        failed += [] if ok else [name]
    print(f"veredito: {'CONVERGIU' if not failed else 'FALTA ' + ','.join(failed)}")
    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())
