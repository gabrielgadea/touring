#!/usr/bin/env python3
"""Guard estrutural: teste que roda o CLI num TempDir DEVE marcar o projeto.

Origem (medida em 2026-08-25, ao destravar o gate de convergência): um
`TempDir` nu sob `/tmp` não carrega marcador de projeto, então o walk-up de
`TouringConfig::normalize_project_root` chega ao topo e cai no fallback
`$HOME`. Dois arquivos de teste (`e2e_diary.rs`, `cli_wave6_e2e.rs`) gravavam
no diário REAL da máquina, liam as entradas uns dos outros e acumulavam entre
execuções — `esperado 1, obtido 3` numa rodada e `6` na seguinte. A suíte era
não-determinística E poluía o diário do operador.

O remédio (`test_project()`, que cria `.touring/` no tmpdir) foi aplicado nos
dois sítios. Este guard existe porque corrigir os sítios conhecidos não impede
o terceiro: é a família `definer-module-cinco-sitios` — a correção pontual
mascara o defeito, o guard estrutural o mantém corrigido.

Regra verificada: todo arquivo em `crates/*/tests/*.rs` que executa o binário
`touring` com `current_dir(<tmp>.path())` precisa criar um marcador de projeto
(`.touring` ou `.git`) no mesmo tmpdir.

Uso:
    python3 scripts/test_cli_tests_isolate_project_root.py        # exit 1 se violado
    python3 scripts/test_cli_tests_isolate_project_root.py --json
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# Chama o CLI dentro de um diretório temporário do próprio teste.
RUNS_CLI_IN_TMPDIR = re.compile(r"current_dir\(\s*(\w+)\.path\(\)\s*\)")
# Cria um marcador que `has_project_marker` reconhece.
CREATES_MARKER = re.compile(r"""join\(\s*["']\.(?:touring|git)["']\s*\)""")


def offenders() -> list[dict[str, object]]:
    """Arquivos de teste que rodam o CLI num tmpdir sem marcar o projeto."""
    found: list[dict[str, object]] = []
    for path in sorted(REPO.glob("crates/*/tests/*.rs")):
        text = path.read_text(encoding="utf-8", errors="ignore")
        runs = RUNS_CLI_IN_TMPDIR.findall(text)
        if not runs:
            continue
        if CREATES_MARKER.search(text):
            continue
        found.append(
            {
                "file": str(path.relative_to(REPO)),
                "cli_call_sites": len(runs),
                "remedy": (
                    "criar um helper test_project() que faz "
                    "std::fs::create_dir_all(tmp.path().join(\".touring\")) "
                    "e usá-lo no lugar de TempDir::new()"
                ),
            }
        )
    return found


def main() -> int:
    """Executa o guard; exit 0 limpo, 1 com violação."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="saída JSON")
    args = parser.parse_args()

    bad = offenders()
    if args.json:
        json.dump({"ok": not bad, "offenders": bad}, sys.stdout, indent=2)
        print()
    elif bad:
        print("VIOLAÇÃO — teste roda o CLI num TempDir sem marcador de projeto:")
        for entry in bad:
            print(f"  {entry['file']} ({entry['cli_call_sites']} chamadas)")
            print(f"    remédio: {entry['remedy']}")
        print(
            "\nSem o marcador, normalize_project_root cai em $HOME e o teste "
            "escreve no estado REAL da máquina."
        )
    else:
        print("OK — todo teste que roda o CLI num tmpdir marca o projeto")
    return 1 if bad else 0


# --------------------------------------------------------------------------
# Entrada pytest. Sem ela o arquivo é coletado, não acha nenhuma função
# `test_*` e reporta "no tests ran" — verde que não afirmou nada. Descoberto
# no cross-audit de 26/08/2026, ao registrar este guard no CI: registrar um
# passo que não verifica é pior que não registrar, porque parece cobertura.
# --------------------------------------------------------------------------


def test_cli_tests_mark_their_project_root():
    """Todo teste que roda o CLI num tmpdir precisa marcar o projeto."""
    bad = offenders()
    assert not bad, "testes sem marcador de projeto: " + ", ".join(
        str(e["file"]) for e in bad
    )


if __name__ == "__main__":
    sys.exit(main())
