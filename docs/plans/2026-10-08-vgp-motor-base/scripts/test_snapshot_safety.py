"""snapshot_safety — o snapshot captura a worktree inteira e não toca nada dela.

Fixtures: repos git sintéticos em tmp (worktree principal, worktree ligada, HEAD destacado).
"""

import hashlib
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import snapshot_safety as ss

REF = "refs/heads/safety/teste/x"


def g(repo: Path, *args: str) -> str:
    r = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True, check=True)
    return r.stdout.strip()


def repo_sujo(tmp_path: Path) -> Path:
    repo = tmp_path / "r"
    repo.mkdir()
    g(repo, "init", "-q", "-b", "main")
    g(repo, "config", "user.name", "t")
    g(repo, "config", "user.email", "t@t")
    (repo / "a.txt").write_text("v1\n")
    (repo / "apagado.txt").write_text("some\n")
    (repo / ".gitignore").write_text("*.log\n")
    g(repo, "add", "-A")
    g(repo, "commit", "-q", "-m", "base")
    (repo / "a.txt").write_text("v2\n")
    (repo / "apagado.txt").unlink()
    (repo / "novo.txt").write_text("n\n")
    (repo / "ruido.log").write_text("ignorado\n")
    return repo


def estado(repo: Path) -> tuple:
    idx = Path(g(repo, "rev-parse", "--path-format=absolute", "--git-path", "index"))
    return (g(repo, "status", "--porcelain"), g(repo, "rev-parse", "HEAD"), hashlib.sha256(idx.read_bytes()).hexdigest())


def test_captura_modificado_novo_e_apagado_sem_tocar_a_worktree(tmp_path):
    repo = repo_sujo(tmp_path)
    antes = estado(repo)
    res = ss.snapshot("x", str(repo), REF, 1 << 20, dry_run=False)
    assert res["status"] == "OK", res
    assert res["por_status"] == {"M": 1, "D": 1, "A": 1}
    assert g(repo, "show", f"{REF}:a.txt") == "v2"
    assert g(repo, "show", f"{REF}:novo.txt") == "n"
    nomes = g(repo, "ls-tree", "--name-only", REF).split()
    assert "apagado.txt" not in nomes and "ruido.log" not in nomes
    assert g(repo, "rev-parse", f"{REF}^") == antes[1]
    assert estado(repo) == antes
    assert g(repo, "branch", "--show-current") == "main"


def test_nunca_sobrescreve_ref_existente(tmp_path):
    repo = repo_sujo(tmp_path)
    assert ss.snapshot("x", str(repo), REF, 1 << 20, dry_run=False)["status"] == "OK"
    primeiro = g(repo, "rev-parse", REF)
    (repo / "a.txt").write_text("v3\n")
    res = ss.snapshot("x", str(repo), REF, 1 << 20, dry_run=False)
    assert res["status"] == "RECUSADO" and g(repo, "rev-parse", REF) == primeiro


def test_dry_run_e_arquivo_acima_do_teto_nao_gravam_ref(tmp_path):
    repo = repo_sujo(tmp_path)
    assert ss.snapshot("x", str(repo), REF, 1 << 20, dry_run=True)["status"] == "DRY-RUN"
    (repo / "grande.bin").write_bytes(b"0" * 2048)
    res = ss.snapshot("x", str(repo), REF, 1024, dry_run=False)
    assert res["status"] == "RECUSADO" and res["acima_do_teto"][0].startswith("grande.bin")
    probe = subprocess.run(["git", "-C", str(repo), "rev-parse", "--verify", "-q", REF], check=False)
    assert probe.returncode != 0


def test_arvore_limpa_fica_coberta_pelo_head(tmp_path):
    repo = repo_sujo(tmp_path)
    g(repo, "add", "-A")
    g(repo, "commit", "-q", "-m", "tudo")
    assert ss.snapshot("x", str(repo), REF, 1 << 20, dry_run=False)["status"] == "COBERTO-PELO-HEAD"


def test_worktree_ligada_com_head_destacado(tmp_path):
    repo = repo_sujo(tmp_path)
    g(repo, "add", "-A")
    g(repo, "commit", "-q", "-m", "tudo")
    wt = tmp_path / "wt"
    g(repo, "worktree", "add", "-q", "--detach", str(wt))
    (wt / "a.txt").write_text("na-ligada\n")
    antes = estado(wt)
    res = ss.snapshot("x", str(wt), REF, 1 << 20, dry_run=False)
    assert res["status"] == "OK", res
    assert g(repo, "show", f"{REF}:a.txt") == "na-ligada"
    assert estado(wt) == antes and estado(repo)[0] == ""


def test_main_recusa_prefixo_fora_de_safety_e_sai_1_em_recusa(tmp_path, capsys):
    repo = repo_sujo(tmp_path)
    try:
        ss.main(["--prefix", "feat/x", f"r={repo}"])
    except SystemExit as e:
        assert e.code == 2
    assert ss.main(["--prefix", "safety/t", "--max-mb", "0", f"r={repo}"]) == 1
    assert '"RECUSADO"' in capsys.readouterr().out
