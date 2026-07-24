"""utilities — Shared formatting helpers (yaml_frontmatter, md_table, write_atomic, sha256_hex, _slug).

Extracted from renderers.py lines 14-60. Each module owns one logical
rendering concern (utility, index/wave/cross-audit, one of the 9 cross-cutting
docs). All public functions are re-exported by ``renderers/__init__.py``.
"""
from __future__ import annotations



# ─── Utilities ───────────────────────────────────────────────────────────────


def yaml_frontmatter(meta: "dict[str, object] | dict[str, str]") -> str:
    """Render YAML frontmatter (single-quoted strings, list items)."""
    lines = ["---"]
    for key, val in meta.items():
        if isinstance(val, list):
            if not val:
                lines.append(f"{key}: []")
            else:
                lines.append(f"{key}:")
                for item in val:
                    lines.append(f"  - {item}")
        elif isinstance(val, dict):
            lines.append(f"{key}:")
            for k, v in val.items():
                lines.append(f"  {k}: {json.dumps(v, ensure_ascii=False)}")
        elif isinstance(val, bool):
            lines.append(f"{key}: {str(val).lower()}")
        else:
            lines.append(f"{key}: {json.dumps(val, ensure_ascii=False)}")
    lines.append("---\n")
    return "\n".join(lines)


def md_table(headers: list[str], rows: list[list[str]]) -> str:
    """Render a markdown table."""
    header_line = "| " + " | ".join(headers) + " |"
    sep_line = "|" + "|".join(["---"] * len(headers)) + "|"
    row_lines = ["| " + " | ".join(r) + " |" for r in rows]
    return "\n".join([header_line, sep_line, *row_lines])


def write_atomic(path: Path, content: str) -> None:
    """Write content to path atomically with mkdir -p."""
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(content, encoding="utf-8")
    tmp.replace(path)


def sha256_hex(content: str) -> str:
    """Compute SHA-256 hex of UTF-8 content."""
    return hashlib.sha256(content.encode("utf-8")).hexdigest()


