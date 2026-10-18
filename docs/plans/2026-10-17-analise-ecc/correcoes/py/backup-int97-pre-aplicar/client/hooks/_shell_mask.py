#!/usr/bin/env python3
"""_shell_mask.py — remove heredoc bodies and quoted literals from a shell command.

Called by block_git.sh to mask data before destructive-pattern matching, so that
git command text appearing inside data (heredoc body, quoted string) does not
trigger a false positive.

Semantic reference: touring_ceg::gateway::gate::mask_shell_data
  (crates/touring-ceg/src/gateway/gate.rs, commit S24, tests s24_shell_data_tests)

Usage: printf '%s' "$CMD" | python3 _shell_mask.py
Exit: always 0 (fail-open — caller falls back to original CMD on error).
"""
import re
import sys


def mask_shell_data(cmd: str) -> str:
    """Return cmd with heredoc bodies and quoted literals replaced by empty equivalents."""
    # Phase 1: mask heredoc bodies.
    lines = cmd.split('\n')
    out: list[str] = []
    hd_end: str | None = None
    for line in lines:
        if hd_end is not None:
            if line.strip() == hd_end:
                hd_end = None
            out.append('')
        else:
            m = re.search(r'<<-?\s*[\"\']?(\w+)[\"\']?', line)
            if m:
                hd_end = m.group(1)
                out.append(line[:m.start()])
            else:
                out.append(line)
    cmd = '\n'.join(out)

    # Phase 2: mask single-quoted strings — nothing inside executes.
    cmd = re.sub(r"'[^']*'", "''", cmd)

    # Phase 3: mask double-quoted strings without $() or backtick (no subshell).
    def _mask_dq(m: re.Match) -> str:
        s = m.group(0)
        if '$(' in s or '`' in s:
            return s    # may execute — keep intact
        return '""'     # safe to blank

    cmd = re.sub(r'"[^"\\\\]*(?:\\\\.[^"\\\\]*)*"', _mask_dq, cmd)
    return cmd


if __name__ == '__main__':
    print(mask_shell_data(sys.stdin.read()), end='')
