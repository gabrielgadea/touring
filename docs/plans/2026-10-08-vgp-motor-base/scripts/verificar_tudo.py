"""Verify everything this session touched — every verdict is an exit code or a measured predicate, never a reading.

Sequential on purpose (a parallel run fabricates contention: the C5 transient failure). Full output of each check
goes to OUT/<name>.log; stdout carries only the aggregate. Usage: python3 verificar_tudo.py
"""

import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

H = Path.home()
T, A = H / "projects/touring", H / "projects/analise"
VGP = T / "docs/plans/2026-10-08-vgp-motor-base"
CMP = T / "docs/plans/2026-10-08-compactacao-65"
DRF = T / "docs/plans/2026-10-08-drift-reconciliacao"
OUT = Path("/tmp") / "verificar_tudo"  # logs volatile; copy what matters into diagnostics/
OUT.mkdir(exist_ok=True)
PY_A = str(A / ".venv/bin/python3")
CLEAN = {k: v for k, v in os.environ.items() if k not in ("TOURING_DAEMON_SOCKET", "TOURING_DAEMON_SOCK")}
KEYS = [f"licao:{s}:2026-10-08" for s in (
    "backup-dentro-do-escopo-revela-o-drift-que-o-escopo-esconde",
    "espelho-client-corre-de-claude-editar-o-espelho-e-dar-apply-desfaz-a-correcao",
    "git-grep-no-sandbox-do-touring-run-morre-no-ignore-global-e-o-laco-imprime-zero",
    "update-touring-de-sessao-per-project-ciclou-o-daemon-do-projeto-pelo-env-herdado",
    "home-falso-nao-isola-script-cujo-alvo-nao-deriva-do-home",
    "set-var-em-teste-rust-paralelo-desliga-a-feature-nos-testes-vizinhos",
    "corte-do-instrumento-fabrica-o-defeito-que-a-delegada-reporta",
    "flag-de-selftest-nao-tratada-no-main-reescreve-o-resultado",
    "no-pytest-cov-com-addopts-cov-rejeita-a-chamada-use-no-cov",
    "chave-de-licao-sem-quando-deriva-a-data-da-execucao",
    "regressao-se-mede-nos-importadores-nao-no-diretorio-do-modulo",
    "lstrip-remove-conjunto-de-caracteres-e-come-o-ponto-do-diretorio",
    "harness-de-aceite-herda-o-env-do-shell-e-fabrica-vermelho-deterministico",
    "teste-de-script-de-deploy-nunca-roda-o-dispatch")] + [
    "diagnostico:g7-promete-reset-por-touring-run-citando-o-arquivo-e-nao-reseta:2026-10-08"]
RUFF = [str(p) for p in [*sorted((VGP / "scripts").glob("*.py")), VGP / "diagnostics/medir_vgp.py",
                         *sorted(DRF.glob("*.py")), CMP / "scripts/verificar_compactacao.py",
                         CMP / "scripts/conservacao.py", CMP / "diagnostics/gravar_licoes_release.py",
                         A / "scripts/memoria/grafo_modelo.py", T / "scripts/test_drift_coverage.py"]]
WRONG = re.compile(r"touring hook-memory-(recall|store)|crates/touring-resource-monitor/src")
AGENT_FILES = [p for base in (T / "client", H / ".claude") for p in (
    base / "agents/touring-scouter.md", base / "agents/touring-architect.md", base / "agents/touring-engineer.md",
    base / "skills/Touring/references/touring-cli-hooks.md")]


def sh(cmd, cwd=None, timeout=900, env=None):
    t0 = time.time()
    try:
        p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout, env=env or os.environ)
        return p.returncode, p.stdout + p.stderr, time.time() - t0
    except subprocess.TimeoutExpired as e:
        out = "".join(s.decode(errors="replace") if isinstance(s, bytes) else s for s in (e.stdout, e.stderr) if s)
        return None, out + f"\n[TIMEOUT {timeout}s]", time.time() - t0


def last(out, pat=None):
    lines = [ln for ln in out.strip().splitlines() if ln.strip()]
    if pat:
        lines = [ln for ln in lines if re.search(pat, ln)] or lines
    return (lines[-1] if lines else "(sem saída)")[:200]


def proc_daemons():
    rows = []
    for d in Path("/proc").iterdir():
        if d.name.isdigit() and (d / "comm").exists():
            try:
                if (d / "comm").read_text().strip() == "touring-daemon":
                    rows.append((int(d.name), os.readlink(d / "exe")))
            except OSError:
                pass
    return sorted(rows)


