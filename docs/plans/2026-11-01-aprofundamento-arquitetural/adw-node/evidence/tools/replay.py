"""Replay every spec of every corpus through lint / explain / explain --cost /
explain --mermaid / test(mock, subprocess stubbed) on ONE adw.py build.

    replay.py --adw <adw.py> --lib <adw-library> --roots <dir> --out <out.json>

<dir>/<corpus>/.touring/adw/*.toml (+ fragments/) is the layout of each corpus. Nothing
here touches a real root: `test` writes run dirs, so every spec runs against a private
copy of its corpus. subprocess.run is replaced by a deterministic fake, time.sleep by a
no-op and new_run_id by a pure function of the spec name, so two builds that behave the
same produce byte-identical output.
"""
import argparse, contextlib, importlib.util, io, json, os, shutil, subprocess, sys, tempfile, pathlib, tomllib

ap = argparse.ArgumentParser()
ap.add_argument("--adw", required=True)
ap.add_argument("--lib", required=True)
ap.add_argument("--roots", required=True)
ap.add_argument("--out", required=True)
args = ap.parse_args()

os.environ["TOURING_ADW_LIBRARY"] = args.lib
spec_ = importlib.util.spec_from_file_location("adw_replay", args.adw)
adw = importlib.util.module_from_spec(spec_)
sys.modules["adw_replay"] = adw
spec_.loader.exec_module(adw)

FAKE_OUT = "NEW_FINDINGS=0\nFACT=k=v\nVERDICT=PASS\nCONTROL=pass\nok\n"


def fake_run(cmd, *a, **kw):
    return subprocess.CompletedProcess(cmd, 0, stdout=FAKE_OUT, stderr="")


adw.subprocess.run = fake_run
adw.time.sleep = lambda *_a, **_k: None
adw.new_run_id = lambda name: f"replay-{name}"


def capture(fn):
    buf = io.StringIO()
    err = io.StringIO()
    with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(err):
        try:
            rc = fn()
        except SystemExit as e:  # argparse
            rc = e.code
        except Exception as e:  # a crash is a result too - and must be identical across builds
            rc = f"EXC {type(e).__name__}: {e}"
    return {"rc": rc, "stdout": buf.getvalue(), "stderr": err.getvalue()}


results = {}
roots = pathlib.Path(args.roots)
for corpus in sorted(p for p in roots.iterdir() if p.is_dir()):
    adw_dir = corpus / ".touring" / "adw"
    for f in sorted(adw_dir.glob("*.toml")):
        name = f.stem
        try:
            data = tomllib.loads(f.read_text(encoding="utf-8"))
        except Exception:
            data = {}
        if not data.get("node") and not data.get("use"):
            continue  # not a flow (tiers.toml, ...)
        key = f"{corpus.name}/{name}"
        entry = {}
        for label, argv in (
            ("lint", ["lint", name]),
            ("explain", ["explain", name]),
            ("explain_cost", ["explain", name, "--cost"]),
            ("mermaid", ["explain", name, "--mermaid"]),
            ("test", ["test", name]),
        ):
            with tempfile.TemporaryDirectory() as tmp:
                work = pathlib.Path(tmp) / "root"
                shutil.copytree(corpus, work)
                entry[label] = capture(lambda: adw.main(["--root", str(work), *argv]))
                # the run id/paths embed the tmp dir: normalise
                for k in ("stdout", "stderr"):
                    entry[label][k] = entry[label][k].replace(str(work), "<ROOT>")
        results[key] = entry
json.dump(results, open(args.out, "w"), indent=1, sort_keys=True, ensure_ascii=False)
print(f"{args.adw}: {len(results)} specs x 5 commands -> {args.out}")
