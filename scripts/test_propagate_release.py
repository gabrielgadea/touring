"""Guards for the clippy + touched-crates TEST gate of `scripts/propagate-release.sh`.

The defect (measured 09/10/2026): gate 1/6 ran `cargo check` and `cargo clippy`
and no test suite at all, and clippy without `--all-targets` does not even
compile test code. Release 30.4.95 shipped with
`classifier_edit_rust_emits_pre_edit_triage_with_tdg_gate` broken and with two
tests in `handlers/index.rs` that never compiled (E0433). CLAUDE.md §3 already
required "testes dos crates tocados"; the executor did not apply it (D8: the
enforcement lives in the executor).

Everything here is proved by EXECUTION, in a throwaway git tree:

  · the real script runs against a fake workspace whose `hw-slot`, `cargo`,
    `touring` and `update-touring` are recording stubs (`cargo metadata` is the
    only real one), so the test sees exactly which commands the gate would run;
  · a **probe** is a function `script_text -> [violations]`. The real script must
    return `[]`; each **mutant** (a mutated COPY of the script) must make its
    probe return something, or the probe is vacuous;
  · one extra test runs the real script with `--dry-run` in THIS repository and
    proves it touches no binary, daemon, toolchain or lock.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from functools import cache
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parent.parent
SCRIPT = REPO / "scripts" / "propagate-release.sh"
VERSION = "0.0.1"

needs_tools = pytest.mark.skipif(
    not all(shutil.which(t) for t in ("bash", "git", "cargo")),
    reason="bash, git and cargo are needed to run the gate in a fake workspace",
)

# ── the fake workspace ──────────────────────────────────────────────────────

#: name -> directory. `inner` is NESTED in `outer`, `pkg-a` is a string prefix of
#: `pkg-ab`: the two traps of "the longest manifest_path that is a prefix".
CRATES = {
    "pkg-a": "crates/pkg-a",
    "pkg-ab": "crates/pkg-ab",
    "pkg-before": "crates/pkg-before",
    "outer": "crates/outer",
    "inner": "crates/outer/inner",
    "touring-hook-handlers": "crates/touring-hook-handlers",
    "pkg-new": "crates/pkg-new",
}

#: What the history below makes "touched since the last release":
#:   inner                  uncommitted edit            (nested package)
#:   pkg-ab                 committed after the release (its body quotes "release ")
#:   pkg-new                untracked crate
#:   touring-hook-handlers  uncommitted edit            (needs --features)
#: and NOT touched: pkg-a (string prefix of pkg-ab), outer (parent of inner),
#: pkg-before (edited BEFORE the release commit).
EXPECTED = ["inner", "pkg-ab", "pkg-new", "touring-hook-handlers"]
ALL_PACKAGES = sorted(CRATES)

FEATURES = {"touring-hook-handlers": "pre-hooks,post-hooks"}


def expected_cmd(pkg: str) -> str:
    cmd = f"--class rust --label release-tests -- cargo test -p {pkg} --no-fail-fast"
    return cmd + (f" --features {FEATURES[pkg]}" if pkg in FEATURES else "")


HW_SLOT_STUB = f"""#!{sys.executable}
import json, os, sys
argv = sys.argv[1:]
log = os.environ["CALLS_LOG"]
with open(log, "a") as fh:
    fh.write("hw-slot " + " ".join(argv) + "\\n")
