#!/usr/bin/env python3
"""_shell_mask.py — the CODE views of a shell command, for block_git.sh.

block_git.sh judges a command by pattern-matching text.  What it matches against decides what it
can see, and the first version blanked every quoted literal and every heredoc body, whatever the
destination.  That is right for `echo "git reset --hard"` (data) and wrong for `bash -c 'git reset
--hard'` (the literal IS the program): of seven nested forms measured on 17/10/2026, only the direct
`git reset --hard` was denied.  The same blanking hid `git -C dir reset --hard`, `git reset '--hard'`
and everything after a heredoc marker on the same line.

What this module returns is a list of VIEWS, one line each at least, all scanned by the same patterns:

* the command with data masked out (quoted literals, comments, heredoc bodies, substitution text
  all blanked) — the old behaviour, now driven by a lexer instead of three regexes, so an
  apostrophe inside a double-quoted string no longer pairs with a quote three words later;
* for every place where text is handed to something that EXECUTES it, that text as a view of its
  own, recursively: `bash|sh|zsh|… -c`, `eval`, a heredoc / here-string / pipe feeding a shell,
  `$(...)`, backticks, `<(...)`, an unquoted heredoc body's substitutions, `find -exec`,
  `touring exec|run --code`, `env -S`, `su|flock|script -c`, and the command a wrapper launches
  (`env`, `timeout N`, `nice`, `nohup`, `command`, `exec`, `sudo`, `time`, `hw-slot … --`, `xargs`);
* the call-arguments of exec functions (`os.system`, `subprocess.*`, `system`, `execSync`, …) in an
  interpreter's inline program (`python3 -c`, `perl -e`, `node -e`, …) or script on stdin;
* each `git` invocation normalised to `git <subcommand> <args>` (global options such as `-C dir`
  dropped, a quoted subcommand or option-looking argument unquoted, any other quoted word blanked),
  so the patterns see the same line whatever way the command was spelled.

A heredoc body whose destination is passive (`cat`, `tee`, `cat > file`) stays DATA.

Hazards no text pattern can see travel as NOTE lines (`\\x1eBLOCK_GIT_NOTE:<kind>:<detail>`):
`xargs-git:<sub>` (stdin supplies the arguments of a destructive subcommand) and `budget:<why>`
(the recursion depth or the work budget — proportional to the input — ran out; an unverifiable
command is denied, never waved through).  block_git.sh turns each NOTE into a deny that names the
route back (`GIT_DESTRUCTIVE_OK=1` on that one command).

Hook bypass (D2, 17/10/2026): `git commit --no-verify|-n` (also in a short cluster, `-nm`), `push|merge|rebase|
cherry-pick --no-verify`, `am --no-verify|-n`, `git -c core.hooksPath=…`, `GIT_CONFIG_KEY_n=core.hooksPath`, `SKIP=…`,
`PRE_COMMIT_ALLOW_NO_CONFIG=1` and `HUSKY=0` (env on the command, on `env`, exported earlier, or inherited from an
outer shell) travel as `hooks-bypass:<form>\x1f<-C dir>` NOTES.  Whether they are a violation depends on the TARGET
repo having active hooks, which is a filesystem fact: `resolve_main` (second entry point, called by block_git.sh only
when such a NOTE exists) resolves it and names the hooks that would be skipped.  `push -n` is a dry run and
`merge -n` is no-stat: only the options that really skip hooks count.

Residual (textual guard, like fable_consent_guard.sh): a program assembled at run time
(`bash -c "$CMD"`, `eval "$x"`, a script FILE, a variable passed to `os.system`) is not seen.

Semantic reference for the data-masking half: touring_ceg::gateway::gate::mask_shell_data.

Usage: printf '%s' "$CMD" | python3 _shell_mask.py
Exit: 0 with the views on stdout; non-zero (traceback on stderr) on an analyser bug — the caller
then falls back to scanning the raw command, which is the conservative direction.
"""

from __future__ import annotations

import os
import posixpath
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

MAX_DEPTH = 8
WORK_FLOOR = 8192
WORK_FACTOR = 16
NOTE_PREFIX = "\x1eBLOCK_GIT_NOTE:"
SEP = "\x1f"  # separates the fields of a structured NOTE (form, target directory)


# ── lexer ───────────────────────────────────────────────────────────────────


@dataclass
class _Word:
    raw: str
    value: str
    quoted: bool


@dataclass(eq=False)
class _Seg:
    """One simple command: its words, the heredoc bodies / here-strings attached to it, and whether a pipe feeds it."""

    words: list[_Word] = field(default_factory=list)
    heredocs: list[str] = field(default_factory=list)
    herestrings: list[str] = field(default_factory=list)
    pipe_in: bool = False

    def empty(self) -> bool:
        return not (self.words or self.heredocs or self.herestrings)


@dataclass
class _Parsed:
    segs: list[_Seg]
    substs: list[str]
    spans: list[tuple[int, int, str]]


_HD_DELIM = re.compile(r"""[ \t]*(?:'([^']*)'|"([^"]*)"|\\?([^\s;&|<>()'"]+))""")


def _read_body(s: str, pos: int, delim: str, strip: bool) -> tuple[list[str], int]:
    """Lines of a heredoc body starting at `pos`, and the index just past its delimiter line (len(s) if unterminated).

    The delimiter line is compared leniently (surrounding whitespace ignored): a body that ends one line
    too early is scanned as code, which only ever errs towards a false positive.
    """
    n = len(s)
    lines: list[str] = []
    while pos < n:
        nl = s.find("\n", pos)
        end = n if nl < 0 else nl
        line = s[pos:end]
        pos = n if nl < 0 else nl + 1
        if (line.lstrip("\t") if strip else line).strip() == delim:
            break
        lines.append(line)
    return lines, pos


def _match_paren(s: str, i: int) -> int:
    """Index of the `)` closing the `(` at s[i] (quote- and heredoc-aware); len(s) when it never closes.

    Heredoc-aware because `git commit -m "$(cat <<'EOF' … don't … EOF)"` is the everyday shape: an
    apostrophe in the body must not be read as a quote.
    """
    n = len(s)
    depth = 0
    j = i
    pending: list[tuple[str, bool]] = []
    while j < n:
        ch = s[j]
        if ch == "\\":
            j += 2
        elif ch == "'":
            k = s.find("'", j + 1)
            j = (n if k < 0 else k) + 1
        elif ch == '"':
            j = _skip_double_quoted(s, j)
        elif ch == "<" and (marker := _heredoc_marker(s, j)) is not None:
            pending.append(marker[:2])
            j = marker[2]
        elif ch == "\n" and pending:
            j += 1
            for delim, strip in pending:
                _, j = _read_body(s, j, delim, strip)
            pending.clear()
        else:
            if ch == "(":
                depth += 1
            elif ch == ")":
                depth -= 1
                if depth == 0:
                    return j
            j += 1
    return n


