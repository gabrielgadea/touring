#!/usr/bin/env python3
"""D8 cross-guard — the git nudge must match the git EXECUTOR.

The `cli_suggester` nudge tells the model what git costs; `block_git.sh` is
what actually denies the command. Until 2026-08-25 they disagreed outright:
the nudge announced "git is prohibited ... block_git.sh will reject this
command" while the executor (REGRA #11 v2, 23/08/2026) allowed every
read-only and additive form and gated only the DESTRUCTIVE class.

That is the D8 anti-pattern inside the product: *the declared text promised
what the executor does not apply*. The institutional remedy is never good
will — it is a test that reads BOTH sides and fails when they drift, the
same shape as `test_code_mode_sdk_section.py` for the code-mode classes.

Run:  python3 scripts/test_git_nudge_matches_executor.py
Exit: 0 all checks passed · 1 a real drift · 2 could not verify (declared,
      never silently green — an absent executor is "unknown", not "safe").
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
RUST = REPO / "crates" / "touring-cli" / "src" / "cli_suggester.rs"
EXECUTOR = Path.home() / ".claude" / "hooks" / "block_git.sh"

# The verbs the executor gates, as they appear in DESTRUCTIVE_RE / carve-outs.
# Each entry: (label, regex fragment that must exist in the shell predicate).
EXPECTED_IN_EXECUTOR = [
    ("reset --hard/merge/keep", r"reset\\s\+\[\^|;&\]\*--\(hard|merge|keep\)"),
    ("rebase", r"rebase"),
    ("clean --force", r"clean"),
    ("push --force", r"push"),
    ("branch -D", r"branch"),
    ("filter-branch", r"filter-branch"),
    ("filter-repo", r"filter-repo"),
    ("reflog expire", r"reflog"),
    ("gc --prune", r"gc"),
    ("stash", r"stash"),
    ("restore", r"restore"),
    ("checkout", r"checkout"),
    ("switch --discard-changes", r"switch"),
]

failures: list[str] = []
unverified: list[str] = []


COMANDO_DESTRUTIVO = "git push origin --delete feature/x"
COMANDO_BENIGNO = "git push origin main"


def _payload_bash(command: str) -> dict:
    """O payload do PreToolUse Bash — o MESMO formato que os testes do proprio executor usam
    (`client/hooks/tests/test_s25_block_git_delete.py`)."""
    return {
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "session_id": "hooks-contract-nudge",
        "tool_input": {"command": command},
    }


def _decisao(script: Path, command: str, env_extra: dict) -> str | None:
    """A DECISAO do executor para um comando: 'deny'/'block' (nega) ou None (nao nega)."""
    env = dict(os.environ)
    env.update(env_extra)
    r = subprocess.run(
        ["bash", str(script)],
        input=json.dumps(_payload_bash(command)),
        capture_output=True,
        text=True,
        timeout=30,
        env=env,
    )
    try:
        j = json.loads(r.stdout or "{}")
    except json.JSONDecodeError:
        return None
    return j.get("hookSpecificOutput", {}).get("permissionDecision")


def guard_bloqueia_com_o_flag_desligado(script: Path) -> bool:
    """O INVARIANTE, provado COMPORTAMENTALMENTE: com `GIT_GUARD_ENABLED=0` no ambiente, o
    guard AINDA bloqueia o comando destrutivo.

    MEDIDO em 01/11 (review de seguranca, TERCEIRA rodada): um predicado de FONTE — busca de
    token, anchored, ou "nenhuma mencao" — nunca prova o invariante, porque o NOME da variavel
    pode ser construido dinamicamente (`N=GIT"_GUARD_ENABLED"; export $N=0`) e escapar de
    qualquer busca. A prova do invariante e comportamental: o guard tem de NEGAR com o flag
    desligado. E assim o token sai de cena: a sonda nao le o fonte do executor, ela pergunta ao
    executor.

    O comando destrutivo e o mesmo dos testes do proprio executor (S25): `git push origin
    --delete feature/x`, que e negado sem o token ritual.
    """
    return _decisao(script, COMANDO_DESTRUTIVO, {"GIT_GUARD_ENABLED": "0"}) in ("deny", "block")


def check(cond: bool, msg: str) -> None:
    if not cond:
        failures.append(msg)


def main() -> int:
    if not RUST.exists():
        print(f"UNVERIFIED: Rust source absent: {RUST}")
        return 2
    rust = RUST.read_text(encoding="utf-8")

    # --- 1. The nudge must NOT claim the revoked prohibition. ------------
    # Scope the search to the classifier arm, so the explanatory comment that
    # documents the historical defect does not trip its own guard.
    arm = re.search(
        r"// Pattern 6: git(.*?)\n    // Pattern 7:", rust, re.DOTALL
    )
    check(arm is not None, "Pattern 6 (git arm) not found in cli_suggester.rs")
    if arm:
        body = arm.group(1)
        reasons = re.findall(r'reason:\s*"((?:[^"\\]|\\.)*)"', body, re.DOTALL)
        check(bool(reasons), "git arm emits no reason string")
        for r in reasons:
            check(
                "prohibited" not in r and "proibido" not in r.lower(),
                f"git arm still claims a revoked prohibition: {r[:90]!r}",
            )
        check(
            "regra-11-git-destructive" in body,
            "git arm lacks the DESTRUCTIVE cluster (the half with consequence)",
        )
        # The ritual must live in the MUST list, not merely be MENTIONED in
        # the prose. Proven necessary by mutation on 2026-08-26: deleting the
        # token from `must:` left this guard green, because the same string
        # also appears in `reason:`. A verifier that reads a wider span than
        # the contract it checks cannot see the contract break.
        must_block = re.search(r"must:\s*vec!\[(.*?)\n\s*\],", body, re.DOTALL)
        check(must_block is not None, "destructive arm has no parseable must: list")
        if must_block:
            musts = must_block.group(1)
            check(
                "GIT_DESTRUCTIVE_OK=1" in musts,
                "the per-command ritual token is not among the MUST steps "
                "(mentioning it in the reason is not the same as requiring it)",
            )
            check(
                "safety/" in musts,
                "the safety-branch snapshot is not among the MUST steps",
            )
            check(
                "git status --porcelain" in musts,
                "the MEASURE step is not among the MUST steps",
            )

    # --- 2. Rust declared verbs vs the executor's predicate. -------------
    if not EXECUTOR.exists():
        unverified.append(
            f"executor absent ({EXECUTOR}) — cross-check against the shell "
            "predicate could not run"
        )
    else:
        shell = EXECUTOR.read_text(encoding="utf-8")
        check(
            guard_bloqueia_com_o_flag_desligado(EXECUTOR),
            "the guard does not deny a destructive command with GIT_GUARD_ENABLED=0 in the "
            "environment — the flag is an off switch and the guard is not unconditional "
            "(S48); the nudge would promise a gate that may not run",
        )
        check(
            "GIT_DESTRUCTIVE_OK=1" in shell,
            "executor does not honour the per-command token the nudge teaches",
        )
        for label, _frag in EXPECTED_IN_EXECUTOR:
            head = label.split()[0]
            check(
                head in shell,
                f"executor does not gate `{label}` but the nudge declares it",
            )
        # Every verb the Rust list declares must be a verb the shell knows.
        declared = re.search(
            r"GIT_DESTRUCTIVE_VERBS: &\[&str\] = &\[(.*?)\];", rust, re.DOTALL
        )
        check(declared is not None, "GIT_DESTRUCTIVE_VERBS not found in Rust")
        if declared:
            verbs = re.findall(r'"([^"]+)"', declared.group(1))
            check(len(verbs) >= 10, f"suspiciously few declared verbs: {verbs}")
            for v in verbs:
                head = v.split()[0]
                check(
                    head in shell,
                    f"Rust declares `{v}` but the executor never mentions "
                    f"`{head}` — declaration without enforcement (D8)",
                )

    for u in unverified:
        print(f"UNVERIFIED: {u}")
    for f in failures:
        print(f"FAIL: {f}")
    if failures:
        print(f"\n{len(failures)} drift(s) between the git nudge and its executor.")
        return 1
    if unverified:
        print("\nchecks on the Rust side passed; executor side UNVERIFIED.")
        return 2
    print("OK: the git nudge and block_git.sh agree (REGRA #11 v2).")
    return 0


# --------------------------------------------------------------------------
# Entradas pytest. Sem elas o arquivo é coletado, não encontra nenhuma função
# `test_*` e reporta "no tests ran" — verde que não verificou nada. É a mesma
# família de "guard que não cobre o artefato em uso": o guard existe, o CI o
# executa, e ninguém percebe que ele não afirmou coisa alguma.
# --------------------------------------------------------------------------


def test_git_nudge_matches_its_executor():
    """O nudge do git e `block_git.sh` não podem divergir (D8)."""
    failures.clear()
    unverified.clear()
    rc = main()
    assert rc != 1, "drift entre o nudge do git e o executor: " + "; ".join(failures)


def test_o_flag_desligado_nao_desliga_o_guard():
    """O proprio meta-teste, testado COMPORTAMENTALMENTE — MEDIDO em 01/11 (review de seguranca,
    terceira rodada).

    O predicado por linha (qualquer um) nao distingue uma atribuicao na RAIZ de uma aninhada em
    funcao/subshell/`case`, e uma busca de token nao ve um nome construido dinamicamente
    (`N=GIT"_GUARD_ENABLED"`). A prova do invariante e comportamental: com o flag em 0 o guard
    tem de NEGAR.

    Os tres casos sao o invariante e DOIS controles: se a sonda negasse tudo, ela passaria
    sozinha — o comando benigno e o que prova que ela ainda distingue.
    """
    if not EXECUTOR.exists():
        return  # ausente = "nao verificado", e o main() reporta o unverified; nunca verde
    d_flag0 = _decisao(EXECUTOR, COMANDO_DESTRUTIVO, {"GIT_GUARD_ENABLED": "0"})
    assert d_flag0 in ("deny", "block"), (
        f"com GIT_GUARD_ENABLED=0 o executor decidiu {d_flag0!r} em vez de negar — "
        "o flag virou off switch"
    )
    d_flag1 = _decisao(EXECUTOR, COMANDO_DESTRUTIVO, {"GIT_GUARD_ENABLED": "1"})
    assert d_flag1 in ("deny", "block"), f"com o flag em 1 o executor decidiu {d_flag1!r}"
    d_benigno = _decisao(EXECUTOR, COMANDO_BENIGNO, {"GIT_GUARD_ENABLED": "0"})
    assert d_benigno not in ("deny", "block"), (
        f"a sonda nega ate o comando benigno ({d_benigno!r}) — ela nao distingue, "
        "entao o caso do flag nao prova nada"
    )


def test_guard_detects_a_revoked_prohibition_claim():
    """O guard precisa REPROVAR se a afirmação revogada voltar.

    Sem esta prova por mutação, o teste acima poderia ser uma constante que
    passa sempre — e um guard que nunca reprova não guarda nada.
    """
    original = RUST.read_text(encoding="utf-8")
    # Muta o `reason` do arm DESTRUTIVO — o único que sobrou depois que o arm
    # `safe` foi removido em 26/08 (tinha confiança 0.55, abaixo do gate
    # conformal de 0.7, e nunca era emitido). Um teste de mutação que aponta
    # para texto que não existe mais falha por invalidez, não por defeito.
    mutated = original.replace(
        "reason: \"REGRA #11 v2 — git é PERMITIDO, mas esta operação é da classe \\",
        "reason: \"REGRA #11 — git is prohibited in TACO. \\",
        1,
    )
    assert mutated != original, "a mutação não aplicou — teste inválido"
    try:
        RUST.write_text(mutated, encoding="utf-8")
        failures.clear()
        unverified.clear()
        assert main() == 1, "o guard não detectou a proibição revogada"
    finally:
        RUST.write_text(original, encoding="utf-8")
        failures.clear()
        unverified.clear()
        assert main() != 1, "o arquivo não voltou ao estado íntegro"


if __name__ == "__main__":
    sys.exit(main())
