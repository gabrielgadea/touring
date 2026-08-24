"""S-0.1 — the tree `client/omarchy/README.md` declares must exist, and what must
run must be executable.

The README is the contract the Omarchy machine is provisioned from; a file it
promises and the repo does not carry is a silent gap that would only surface
after the wipe (P1), where it costs a reboot to discover. A missing execute bit
is the 23/07/2026 lesson (`+x` lost → every UserPromptSubmit failed with
Permission denied) applied before the scripts leave this machine.
"""

from __future__ import annotations

import os
import re
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]

# Declared in README.md "Mapa da árvore". Executables carry the bit; data files do not.
EXECUTABLE = [
    "bin/migration_kit.sh",
    "bin/kit_check.sh",
    "bin/settings_stage.py",
    "bin/hooks_runnable.py",
    *[f"bin/validate_p{n}.sh" for n in range(9)],
    "bin/validate_all.sh",
    "bin/wipe_target.sh",
    "bin/omarchy-skill-run",
    "bin/routines_gen.py",
    "bin/cc_build.py",
    "bin/cidata_build.sh",
    "bin/cidata_iso.py",
    "bin/cidata_gen.py",
    "bin/herdr_branch.sh",
    "bin/bootorder_guard.sh",
    "hooks/post-boot.d/05-count-boot",
    "hooks/post-boot.d/20-touring-daemon",
    "hooks/post-update.d/30-touring-doctor",
    "hooks/post-update.d/35-hooks-runnable",
    "hooks/post-update.d/40-limine-rescan",
    "hooks/post-update.d/45-touring-ci-fire",
    "vendor/omarchy-plugin-validate",
    "tests/bin/herdr",
]

DATA = [
    "README.md",
    "skills-deck/manifest.json",
    "skills-deck/BarWidget.qml",
    "skills-deck/Panel.qml",
    "skills-deck/deck.json",
    "skills-deck/README.md",
    "extensions/omarchy-menu.jsonc",
    "adw/fragments/herdr-fanout.toml",
    "adw/herdr-fanout-demo.toml",
    "cc/cc.json.example",
    "cc/cc.service",
    "cc/cc.timer",
    "cc/cc-serve.service",
    "systemd/herdr-server.service",
    "systemd/omarchy-bootorder-guard.service",
    "cidata/user_configuration.template.json",
    "cidata/README.md",
    "hypr/bindings.skills.lua",
    "routines.example.toml",
]


@pytest.mark.parametrize("rel", EXECUTABLE + DATA)
def test_every_declared_file_exists(rel: str) -> None:
    assert (ROOT / rel).is_file(), f"README promises {rel} and it is not there"


@pytest.mark.parametrize("rel", EXECUTABLE)
def test_every_declared_file_is_executable(rel: str) -> None:
    path = ROOT / rel
    if not path.is_file():
        pytest.skip("existence is asserted by the other test")
    assert os.access(path, os.X_OK), f"{rel} has no execute bit"


def test_no_symlink_inside_the_plugin_folder() -> None:
    """`omarchy-plugin-validate` refuses any symlink under the plugin dir, and a
    symlink that survives into `~/.config/omarchy/plugins/` could point the
    trusted folder back at arbitrary files. Assert it here, where the fix is a
    one-line `cp -L`, not on the Omarchy shell's console."""
    links = [p for p in (ROOT / "skills-deck").rglob("*") if p.is_symlink()]
    assert links == [], f"symlinks inside skills-deck: {links}"


def test_bash_scripts_declare_strict_mode() -> None:
    """`set -euo pipefail` is the README contract for every bash script; a hook
    that silently continues past a failed command is exactly the silent partial
    the validators exist to prevent."""
    offenders = []
    for rel in EXECUTABLE:
        if rel.startswith("vendor/"):
            continue  # upstream Omarchy script, kept byte-identical on purpose
        path = ROOT / rel
        if not path.is_file():
            continue
        # Header comments run long here by design (they explain WHY); the
        # strict-mode line must still come before the first command, so the
        # window is the prologue, not a fixed 12 lines.
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        if not lines or "bash" not in lines[0]:
            continue
        prologue = []
        for line in lines[1:]:
            if line.strip() and not line.lstrip().startswith("#"):
                prologue.append(line)
                break
            prologue.append(line)
        if not any("set -euo pipefail" in line for line in prologue):
            offenders.append(rel)
    assert offenders == [], f"bash scripts whose first command is not `set -euo pipefail`: {offenders}"


def test_no_pipefail_append_fallback_pattern() -> None:
    """Class guard (3 occurrences on 23/08 alone: kit.json version, claude
    version, btrfs count): under `set -o pipefail`, `producer | head/wc ... ||
    printf fallback` APPENDS the fallback to output the pipe already produced —
    "30.4.13unknown", "0\\n0" — because the producer dies of SIGPIPE or rc≠0
    while stdout was already captured. The fix is capture-then-trim
    (`v="$(cmd)"; ${v%%$'\\n'*}`) or a tool that cannot fail. Forbid the shape."""
    offender = re.compile(r"\|\s*(head|wc|tail)\b[^\n|]*\)?\"?\s*\|\|\s*(printf|echo)")
    hits = []
    for sh in ROOT.glob("bin/*.sh"):
        for i, line in enumerate(sh.read_text(encoding="utf-8").splitlines(), 1):
            if offender.search(line):
                hits.append(f"{sh.name}:{i}: {line.strip()}")
    assert hits == [], "pipefail+pipe+fallback appends instead of replacing:\n" + "\n".join(hits)


def test_no_grep_count_or_printf_fallback() -> None:
    """Class guard, irma da anterior mas com OUTRO mecanismo (4 ocorrencias em
    validate_p0/p6/p7/p8 em 23/08): `grep -c` IMPRIME a contagem ("0") e SAI 1
    quando nao ha match — nao e' SIGPIPE, e' o contrato do proprio grep. Logo
    `x=$(... grep -c PAT || printf '0')` ANEXA um segundo zero e x vira "0\\n0",
    que estoura como "arithmetic syntax error" no `[[ -eq/-ge ]]` seguinte: o
    check nao reprova, ele QUEBRA. A forma correta separa captura de fallback:
    `x=$(... grep -c PAT) || x=0`. Proibir a forma antiga.

    Nao vale para `stat`/`python3`/`curl` + `|| printf`, que nao imprimem nada
    ao falhar — por isso o padrao exige `grep -c` explicitamente.
    """
    offender = re.compile(r"grep\s+-[a-zA-Z]*c[a-zA-Z]*\s[^\n|]*\|\|\s*(printf|echo)")
    hits = []
    for sh in ROOT.glob("bin/*.sh"):
        for i, line in enumerate(sh.read_text(encoding="utf-8").splitlines(), 1):
            if offender.search(line):
                hits.append(f"{sh.name}:{i}: {line.strip()}")
    assert hits == [], (
        "`grep -c ... || printf` appends a second count instead of replacing:\n"
        + "\n".join(hits)
    )
