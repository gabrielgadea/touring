"""test_kit.py — Testes de migration_kit.sh e kit_check.sh.

Cria um HOME falso com symlinks rel/abs, arquivos .old, .jsonl e
verifica o kit gerado pelo script bash.
"""
import subprocess
from pathlib import Path

import pytest

# ---------------------------------------------------------------------------
# Caminhos dos scripts (relativos a este arquivo)
# ---------------------------------------------------------------------------
REPO_ROOT = Path(__file__).resolve().parent.parent.parent.parent  # projects/touring
OMARCHY = REPO_ROOT / "client" / "omarchy"
KIT_SH = OMARCHY / "bin" / "migration_kit.sh"
CHECK_SH = OMARCHY / "bin" / "kit_check.sh"


def _run(cmd: list[str], **kwargs) -> subprocess.CompletedProcess:
    kwargs.setdefault("capture_output", True)
    kwargs.setdefault("text", True)
    return subprocess.run(cmd, **kwargs)


# ---------------------------------------------------------------------------
# Fixture: HOME falso com estrutura mínima
# ---------------------------------------------------------------------------
@pytest.fixture()
def fake_home(tmp_path: Path) -> Path:
    """Cria um $HOME falso com os itens que migration_kit.sh copia."""
    h = tmp_path / "home"
    h.mkdir()

    # .claude/skills com symlinks rel e abs
    skills = h / ".claude" / "skills"
    skills.mkdir(parents=True)
    agents_target = h / ".agents" / "skills" / "rel-skill"
    agents_target.mkdir(parents=True)
    (agents_target / "SKILL.md").write_text("rel skill content")

    # Symlink relativo (como os 96 que apontam para ~/.agents)
    # path: .claude/skills/rel-skill -> ../../.agents/skills/rel-skill
    (skills / "rel-skill").symlink_to("../../.agents/skills/rel-skill")

    # Symlink absoluto externo (como os 4 que apontam para analise)
    ext_target = tmp_path / "projects" / "analise" / ".claude" / "skills" / "abs-skill"
    ext_target.mkdir(parents=True)
    (ext_target / "SKILL.md").write_text("abs skill content")
    (skills / "abs-skill").symlink_to(str(ext_target))

    # .claude/hooks com um arquivo .old e um script normal
    hooks = h / ".claude" / "hooks"
    hooks.mkdir(parents=True)
    (hooks / "touring-hook").write_text("#!/bin/bash\necho hi")
    (hooks / "old-binary.old").write_text("should be excluded")

    # .claude/projects/myproj/memory com .md e .jsonl (jsonl excluído)
    mem = h / ".claude" / "projects" / "myproj" / "memory"
    mem.mkdir(parents=True)
    (mem / "note.md").write_text("memory note")
    (mem / "log.jsonl").write_text('{"a":1}\n')

    # .claude/CLAUDE.md e settings.json mínimo
    (h / ".claude" / "CLAUDE.md").write_text("# TACO")
    settings = {
        "hooks": {
            "SessionStart": [
                {"hooks": [{"type": "command", "command": "/bin/true"}]}
            ]
        }
    }
    import json
    (h / ".claude" / "settings.json").write_text(json.dumps(settings, indent=2))

    # .claude.json
    (h / ".claude.json").write_text('{"version":1}')

    # .agents/ inteiro (alvo dos symlinks relativos)
    (h / ".agents" / "skills").mkdir(parents=True, exist_ok=True)

    return h


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
def _run_kit(fake_home: Path, dest: Path, extra: list[str] | None = None) -> subprocess.CompletedProcess:
    cmd = ["bash", str(KIT_SH), "--home", str(fake_home), "--dest", str(dest)]
    if extra:
        cmd.extend(extra)
    return _run(cmd)