def _skip_double_quoted(s: str, j: int) -> int:
    """Index just past the double-quoted string opening at s[j] (`$(...)` inside it nests)."""
    n = len(s)
    j += 1
    while j < n and s[j] != '"':
        if s[j] == "\\":
            j += 1
        elif s.startswith("$(", j):
            j = _match_paren(s, j + 1)
        j += 1
    return j + 1


def _heredoc_marker(s: str, j: int) -> tuple[str, bool, int] | None:
    """(delimiter, tab-stripping, index past the delimiter word) when a `<<[-]WORD` heredoc operator starts at s[j]."""
    if not s.startswith("<<", j) or s.startswith("<<<", j):
        return None
    strip = s.startswith("-", j + 2)
    m = _HD_DELIM.match(s, j + 3 if strip else j + 2)
    if m is None:
        return None
    return next(g for g in m.groups() if g is not None), strip, m.end()


def _find_backtick(s: str, i: int) -> int:
    """Index of the backtick closing the one at s[i]; len(s) when unclosed."""
    n = len(s)
    k = i + 1
    while k < n and s[k] != "`":
        k += 2 if s[k] == "\\" else 1
    return min(k, n)


def _match_brace(s: str, i: int) -> int:
    """Index of the `}` closing the `${` at s[i]; len(s) when unclosed."""
    depth = 0
    for j in range(i, len(s)):
        if s[j] == "{":
            depth += 1
        elif s[j] == "}":
            depth -= 1
            if depth == 0:
                return j
    return len(s)


def _body_substs(body: str) -> list[str]:
    """The `$(...)` / backtick programs inside an UNQUOTED-delimiter heredoc body: they run, though the body is data."""
    out: list[str] = []
    i = 0
    n = len(body)
    while i < n:
        if body[i] == "\\":
            i += 2
        elif body.startswith("$(", i):
            k = _match_paren(body, i + 1)
            out.append(body[i + 2 : k])
            i = k + 1
        elif body[i] == "`":
            k = _find_backtick(body, i)
            out.append(body[i + 1 : k])
            i = k + 1
        else:
            i += 1
    return out


_ANSI_ESC = re.compile(r"\\(x[0-9A-Fa-f]{1,2}|u[0-9A-Fa-f]{1,4}|[0-7]{1,3}|.)", re.DOTALL)
_ANSI_SIMPLE = {"n": "\n", "t": "\t", "r": "\r", "a": "\a", "b": "\b", "e": "\x1b", "f": "\f", "v": "\v"}


def _ansi_c(txt: str) -> str:
    """Decode the body of a `$'...'` literal (the escapes that matter for flags and names)."""

    def one(m: re.Match[str]) -> str:
        esc = m.group(1)
        if esc[0] in "xu" and len(esc) > 1:
            return chr(int(esc[1:], 16))
        if esc[0].isdigit():
            return chr(int(esc, 8) & 0xFF)
        return _ANSI_SIMPLE.get(esc, esc)

    return _ANSI_ESC.sub(one, txt)


