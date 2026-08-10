"""E2E proof for the capability portfolio and everything wired to it.

Phase 6 of the cross-audit. Every assertion runs a real command against the
INSTALLED binary and checks its observable output — nothing here is mocked, and
nothing passes on assertion alone. Exit 0 iff every check passes.

Usage:
    python3 docs/audits/prove_portfolio_e2e.py [--json]
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

TIMEOUT_S = 120
#: Marker used to detect shell injection; harmless if it ever executes.
INJECTION_MARKER = "MARCADOR" + "_INJECAO"


@dataclass
class Report:
    """Accumulated check results."""

    checks: list[dict] = field(default_factory=list)

    def record(self, name: str, ok: bool, detail: str = "") -> bool:
        """Record one check and echo it."""
        self.checks.append({"check": name, "ok": ok, "detail": detail[:300]})
        print(f"  {'PASS' if ok else 'FAIL'}  {name}" + (f"\n        {detail[:220]}" if detail else ""))
        return ok

    @property
    def failed(self) -> list[dict]:
        """Checks that did not pass."""
        return [c for c in self.checks if not c["ok"]]


def run(cmd: list[str], env: dict | None = None, stdin: str | None = None) -> subprocess.CompletedProcess:
    """Run a command with a timeout, capturing text output."""
    merged = {**os.environ, **(env or {})}
    return subprocess.run(
        cmd, capture_output=True, text=True, timeout=TIMEOUT_S, env=merged, input=stdin, check=False
    )


def touring() -> str:
    """Absolute path of the installed binary under test."""
    found = shutil.which("touring")
    if not found:
        raise SystemExit("touring not on PATH — deploy first")
    return found


def prove_portfolio_core(rep: Report, scratch: Path) -> None:
    """Mine an isolated corpus and prove the three-section contract holds."""
    print("\n── portfólio: núcleo e contrato ──")
    corpus = scratch / "corpus"
    (corpus / "scripts").mkdir(parents=True, exist_ok=True)
    (corpus / "SKILL.md").write_text(
        "---\nname: pdfkit\ndescription: Toolkit for generating professional PDF documents from templates.\n---\n",
        encoding="utf-8",
    )
    (corpus / "scripts" / "render_map.py").write_text(
        '"""Generate the module dependency map as an SVG diagram for review."""\n', encoding="utf-8"
    )
    (corpus / "scripts" / "no_prose.py").write_text("import sys\n", encoding="utf-8")
    env = {"TOURING_PORTFOLIO_DIR": str(scratch / "pf")}

    r = run([touring(), "portfolio", "refresh", "--root", str(corpus)], env)
    rep.record("refresh exits 0", r.returncode == 0, r.stderr.strip())
    rep.record("refresh reports a count", "artefatos" in r.stdout, r.stdout.strip()[:120])

    r = run([touring(), "portfolio", "gerar um mapa", "-j"], env)
    ok = r.returncode == 0
    ans = json.loads(r.stdout) if ok and r.stdout.strip() else {}
    rep.record("query exits 0", ok, r.stderr.strip())
    for section in ("prior_art", "gaps", "external", "verdict_required", "corpus_size"):
        rep.record(f"answer carries `{section}` (E3)", section in ans)
    rep.record(
        "verdict is demanded, all four options",
        sorted(ans.get("verdict_required", [])) == ["create_new", "extend", "reuse", "supersede"],
        str(ans.get("verdict_required")),
    )
    names = [h["entry"]["name"] for h in ans.get("prior_art", [])]
    rep.record("pt intent finds the en artifact (E7)", "render_map" in names, str(names))

    # Bundle inheritance — the pdf-anthropic case.
    r = run([touring(), "portfolio", "gerar PDF profissional", "-j"], env)
    ans = json.loads(r.stdout) if r.stdout.strip() else {}
    inherited = [h["entry"] for h in ans.get("prior_art", []) if h["entry"].get("purpose_inherited")]
    rep.record(
        "prose-less script rescued by bundle inheritance",
        any(e["name"] == "no_prose" for e in inherited),
        str([e["name"] for e in inherited]),
    )

    # Honest emptiness.
    r = run([touring(), "portfolio", "treinar rede neural convolucional", "-j"], env)
    ans = json.loads(r.stdout) if r.stdout.strip() else {}
    rep.record("unknown intent returns empty prior_art", ans.get("prior_art") == [])
    rep.record(
        "…and SAYS so, with the corpus size (E4)",
        any("nenhum artefato conhecido" in g for g in ans.get("gaps", []))
        and isinstance(ans.get("corpus_size"), int),
        str(ans.get("gaps"))[:160],
    )
    rep.record(
        "external lens names a subject, never a placeholder (E8)",
        bool(ans.get("external"))
        and "<" not in json.dumps(ans["external"]),
        json.dumps(ans.get("external", []))[:160],
    )


def prove_verdict_loop(rep: Report, scratch: Path) -> None:
    """A recorded verdict must come back as evidence after a refresh."""
    print("\n── veredito: o laço que faz compor ──")
    corpus = scratch / "corpus"
    env = {"TOURING_PORTFOLIO_DIR": str(scratch / "pf")}
    artifact_id = "script:" + str(corpus / "scripts" / "render_map.py").replace(str(Path.home()), "~")

    r = run(
        [
            touring(), "portfolio", "verdict", "gerar um mapa",
            "--choice", "extend", "--why", "cobre SVG mas nao cobre PNG",
            "--artifact", artifact_id, "--reward", "0.7",
        ],
        env,
    )
    rep.record("verdict exits 0", r.returncode == 0, r.stderr.strip())

    r = run([touring(), "portfolio", "history", "-j"], env)
    hist = json.loads(r.stdout) if r.stdout.strip() else {}
    rep.record("history lists the verdict", hist.get("records", 0) >= 1, str(hist.get("records")))

    run([touring(), "portfolio", "refresh", "--root", str(corpus)], env)
    r = run([touring(), "portfolio", "gerar um mapa", "-j"], env)
    ans = json.loads(r.stdout) if r.stdout.strip() else {}
    with_verdict = [
        h["entry"] for h in ans.get("prior_art", []) if h["entry"]["evidence"].get("prior_verdict")
    ]
    rep.record(
        "verdict returns as evidence after refresh (the pheromone)",
        any(e["evidence"]["prior_verdict"] == "extend" for e in with_verdict),
        str([(e["name"], e["evidence"]["prior_verdict"]) for e in with_verdict]),
    )


def prove_inspect(rep: Report, scratch: Path) -> None:
    """`inspect` must explain what the miner extracted, including absences."""
    print("\n── inspect: a superfície de depuração ──")
    env = {"TOURING_PORTFOLIO_DIR": str(scratch / "pf")}
    target = scratch / "corpus" / "scripts" / "render_map.py"
    r = run([touring(), "portfolio", "inspect", str(target), "-j"], env)
    out = json.loads(r.stdout) if r.stdout.strip() else {}
    rep.record("inspect exits 0", r.returncode == 0, r.stderr.strip())
    rep.record("names the extractor that fired", bool(out.get("purpose_source")), str(out.get("purpose_source")))
    rep.record("returns the mined purpose", "dependency map" in (out.get("artifact_purpose") or ""))

    empty = scratch / "corpus" / "scripts" / "no_prose.py"
    r = run([touring(), "portfolio", "inspect", str(empty), "-j"], env)
    out = json.loads(r.stdout) if r.stdout.strip() else {}
    rep.record("absence is explicit, not a crash (E4)", out.get("artifact_purpose") is None, str(out))


def prove_find_code(rep: Report) -> None:
    """`find-code` must never fabricate and must declare its corpus."""
    print("\n── find-code: nunca fabricar ──")
    r = run([touring(), "find-code", "search", "generate professional PDF"])
    rep.record("find-code exits 0", r.returncode == 0, r.stderr.strip()[:160])
    body = r.stdout
    rep.record("no fabricated doc_kw_* id (E5)", "doc_kw_" not in body, body[:160])
    rep.record("no fabricated doc_sem_* id (E5)", "doc_sem_" not in body, body[:160])
    try:
        parsed = json.loads(body)
    except json.JSONDecodeError:
        parsed = {}
    rep.record("declares which backends answered (E5b)", "backends" in parsed, str(parsed.get("backends")))
    # Two different queries must not produce identical results.
    other = run([touring(), "find-code", "search", "parse a toml configuration"]).stdout
    rep.record(
        "different queries give different answers",
        body != other or (parsed.get("results") == [] and json.loads(other or "{}").get("results") == []),
        "identical non-empty output would mean a fixed corpus",
    )


def prove_hook(rep: Report) -> None:
    """The PreToolUse Write hook must derive real intents and skip boilerplate."""
    print("\n── hook: enriquecimento no Write ──")
    hook = shutil.which("touring-hook")
    if not hook:
        rep.record("touring-hook on PATH", False, "UNVERIFIED — binary absent")
        return

    def context_for(path: str, content: str) -> str:
        payload = json.dumps(
            {"tool_name": "Write", "tool_input": {"file_path": path, "content": content},
             "cwd": str(Path.cwd())}
        )
        out = run([hook, "cli-suggest"], stdin=payload)
        try:
            return json.loads(out.stdout)["hookSpecificOutput"]["additionalContext"]
        except (json.JSONDecodeError, KeyError):
            return ""

    # `cli-suggest` suppresses a repeated signature for 300s. A proof that
    # depends on cache state is flaky by construction, so every run uses a path
    # that has never been seen.
    uniq = os.getpid()
    ctx = context_for(
        f"/tmp/x/gerar_pdf_relatorio_{uniq}.py",
        '"""Gera um relatorio PDF profissional a partir de markdown."""\nimport sys\n',
    )
    rep.record("hook emits context for a new file", bool(ctx), ctx[:120])
    # E8 applies to the PORTFOLIO nudge. The create-pipeline MUST line carries a
    # legitimate `<intent>` placeholder: the Write-tool invocation has no
    # derivable intent, so scoping the assertion is correctness, not leniency.
    portfolio_lines = [l for l in ctx.splitlines() if "touring portfolio" in l]
    rep.record(
        "intent is derived from the content, not a placeholder (E8)",
        bool(portfolio_lines)
        and "relatorio PDF profissional" in portfolio_lines[0]
        and "<" not in portfolio_lines[0],
        portfolio_lines[0][:170] if portfolio_lines else "(no portfolio nudge)",
    )
    rep.record(
        "the payload demands a verdict (E3)",
        "reuse | extend | supersede | create_new" in ctx,
    )

    banner = context_for(
        f"/tmp/x/data_loader_{uniq}.py",
        "# Copyright 2026 Acme Incorporated. All rights reserved worldwide.\nimport sys\n",
    )
    rep.record(
        "licence banner never becomes the intent (E8)",
        "Copyright" not in banner and "rights reserved" not in banner,
        [l for l in banner.splitlines() if "portfolio" in l][:1],
    )


def prove_adw_injection_is_inert(rep: Report) -> None:
    """The hardened ADW command shape must not let a value reach the shell."""
    print("\n── ADW: injeção de template neutralizada ──")
    payload = "mapa'; echo " + INJECTION_MARKER + "; echo '"
    old = ["bash", "-c", f"echo ARG: '{payload}' >/dev/null; true"]
    new = ["bash", "-c", 'echo ARG: "$1" >/dev/null; true', "--", payload]
    out_old = run(old).stdout
    out_new = run(new).stdout
    rep.record("the OLD shape did inject (control)", INJECTION_MARKER in out_old)
    rep.record("the HARDENED shape does not inject", INJECTION_MARKER not in out_new, out_new[:120])
    echo_back = run(["bash", "-c", 'printf "%s" "$1"', "--", payload]).stdout
    rep.record("…and the value still arrives intact", echo_back == payload)


def prove_exit_zero_invariant(rep: Report, scratch: Path) -> None:
    """Every portfolio entry point exits 0, including on hostile input."""
    print("\n── invariante: exit 0 sempre ──")
    env = {"TOURING_PORTFOLIO_DIR": str(scratch / "pf")}
    cases: list[tuple[str, list[str]]] = [
        ("no arguments", ["portfolio"]),
        ("empty intent", ["portfolio", ""]),
        ("only stopwords", ["portfolio", "de o um para"]),
        ("very long intent", ["portfolio", "gerar " * 400]),
        ("unicode + emoji", ["portfolio", "gerar gráfico ✨ 日本語"]),
        ("shell metacharacters", ["portfolio", "mapa; echo " + INJECTION_MARKER]),
        ("unknown subcommand", ["portfolio", "refreshhh"]),
        ("verdict without why", ["portfolio", "verdict", "x", "--choice", "reuse"]),
        ("verdict bad choice", ["portfolio", "verdict", "x", "--choice", "talvez", "--why", "y"]),
        ("inspect missing file", ["portfolio", "inspect", "/nao/existe.py"]),
        ("status", ["portfolio", "status"]),
        ("history", ["portfolio", "history"]),
    ]
    for name, args in cases:
        r = run([touring(), *args], env)
        # The marker EXECUTING would print it on a line of its own; the intent
        # being echoed back embeds it in a sentence. Only the former is injection.
        executed = any(line.strip() == INJECTION_MARKER for line in (r.stdout + r.stderr).splitlines())
        ok = r.returncode == 0 and not executed
        rep.record(f"exit 0 · {name}", ok, f"rc={r.returncode} executed={executed} {r.stderr.strip()[:80]}")


def main() -> int:
    """Run every proof; exit non-zero if any check failed."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="emit the report as JSON")
    args = parser.parse_args()

    rep = Report()
    with tempfile.TemporaryDirectory(prefix="portfolio-e2e-") as tmp:
        scratch = Path(tmp)
        prove_portfolio_core(rep, scratch)
        prove_verdict_loop(rep, scratch)
        prove_inspect(rep, scratch)
        prove_find_code(rep)
        prove_hook(rep)
        prove_adw_injection_is_inert(rep)
        prove_exit_zero_invariant(rep, scratch)

    total, bad = len(rep.checks), len(rep.failed)
    if args.json:
        print(json.dumps({"total": total, "failed": bad, "checks": rep.checks}, ensure_ascii=False))
    print(f"\n{'=' * 60}\n{total - bad}/{total} checks passaram")
    for c in rep.failed:
        print(f"  FALHOU: {c['check']} — {c['detail']}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
