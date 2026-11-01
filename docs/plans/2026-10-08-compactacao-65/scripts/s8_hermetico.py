"""(a) S8 acceptance ② in a CLEAN env (20 sequential runs); (b) hermeticity matrix: the suite once per documented
TOURING_* variable set — TOURING_CODE_MODE=native is the positive control (15 failures measured by hand)."""
import concurrent.futures, os, re, subprocess
T = os.path.expanduser("~/projects/touring")
B = f"{T}/target/debug/deps/touring_cli-f350bd0b6ec11555"
CLEAN = {k: v for k, v in os.environ.items() if not k.startswith("TOURING_CODE_MODE")}

def run(extra):
    p = subprocess.run([B, "-q"], cwd=T, env={**CLEAN, **extra}, capture_output=True, text=True)
    m = re.search(r"(\d+) passed; (\d+) failed", p.stdout)
    return p.returncode, (m.groups() if m else ("?", "?"))

clean = [run({}) for _ in range(20)]
print(f"(a) ② limpo: 20 rodadas, {sum(rc != 0 for rc, _ in clean)} falhas · passed/failed: {sorted(set(r for _, r in clean))}")
MATRIX = [{"TOURING_CODE_MODE": "native"}, {"TOURING_CODE_MODE": "code"}, {"TOURING_GATE_OK": "1"},
          {"TOURING_SUGGESTER_DISABLED": "1"}, {"TOURING_TURN_BUDGET": "100"}, {"TOURING_DEDUP_DISABLED": "1"},
          {"TOURING_CODE_GATES_DISABLED": "1"}, {"TOURING_PILLAR_INDUCTION_ARMED": "1"}]
with concurrent.futures.ThreadPoolExecutor(4) as pool:
    for env, (rc, (ok, bad)) in zip(MATRIX, pool.map(run, MATRIX)):
        print(f"(b) {list(env.items())[0]}: rc {rc} · passed {ok} · failed {bad}")