class _Lexer:
    """Splits a command into simple commands (words with their quoting), and records what to blank in the data view."""

    def __init__(self, text: str) -> None:
        self.s = text
        self.n = len(text)
        self.i = 0
        self.segs: list[_Seg] = []
        self.substs: list[str] = []
        self.spans: list[tuple[int, int, str]] = []
        self.seg = _Seg()
        self.raw: list[str] = []
        self.val: list[str] = []
        self.in_word = False
        self.quoted = False
        self.mode: str | None = None  # next word is: redir (dropped) | here (here-string) | hd (heredoc delimiter)
        self.hd_start = 0
        self.hd_strip = False
        self.pending: list[tuple[str, bool, bool, _Seg]] = []

    # word bookkeeping
    def _add(self, raw: str, val: str, quoted: bool = False) -> None:
        self.in_word = True
        self.quoted = self.quoted or quoted
        self.raw.append(raw)
        self.val.append(val)

    def _drop_fd(self) -> None:
        if self.in_word and not self.quoted and "".join(self.val).isdigit():
            self.raw, self.val, self.in_word = [], [], False

    def _end_word(self) -> None:
        if not self.in_word:
            return
        word = _Word("".join(self.raw), "".join(self.val), self.quoted)
        self.raw, self.val, self.in_word, self.quoted = [], [], False, False
        mode, self.mode = self.mode, None
        if mode == "redir":
            return
        if mode == "here":
            self.seg.herestrings.append(word.value)
        elif mode == "hd":
            self.pending.append((word.value, self.hd_strip, word.quoted, self.seg))
            self.spans.append((self.hd_start, self.i, ""))
        else:
            self.seg.words.append(word)

    def _end_seg(self, pipe_next: bool) -> None:
        self.mode = None
        if not self.seg.empty():
            self.segs.append(self.seg)
            self.seg = _Seg(pipe_in=pipe_next)
        elif pipe_next:
            self.seg.pipe_in = True

    # scanners
    def run(self) -> _Parsed:
        s, n = self.s, self.n
        while self.i < n:
            c = s[self.i]
            if c == "\\":
                self._escape()
            elif c == "'":
                self._single()
            elif c == '"':
                self._double()
            elif c == "`":
                self._backtick()
            elif c == "$":
                self._dollar()
            elif c in "<>":
                self._angle()
            elif c in " \t\r":
                self._end_word()
                self.i += 1
            elif c == "\n":
                self._newline()
            elif c in ";|&":
                self._operator()
            elif c in "()":
                self._paren()
            elif c == "#" and not self.in_word:
                end = s.find("\n", self.i)
                end = n if end < 0 else end
                self.spans.append((self.i, end, ""))
                self.i = end
            else:
                self._add(c, c)
                self.i += 1
        self._end_word()
        self._end_seg(False)
        return _Parsed(self.segs, self.substs, self.spans)

    def _paren(self) -> None:
        """`(` / `)` end a command; `((...))` is an arithmetic command, one opaque word: its `<<` is a shift, not a
        heredoc that would swallow the lines after it."""
        s, i = self.s, self.i
        self._end_word()
        if s.startswith("((", i):
            k = _match_paren(s, i)
            self._add(s[i : k + 1], s[i : k + 1])
            self._end_word()
            self.i = k + 1
            return
        self._end_seg(False)
        self.i += 1

    def _escape(self) -> None:
        s, i = self.s, self.i
        if i + 1 >= self.n:
            self._add("\\", "\\")
            self.i += 1
        elif s[i + 1] == "\n":
            self.i += 2
        else:
            self._add(s[i : i + 2], s[i + 1], True)
            self.i += 2

    def _single(self) -> None:
        s, i = self.s, self.i
        j = s.find("'", i + 1)
        if j < 0:  # unterminated: the shell would not run it; keep the text visible, never blank it
            self._add(s[i:], s[i:], True)
            self.i = self.n
            return
        self._add(s[i : j + 1], s[i + 1 : j], True)
        self.spans.append((i, j + 1, "''"))
        self.i = j + 1

    def _double(self) -> None:
        s, i, n = self.s, self.i, self.n
        j = i + 1
        buf: list[str] = []
        while j < n and s[j] != '"':
            ch = s[j]
            if ch == "\\" and j + 1 < n:
                nxt = s[j + 1]
                if nxt in '"\\$`':
                    buf.append(nxt)
                elif nxt != "\n":
                    buf.append("\\" + nxt)
                j += 2
            elif ch == "`":
                k = _find_backtick(s, j)
                self.substs.append(s[j + 1 : k])
                buf.append(s[j : k + 1])
                j = k + 1
            elif ch == "$" and s.startswith("$(", j):
                k = _match_paren(s, j + 1)
                self.substs.append(s[j + 2 : k])
                buf.append(s[j : k + 1])
                j = k + 1
            else:
                buf.append(ch)
                j += 1
        if j >= n:  # unterminated
            self._add(s[i:], s[i:], True)
            self.i = n
            return
        self._add(s[i : j + 1], "".join(buf), True)
        self.spans.append((i, j + 1, '""'))
        self.i = j + 1

    def _backtick(self) -> None:
        s, i = self.s, self.i
        k = _find_backtick(s, i)
        self.substs.append(s[i + 1 : k])
        self._add(s[i : k + 1], s[i : k + 1])
        self.spans.append((i, k + 1, "``"))
        self.i = k + 1

    def _dollar(self) -> None:
        s, i, n = self.s, self.i, self.n
        if s.startswith("$(", i):
            k = _match_paren(s, i + 1)
            self.substs.append(s[i + 2 : k])
            self._add(s[i : k + 1], s[i : k + 1])
            self.spans.append((i, k + 1, "$()"))
            self.i = k + 1
        elif s.startswith("$'", i):
            j = i + 2
            while j < n and s[j] != "'":
                j += 2 if s[j] == "\\" else 1
            if j >= n:
                self._add(s[i:], s[i:], True)
                self.i = n
                return
            self._add(s[i : j + 1], _ansi_c(s[i + 2 : j]), True)
            self.spans.append((i, j + 1, "$''"))
            self.i = j + 1
        elif s.startswith("${", i):
            k = min(_match_brace(s, i + 1), n - 1)
            self._add(s[i : k + 1], s[i : k + 1])
            self.i = k + 1
        else:
            self._add("$", "$")
            self.i += 1

    def _angle(self) -> None:
        s, i = self.s, self.i
        if s.startswith("<<<", i):
            self._drop_fd()
            self._end_word()
            self.mode = "here"
            self.i = i + 3
        elif s.startswith("<<", i):
            self._drop_fd()
            self._end_word()
            strip = s.startswith("-", i + 2)
            self.mode, self.hd_start, self.hd_strip = "hd", i, strip
            self.i = i + 3 if strip else i + 2
        elif s.startswith(("<(", ">("), i):
            self._end_word()
            k = _match_paren(s, i + 1)
            self.substs.append(s[i + 2 : k])
            self._add(s[i : k + 1], s[i : k + 1])
            self.spans.append((i, k + 1, s[i] + "()"))
            self._end_word()
            self.i = k + 1
        else:
            self._drop_fd()
            self._end_word()
            j = i + 1
            if s[i] == ">" and s.startswith(">", j):
                j += 1
            if s.startswith(("&", "|"), j) or (s[i] == "<" and s.startswith(">", j)):
                j += 1
            self.mode = "redir"
            self.i = j

    def _operator(self) -> None:
        s, i = self.s, self.i
        if s.startswith("&>", i):
            self._end_word()
            self.mode = "redir"
            self.i = i + (3 if s.startswith("&>>", i) else 2)
            return
        self._end_word()
        width = 2 if s.startswith(("||", "&&", ";;", "|&"), i) else 1
        self._end_seg(s[i] == "|" and not s.startswith("||", i))
        self.i = i + width

    def _newline(self) -> None:
        self._end_word()
        self._end_seg(False)
        if self.pending:
            self._read_heredocs()
        else:
            self.i += 1

    def _read_heredocs(self) -> None:
        pos = self.i + 1
        for delim, strip, quoted_delim, owner in self.pending:
            body_start = pos
            lines, pos = _read_body(self.s, pos, delim, strip)
            body = "\n".join(lines)
            owner.heredocs.append(body)
            if not quoted_delim:
                self.substs.extend(_body_substs(body))
            self.spans.append((body_start, pos, ""))
        self.pending.clear()
        self.i = pos


def _apply(text: str, spans: list[tuple[int, int, str]]) -> str:
    out: list[str] = []
    cur = 0
    for a, b, repl in sorted(spans, key=lambda t: (t[0], -t[1])):
        if a < cur:
            continue
        out.append(text[cur:a])
        out.append(repl)
        cur = b
    out.append(text[cur:])
    return "".join(out)


def mask_shell_data(cmd: str) -> str:
    """cmd with data blanked: quoted literals, comments, heredoc bodies and substitution text. One level only."""
    return _apply(cmd, _Lexer(cmd).run().spans)


# ── analysis ────────────────────────────────────────────────────────────────


class _Overflow(Exception):
    """The recursion depth or the work budget ran out: the command cannot be verified."""


@dataclass
class Scan:
    views: list[str]
    notes: list[str]


@dataclass(frozen=True)
class _Wrap:
    arg_opts: frozenset[str] = frozenset()
    positional: int = 0


