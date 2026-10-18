#!/usr/bin/env python3
"""Structure tests for .github/workflows/{ci,scheduled}.yml (ECC analysis, 17/10/2026).

`actionlint` is not available on the maintainer machine, and a workflow can only be PROVEN by
running on GitHub — so what is checked here is the part that does not need a runner: the
structure the decisions promised. Each test names the decision it guards:

* "contents: write do job gates" (canvas): `gates` is read-only and leaves no credential behind;
  the only writer is a tiny `sync-architecture` job, push-to-main only, behind `needs: gates`;
  a fork PR with drift still fails hard (the old contract) and a same-repo PR only warns.
* N3: a weekly `scheduled.yml` (Monday 09:00 UTC) with least privilege and two jobs.
* N1: `cargo mutants --in-diff` in ci.yml as an ADVISORY (continue-on-error) job, and the full
  scan sharded in scheduled.yml with a shard count that agrees with its `--shard k/n`.
* N2: the coverage ratchet runs in the `coverage` job, after the lcov artifact is uploaded.
* Supply chain: every `uses:` in these two files stays pinned by a 40-hex commit SHA.
"""
from __future__ import annotations

import re
from pathlib import Path

import pytest
import yaml

ROOT = Path(__file__).resolve().parent.parent
WF = ROOT / ".github" / "workflows"
SHA_PIN = re.compile(r"^[\w.-]+/[\w./-]+@[0-9a-f]{40}$")


def carregar(nome: str) -> dict:
    doc = yaml.safe_load((WF / nome).read_text(encoding="utf-8"))
    doc["on"] = doc.pop(True, doc.get("on"))  # YAML 1.1 reads the key `on` as boolean True
    return doc


def passos(job: dict) -> list[dict]:
    return job.get("steps") or []


def comandos(job: dict) -> str:
    return "\n".join(p.get("run", "") for p in passos(job))


def usos(doc: dict) -> list[str]:
    return [p["uses"] for j in doc["jobs"].values() for p in passos(j) if "uses" in p]


@pytest.fixture(scope="module")
def ci() -> dict:
    return carregar("ci.yml")


@pytest.fixture(scope="module")
def sched() -> dict:
    return carregar("scheduled.yml")


# --- every workflow parses -------------------------------------------------------------------------

@pytest.mark.parametrize("nome", sorted(p.name for p in WF.glob("*.yml")))
def test_every_workflow_is_valid_yaml_with_triggers_and_jobs(nome):
    doc = carregar(nome)
    assert doc.get("on"), f"{nome}: no trigger"
    assert doc.get("jobs"), f"{nome}: no jobs"
    for jid, job in doc["jobs"].items():
        assert "runs-on" in job or "uses" in job, f"{nome}:{jid}: neither runs-on nor reusable workflow"


@pytest.mark.parametrize("nome", ["ci.yml", "scheduled.yml"])
def test_every_action_is_pinned_by_commit_sha(nome):
    soltas = [u for u in usos(carregar(nome)) if not SHA_PIN.match(u)]
    assert soltas == [], f"{nome}: actions not pinned by SHA: {soltas}"


# --- contents: write do job gates ----------------------------------------------------------------

def test_workflow_default_permission_is_read_only(ci):
    assert ci["permissions"] == {"contents": "read"}


def test_only_sync_architecture_can_write(ci):
    escritores = [jid for jid, j in ci["jobs"].items() if "write" in (j.get("permissions") or {}).values()]
    assert escritores == ["sync-architecture"]
    assert ci["jobs"]["sync-architecture"]["permissions"] == {"contents": "write"}


def test_gates_is_read_only_and_leaves_no_credential_in_git_config(ci):
    gates = ci["jobs"]["gates"]
    assert "permissions" not in gates or "write" not in gates["permissions"].values()
    checkouts = [p for p in passos(gates) if p.get("uses", "").startswith("actions/checkout@")]
    assert checkouts and all((p.get("with") or {}).get("persist-credentials") is False for p in checkouts)


def test_gates_never_commits_or_pushes(ci):
    cmd = comandos(ci["jobs"]["gates"])
    for proibido in ("git push", "git commit", "git config user", "git add "):
        assert proibido not in cmd, proibido


def test_gates_sync_step_keeps_the_fork_contract_and_only_warns_on_same_repo_prs(ci):
    sync = next(p for p in passos(ci["jobs"]["gates"]) if "sync_metrics" in p.get("name", ""))
    run = sync["run"]
    assert re.search(r"^\s*python3 docs/sync_metrics\.py --sync$", run, re.MULTILINE), "the step must actually run the sync, not just mention it"
    fork = run[run.index('"${IS_FORK}" = "true"'):]
    assert fork.index("exit 1") < fork.index("::warning::"), "a fork PR with drift must still FAIL hard, before the warning branch"
    assert "::warning::" in run and "pull_request" in run, "same-repo PR: drift is only a warning (main syncs after the merge)"
    assert sync["env"]["IS_FORK"], "the fork detector must stay"
    assert "EVENT_NAME" in sync["env"]


