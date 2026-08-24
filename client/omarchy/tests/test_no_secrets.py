"""Verifica que nenhum segredo ou valor sensível foi commitado em client/omarchy/.

Patterns acusam apenas VALORES reais (não nomes de chave/comentários).
Os literais são construídos em runtime para não disparar o gate F2.4
em arquivos que apenas os definem como padrões de busca.
"""

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]

# Construção em runtime para que o gate F2.4 não acuse falso positivo
# neste arquivo de teste (que apenas descreve padrões, não contém segredos).
def _build_patterns() -> list[re.Pattern[str]]:
    # Token Anthropic API
    tok_ant = "sk-" + "ant-"
    # Token GitHub personal
    tok_gh = "ghp" + "_"
    # Chave privada PEM
    pem_rsa = "BEGIN RSA PRIVATE KEY"
    pem_ssh = "BEGIN OPENSSH PRIVATE KEY"
    # disk_encryption com valor não-vazio dentro do objeto JSON
    disk_enc = r'"disk_encryption"\s*:\s*\{[^}]*"(?:passphrase|key)"\s*:\s*"[^"]{3,}'
    # passphrase com valor não-vazio (>= 3 chars)
    pp = r'\bpassphrase\s*[:=]\s*"[^"]{3,}'
    return [
        re.compile(re.escape(tok_ant)),
        re.compile(re.escape(tok_gh)),
        re.compile(re.escape(pem_rsa)),
        re.compile(re.escape(pem_ssh)),
        re.compile(disk_enc),
        re.compile(pp),
    ]


SECRET_PATTERNS = _build_patterns()

BINARY_SUFFIXES = frozenset({
    ".pyc", ".db", ".iso", ".sig", ".gz", ".tar", ".zip",
    ".png", ".jpg", ".jpeg", ".gif", ".svg", ".ico",
})


def _text_files(root: Path):
    """Itera arquivos de texto em root (exclui binários e symlinks quebrados)."""
    for path in root.rglob("*"):
        if not path.is_file() or path.is_symlink():
            continue
        if path.suffix.lower() in BINARY_SUFFIXES:
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="strict")
        except (UnicodeDecodeError, PermissionError, OSError):
            continue
        yield path, text


def test_no_secrets_in_tree() -> None:
    """Nenhum padrão de segredo deve estar presente em client/omarchy/."""
    hits: list[str] = []
    for path, text in _text_files(ROOT):
        rel = path.relative_to(ROOT)
        # Pular este próprio arquivo (contém os padrões por definição)
        if path.name == "test_no_secrets.py":
            continue
        for lineno, line in enumerate(text.splitlines(), 1):
            for pat in SECRET_PATTERNS:
                if pat.search(line):
                    hits.append(f"{rel}:{lineno}: {line.strip()[:100]}")
                    break  # um hit por linha é suficiente
    assert hits == [], (
        "Secret pattern(s) found in client/omarchy tree:\n" + "\n".join(hits)
    )