def snap_real():
    links = {n: os.readlink(H / n) for n in (".local/bin/touring", ".local/bin/touring-hook", ".local/bin/touring-daemon",
                                             ".claude/hooks/touring", ".claude/hooks/touring-daemon")}
    mt = {n: (T / "target/release" / n).stat().st_mtime for n in ("touring", "touring-hook", "touring-daemon")}
    return links, proc_daemons(), mt


# ---------- checks: each returns (ok, key_line, full_output) ----------

def c_versoes():
    outs, ok = [], True
    for label, binp in (("global", "touring"), ("toolchain", str(H / ".touring/toolchains/30.4.93/bin/touring")),
                        ("analise", str(A / ".touring/bin/touring")), ("konverter", str(H / "projects/konverter/.touring/bin/touring"))):
        rc, out, _ = sh([binp, "--version"], cwd="/tmp", env=CLEAN)
        first = out.strip().splitlines()[0] if out.strip() else "(vazio)"
        ok &= rc == 0 and first.startswith("touring 30.4.93")
        outs.append(f"{label}={first}")
    rc, out, _ = sh(["touring", "toolchain", "list"], cwd="/tmp", env=CLEAN)
    ok &= "30.4.93 (default)" in out
    return ok, " · ".join(outs) + f" · default 30.4.93: {'30.4.93 (default)' in out}", "\n".join(outs) + "\n" + out


def c_daemons():
    rows = proc_daemons()
    pid_global = Path(f"/run/user/{os.getuid()}/touring-daemon.pid").read_text().strip()
    deleted = [r for r in rows if r[1].endswith("(deleted)")]
    _, out, _ = sh(["touring", "daemon-ctl", "status", "-j"], cwd="/tmp", env=CLEAN)
    try:
        st = json.loads(out[out.index("{"):])
    except ValueError:
        st = {}
    ok = not deleted and st.get("socket_alive") is True and str(st.get("daemon_pid")) == pid_global
    return ok, f"{len(rows)} daemons · deleted {len(deleted)} · global pid {st.get('daemon_pid')} (pidfile {pid_global}) alive {st.get('socket_alive')}", json.dumps(rows) + "\n" + out