def test_sync_architecture_runs_only_on_push_to_main_after_gates(ci):
    job = ci["jobs"]["sync-architecture"]
    assert job["needs"] in ("gates", ["gates"])
    cond = job["if"]
    assert "github.event_name == 'push'" in cond and "refs/heads/main" in cond
    assert "pull_request" not in cond


def test_sync_architecture_runs_nothing_but_the_sync_and_the_push(ci):
    job = ci["jobs"]["sync-architecture"]
    p = passos(job)
    assert len(p) == 2, [x.get("name") or x.get("uses") for x in p]
    assert p[0]["uses"].startswith("actions/checkout@")
    assert p[0]["with"]["ref"] == "${{ github.ref_name }}", "check out the branch itself, or there is nothing to push to"
    run = p[1]["run"]
    assert "python3 docs/sync_metrics.py --sync" in run
    assert "[skip ci]" in run, "without it the push would re-trigger this workflow forever"
    assert "git push" in run and "git pull --rebase" in run, "concurrent pushes are rebased and retried"
    for estranho in ("pip install", "pytest", "cargo ", "mkdocs", "apt-get"):
        assert estranho not in run, f"the write token must not reach {estranho!r}"


@pytest.mark.parametrize("nome", ["ci.yml", "scheduled.yml"])
def test_cargo_mutants_is_pinned_to_the_version_whose_flags_were_checked(nome):
    """--shard is 0-based and --in-diff/--shuffle/--in-place were read from THIS version's --help."""
    instalacoes = [
        p for j in carregar(nome)["jobs"].values() for p in passos(j)
        if p.get("uses", "").startswith("taiki-e/install-action@") and "cargo-mutants" in str((p.get("with") or {}).get("tool", ""))
    ]
    assert instalacoes, f"{nome}: no cargo-mutants install step"
    for p in instalacoes:
        assert re.search(r"cargo-mutants@\d+\.\d+\.\d+", p["with"]["tool"]), p["with"]["tool"]


def test_gates_runs_the_kpi_tooling_tests_in_a_pinned_venv(ci):
    """A guard that exists and never runs is a certificate: these four suites must be wired in."""
    passo = next(p for p in passos(ci["jobs"]["gates"]) if p.get("name", "").startswith("kpi tooling"))
    run = passo["run"]
    for suite in ("test_readiness.py", "test_journal_gc.py", "test_coverage_ratchet.py", "test_ci_workflows.py"):
        assert f"scripts/{suite}" in run, suite
    assert re.search(r"pip\"? install -q pytest==\d+\.\d+\.\d+ pyyaml==\d+\.\d+\.\d+", run), "deps must be pinned"
    assert "kpi-venv/bin/python" in run, "PyYAML is not guaranteed on the runner's system python"


# --- N2: coverage ratchet ----------------------------------------------------------------------------

def test_coverage_ratchet_runs_after_the_artifact_is_uploaded(ci):
    nomes = [(p.get("uses") or p.get("run") or "") for p in passos(ci["jobs"]["coverage"])]
    up = next(i for i, n in enumerate(nomes) if n.startswith("actions/upload-artifact@"))
    rat = next(i for i, n in enumerate(nomes) if "scripts/coverage_ratchet.py" in n)
    assert up < rat, "the lcov artifact must survive a ratchet failure"
    run = nomes[rat]
    assert "--lcov lcov.info" in run and "docs/kpi/coverage-floor.json" in run


# --- N1: mutation in CI ---------------------------------------------------------------------------------

def test_mutants_diff_is_advisory_and_diffs_the_whole_push(ci):
    job = ci["jobs"]["mutants-diff"]
    checkout = next(p for p in passos(job) if p.get("uses", "").startswith("actions/checkout@"))
    assert checkout["with"]["fetch-depth"] == 0, "the push base can be several commits back"
    passo = next(p for p in passos(job) if "cargo mutants" in p.get("run", ""))
    assert passo.get("continue-on-error") is True, "advisory until a measured false-red rate promotes it"
    assert "--in-diff" in passo["run"]
    assert "--in-place" in passo["run"], "the e2e tier needs the binaries built in the checkout's own target/"
    assert job["timeout-minutes"] <= 120


def _diff_step(ci) -> dict:
    return next(p for p in passos(ci["jobs"]["mutants-diff"]) if "changes.diff" in p.get("run", "")
                and "cargo mutants" not in p.get("run", ""))


