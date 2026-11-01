"""Mutation proof for the compaction recorder: each mutant must turn the suite red.

Copies the loop-engineering hooks directory into a TemporaryDirectory (never mutates the live
tree, which every running session's hooks import), applies one textual mutation, runs
test_compaction_ledger.py, and reports killed/survived. Exit 0 only if every mutant is killed
and the unmutated copy is green (positive control).
"""

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HOOKS = Path.home() / ".claude/skills/loop-engineering/scripts/hooks"
MUTANTS = {
    "segment_ignora_limite_fresco": ("compaction_ledger.py",
                                     'if any(r.get("type") == "assistant" for r in recs[last + 1:]):',
                                     "if True:"),
    "juiz_sem_nome_base": ("compaction_ledger.py", "    tail = Path(token).name\n", "    tail = ''\n"),
    "dedupe_desligado": ("compaction_ledger.py", 'if previous.get("key") != key:', "if True:"),
    "resume_nao_injeta_lacunas": ("loop_resume.py", "return compaction_ledger.gaps_context(payload)",
                                  "return None"),
    "resume_volta_cedo_sem_marcador": ("loop_resume.py", "gaps = _compaction_step(event, payload)",
                                       "gaps = None"),
}


def run(tree: Path) -> int:
    return subprocess.run([sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider",
                           str(tree / "test_compaction_ledger.py")], capture_output=True, cwd=tree).returncode


def main() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        base = Path(tmp) / "hooks"
        shutil.copytree(HOOKS, base, ignore=shutil.ignore_patterns("__pycache__"))
        control = run(base)
        print(f"controle (sem mutação): rc={control} {'VERDE' if control == 0 else 'VERMELHO — inválido'}")
        killed = 0
        for name, (fname, old, new) in MUTANTS.items():
            tree = Path(tmp) / name
            shutil.copytree(base, tree)
            src = (tree / fname).read_text()
            assert old in src, f"mutação {name}: trecho não encontrado (o teste da mutação estaria vazio)"
            (tree / fname).write_text(src.replace(old, new, 1))
            rc = run(tree)
            killed += rc != 0
            print(f"  {name:<34} rc={rc} {'MORTA' if rc else 'SOBREVIVEU'}")
    print(f"mutantes mortos: {killed}/{len(MUTANTS)}")
    return 0 if control == 0 and killed == len(MUTANTS) else 1


if __name__ == "__main__":
    sys.exit(main())