pkg = argv[argv.index("-p") + 1] if "-p" in argv else ""
seen = sum(1 for l in open(log) if f" -p {{pkg}} " in l or l.rstrip().endswith(f" -p {{pkg}}"))
rcs = json.loads(os.environ.get("FAKE_RC", "{{}}")).get(pkg, [0])
sys.exit(rcs[min(seen, len(rcs)) - 1])
"""

#: `cargo metadata` is the only real cargo call; anything else is recorded and
#: succeeds without compiling a thing.
CARGO_STUB = """#!/bin/sh
echo "cargo $*" >> "$CALLS_LOG"
if [ "$1" = "metadata" ]; then exec "$REAL_CARGO" "$@"; fi
exit 0
"""

#: `python3 -` (the script's inline programs) runs for real; `python3 -m pytest`,
#: `python3 scripts/x.py` and `python3 -c` (project list) are no-ops.
PYTHON_STUB = """#!/bin/sh
if [ "$1" = "-" ]; then exec "$REAL_PYTHON" "$@"; fi
exit 0
"""

RECORD_STUB = """#!/bin/sh
echo "$(basename "$0") $*" >> "$CALLS_LOG"
exit 0
"""


def _write(path: Path, text: str, mode: int | None = None) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    if mode is not None:
        path.chmod(mode)


def _clean_env() -> dict[str, str]:
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    env.update(
        GIT_CONFIG_GLOBAL="/dev/null",
        GIT_CONFIG_NOSYSTEM="1",
        GIT_AUTHOR_NAME="t",
        GIT_AUTHOR_EMAIL="t@t",
        GIT_COMMITTER_NAME="t",
        GIT_COMMITTER_EMAIL="t@t",
        NO_COLOR="1",
    )
    env.pop("TOURING_DAEMON_SOCKET", None)
    env.pop("TOURING_DAEMON_SOCK", None)
    return env


def _git(tree: Path, *args: str) -> None:
    cmd = ["git", "-C", str(tree), "-c", "core.hooksPath=/dev/null", *args]
    subprocess.run(cmd, check=True, capture_output=True, text=True, env=_clean_env())


def _commit(tree: Path, message: str) -> None:
    _git(tree, "add", "-A")
    _git(tree, "commit", "-q", "--allow-empty", "-m", message)


def _append(path: Path, line: str) -> None:
    with path.open("a", encoding="utf-8") as fh:
        fh.write(line)


def build_tree(tree: Path, script_text: str, kind: str) -> None:
    """kind: "git" (a release commit exists) | "nobase" (none does) | "nogit"."""
    _write(tree / "scripts" / "propagate-release.sh", script_text, 0o755)
    _write(tree / "scripts" / "hw-slot", HW_SLOT_STUB, 0o755)
    _write(tree / "scripts" / "r3-s1a-gate", "#!/bin/sh\nexit 0\n", 0o755)
    _write(
        tree / "scripts" / "test_propagate_release_dummy.py",
        "def test_x():\n    pass\n",
    )
    _write(
        tree / "target" / "release" / "touring",
        f'#!/bin/sh\necho "touring {VERSION}" >&2\n',
        0o755,
    )
    _write(tree / ".gitignore", "target/\n")
    _write(tree / "docs" / "note.md", "note\n")
    members = ", ".join(f'"{d}"' for d in CRATES.values())
    _write(tree / "Cargo.toml", f'[workspace]\nresolver = "2"\nmembers = [{members}]\n')
    for name, rel in CRATES.items():
        if name == "pkg-new" and kind != "nogit":
            continue  # created untracked, after the history, in the "git" kinds
        _write(
            tree / rel / "Cargo.toml",
            f'[package]\nname = "{name}"\nversion = "0.0.0"\nedition = "2021"\n',
        )
        _write(tree / rel / "src" / "lib.rs", "pub fn f() {}\n")
    if kind == "nogit":
        _write(
            tree / CRATES["pkg-new"] / "Cargo.toml",
            '[package]\nname = "pkg-new"\nversion = "0.0.0"\nedition = "2021"\n',
        )
        _write(tree / CRATES["pkg-new"] / "src" / "lib.rs", "pub fn f() {}\n")
        return
    _git(tree, "init", "-q")
    _commit(tree, "init")
    _append(tree / CRATES["pkg-before"] / "src" / "lib.rs", "// before the release\n")
    _commit(tree, "feat: before the release")
    _commit(tree, f"release {VERSION}: base" if kind == "git" else "chore: base")
    _append(tree / CRATES["pkg-ab"] / "src" / "lib.rs", "// after the release\n")
    # A BODY line that starts with "release ": `git log --grep '^release'` alone
    # picks this commit as the base and narrows pkg-ab out of the gate.
    _commit(tree, "fix: after the release\n\nrelease notes: a decoy for the grep")
    # working tree: two uncommitted edits, one untracked crate, files outside any package
    _append(tree / CRATES["inner"] / "src" / "lib.rs", "// uncommitted\n")
    _append(
        tree / CRATES["touring-hook-handlers"] / "src" / "lib.rs", "// uncommitted\n"
    )
    _append(tree / "Cargo.toml", "# touched\n")
    _append(tree / "docs" / "note.md", "more\n")
    _write(
        tree / CRATES["pkg-new"] / "Cargo.toml",
        '[package]\nname = "pkg-new"\nversion = "0.0.0"\nedition = "2021"\n',
    )
    _write(tree / CRATES["pkg-new"] / "src" / "lib.rs", "pub fn f() {}\n")


@dataclass(frozen=True)
class Scenario:
    args: tuple[str, ...]
    fake_rc: tuple[tuple[str, tuple[int, ...]], ...] = ()
    tree: str = "git"


@dataclass(frozen=True)
class Result:
    rc: int
    out: str
    calls: tuple[str, ...]

    def hw(self) -> list[str]:
        return [
            c.removeprefix("hw-slot ") for c in self.calls if c.startswith("hw-slot ")
        ]

    def hw_packages(self) -> list[str]:
        return [m.group(1) for c in self.hw() if (m := re.search(r"-p (\S+)", c))]


REAL = Scenario(
    ("--skip-build", "--skip-freeze", "--no-default", "--stop-before-propagate")
)
DRY = Scenario(("--dry-run", "--allow-no-projects"))
SKIP_GATES = Scenario(
    (
        "--skip-gates",
        "--skip-build",
        "--skip-freeze",
        "--no-default",
        "--stop-before-propagate",
    )
)
FAIL = Scenario(REAL.args, (("pkg-ab", (101,)),))
NO_SLOT = Scenario(REAL.args, (("pkg-ab", (75,)),))
SLOT_THEN_OK = Scenario(REAL.args, (("pkg-ab", (75, 0)),))
MACHINE_CHECK = Scenario(REAL.args, (("pkg-ab", (70,)),))
DRY_NOBASE = Scenario(DRY.args, tree="nobase")
DRY_NOGIT = Scenario(DRY.args, tree="nogit")


@cache
def run_scenario(script_text: str, sc: Scenario) -> Result:
    tmp = Path(tempfile.mkdtemp(prefix="rel-gate-"))
    try:
        tree, aux = tmp / "tree", tmp / "aux"
        build_tree(tree, script_text, sc.tree)
        for name, body in (
            ("cargo", CARGO_STUB),
            ("python3", PYTHON_STUB),
            ("touring", RECORD_STUB),
            ("update-touring", RECORD_STUB),
        ):
            _write(aux / "bin" / name, body, 0o755)
        calls_log = aux / "calls.log"
        calls_log.write_text("", encoding="utf-8")
        env = _clean_env()
        env.update(
            PATH=f"{aux / 'bin'}:{os.environ['PATH']}",
            CALLS_LOG=str(calls_log),
            REAL_CARGO=shutil.which("cargo") or "cargo",
            REAL_PYTHON=sys.executable,
            FAKE_RC=json.dumps({k: list(v) for k, v in sc.fake_rc}),
        )
        proc = subprocess.run(
            ["bash", str(tree / "scripts" / "propagate-release.sh"), VERSION, *sc.args],
            capture_output=True,
            text=True,
            timeout=180,
            env=env,
            cwd=tmp,
            check=False,
        )
        calls = tuple(calls_log.read_text(encoding="utf-8").splitlines())
        return Result(proc.returncode, proc.stdout + proc.stderr, calls)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def executable_lines(script_text: str) -> list[str]:
    return [
        l
        for l in script_text.splitlines()
        if l.strip() and not l.strip().startswith("#")
    ]


# ── probes: script text -> list of violations ([] = the contract holds) ──────

CLIPPY = "cargo clippy --workspace --all-targets -- -D warnings"


def probe_clippy(script: str) -> list[str]:
    bad = []
    decl = [
        l for l in executable_lines(script) if l.lstrip().startswith("CLIPPY_GATE=(")
    ]
    if len(decl) != 1 or "--all-targets" not in decl[0]:
        bad.append(f"CLIPPY_GATE must be declared once with --all-targets, got {decl}")
    real, dry = run_scenario(script, REAL), run_scenario(script, DRY)
    if CLIPPY not in real.calls:
        bad.append(
            f"the real run did not execute {CLIPPY!r}; cargo calls: {[c for c in real.calls if c.startswith('cargo')]}"
        )
    if f"[dry-run] cargo check --workspace && {CLIPPY}" not in dry.out:
        bad.append("the [dry-run] clippy line is not the command the real run executes")
    return bad


def probe_tests_under_hw_slot(script: str) -> list[str]:
    real = run_scenario(script, REAL)
    bad = []
    if real.rc != 0 or "gates OK" not in real.out:
        bad.append(f"the real run must pass its gates, rc={real.rc}: {real.out[-400:]}")
    want = [expected_cmd(p) for p in EXPECTED]
    if real.hw() != want:
        bad.append(f"hw-slot calls {real.hw()} != {want}")
    direct = [c for c in real.calls if c.startswith("cargo test")]
    if direct:
        bad.append(f"cargo test ran outside hw-slot: {direct}")
    return bad


def probe_features(script: str) -> list[str]:
    real = run_scenario(script, REAL)
    got = {}
    for cmd in real.hw():
        pkg = re.search(r"-p (\S+)", cmd)
        feat = re.search(r"--features (\S+)", cmd)
        if pkg:
            got[pkg.group(1)] = feat.group(1) if feat else None
    want = {p: FEATURES.get(p) for p in EXPECTED}
    return [] if got == want else [f"features per package {got} != {want}"]


def probe_mapping(script: str) -> list[str]:
    real = run_scenario(script, REAL)
    got = real.hw_packages()
    if got == EXPECTED:
        return []
    return [
        f"touched packages: missing {sorted(set(EXPECTED) - set(got))}, extra {sorted(set(got) - set(EXPECTED))}, got {got}"
    ]


def probe_dry_run(script: str) -> list[str]:
    dry, real = run_scenario(script, DRY), run_scenario(script, REAL)
    bad = []
    if dry.rc != 0:
        bad.append(f"dry-run rc={dry.rc}: {dry.out[-400:]}")
    executed = [c for c in dry.calls if not c.startswith("cargo metadata")]
    if executed:
        bad.append(f"the dry-run EXECUTED: {executed}")
    listed = re.findall(
        r"\[dry-run\]\s+\S*scripts/hw-slot (.+)$", dry.out, re.MULTILINE
    )
    if listed != real.hw():
        bad.append(
            f"dry-run listing {listed} != the commands the real run executes {real.hw()}"
        )
    if f"{len(EXPECTED)} pacote(s)" not in dry.out:
        bad.append("the dry-run must announce how many packages it lists")
    return bad


def probe_failure(script: str) -> list[str]:
    r = run_scenario(script, FAIL)
    bad = []
    if r.rc == 0 or "gates OK" in r.out:
        bad.append(f"a failing package must fail the gate, rc={r.rc}")
    if (
        "testes de pkg-ab falharam (rc 101)" not in r.out
        or "testes falharam em: pkg-ab" not in r.out
    ):
        bad.append(f"the failure must name the package: {r.out[-300:]}")
    if r.hw_packages() != EXPECTED:
        bad.append(
            f"the remaining packages must still run so ALL failures show at once: {r.hw_packages()}"
        )
    return bad


def probe_no_slot(script: str) -> list[str]:
    bad = []
    r = run_scenario(script, NO_SLOT)
    if r.rc == 0 or "sem vaga" not in r.out or "falharam" in r.out:
        bad.append(
            f"rc 75 forever must fail as 'sem vaga', not as a test failure, rc={r.rc}: {r.out[-300:]}"
        )
    if r.hw_packages().count("pkg-ab") != 2 or "pkg-new" in r.hw_packages():
        bad.append(
            f"rc 75 must be retried exactly once and then abort: {r.hw_packages()}"
        )
    ok = run_scenario(script, SLOT_THEN_OK)
    if ok.rc != 0 or ok.hw_packages().count("pkg-ab") != 2 or "gates OK" not in ok.out:
        bad.append(
            f"rc 75 then 0 must pass after ONE retry, rc={ok.rc}: {ok.hw_packages()}"
        )
    return bad


def probe_machine_check(script: str) -> list[str]:
    r = run_scenario(script, MACHINE_CHECK)
    if r.rc != 0 and "machine check" in r.out and "falharam" not in r.out:
        return []
    return [
        f"rc 70 (hw-slot machine check) must abort naming it, rc={r.rc}: {r.out[-300:]}"
    ]


def probe_skip_gates(script: str) -> list[str]:
    r = run_scenario(script, SKIP_GATES)
    stray = [
        c
        for c in r.calls
        if c.startswith(("hw-slot", "cargo test", "cargo clippy", "cargo check"))
    ]
    return (
        [f"--skip-gates must skip everything, ran: {stray}"]
        if stray or r.rc != 0
        else []
    )


def probe_fallback(script: str) -> list[str]:
    bad = []
    for name, sc in (
        ("no release commit", DRY_NOBASE),
        ("no git repository", DRY_NOGIT),
    ):
        r = run_scenario(script, sc)
        listed = re.findall(
            r"-p (\S+)",
            " ".join(
                re.findall(
                    r"\[dry-run\]\s+\S*scripts/hw-slot (.+)$", r.out, re.MULTILINE
                )
            ),
        )
        if (
            sorted(listed) != ALL_PACKAGES
            or f"fallback: TODOS os {len(ALL_PACKAGES)} pacotes" not in r.out
        ):
            bad.append(
                f"{name}: the fallback must widen to ALL {len(ALL_PACKAGES)} packages, listed {listed}"
            )
    return bad


# ── the real script honours every probe ──────────────────────────────────────

PROBES = {
    "clippy": probe_clippy,
    "tests_under_hw_slot": probe_tests_under_hw_slot,
    "features": probe_features,
    "mapping": probe_mapping,
    "dry_run": probe_dry_run,
    "failure": probe_failure,
    "no_slot": probe_no_slot,
    "machine_check": probe_machine_check,
    "skip_gates": probe_skip_gates,
    "fallback": probe_fallback,
}


@needs_tools
@pytest.mark.parametrize("name", sorted(PROBES))
def test_the_real_script_honours_the_probe(name):
    violations = PROBES[name](SCRIPT.read_text(encoding="utf-8"))
    assert violations == [], "\n".join(violations)


# ── mutants: each mutated COPY must make its probe fail ──────────────────────

DRY_LISTING_LINE = 'echo "  ${YELLOW}[dry-run]${RESET}   ${TEST_CMD[*]}"'

MUTANTS = [
    (
        "clippy without --all-targets",
        "clippy",
        [
            (
                "CLIPPY_GATE=(cargo clippy --workspace --all-targets -- -D warnings)",
                "CLIPPY_GATE=(cargo clippy --workspace -- -D warnings)",
            )
        ],
    ),
    (
        "dry-run clippy line no longer the executed command",
        "clippy",
        [
            (
                "check --workspace && ${CLIPPY_GATE[*]}",
                "check --workspace && cargo clippy --workspace -- -D warnings",
            )
        ],
    ),
    (
        "the test gate runs nothing",
        "tests_under_hw_slot",
        [('( cd "$WORKSPACE" && "${TEST_CMD[@]}" ) || rc=$?', "true || rc=$?")],
    ),
    (
        "the test gate bypasses hw-slot",
        "tests_under_hw_slot",
        [
            (
                'TEST_CMD=("$WORKSPACE/scripts/hw-slot" --class rust --label release-tests -- cargo test',
                "TEST_CMD=(cargo test",
            )
        ],
    ),
    (
        "the features table gives the wrong features",
        "features",
        [
            (
                '[touring-hook-handlers]="pre-hooks,post-hooks"',
                '[touring-hook-handlers]="pre-hooks"',
            )
        ],
    ),
    (
        "the features table lost the entry",
        "features",
        [('    [touring-hook-handlers]="pre-hooks,post-hooks"\n', "")],
    ),
    (
        "the dry-run executes the listed commands",
        "dry_run",
        [(DRY_LISTING_LINE, DRY_LISTING_LINE + '\n            "${TEST_CMD[@]}"')],
    ),
    (
        "rc 75 is not retried",
        "no_slot",
        [('if [ "$rc" -eq 75 ] && [ "$attempt" -eq 1 ]; then', "if false; then")],
    ),
    (
        "rc 75 is reported as a test failure",
        "no_slot",
        [('75) die "sem vaga', '75) warn "x"; failed_pkgs+=("$pkg"); : "sem vaga')],
    ),
    (
        "rc 70 is reported as a test failure",
        "machine_check",
        [("70) die ", '70) warn "x"; failed_pkgs+=("$pkg"); : ')],
    ),
    (
        "a failing package does not fail the gate",
        "failure",
        [('failed_pkgs+=("$pkg")', ":")],
    ),
    (
        "the base is picked by --grep alone (body line wins)",
        "mapping",
        [
            (
                'RELEASE_SUBJECT = re.compile(r"^release[: ]")',
                'RELEASE_SUBJECT = re.compile(r"")',
            )
        ],
    ),
    (
        "the working tree is ignored",
        "mapping",
        [("    files.add(tok[3:])\n", "    pass\n")],
    ),
    (
        "the SHORTEST manifest prefix wins",
        "mapping",
        [
            (
                "key=lambda kv: len(kv[1].parts), reverse=True",
                "key=lambda kv: len(kv[1].parts), reverse=False",
            )
        ],
    ),
    (
        "prefix matched as a string, not as a path",
        "mapping",
        [("path == d or d in path.parents", "str(path).startswith(str(d))")],
    ),
    (
        "the fallback narrows instead of widening",
        "fallback",
        [
            (
                '    print("\\n".join(sorted(pkgs)))\n    sys.exit(0)',
                '    print("")\n    sys.exit(0)',
            )
        ],
    ),
    (
        "--skip-gates no longer skips the test gate",
        "skip_gates",
        [
            (
                'if [ "$SKIP_GATES" -eq 1 ]; then\n    warn "gates pulados (--skip-gates)"\nelse',
                'if false; then\n    warn "gates pulados (--skip-gates)"\nelse',
            )
        ],
    ),
]


def mutate(script_text: str, edits: list[tuple[str, str]]) -> str:
    out = script_text
    for old, new in edits:
        assert out.count(old) == 1, (
            f"the mutation anchor is not unique/present (stale mutant?): {old!r}"
        )
        out = out.replace(old, new)
    assert out != script_text, "a mutant that changes nothing proves nothing"
    return out


@needs_tools
@pytest.mark.parametrize(
    ("name", "probe", "edits"), MUTANTS, ids=[m[0] for m in MUTANTS]
)
def test_each_mutant_is_killed_by_its_probe(name, probe, edits):
    mutant = mutate(SCRIPT.read_text(encoding="utf-8"), edits)
    assert PROBES[probe](mutant), f"mutant {name!r} SURVIVED probe {probe!r}"


def test_the_mutants_cover_the_three_required_defects():
    names = " | ".join(m[0] for m in MUTANTS)
    for needle in ("--all-targets", "test gate", "features table"):
        assert needle in names, f"no mutant for: {needle}"


# ── the real dry-run, in THIS repository ─────────────────────────────────────


def _stubbed_env(tmp_path: Path) -> tuple[dict[str, str], Path, Path]:
    """PATH with recording stubs; any cargo call but `metadata` exits 99 (loud)."""
    bindir, log, slots = tmp_path / "bin", tmp_path / "calls.log", tmp_path / "slots"
    log.write_text("", encoding="utf-8")
    stubs = (
        ("cargo", CARGO_STUB.replace("exit 0", "exit 99")),
        ("python3", PYTHON_STUB),
        ("touring", RECORD_STUB),
        ("update-touring", RECORD_STUB),
    )
    for name, body in stubs:
        _write(bindir / name, body, 0o755)
    env = _clean_env()
    env.update(
        PATH=f"{bindir}:{os.environ['PATH']}",
        CALLS_LOG=str(log),
        REAL_CARGO=shutil.which("cargo") or "cargo",
        REAL_PYTHON=sys.executable,
        HW_SLOT_DIR=str(slots),
    )
    return env, log, slots


def _digests(paths: list[Path]) -> list[str]:
    return [hashlib.sha256(p.read_bytes()).hexdigest() for p in paths if p.exists()]


def _workspace_members() -> set[str]:
    meta = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--offline"],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=True,
    )
    return {p["name"] for p in json.loads(meta.stdout)["packages"]}


@needs_tools
def test_real_dry_run_in_this_repo_lists_the_gate_and_touches_nothing(tmp_path):
    """`propagate-release.sh <v> --dry-run`, for real, on the real tree.

    What a dry-run does (read from the script before running it): gates are only
    echoed, steps 2-4 go through `run` (echo), the label guard is skipped, step 5
    calls `touring update <v> --project <p> --dry-run` per project (read-only:
    update.rs returns before it relinks anything), 5.5 and 6 are skipped. Here
    `touring` and `update-touring` are recording stubs and `hw-slot` runs on an
    empty private slot dir, so ANY execution leaves a trace; the only real cargo
    call allowed is `cargo metadata`, and Cargo.toml/Cargo.lock must stay
    byte-identical."""
    env, log, slots = _stubbed_env(tmp_path)
    watched = [REPO / "Cargo.toml", REPO / "Cargo.lock"]
    before = _digests(watched)
    proc = subprocess.run(
        [str(SCRIPT), "30.4.96", "--dry-run", "--allow-no-projects"],
        capture_output=True,
        text=True,
        timeout=180,
        env=env,
        cwd=tmp_path,
        check=False,
    )
    out = proc.stdout + proc.stderr
    assert proc.returncode == 0, out[-600:]
    calls = log.read_text(encoding="utf-8").splitlines()
    assert all(c.startswith("cargo metadata") for c in calls), f"executed: {calls}"
    assert not slots.exists() or not any(slots.iterdir()), "hw-slot ran in a dry-run"
    assert _digests(watched) == before, "the dry-run rewrote Cargo.toml/Cargo.lock"
    assert f"[dry-run] cargo check --workspace && {CLIPPY}" in out
    header = re.search(r"tocados desde o último release: (\d+) pacote\(s\)", out)
    assert header, out[-800:]
    cmd_re = r"\[dry-run\]\s+\S*scripts/hw-slot --class rust --label release-tests -- cargo test -p (\S+) --no-fail-fast"
    listed = re.findall(cmd_re, out)
    assert len(listed) == int(header.group(1))
    assert set(listed) <= _workspace_members(), (
        "the gate listed a non-workspace package"
    )
    if "touring-hook-handlers" in listed:
        assert (
            "-p touring-hook-handlers --no-fail-fast --features pre-hooks,post-hooks"
            in out
        )


# ── hygiene of the script itself ─────────────────────────────────────────────


def test_the_script_parses_and_is_executable():
    assert os.access(SCRIPT, os.X_OK)
    proc = subprocess.run(
        ["bash", "-n", str(SCRIPT)], capture_output=True, text=True, check=False
    )
    assert proc.returncode == 0, proc.stderr


@pytest.mark.skipif(
    shutil.which("shellcheck") is None, reason="shellcheck not installed"
)
def test_shellcheck_is_clean():
    proc = subprocess.run(
        ["shellcheck", "-S", "warning", str(SCRIPT)],
        capture_output=True,
        text=True,
        check=False,
    )
    assert proc.returncode == 0, proc.stdout


def test_claude_md_documents_the_test_gate_in_section_2_1():
    text = (REPO / "CLAUDE.md").read_text(encoding="utf-8")
    start = text.index("2.1. **Propagação reprodutível")
    section = text[start : text.index("\n2.2. ", start)]
    item = re.search(r"\(e\)(.+?)(?=\n\s*\(f\)|\Z)", section, re.DOTALL)
    assert item, "CLAUDE.md §2.1 must list the test gate as item (e)"
    for needle in ("testes", "hw-slot", "--all-targets"):
        assert needle in item.group(1), f"item (e) must mention {needle!r}"


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-q"]))