_WRAPPERS: dict[str, _Wrap] = {
    "env": _Wrap(frozenset({"-u", "--unset", "-C", "--chdir", "-a", "--argv0"})),
    "timeout": _Wrap(frozenset({"-s", "--signal", "-k", "--kill-after"}), 1),
    "nice": _Wrap(frozenset({"-n", "--adjustment"})),
    "nohup": _Wrap(),
    "command": _Wrap(),
    "builtin": _Wrap(),
    "exec": _Wrap(frozenset({"-a"})),
    "sudo": _Wrap(frozenset({"-u", "-g", "-C", "-D", "-h", "-p", "-r", "-t", "-T", "-U", "--user", "--group", "--chdir",
                             "--host", "--prompt", "--role", "--type", "--close-from", "--command-timeout"})),
    "doas": _Wrap(frozenset({"-u", "-C"})),
    "time": _Wrap(frozenset({"-f", "-o", "--format", "--output"})),
    "setsid": _Wrap(),
    "ionice": _Wrap(frozenset({"-c", "-n", "-p", "-P", "-u", "--class", "--classdata"})),
    "stdbuf": _Wrap(frozenset({"-i", "-o", "-e"})),
    "busybox": _Wrap(),
    "hw-slot": _Wrap(frozenset({"--class", "--wait", "--max-temp", "--min-mem-gb", "--min-vram-gb", "--abort-temp",
                                "--label"})),
    "xargs": _Wrap(frozenset({"-a", "--arg-file", "-d", "--delimiter", "-E", "-I", "-L", "-n", "--max-args",
                              "--max-lines", "-P", "--max-procs", "-s", "--max-chars", "--process-slot-var"})),
}
_SHELLS = frozenset({"bash", "sh", "zsh", "dash", "ksh", "ash", "fish", "mksh", "rbash", "csh", "tcsh"})
#: name -> the option that carries a shell program (su -c 'cmd', flock f -c 'cmd', script -c 'cmd').
_STRING_RUNNERS = {"su": ("-c", "--command"), "flock": ("-c", "--command"), "script": ("-c", "--command")}
_QUERY_FLAGS = {"command": frozenset({"-v", "-V"})}  # `command -v git` prints a path, runs nothing
_PROGRAM_FLAGS = {"env": frozenset({"-S", "--split-string"})}  # the next word is a program, not a command name
_SUDO_SHELL = frozenset({"-s", "-i", "--shell", "--login"})  # sudo runs the rest as `$SHELL -c '…'`
_SHELL_VALUE_OPTS = frozenset({"-o", "+o", "-O", "+O", "--rcfile", "--init-file"})
_ASSIGN = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*(?:\[[^\]]*\])?\+?=")
_RESERVED = frozenset({"{", "}", "!", "if", "then", "else", "elif", "fi", "do", "done", "while", "until"})
_GIT_GLOBAL_ARG = frozenset({"-C", "-c", "--git-dir", "--work-tree", "--namespace", "--exec-path", "--super-prefix",
                             "--config-env"})
#: Subcommands whose destructive forms an `xargs` can complete from stdin, invisibly to the guard.
_XARGS_RISKY = frozenset({"reset", "clean", "checkout", "switch", "restore", "stash", "rebase", "filter-branch",
                          "filter-repo"})
_OPTION_LIKE = re.compile(r"-{1,2}[A-Za-z0-9][A-Za-z0-9_=./:@+,-]*")
_SUBCOMMAND = re.compile(r"[a-z][a-z0-9-]*")

_EXEC_CALL = re.compile(
    r"(?<![\w$@])(os\.(?:system|popen\w*|exec\w*|spawn\w*|posix_spawn\w*)|subprocess\.\w+|commands\.\w+|pty\.spawn"
    r"|child_process\.\w+|Open3\.\w+|Kernel\.\w+|IO\.popen|io\.popen|os\.execute|execSync|execFileSync|spawnSync"
    r"|execFile|exec|spawn|system|passthru|shell_exec|proc_open|popen|Popen|check_output|check_call|getoutput"
    r"|getstatusoutput|run|call)(?!\w)"
)
#: Exec calls that take their program without parentheses (`system "git clean -fd"` in Perl/Ruby).
_NO_PAREN_OK = frozenset({"system", "exec", "spawn", "passthru", "shell_exec", "popen"})
_STRING = re.compile(
    r'"""(.*?)"""|\'\'\'(.*?)\'\'\'|"((?:\\.|[^"\\])*)"|\'((?:\\.|[^\'\\])*)\'|`((?:\\.|[^`\\])*)`', re.DOTALL
)
_INLINE_FLAG = {
    "python": re.compile(r"-[A-Za-z]*c"),
    "perl": re.compile(r"-[A-Za-z]*[eE]"),
    "ruby": re.compile(r"-[A-Za-z]*e"),
    "lua": re.compile(r"-[A-Za-z]*e"),
    "js": re.compile(r"-(?:e|p|pe|ep)|--eval|--print"),
    "php": re.compile(r"-r"),
}
_VALUE_OPTS = {"python": {"-W", "-X", "-Q"}, "perl": {"-I"}, "ruby": {"-I", "-r"}, "js": {"-r", "--require", "--import", "--loader"},
               "php": {"-d"}, "lua": {"-l"}, "awk": {"-F", "-v"}}


def _lang(name: str) -> str | None:
    if re.fullmatch(r"(?:ipython|pypy|python)[0-9.]*", name):
        return "python"
    return {"perl": "perl", "ruby": "ruby", "irb": "ruby", "node": "js", "nodejs": "js", "bun": "js", "deno": "js",
            "php": "php", "lua": "lua", "luajit": "lua", "awk": "awk", "gawk": "awk", "mawk": "awk",
            "nawk": "awk"}.get(name)


def _strip_leading(words: list[_Word]) -> list[_Word]:
    i = 0
    while i < len(words):
        w = words[i]
        if _ASSIGN.match(w.raw) or (not w.quoted and w.value in _RESERVED):
            i += 1
            continue
        break
    return words[i:]


def _norm_arg(w: _Word) -> str:
    """A git argument as the patterns should see it: an option-looking word unquoted, any other quoted data blanked."""
    if w.quoted and _OPTION_LIKE.fullmatch(w.value):
        return w.value
    return mask_shell_data(w.raw)


def _norm_sub(w: _Word) -> str:
    return w.value if _SUBCOMMAND.fullmatch(w.value) else mask_shell_data(w.raw)


@dataclass
class _GitGlobals:
    """What precedes the subcommand: its index (None = no subcommand), the `-C` directories and the `-c` settings."""

    sub_index: int | None
    dirs: list[str]
    configs: list[str]


def _git_globals(args: list[_Word]) -> _GitGlobals:
    dirs: list[str] = []
    configs: list[str] = []
    i = 0
    while i < len(args):
        a = args[i].value
        if a in ("-C", "-c") and i + 1 < len(args):
            (dirs if a == "-C" else configs).append(args[i + 1].value)
            i += 2
        elif a.startswith("--config-env="):
            configs.append(a.split("=", 1)[1])
            i += 1
        elif a in _GIT_GLOBAL_ARG:
            i += 2
        elif a.startswith("-"):
            i += 1
        else:
            return _GitGlobals(i, dirs, configs)
    return _GitGlobals(None, dirs, configs)


#: Subcommands whose hooks `--no-verify` skips (commit/push/merge/am/rebase/cherry-pick), and those that run hooks at all.
_NOVERIFY_SUBS = frozenset({"commit", "push", "merge", "am", "rebase", "cherry-pick"})
_HOOK_RUNNING = frozenset({"commit", "push", "merge", "am", "rebase", "cherry-pick", "revert", "pull"})
#: Short options where `-n` really means --no-verify: `commit -n`, `am -n`.  (push -n = dry run, merge -n = no-stat,
#: rebase -n = no-stat.)  Per subcommand: (letters whose value is the next word or the cluster's tail, letters whose
#: optional value can only be attached — either way the rest of a cluster is a value, not more flags).
_SHORT_N = {"commit": ("mFCct", "uS"), "am": ("", "CpS")}
_COMMIT_VALUE_LONG = frozenset({"--message", "--file", "--reuse-message", "--reedit-message", "--template", "--author",
                                "--date", "--cleanup", "--fixup", "--squash", "--trailer", "--pathspec-from-file"})


