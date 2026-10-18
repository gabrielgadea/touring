import hashlib, os, shutil, subprocess, sys
from pathlib import Path
A = Path("/home/gabrielgadea/projects/touring-wt-g1/crates/touring-cli/src/cli_suggester.rs")
C = Path(sys.argv[1]) / "cli_suggester.rs.pristino_d"
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
shutil.copy2(A, C); s0 = sha(A); print("sha inicial :", s0)
src = A.read_text(encoding="utf-8")
sai = "    burst_ledger().invalidate(&key);\n    // S212 no G1"
ent = "    if !ovr.g1_rewrite_disabled {\n        record_gate_event(GateId::G1, GateEvent::Emitted);\n        record_gate_event(GateId::G1, GateEvent::Followed);"
assert src.count(sai) == 1 and src.count(ent) == 1, "trechos nao unicos"
mut = src.replace(sai, "    // S212 no G1").replace(ent, "    burst_ledger().invalidate(&key);\n" + ent)
try:
    A.write_text(mut, encoding="utf-8"); print("sha mutado  :", sha(A))
    r = subprocess.run(["cargo","test","-p","touring-cli","--lib","g1_no_replay_tests"], cwd=A.parents[3],
        env={**os.environ,"CARGO_TARGET_DIR":os.path.expanduser("~/projects/touring/target")}, capture_output=True, text=True)
    print("\n".join(l for l in (r.stdout+r.stderr).splitlines() if l.startswith("test ") or l.startswith("test result") or "error[" in l))
    print("cargo test rc =", r.returncode)
finally:
    shutil.copy2(C, A); s1 = sha(A); print("sha restaur.:", s1, "IGUAL" if s1 == s0 else "DIVERGE!!")
