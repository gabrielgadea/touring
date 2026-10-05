"""Per-project proof of 30.4.92 with each project's OWN binary, plus the 30.4.90 toolchain as negative control.

Probes that only the new build passes, all client-side and side-effect free:
  P1 `decompose get X -j --help`  -> help (old: "unexpected argument '-j'", rc 2)
  P2 `gotcha list --json --help`  -> help (missed by the first routing pass; old: rejected)
  P3 `init-project --help` in an empty scratch cwd -> help and the cwd stays empty (old: wrote .touring/)
Plus the live state: the per-project daemon's executable resolves into the 30.4.92 toolchain, and the hook
shim, asked from inside the project, names the project's own bin.
"""
import os, subprocess, tempfile

HOME = os.path.expanduser("~")
PROJECTS = {p: f"{HOME}/projects/{p}" for p in ("analise", "konverter")}
OLD = f"{HOME}/.touring/toolchains/30.4.90/bin/touring"


def env_for(scratch):
    e = {k: v for k, v in os.environ.items() if not k.startswith("TOURING_DAEMON_SOCK")}
    e["NO_COLOR"] = "1"
    return e


def probe(binary):
    out = {}
    with tempfile.TemporaryDirectory(prefix="prove-") as d:
        env = env_for(d)
        for name, argv in (("P1 decompose get -j", ["decompose", "get", "X", "-j", "--help"]),
                           ("P2 gotcha list --json", ["gotcha", "list", "--json", "--help"])):
            p = subprocess.run([binary, *argv], cwd=d, env=env, capture_output=True, text=True, timeout=30)
            txt = p.stdout + p.stderr
            out[name] = p.returncode == 0 and "unexpected argument" not in txt and "Usage" in txt
        cwd = os.path.join(d, "empty")
        os.mkdir(cwd)
        p = subprocess.run([binary, "init-project", "--help"], cwd=cwd, env=env, capture_output=True,
                           text=True, timeout=30)
        out["P3 init-project --help nada grava"] = p.returncode == 0 and os.listdir(cwd) == []
    return out


def daemon_exe(root):
    sock = f"{root}/.touring/daemon.sock"
    r = subprocess.run(["touring", "daemon-ctl", "list-all"], capture_output=True, text=True, timeout=30)
    for line in (r.stdout + r.stderr).splitlines():  # the listing is human output, on stderr
        if sock in line and "pid=" in line:
            pid = line.split("pid=")[1].split()[0]
            try:
                return pid, os.path.realpath(f"/proc/{pid}/exe")
            except OSError as e:
                return pid, f"ilegivel: {e}"
    return None, "socket sem daemon listado"


def shim_target(root):
    env = {k: v for k, v in os.environ.items() if not k.startswith("TOURING_DAEMON_SOCK")}
    env["TOURING_HOOK_SHIM_TRACE"] = "1"
    p = subprocess.run([f"{HOME}/.claude/hooks/touring-hook", "--version"], cwd=root, env=env,
                       capture_output=True, text=True, timeout=30)
    trace = [l for l in (p.stderr + p.stdout).splitlines() if "shim" in l.lower() or ".touring" in l]
    return trace[:2]


ok_all = True
for proj, root in PROJECTS.items():
    binary = f"{root}/.touring/bin/touring"
    real = os.path.realpath(binary)
    ver = subprocess.run([binary, "--version"], capture_output=True, text=True, timeout=30)
    res = probe(binary)
    pid, exe = daemon_exe(root)
    print(f"== {proj}: bin -> {real}")
    print(f"   --version (stderr+stdout): {(ver.stdout + ver.stderr).strip().splitlines()[0] if (ver.stdout + ver.stderr).strip() else '(vazio)'}")
    for k, v in res.items():
        print(f"   {k}: {'PASSA' if v else 'REPROVA'}")
    print(f"   daemon per-project pid={pid} exe={exe}")
    print(f"   shim: {shim_target(root)}")
    ok_all &= all(res.values()) and "/30.4.92/" in real and "/30.4.92/" in exe

print("== controle negativo: toolchain 30.4.90")
if os.path.exists(OLD):
    neg = probe(OLD)
    for k, v in neg.items():
        print(f"   {k}: {'PASSA' if v else 'REPROVA (esperado)'}")
    discriminates = not any(neg.values())
    print(f"   as sondas discriminam o build velho: {discriminates}")
    ok_all &= discriminates
else:
    print("   toolchain 30.4.90 ausente: controle negativo NAO MEDIDO")
    ok_all = False
print("VEREDITO:", "PROVADO" if ok_all else "NAO PROVADO")