def _repo_with_three_commits(tmp: Path) -> list[str]:
    import subprocess
    def git(*a):
        return subprocess.run(["git", "-C", str(tmp), *a], capture_output=True, text=True, check=True).stdout.strip()
    git("init", "-q")
    shas = []
    for i in range(3):
        (tmp / f"f{i}.rs").write_text(f"fn f{i}() {{}}\n", encoding="utf-8")
        git("add", "-A")
        git("-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", f"c{i}")
        shas.append(git("rev-parse", "HEAD"))
    return shas


@pytest.mark.parametrize(("event", "before_idx", "expect"), [
    ("push", 0, {"f1.rs", "f2.rs"}),        # a push of two commits mutates BOTH (HEAD~1 saw only f2)
    ("push", None, {"f2.rs"}),              # new branch: all-zero `before` -> HEAD~1, said out loud
    ("pull_request", 0, {"f2.rs"}),         # a PR ignores `before`: HEAD is the merge commit
])
def test_mutants_diff_step_picks_the_base_of_the_change(ci, tmp_path, event, before_idx, expect):
    import os
    import subprocess
    shas = _repo_with_three_commits(tmp_path)
    before = shas[before_idx] if before_idx is not None else "0" * 40
    env = {**os.environ, "BEFORE": before, "EVENT_NAME": event, "RUNNER_TEMP": str(tmp_path)}
    subprocess.run(["bash", "-c", _diff_step(ci)["run"]], cwd=tmp_path, env=env, check=True, capture_output=True)
    diff = (tmp_path / "changes.diff").read_text(encoding="utf-8")
    assert set(re.findall(r"^\+\+\+ b/(\S+)", diff, flags=re.MULTILINE)) == expect


def test_mutants_diff_states_how_it_gets_promoted(ci):
    texto = (WF / "ci.yml").read_text(encoding="utf-8")
    assert "false-red" in texto and "promot" in texto.lower()


def test_mutants_diff_job_has_no_write_permission(ci):
    job = ci["jobs"]["mutants-diff"]
    assert "permissions" not in job or "write" not in job["permissions"].values()


# --- N3: scheduled.yml -----------------------------------------------------------------------------------

def test_scheduled_runs_monday_09_utc_and_on_demand(sched):
    assert sched["on"]["schedule"] == [{"cron": "0 9 * * 1"}]
    assert "workflow_dispatch" in sched["on"], "the first run calibrates the shard count: it must be triggerable by hand"


def test_scheduled_is_least_privilege(sched):
    assert sched["permissions"] == {"contents": "read"}
    for jid, j in sched["jobs"].items():
        assert "write" not in (j.get("permissions") or {}).values(), jid


def test_scheduled_has_the_advisories_and_the_full_mutation_scan(sched):
    assert set(sched["jobs"]) == {"advisories", "mutants-full"}
    assert "cargo deny check advisories" in comandos(sched["jobs"]["advisories"])
    assert (ROOT / "deny.toml").is_file(), "cargo-deny needs the repo policy"


def test_scheduled_mutation_scan_shards_agree_with_the_matrix(sched):
    job = sched["jobs"]["mutants-full"]
    shards = job["strategy"]["matrix"]["shard"]
    assert shards == list(range(len(shards))) and len(shards) >= 2
    assert job["strategy"]["fail-fast"] is False, "one slow shard must not cancel the others"
    passo = next(p for p in passos(job) if "cargo mutants" in p.get("run", ""))
    assert f"--shard ${{{{ matrix.shard }}}}/{len(shards)}" in passo["run"], "k/n in the command must use the matrix size"
    assert "--in-place" in passo["run"]
    assert job["timeout-minutes"] <= 360, "GitHub-hosted jobs are killed at 6 h"


def test_scheduled_mutation_report_is_uploaded_even_when_the_scan_fails_or_finds_survivors(sched):
    job = sched["jobs"]["mutants-full"]
    up = next(p for p in passos(job) if p.get("uses", "").startswith("actions/upload-artifact@"))
    assert up["if"] == "always()"
    assert "mutants.out" in up["with"]["path"] and "${{ matrix.shard }}" in up["with"]["name"]


def test_scheduled_mutation_scan_reports_survivors_without_failing_the_job_but_fails_on_a_broken_baseline(sched):
    run = next(p for p in passos(sched["jobs"]["mutants-full"]) if "cargo mutants" in p.get("run", ""))["run"]
    assert "rc=$?" in run
    aceitos = re.search(r"^\s*(\d+(?:\|\d+)*)\) exit 0 ;;", run, re.MULTILINE)
    assert aceitos and set(aceitos.group(1).split("|")) == {"0", "2", "3", "124"}, "missed (2), timeouts (3), budget (124) and success are reportable; nothing else"
    assert "timeout" in run and "--shuffle" in run, "a budget cut must drop a RANDOM subset, not always the same tail"
    assert "exit \"$rc\"" in run or "exit $rc" in run, "any other exit (broken baseline, usage error) stays red"