def _is_noverify_long(option: str) -> bool:
    """`--no-verify` or an unambiguous abbreviation of it (git accepts `--no-verif`; `--no-ver` also fits `--no-verbose`)."""
    return option == "--no-verify" or ("--no-verify".startswith(option) and len(option) >= len("--no-veri"))


def _cluster_has_n(cluster: str, next_word: str, attached: str) -> tuple[bool, bool]:
    """(the cluster holds `n` as a flag, the cluster ends on an option that takes the NEXT word as its value)."""
    for pos, letter in enumerate(cluster):
        if letter == "n":
            return True, False
        if letter in next_word:
            return False, pos == len(cluster) - 1
        if letter in attached:
            return False, False
    return False, False


def _noverify_token(sub: str, rest: list[_Word]) -> str | None:
    """The option that makes `git <sub> …` skip its hooks (`--no-verify` / `-n`), or None."""
    next_word, attached = _SHORT_N.get(sub, ("", ""))
    skip = False
    for w in rest:
        a = w.value
        if skip:
            skip = False
        elif a == "--":
            return None
        elif a.startswith("--"):
            name = a.split("=", 1)[0]
            if _is_noverify_long(name):
                return "--no-verify"
            skip = sub == "commit" and name in _COMMIT_VALUE_LONG and "=" not in a
        elif a.startswith("-") and len(a) > 1 and sub in _SHORT_N:
            found, takes_next = _cluster_has_n(a[1:], next_word, attached)
            if found:
                return "-n"
            skip = takes_next
    return None


def _config_sets_hooks_path(setting: str) -> bool:
    return setting.split("=", 1)[0].lower() == "core.hookspath"


def _assignment(word: _Word) -> dict[str, str]:
    name, _, value = word.value.partition("=")
    return {name.rstrip("+"): value}


def _leading_assignments(words: list[_Word]) -> dict[str, str]:
    found: dict[str, str] = {}
    for w in words:
        if _ASSIGN.match(w.raw):
            found.update(_assignment(w))
        elif w.quoted or w.value not in _RESERVED:
            break
    return found


def _env_bypass_forms(env: dict[str, str]) -> list[str]:
    """Environment settings that switch a repository's hooks off (git's own config env, pre-commit, husky)."""
    forms = []
    for name, value in env.items():
        low = value.strip().lower()
        if re.fullmatch(r"GIT_CONFIG_KEY_\d+", name) and low == "core.hookspath":
            forms.append(f"{name}=core.hooksPath")
        elif name == "GIT_CONFIG_PARAMETERS" and "core.hookspath" in low:
            forms.append("GIT_CONFIG_PARAMETERS=core.hooksPath")
        elif name == "SKIP" and low:
            forms.append(f"SKIP={value.strip()}")
        elif name == "PRE_COMMIT_ALLOW_NO_CONFIG" and low == "1":
            forms.append("PRE_COMMIT_ALLOW_NO_CONFIG=1")
        elif name == "HUSKY" and low == "0":
            forms.append("HUSKY=0")
    return forms


def _join_dirs(dirs: list[str]) -> str:
    """The directory `git -C a -C b` ends in (relative ones chain, an absolute one resets); '' = the cwd."""
    out = ""
    for d in dirs:
        out = os.path.join(out, os.path.expandvars(os.path.expanduser(d)))  # `join` lets an absolute path reset
    return out


def _xargs_git_is_safe(sub: str, rest: list[_Word]) -> bool:
    values = [w.value for w in rest]
    if sub == "stash":
        return bool(values) and values[0] in ("list", "show")
    if sub == "restore":
        staged = any(v in ("--staged", "-S") for v in values)
        return staged and not any(v in ("--worktree", "-W") for v in values)
    return False


def _skip_string(code: str, j: int) -> int:
    """Index just past the string literal opening at code[j] (triple quotes and escapes handled)."""
    n = len(code)
    q = code[j]
    if code.startswith(q * 3, j):
        end = code.find(q * 3, j + 3)
        return n if end < 0 else end + 3
    k = j + 1
    while k < n and code[k] != q:
        k += 2 if code[k] == "\\" else 1
    return min(k + 1, n)


def _region(code: str, pos: int, cap: int, parens: bool) -> str:
    """The arguments of an exec call: balanced parentheses (string-aware), or the rest of the statement."""
    n = len(code)
    depth = 0
    j = pos
    while j < n:
        if j - pos > cap:
            raise _Overflow("região de chamada além do orçamento")
        ch = code[j]
        if ch in "\"'`":
            j = _skip_string(code, j)
            continue
        if parens:
            if ch == "(":
                depth += 1
            elif ch == ")":
                depth -= 1
                if depth == 0:
                    return code[pos + 1 : j]
        elif ch in ";\n":
            return code[pos:j]
        j += 1
    return code[pos + 1 :] if parens else code[pos:]


def _literals(region: str) -> list[str]:
    out = []
    for m in _STRING.finditer(region):
        text = next(g for g in m.groups() if g is not None)
        out.append(re.sub(r"\\(.)", lambda e: " " if e.group(1) in "nt" else e.group(1), text))
    return out


