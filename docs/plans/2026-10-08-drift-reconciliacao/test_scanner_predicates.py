#!/usr/bin/env python3
"""Testes dos predicados corrigidos do drift_semantic_scan.py (reconciliação 08/10/2026).

Cada mudança de predicado (disposição b do ledger) tem controle POSITIVO (o caso
que DEVE ser flagged continua flagged) e NEGATIVO (o falso positivo NÃO flaga).

Coletável por pytest (funções test_* sem parâmetros) e executável standalone:
  python3 test_scanner_predicates.py   # exit 0 se tudo passa
"""
import importlib.util
import os
import sys
from pathlib import Path

SCANNER = Path(__file__).resolve().parents[3] / "scripts" / "drift_semantic_scan.py"

_spec = importlib.util.spec_from_file_location("drift_semantic_scan", SCANNER)
if _spec is None or _spec.loader is None:
    raise RuntimeError(f"não consegui carregar {SCANNER}")
dss = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(dss)

GT_CLI = dss.gt_actual_cli_commands()
GT_VARS = dss.gt_actual_envvars()


def test_c4_negativo_prosa() -> None:
    """NEGATIVO: prosa portuguesa ('o touring deixa de ser um CLI') NÃO é citação."""
    out = dss.scan_c4_cli({"doc.md": "O touring deixa de ser um CLI e vira ambiente."}, GT_CLI)
    assert "deixa" not in out["potential_stale"], out["potential_stale"]


def test_c4_negativo_path_diretorio() -> None:
    """NEGATIVO: '.../projects/touring timeout 900' é PATH, não subcomando."""
    doc = "```bash\nTOURING_PROJECT_ROOT=/home/gabrielgadea/projects/touring timeout 900 cargo test\n```\n"
    out = dss.scan_c4_cli({"doc.md": doc}, GT_CLI)
    assert "timeout" not in out["potential_stale"], out["potential_stale"]


def test_c4_positivo_fence() -> None:
    """POSITIVO: comando inventado em code fence DEVE ser flagged."""
    doc = "```bash\ntouring subcomando-inventado-xyz --foo\n```\n"
    out = dss.scan_c4_cli({"doc.md": doc}, GT_CLI)
    assert "subcomando-inventado-xyz" in out["potential_stale"]


def test_c4_positivo_inline_code() -> None:
    """POSITIVO: comando inventado em inline code DEVE ser flagged."""
    out = dss.scan_c4_cli({"doc.md": "Rode `touring foo-xyz-qq` agora.\n"}, GT_CLI)
    assert "foo-xyz-qq" in out["potential_stale"]


def test_c4_positivo_shell_line() -> None:
    """POSITIVO: linha de shell fora de fence ('$ touring bar-xyz') DEVE contar."""
    out = dss.scan_c4_cli({"doc.md": "$ touring bar-xyz-qq --flag\n"}, GT_CLI)
    assert "bar-xyz-qq" in out["potential_stale"]


def test_c4_negativo_comando_real() -> None:
    """NEGATIVO: comando que EXISTE no help não é stale mesmo em fence."""
    real = sorted(GT_CLI)[0] if GT_CLI else None
    assert real, "touring --help vazio — ground truth indisponível"
    doc = f"```bash\ntouring {real}\n```\n"
    out = dss.scan_c4_cli({"doc.md": doc}, GT_CLI)
    assert real not in out["potential_stale"]


def test_c4_ground_truth_help_nao_vazio() -> None:
    """POSITIVO de régua: o ground truth do C4 lê o help (stderr) — nunca vazio.

    Regressão medida: de 31/08 até esta correção, gt vinha VAZIO (o binário
    imprime o help no stderr e o scanner lia só stdout) — todo citado era stale.
    """
    assert len(GT_CLI) > 50, f"touring --help parseado com só {len(GT_CLI)} comandos"
    assert "status" in GT_CLI or "doctor" in GT_CLI, sorted(GT_CLI)[:10]


def test_c6_ground_truth_nao_depende_do_env() -> None:
    """NEGATIVO estrutural: o ground truth ignora os.environ (determinístico).

    A var falsa é montada por concatenação para não se auto-referenciar: o
    ground truth varre .py do repo, inclusive ESTE arquivo de teste.
    """
    var_falsa = "TOURING_TESTE_FALSO_" + "QQXYZ"
    os.environ[var_falsa] = "1"
    try:
        gt = dss.gt_actual_envvars()
    finally:
        del os.environ[var_falsa]
    assert var_falsa not in gt
    assert len(gt) > 100, f"ground truth suspeitosamente pequeno: {len(gt)}"


def test_c6_positivo_var_inventada() -> None:
    """POSITIVO: var citada ausente do ground truth DEVE ser stale (gt sintético)."""
    var_inventada = "TOURING_XYZ_" + "INVENTADA_QQ"  # concatenação anti-self-reference
    out = dss.scan_c6_envvars({"doc.md": f"Exporte {var_inventada} antes.\n"}, ["TOURING_REAL"])
    assert var_inventada in out["stale"]


def test_c6_negativo_var_reconhecida() -> None:
    """NEGATIVO: var presente no ground truth não é stale (gt sintético)."""
    out = dss.scan_c6_envvars({"doc.md": "Use TOURING_DAEMON_SOCKET aqui.\n"}, ["TOURING_DAEMON_SOCKET"])
    assert "TOURING_DAEMON_SOCKET" not in out["stale"]


def test_c6_integracao_var_real_no_codigo() -> None:
    """POSITIVO de integração: o ground truth real acha TOURING_DAEMON_SOCKET (16 .rs)."""
    assert "TOURING_DAEMON_SOCKET" in GT_VARS


def test_c6_docs_nao_e_ground_truth() -> None:
    """NEGATIVO estrutural: artefato em docs/ NÃO batiza uma var como existente.

    Controle real: o próprio bundle da reconciliação
    (docs/plans/2026-10-08-drift-reconciliacao/gerar_ledger.py) cita
    "TOURING_JEV_" + "INTENT" em string — se docs/ entrasse no ground truth,
    ela constaria (foi o defeito medido 26→19 antes desta exclusão).
    """
    var_so_em_docs = "TOURING_JEV_" + "INTENT"
    assert var_so_em_docs not in GT_VARS, (
        "var citada só em .py de docs/plans entrou no ground truth — "
        "a exclusão docs/ regrediu"
    )


def main() -> int:
    tests = [v for k, v in sorted(globals().items()) if k.startswith("test_")]
    falhas = 0
    for t in tests:
        try:
            t()
            print(f"  PASS {t.__name__}")
        except AssertionError as e:
            falhas += 1
            print(f"  FAIL {t.__name__}: {e}")
    print(f"{len(tests)} testes, {falhas} falhas")
    return 1 if falhas else 0


if __name__ == "__main__":
    sys.exit(main())