def _read_manifest(kit_dir: Path) -> dict[str, str]:
    """Lê MANIFEST.sha256 e retorna {relative_path: hash}.

    sha256sum gera linhas como: `<hash>  ./path/to/file`
    Normaliza para `path/to/file` (sem './' inicial).
    """
    manifest = kit_dir / "MANIFEST.sha256"
    result: dict[str, str] = {}
    for line in manifest.read_text().splitlines():
        if not line.strip():
            continue
        parts = line.split(None, 1)
        if len(parts) == 2:
            digest, path = parts
            # Normaliza: './foo' → 'foo'; preserva '.' em '.claude'
            if path.startswith("./"):
                path = path[2:]
            result[path] = digest
    return result


# Arquivos excluídos do manifest por design (gerados após o manifest)
_MANIFEST_EXCLUSIONS = frozenset({"MANIFEST.sha256", "kit.json"})


# ---------------------------------------------------------------------------
# Testes
# ---------------------------------------------------------------------------
def test_manifest_covers_every_regular_file(fake_home: Path, tmp_path: Path) -> None:
    """MANIFEST.sha256 deve ter exatamente um entry para cada arquivo regular do kit
    (exceto ele mesmo e kit.json, que é gerado depois).
    """
    dest = tmp_path / "kit"
    result = _run_kit(fake_home, dest)
    assert result.returncode == 0, f"kit failed:\n{result.stdout}\n{result.stderr}"

    manifest_entries = _read_manifest(dest)
    assert len(manifest_entries) > 0, "MANIFEST.sha256 está vazio"

    # Todos os arquivos regulares exceto os gerados após o manifest
    actual_files = {
        str(f.relative_to(dest))
        for f in dest.rglob("*")
        if f.is_file() and not f.is_symlink() and f.name not in _MANIFEST_EXCLUSIONS
    }
    assert set(manifest_entries.keys()) == actual_files, (
        f"Divergência:\n"
        f"  em manifest mas não no disco: {set(manifest_entries.keys()) - actual_files}\n"
        f"  no disco mas não no manifest: {actual_files - set(manifest_entries.keys())}"
    )


def test_symlinks_tsv_counts_rel_and_abs(fake_home: Path, tmp_path: Path) -> None:
    """SYMLINKS.tsv deve listar symlinks rel e abs com o tipo correto (fixture)."""
    dest = tmp_path / "kit"
    result = _run_kit(fake_home, dest)
    assert result.returncode == 0, result.stdout + result.stderr

    tsv = dest / "SYMLINKS.tsv"
    assert tsv.exists(), "SYMLINKS.tsv não gerado"

    lines = tsv.read_text().splitlines()
    # Primeira linha é cabeçalho
    assert lines[0].startswith("path"), f"cabeçalho inesperado: {lines[0]}"
    data_lines = lines[1:]
    assert len(data_lines) > 0, "SYMLINKS.tsv sem linhas de dados"

    types = [line.split("\t")[2] for line in data_lines if "\t" in line]
    assert "rel" in types, "nenhum symlink relativo no TSV"
    assert "abs" in types, "nenhum symlink absoluto no TSV"

    # Contagem deve bater com find
    actual_links = sum(1 for _ in dest.rglob("*") if _.is_symlink())
    assert len(data_lines) == actual_links, (
        f"TSV tem {len(data_lines)} entradas mas existem {actual_links} symlinks"
    )


def test_old_and_jsonl_excluded(fake_home: Path, tmp_path: Path) -> None:
    """Arquivos .old em hooks e .jsonl em memory devem ser excluídos do kit."""
    dest = tmp_path / "kit"
    result = _run_kit(fake_home, dest)
    assert result.returncode == 0, result.stdout + result.stderr

    # Nenhum .old no kit inteiro
    old_files = list(dest.rglob("*.old"))
    assert old_files == [], f"Arquivos .old presentes: {old_files}"

    # Nenhum .jsonl no kit inteiro
    jsonl_files = list(dest.rglob("*.jsonl"))
    assert jsonl_files == [], f"Arquivos .jsonl presentes: {jsonl_files}"