class _Analyzer:
    def __init__(self, cmd: str) -> None:
        self.limit = max(WORK_FLOOR, WORK_FACTOR * len(cmd))
        self.work = 0
        self.views: list[str] = []
        self.notes: list[str] = []
        self.env: dict[str, str] = {}  # assignments in force for the command being analysed
        self.cwd = ""  # where `cd` moved the shell, relative to the payload's cwd ("" = not moved)

    def charge(self, units: int) -> None:
        self.work += units
        if self.work > self.limit:
            raise _Overflow(f"trabalho {self.work} > orçamento {self.limit}")

    def text(self, code: str, depth: int) -> None:
        """Analyse `code` as a shell program: its data-masked view, its substitutions, then each command."""
        if depth > MAX_DEPTH:
            raise _Overflow(f"profundidade > {MAX_DEPTH}")
        self.charge(len(code) + 1)
        parsed = _Lexer(code).run()
        self.views.append(_apply(code, parsed.spans))
        outer_env, outer_cwd = self.env, self.cwd
        self.env = dict(outer_env)  # `export X=1` / a bare `X=1` / `cd d` bind for the rest of THIS text and what it runs
        for payload in parsed.substs:
            self.text(payload, depth + 1)
        for k, seg in enumerate(parsed.segs):
            self.charge(8)
            prev = parsed.segs[k - 1] if seg.pipe_in and k > 0 else None
            self.command(seg.words, seg, prev, depth, False)
            self._persist_assignments(seg.words)
        self.env, self.cwd = outer_env, outer_cwd

    def command(self, words: list[_Word], seg: _Seg, prev: _Seg | None, depth: int, via_xargs: bool) -> None:
        saved = self.env
        self.env = {**saved, **_leading_assignments(words)}  # `X=1 cmd`: X for this command (and what it runs) only
        try:
            self._command(words, seg, prev, depth, via_xargs)
        finally:
            self.env = saved

    def _persist_assignments(self, words: list[_Word]) -> None:
        """A bare `X=1` line, or `export X=1`, stays in force for the commands that follow in the same text."""
        rest = _strip_leading(words)
        if not rest:
            self.env = {**self.env, **_leading_assignments(words)}
        elif rest[0].value == "export":
            for w in rest[1:]:
                if _ASSIGN.match(w.raw):
                    self.env = {**self.env, **_assignment(w)}
        elif rest[0].value == "cd":
            operands = [w.value for w in rest[1:] if not (w.value.startswith("-") and len(w.value) > 1)]
            if len(operands) <= 1 and operands != ["-"]:  # `cd -` goes somewhere unknown: keep the last known place
                self.cwd = _join_dirs([self.cwd, operands[0] if operands else "~"])

    def _command(self, words: list[_Word], seg: _Seg, prev: _Seg | None, depth: int, via_xargs: bool) -> None:
        words = _strip_leading(words)
        if not words:
            return
        name = posixpath.basename(words[0].value)
        args = words[1:]
        if name in _WRAPPERS:
            inner = self._unwrap(name, args, depth)
            if inner:
                self.command(inner, seg, prev, depth, via_xargs or name == "xargs")
        elif name in _SHELLS:
            self._shell(args, seg, prev, depth)
        elif name == "git":
            self._git(args, via_xargs)
        elif (lang := _lang(name)) is not None:
            self._interpreter(lang, args, seg, prev, depth)
        else:
            self._program_carrier(name, args, depth)

    def _program_carrier(self, name: str, args: list[_Word], depth: int) -> None:
        """Commands that take a program as a string: eval, watch, find -exec, touring exec|run, su|flock|script -c."""
        handlers = {"eval": self._eval, "watch": self._watch, "find": self._find, "touring": self._touring}
        if name in handlers:
            handlers[name](args, depth)
        elif name in _STRING_RUNNERS:
            self._string_runner(name, args, depth)

    def _eval(self, args: list[_Word], depth: int) -> None:
        if args:
            self.text(" ".join(w.value for w in args), depth + 1)

    def _watch(self, args: list[_Word], depth: int) -> None:
        program = [w.value for w in args if not w.value.startswith("-")]
        if program:
            self.text(" ".join(program), depth + 1)

    def _unwrap(self, name: str, args: list[_Word], depth: int) -> list[_Word] | None:
        """The command a launcher runs, or None when it runs nothing / its program was already analysed."""
        spec = _WRAPPERS[name]
        skip_positional = spec.positional
        shell_mode = False
        i = 0
        while i < len(args):
            a = args[i].value
            if a == "--":
                i += 1
                break
            if a in _QUERY_FLAGS.get(name, ()):
                return None
            if a in _PROGRAM_FLAGS.get(name, ()):
                if i + 1 < len(args):
                    self.text(args[i + 1].value, depth + 1)
                return None
            shell_mode = shell_mode or (name == "sudo" and a in _SUDO_SHELL)
            if name == "env" and _ASSIGN.match(args[i].raw):  # `env X=1 cmd` exports X to cmd
                self.env = {**self.env, **_assignment(args[i])}
            width = _wrapper_option_width(name, spec, args[i])
            if width is None:  # not an option of the launcher: a positional it skips (timeout's DURATION) or the command
                if skip_positional <= 0:
                    break
                skip_positional -= 1
                width = 1
            i += width
        rest = args[i:]
        if shell_mode and rest:
            self.text(" ".join(w.value for w in rest), depth + 1)
            return None
        return rest or None

    def _shell(self, args: list[_Word], seg: _Seg, prev: _Seg | None, depth: int) -> None:
        payload, reads_stdin = _shell_invocation(args)
        if payload is not None:
            self.text(payload, depth + 1)
        elif reads_stdin:
            for code in _stdin_code(seg, prev):
                self.text(code, depth + 1)

    def _string_runner(self, name: str, args: list[_Word], depth: int) -> None:
        flags = _STRING_RUNNERS[name]
        for k, w in enumerate(args):
            if w.value in flags and k + 1 < len(args):
                self.text(args[k + 1].value, depth + 1)
            elif w.value.startswith("--command="):
                self.text(w.value.split("=", 1)[1], depth + 1)

    def _find(self, args: list[_Word], depth: int) -> None:
        i = 0
        while i < len(args):
            if args[i].value in ("-exec", "-execdir", "-ok", "-okdir"):
                j = i + 1
                sub: list[_Word] = []
                while j < len(args) and args[j].value not in (";", "+"):
                    sub.append(args[j])
                    j += 1
                self.command(sub, _Seg(), None, depth, False)
                i = j
            i += 1

    def _touring(self, args: list[_Word], depth: int) -> None:
        """`touring exec '<shell>'` and `touring run --lang L --code '<program>'` run a string."""
        vals = [w.value for w in args]
        if vals[:1] == ["exec"]:
            program = [v for v in vals[1:] if not v.startswith("-")]
            if program:
                self.text(" ".join(program), depth + 1)
        elif vals[:1] == ["run"]:
            lang = _option_value(vals, "--lang")
            code = _option_value(vals, "--code")
            if code is None:
                return
            if lang in (None, "bash", "sh", "shell", "zsh"):
                self.text(code, depth + 1)
            family = _lang(lang or "")
            if family is not None or lang is None:  # no --lang: judge it as a shell AND as a Python program
                self.exec_scan(family or "python", code, depth)

    def _hooks_bypass(self, sub: str, rest: list[_Word], g: _GitGlobals) -> None:
        """Note every way this `git <sub>` has of skipping hooks; block_git.sh decides if the target repo has any."""
        forms = []
        if any(_config_sets_hooks_path(c) for c in g.configs):
            forms.append("git -c core.hooksPath=…")
        if sub in _NOVERIFY_SUBS and (token := _noverify_token(sub, rest)):
            forms.append(f"git {sub} {token}")
        if sub in _HOOK_RUNNING:
            forms.extend(_env_bypass_forms(self.env))
        target = _join_dirs([self.cwd, *g.dirs])
        for form in forms:
            note = f"hooks-bypass:{form}{SEP}{target}"
            if note not in self.notes:
                self.notes.append(note)

    def _git(self, args: list[_Word], via_xargs: bool) -> None:
        g = _git_globals(args)
        idx = g.sub_index
        if idx is None:
            if g.configs and any(_config_sets_hooks_path(c) for c in g.configs):
                self._hooks_bypass("", [], g)
            if via_xargs:
                self.notes.append("xargs-git:?")
            return
        sub = args[idx]
        rest = args[idx + 1 :]
        self._hooks_bypass(sub.value, rest, g)
        line = " ".join(["git", _norm_sub(sub), *(_norm_arg(w) for w in rest)])
        self.charge(len(line) + 1)
        self.views.append(line)
        if via_xargs and sub.value in _XARGS_RISKY and not _xargs_git_is_safe(sub.value, rest):
            self.notes.append(f"xargs-git:{sub.value}")

    def _interpreter(self, lang: str, args: list[_Word], seg: _Seg, prev: _Seg | None, depth: int) -> None:
        codes, reads_stdin = _interpreter_invocation(lang, [w.value for w in args])
        for code in codes:
            self.exec_scan(lang, code, depth)
        if reads_stdin and not codes:
            for code in _stdin_code(seg, prev):
                self.exec_scan(lang, code, depth)

    def _call_region(self, code: str, call: re.Match[str]) -> str | None:
        """The argument text of an exec call (`f(...)`, or `system "..."` without parentheses); None when it has none."""
        pos = call.end()
        while pos < len(code) and code[pos] in " \t":
            pos += 1
        if pos >= len(code):
            return None
        cap = self.limit - self.work
        if code[pos] == "(":
            return _region(code, pos, cap, True)
        if call.group(0).split(".")[-1] in _NO_PAREN_OK and code[pos] in "\"'[`":
            return _region(code, pos, cap, False)
        return None

    def exec_scan(self, lang: str, code: str, depth: int) -> None:
        """The programs an interpreter would hand to a shell: string literals inside exec-call arguments."""
        if depth + 1 > MAX_DEPTH:
            raise _Overflow(f"profundidade > {MAX_DEPTH}")
        self.charge(len(code) + 1)
        for m in _EXEC_CALL.finditer(code):
            region = self._call_region(code, m)
            if region is None:
                continue
            self.charge(len(region) + 1)
            lits = _literals(region)
            if not lits:
                continue
            joined = " ".join(lits)
            self.text(joined, depth + 1)
            for lit in lits:
                if lit != joined and re.search(r"\s", lit):
                    self.text(lit, depth + 1)
        if lang in ("perl", "ruby"):
            for m in re.finditer(r"`([^`]*)`", code):
                self.text(m.group(1), depth + 1)
            for m in re.finditer(r"(?:\bqx|%x)\s*[({\[/|]([^)}\]/|]*)", code):
                self.text(m.group(1), depth + 1)