def c_update_touring_inocuo():
    before = snap_real()
    rc1, o1, _ = sh([sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", "scripts/test_update_touring.py"], cwd=T)
    rc2, o2, _ = sh([sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", "scripts/test_update_touring.py"],
                    cwd=T, env={**os.environ, "UPDATE_TOURING_REQUIRE_SYMLINK": "1"})
    same = before == snap_real()
    return rc1 == 0 and rc2 == 0 and same, f"{last(o1)} | SYMLINK=1: {last(o2)} | estado real antes==depois: {same}", o1 + o2


def c_memoria():
    bad, out_all = [], ""
    for k in KEYS:
        rc, out, _ = sh([PY_A, "scripts/memoria/grafo_memoria.py", "verificar", k], cwd=A, timeout=120)
        out_all += f"== {k} rc {rc}\n{out}\n"
        if rc != 0:
            bad.append(k.split(":")[1][:40])
    return not bad, f"{len(KEYS) - len(bad)}/{len(KEYS)} nós com arestas+procedência" + (f" · FALHA: {bad}" if bad else ""), out_all


def c_agentes_sem_erro():
    hits = [f"{p}:{i}" for p in AGENT_FILES for i, ln in enumerate(p.read_text().splitlines(), 1) if WRONG.search(ln)]
    pos = WRONG.search('x touring hook-memory-recall "q"') is not None  # positive control of the pattern
    return not hits and pos, f"{len(AGENT_FILES)} arquivos vivos (fonte+espelho) · ocorrências erradas {len(hits)} · controle positivo {pos}", "\n".join(hits)


def c_dags():
    lines, ok = [], True
    for tid, want in (("task_1791474484429430085", "finalized"), ("task_1791479126715571926", None), ("task_1791484992466538442", None)):
        # the DAGs live in the ANALISE per-project store (cwd decides it) and stdout alone is the JSON — the
        # 09/10 run read stdout+stderr from the scratchpad cwd and printed "None 0/0" for three healthy DAGs
        p = subprocess.run(["touring", "decompose", "get", tid, "-j"], cwd=A, capture_output=True, text=True, timeout=60)
        try:
            d = json.loads(p.stdout)
        except ValueError:
            ok, d = False, {}
        subs = d.get("subtasks", [])
        status = d.get("task", {}).get("status")
        pend = [s["subtask_id"].split("::")[-1] for s in subs if s.get("status") != "completed"]
        ok &= want is None or status == want
        lines.append(f"{tid[-6:]} {status} {len(subs) - len(pend)}/{len(subs)} pend={pend}")
    return ok, " · ".join(lines), "\n".join(lines)


def cmd(argv, cwd, timeout=900, env=None, pat=None):
    def run():
        rc, out, _ = sh(argv, cwd=cwd, timeout=timeout, env=env)
        return rc == 0, f"rc {rc} | {last(out, pat)}", out
    return run


CHECKS = [
    ("release.versoes", c_versoes),
    ("release.daemons", c_daemons),
    ("release.espelho_client", cmd([sys.executable, "scripts/sync-client-skills.py", "--check"], T)),
    ("release.update_touring_suite_inocua", c_update_touring_inocuo),
    ("drift.A1", cmd([sys.executable, "scripts/test_drift_coverage.py"], T)),
    ("drift.A2_pytest", cmd([sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", "scripts/test_drift_coverage.py"], T)),
    ("drift.A3_verify", cmd([sys.executable, str(DRF / "reconciliar.py"), "verify"], T, pat="VERIFY|FAIL")),
    ("drift.A4_phantom", cmd([sys.executable, "scripts/test_docs_no_phantom_crates.py"], T)),
    ("drift.predicados", cmd([sys.executable, str(DRF / "test_scanner_predicates.py")], T)),
    ("drift.agentes_sem_literal_errado", c_agentes_sem_erro),
    ("compactacao.conservacao_agentes", cmd([sys.executable, str(CMP / "scripts/conservacao.py"), "--agents"], CMP, pat="agentes:")),
    ("compactacao.juiz_unico", cmd([sys.executable, str(CMP / "scripts/verificar_compactacao.py")], CMP, timeout=1800, pat="veredito|FAIL")),
    ("vgp.inventario_selftest", cmd([sys.executable, str(VGP / "scripts/inventario_erros.py"), "--selftest"], A)),
    ("vgp.vivos_selftest", cmd([sys.executable, str(VGP / "scripts/vivos.py"), "--selftest"], A)),
    ("vgp.replay_selftest", cmd([sys.executable, str(VGP / "scripts/replay.py"), "--selftest"], A, timeout=900)),
    ("vgp.juiz_v0c", cmd([sys.executable, str(VGP / "scripts/juiz_v0c.py")], A, timeout=2400, pat="veredito|CONVERG|lacuna|gap")),
    ("vgp.juiz_v0d_selftest", cmd([sys.executable, str(VGP / "scripts/juiz_v0d.py"), "--selftest"], A, timeout=900, pat="mutantes")),
    ("vgp.juiz_v0d_corpus", cmd([sys.executable, str(VGP / "scripts/juiz_v0d.py")], A, timeout=2400, pat="veredito")),
    ("analise.grafo_tests", cmd([PY_A, "-m", "pytest", "-q", "-p", "no:cacheprovider", "--no-cov", "scripts/memoria/tests"], A)),
    ("analise.emendas_licoes", cmd([PY_A, "-m", "pytest", "-q", "-p", "no:cacheprovider", "--no-cov",
                                    "docs/plans/2026-10-05-emendas-pimenta/diagnostics/test_gravar_licoes.py"], A)),
    ("analise.consertos_a9", cmd([PY_A, "-m", "pytest", "-q", "-p", "no:cacheprovider", "--no-cov",
                                  "scripts/lexhub/tests/test_local_o5_caminho.py",
                                  "docs/plans/2026-10-06-geopolitica-logistica-eua/diagnostics/test_t010_auditar.py"], A)),
    ("lint.ruff", cmd(["ruff", "check", *RUFF], T)),
    ("memoria.nos_relidos", c_memoria),
    ("dags", c_dags),
]


def main():
    results = []
    for name, fn in CHECKS:
        t0 = time.time()
        try:
            ok, key, full = fn()
        except Exception as e:  # a check that cannot run is a failed check, never a pass
            ok, key, full = False, f"{type(e).__name__}: {e}", repr(e)
        (OUT / f"{name}.log").write_text(full or "", encoding="utf-8")
        results.append({"check": name, "ok": ok, "s": round(time.time() - t0), "key": key})
        print(f"{'PASS' if ok else 'FAIL'} {name} ({round(time.time() - t0)}s) — {key}", flush=True)
    (OUT / "resumo.json").write_text(json.dumps(results, ensure_ascii=False, indent=1), encoding="utf-8")
    bad = [r["check"] for r in results if not r["ok"]]
    print(f"\nVEREDITO: {len(results) - len(bad)}/{len(results)} PASS" + (f" · FALHAS: {bad}" if bad else ""))
    return 0 if not bad else 1


if __name__ == "__main__":
    sys.exit(main())