def test_kit_check_self_passes(fake_home: Path, tmp_path: Path) -> None:
    """kit_check.sh --self deve passar (exit 0) num kit recém-gerado."""
    dest = tmp_path / "kit"
    result = _run_kit(fake_home, dest)
    assert result.returncode == 0, result.stdout + result.stderr

    check = _run(["bash", str(CHECK_SH), str(dest), "--self"])
    assert check.returncode == 0, (
        f"kit_check.sh --self falhou:\n{check.stdout}\n{check.stderr}"
    )
    assert "FAIL" not in check.stdout, (
        f"FAIL encontrado no output de kit_check --self:\n{check.stdout}"
    )


def test_kit_check_fails_on_tampered_hash(fake_home: Path, tmp_path: Path) -> None:
    """kit_check.sh --self deve retornar rc != 0 e imprimir FAIL após alteração de 1 byte."""
    dest = tmp_path / "kit"
    result = _run_kit(fake_home, dest)
    assert result.returncode == 0, result.stdout + result.stderr

    # Altera 1 byte num arquivo QUE ESTA NO MANIFESTO.
    #
    # Nao usar `dest.rglob("*")[0]`: migration_kit.sh exclui do manifesto tanto
    # `MANIFEST.sha256` quanto `kit.json` (este ultimo carrega o sha256 do
    # proprio manifesto, logo nao pode constar nele). Escolher pela ordem do
    # rglob — que depende do filesystem — podia cair no `kit.json`, cuja
    # adulteracao e' indetectavel POR DESENHO, e o teste falhava acusando o
    # kit_check.sh de um defeito que nao existe. Observado 2026-08-23 na
    # migracao Pop!_OS(ext4) -> Omarchy(btrfs): verde num, vermelho no outro.
    # Ler o manifesto e' deterministico e testa exatamente a propriedade certa.
    manifest_lines = (dest / "MANIFEST.sha256").read_text().splitlines()
    manifest_rels = [ln.split("  ", 1)[1] for ln in manifest_lines if "  " in ln]
    targets = [
        f for f in (dest / rel for rel in manifest_rels)
        if f.is_file() and not f.is_symlink()
    ]
    assert targets, "nenhum arquivo do manifesto disponivel para adulterar"
    target = targets[0]
    data = bytearray(target.read_bytes())
    data[0] ^= 0xFF  # flip do primeiro byte
    target.write_bytes(bytes(data))

    check = _run(["bash", str(CHECK_SH), str(dest), "--self"])
    assert check.returncode != 0, "kit_check.sh deveria retornar != 0 após adulteração"
    assert "FAIL" in check.stdout, (
        f"'FAIL' não encontrado no output:\n{check.stdout}"
    )


def test_real_kit_symlinks_match_kit_json_and_allowlist() -> None:
    """Against the REAL kit, when it exists: the TSV count equals kit.json, and
    every absolute target sits under ~/projects/{analise,touring} — the only
    two places the plan restores (D7 / P4). The plan first said "100 symlinks,
    4 abs"; the measured kit has 102/6 (the two extra are hooks/touring{,-daemon}
    → target/release, rebuilt by update-touring). The number is data; the
    allowlist is the invariant."""
    import json
    kit = Path.home() / "omarchy-kit"
    tsv, meta = kit / "SYMLINKS.tsv", kit / "kit.json"
    if not (tsv.is_file() and meta.is_file()):
        pytest.skip("real kit not built on this machine")
    rows = [line.split("\t") for line in tsv.read_text(encoding="utf-8").splitlines()[1:] if line]
    assert len(rows) == json.loads(meta.read_text(encoding="utf-8"))["symlinks_total"]
    abs_rows = [r for r in rows if r[2] == "abs"]
    assert abs_rows, "a kit with no absolute symlink means the analise skills were not captured"
    for _path, target, _kind in abs_rows:
        assert target.startswith(("/home/gabrielgadea/projects/analise/", "/home/gabrielgadea/projects/touring/")), target