def _wrapper_option_width(name: str, spec: _Wrap, word: _Word) -> int | None:
    """Words taken by the launcher's own option at `word` (2 = it consumes a value); None when `word` is not one."""
    a = word.value
    if a in spec.arg_opts:
        return 2
    if (a.startswith("-") and len(a) > 1) or (name == "env" and _ASSIGN.match(word.raw)):
        return 1
    return None


def _option_value(vals: list[str], flag: str) -> str | None:
    """The value of `flag V` or `flag=V` among the words, if any."""
    for k, v in enumerate(vals):
        if v == flag and k + 1 < len(vals):
            return vals[k + 1]
        if v.startswith(flag + "="):
            return v.split("=", 1)[1]
    return None


def _shell_invocation(args: list[_Word]) -> tuple[str | None, bool]:
    """(the `-c` program, whether the shell reads its program from stdin) for the arguments of `bash|sh|…`."""
    c_seen = stdin_flag = positional = False
    i = 0
    while i < len(args):
        a = args[i].value
        if a == "--":
            if i + 1 < len(args):
                if c_seen:
                    return args[i + 1].value, False
                positional = args[i + 1].value != "-"
            break
        if a in _SHELL_VALUE_OPTS:
            i += 2
        elif a.startswith("--"):
            i += 1
        elif a[:1] in "-+" and len(a) > 1:
            c_seen = c_seen or "c" in a[1:]
            stdin_flag = stdin_flag or "s" in a[1:]
            i += 1
        else:
            if c_seen:
                return a, False
            positional = a != "-"
            break
    return None, (not c_seen) and (stdin_flag or not positional)


def _awk_programs(vals: list[str]) -> tuple[list[str], bool]:
    """awk takes its program as the first positional word (`-f file` means a file: not inline)."""
    i = 0
    while i < len(vals):
        if vals[i] in ("-F", "-v"):
            i += 2
        elif vals[i] == "-f":
            return [], False
        elif vals[i].startswith("-"):
            i += 1
        else:
            return [vals[i]], False
    return [], True


def _interpreter_invocation(lang: str, vals: list[str]) -> tuple[list[str], bool]:
    """(inline programs, whether the interpreter reads its program from stdin) for an interpreter's arguments.

    The first positional word is a script FILE or module (everything after it belongs to that script, so a
    later `-c` is not ours); a lone `-` is stdin.
    """
    if lang == "awk":
        return _awk_programs(vals)
    inline = _INLINE_FLAG.get(lang)
    codes: list[str] = []
    value_opts = _VALUE_OPTS.get(lang, ())
    i = 0
    while i < len(vals):
        a = vals[i]
        if a == "-":
            return codes, True
        if not a.startswith("-") or (lang == "python" and a == "-m"):
            return codes, False
        if a.startswith(("--eval=", "--print=")):
            codes.append(a.split("=", 1)[1])
        elif inline is not None and inline.fullmatch(a):
            if i + 1 < len(vals):
                codes.append(vals[i + 1])
                i += 1
        elif a in value_opts:
            i += 1
        i += 1
    return codes, not codes


def _stdin_code(seg: _Seg, prev: _Seg | None) -> list[str]:
    """What a stdin-reading shell/interpreter would run: this command's heredocs and here-strings, plus what a
    pipe delivers (the previous command's heredocs, and the arguments of an `echo`/`printf` there)."""
    out = [*seg.heredocs, *seg.herestrings]
    if prev is not None:
        out += [*prev.heredocs, *prev.herestrings]
        pw = _strip_leading(prev.words)
        if pw and posixpath.basename(pw[0].value) in ("echo", "printf"):
            out += [w.value for w in pw[1:]]
    return out


def scan(cmd: str) -> Scan:
    """Every view of `cmd` (data-masked, outermost first) and the NOTES no text pattern can express."""
    an = _Analyzer(cmd)
    try:
        an.text(cmd, 0)
    except _Overflow as exc:
        an.notes.append(f"budget:{exc}")
    except RecursionError:
        an.notes.append("budget:recursão além do limite do interpretador")
    return Scan(an.views, an.notes)


def render(result: Scan) -> str:
    lines = [*result.views, *(f"{NOTE_PREFIX}{note}" for note in result.notes)]
    return "\n".join(lines) + "\n"


# ── D2: would the bypass skip anything? (filesystem facts, resolved only when a bypass NOTE exists) ─────────────────

BYPASS_NOTE = "hooks-bypass:"
_PRECOMMIT_ID = re.compile(r"^\s*-\s*id:\s*(\S+)", re.MULTILINE)
_GIT_TIMEOUT_S = 5
_LIST_CAP = 12
#: The names git runs as hooks (githooks(5), git 2.55.0).  Any other file in the hooks dir never runs: a `.sample`, and
#: the executable backups people keep next to the real hook (~/projects/analise holds `pre-commit.pre-pii-20260925`
#: and four more), which the first D2 listing named as active hooks (17/10/2026).
_GIT_HOOK_NAMES = frozenset((
    "applypatch-msg", "commit-msg", "fsmonitor-watchman", "p4-changelist", "p4-post-changelist",
    "p4-pre-submit", "p4-prepare-changelist", "post-applypatch", "post-checkout", "post-commit",
    "post-index-change", "post-merge", "post-receive", "post-rewrite", "post-update", "pre-applypatch",
    "pre-auto-gc", "pre-commit", "pre-merge-commit", "pre-push", "pre-rebase", "pre-receive",
    "prepare-commit-msg", "proc-receive", "push-to-checkout", "reference-transaction", "sendemail-validate",
    "update",
))


@dataclass
class ActiveHooks:
    repo: str
    hooks_dir: str
    names: list[str]  # executable regular files of hooks_dir named like a githooks(5) hook: what git would run
    framework_ids: list[str]  # hook ids of .pre-commit-config.yaml when pre-commit installed its shim here


def _git_stdout(directory: str, *args: str) -> str | None:
    """`git -C <directory> <args>` stdout; None when git fails.  An empty directory is the cwd (git: `-C ""` is a no-op)."""
    try:
        done = subprocess.run(["git", "-C", directory, *args], capture_output=True, text=True,
                              errors="replace", timeout=_GIT_TIMEOUT_S, check=False)
    except (OSError, subprocess.SubprocessError):
        return None
    return done.stdout.strip() if done.returncode == 0 else None


def _framework_ids(repo: str, hooks_dir: str, names: list[str]) -> list[str]:
    config = Path(repo, ".pre-commit-config.yaml")
    try:
        text = config.read_text(encoding="utf-8", errors="replace")
        installed = any(b"pre-commit" in Path(hooks_dir, n).read_bytes()[:4096] for n in names)
    except OSError:
        return []
    return _PRECOMMIT_ID.findall(text) if installed else []


def active_hooks(repo: str) -> ActiveHooks | None:
    """The hooks git would run in the repository whose top level is `repo`; None when no hook is active."""
    hooks_dir = _git_stdout(repo, "rev-parse", "--git-path", "hooks")  # git itself honors core.hooksPath (and its ~)
    if not hooks_dir:
        return None
    hooks_dir = os.path.join(repo, hooks_dir)  # `join` keeps an absolute path as it is
    try:
        entries = sorted(os.listdir(hooks_dir))
    except OSError:
        return None
    names = [n for n in entries
             if n in _GIT_HOOK_NAMES and os.path.isfile(os.path.join(hooks_dir, n))
             and os.access(os.path.join(hooks_dir, n), os.X_OK)]
    if not names:
        return None
    return ActiveHooks(repo, hooks_dir, names, _framework_ids(repo, hooks_dir, names))


def _describe(h: ActiveHooks) -> str:
    inside = h.hooks_dir.startswith(h.repo.rstrip(os.sep) + os.sep)
    where = os.path.relpath(h.hooks_dir, h.repo) if inside else h.hooks_dir
    extra = len(h.names) - _LIST_CAP
    text = f"hooks ativos em {where}/: {', '.join(h.names[:_LIST_CAP])}" + (f" (+{extra})" if extra > 0 else "")
    if h.framework_ids:
        ids = h.framework_ids[:_LIST_CAP]
        text += f"; framework pre-commit: {', '.join(ids)}" + (f" (+{len(h.framework_ids) - _LIST_CAP})" if len(h.framework_ids) > _LIST_CAP else "")
    return text


def bypass_message(notes: list[str], cwd: str) -> str:
    """The deny text for the bypass NOTES whose target repository has active hooks ('' = nothing at stake).

    One line (the JSON reason has no room for a raw newline).  Names the form, the repository, the hooks that
    would be skipped and the way out: GIT_HOOKS_BYPASS_OK=1, a token of its own — skipping a check loses no data,
    so unlike GIT_DESTRUCTIVE_OK it asks for no snapshot.
    """
    by_repo: dict[str, list[str]] = {}
    found: dict[str, ActiveHooks] = {}
    for note in notes:
        form, _, target = note.partition(BYPASS_NOTE)[2].partition(SEP)
        directory = os.path.join(cwd, target) if target else cwd
        top = _git_stdout(directory, "rev-parse", "--show-toplevel") or _git_stdout(cwd, "rev-parse", "--show-toplevel")
        hooks = active_hooks(top) if top else None
        if top and hooks:
            found[top] = hooks
            if form not in by_repo.setdefault(top, []):
                by_repo[top].append(form)
    parts = [f"{' e '.join(f'`{f}`' for f in forms)} {'contorna' if len(forms) == 1 else 'contornam'} os hooks do "
             f"repositório {top} ({_describe(found[top])})" for top, forms in by_repo.items()]
    if not parts:
        return ""
    return ("Contorno de hooks do git: " + "; ".join(parts) + ". Contornar não perde dado, mas desliga a verificação que o "
            "repositório exige antes do commit/push. Rode sem a opção e corrija o que o hook reporta; se o contorno for "
            "mesmo intencional, prefixe o comando com GIT_HOOKS_BYPASS_OK=1 (token por comando, diferente de "
            "GIT_DESTRUCTIVE_OK=1: não exige snapshot).")


def resolve_main(cwd: str) -> int:
    """stdin: the hooks-bypass NOTE lines of a scan; prints bypass_message(...) for `cwd` ('' = allow)."""
    lines = [ln for ln in sys.stdin.read().split("\n") if BYPASS_NOTE in ln]
    sys.stdout.write(bypass_message(lines, cwd))
    return 0


def main() -> int:
    raw = sys.stdin.buffer.read().decode("utf-8", "surrogateescape")
    sys.stdout.buffer.write(render(scan(raw)).encode("utf-8", "surrogateescape"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
