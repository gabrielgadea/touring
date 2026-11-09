//! What a command IS: the shell-aware classification the gates decide on.
//!
//! Wrappers (`time`, `sudo`, `env`…) are transparent, `cd` and sequences are walked step by
//! step, redirects are read after the CEG shell mask (quoted text and heredoc bodies are data),
//! and every classifier fails toward "not an inspection" — the loose direction.

use std::path::Path;
use std::sync::OnceLock;

use super::*;

/// True iff `command` starts an atomic content/file search that `ctx_execute`
/// could fold into one pass.
/// `VAR=value` prefix (including the bypass `TOURING_GATE_OK=1`): the class belongs
/// to the executed command, not to the environment that precedes it. Predicate identical to
/// what `scan_class_of` always used — extracted so the 5 sites share it.
pub(super) fn is_env_assignment(t: &str) -> bool {
    t.contains('=')
        && t.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
}

/// Execution wrapper recognized by the resolver: flags that take a value +
/// how many positionals the wrapper ITSELF consumes (the `timeout` duration, the
/// `taskset` mask, the `chrt` priority).
pub(super) struct WrapperSpec {
    value_flags: &'static [&'static str],
    positionals: usize,
}

/// S1 (26/08/2026) — the classifiers read the 1st token: `time grep`,
/// `nice -n 5 find`, `sudo cat`, `env F=1 rg` escaped ALL the gates and
/// nudges at once (coverage was born at zero where the class is born).
pub(super) fn wrapper_spec(verb: &str) -> Option<WrapperSpec> {
    Some(match verb {
        "env" => WrapperSpec {
            value_flags: &["-u", "--unset", "-C", "--chdir", "-S", "--split-string"],
            positionals: 0,
        },
        "time" => WrapperSpec {
            value_flags: &["-o", "--output", "-f", "--format"],
            positionals: 0,
        },
        "nice" => WrapperSpec {
            value_flags: &["-n", "--adjustment"],
            positionals: 0,
        },
        "ionice" => WrapperSpec {
            value_flags: &["-c", "-n", "-p", "-P"],
            positionals: 0,
        },
        "sudo" => WrapperSpec {
            value_flags: &[
                "-u", "-g", "-h", "-p", "-C", "-r", "-t", "-T", "-U", "-D", "-R", "--user",
                "--group", "--host", "--prompt", "--role", "--type", "--chdir", "--chroot",
            ],
            positionals: 0,
        },
        "command" | "builtin" => WrapperSpec {
            value_flags: &[],
            positionals: 0,
        },
        "timeout" => WrapperSpec {
            value_flags: &["-s", "--signal", "-k", "--kill-after"],
            positionals: 1,
        },
        "stdbuf" => WrapperSpec {
            value_flags: &["-i", "-o", "-e"],
            positionals: 0,
        },
        "taskset" => WrapperSpec {
            value_flags: &["-c", "--cpu-list"],
            positionals: 1,
        },
        "chrt" => WrapperSpec {
            value_flags: &[],
            positionals: 1,
        },
        _ => return None,
    })
}

/// Consumes the wrapper at `toks[i]` (verb + flags + its positionals) and returns
/// the next index; `i` unchanged when `toks[i]` is not a wrapper. A flag
/// identical to the one in the table consumes the value in the next token (`-n 5`); the
/// glued form (`-n5`) and the long one with `=` (`--signal=KILL`) consume a single token.
pub(super) fn skip_wrapper(toks: &[&str], i: usize) -> usize {
    let Some(spec) = toks.get(i).and_then(|v| wrapper_spec(v)) else {
        return i;
    };
    let mut j = i + 1;
    let mut positionals = spec.positionals;
    while j < toks.len() {
        let t = toks[j];
        if is_env_assignment(t) {
            j += 1; // `sudo FOO=1 cmd` / the assignments of `env` itself
            continue;
        }
        if t.starts_with('-') && t != "-" {
            j += if spec.value_flags.contains(&t) { 2 } else { 1 };
            continue;
        }
        if positionals > 0 {
            positionals -= 1;
            j += 1;
            continue;
        }
        break; // the real verb
    }
    j
}

/// Tokens starting from the REAL VERB: `VAR=value` prefixes and execution wrappers
/// (`env`/`time`/`nice`/`sudo`/`command`/`builtin`/`timeout`/`stdbuf`/
/// `ionice`/`taskset`/`chrt`, chained in any order) removed. Empty
/// when the command is only a shell (a bare `env` prints the environment). A single
/// resolver for all the classifiers — fixing only one site masked
/// the defect once (memory `definer-module-cinco-sitios`); not twice.
pub(super) fn resolved_tokens<'a, 'b>(toks: &'b [&'a str]) -> &'b [&'a str] {
    let mut i = 0;
    while i < toks.len() {
        if is_env_assignment(toks[i]) {
            i += 1;
            continue;
        }
        let j = skip_wrapper(toks, i);
        if j == i {
            break;
        }
        i = j;
    }
    &toks[i..]
}

/// Command segments separated by sequence operators (`&&`, `||`,
/// `;`, newline). Naive about quotes (`grep 'a;b'` cuts inside the
/// quote) — safe for CLASSIFICATION: the segment's verb does not change with the
/// cut, which is the only use (S1, 26/08).
pub(super) fn command_segments(cmd: &str) -> Vec<&str> {
    cmd.split(['\n', ';'])
        .flat_map(|part| part.split("&&"))
        .flat_map(|part| part.split("||"))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// Effective tokens: segments by sequence operators, skips `cd <dir>`
/// segments (navigation prefix — `cd /x && grep foo` is `grep`), and
/// resolves wrappers in the first segment that is not only navigation. All `cd`
/// (or nothing) returns empty. It is resolved_tokens + the gap that the S1
/// strategy named (`cd <dir> (&&|;\n)`) and the first delivery let through.
pub(crate) fn effective_tokens(cmd: &str) -> Vec<&str> {
    for seg in command_segments(cmd) {
        let toks: Vec<&str> = seg.split_whitespace().collect();
        let rest = resolved_tokens(&toks);
        match rest.first() {
            Some(&"cd") => continue, // navigation prefix — the next one decides
            // A segment that resolves to NOTHING (a lone `P=x`, a bare `env`): the
            // next segment decides. Without this arm, `P=x\npython3 f.py`
            // returned empty and ALL classification died at the assignment —
            // proven live (29/08): 12 python executions behind
            // `VAR=...\n` and zero G10 denies in the 60-turn run of `analise`.
            None => continue,
            _ => return rest.to_vec(),
        }
    }
    Vec::new()
}

/// The file an inline `sed -i`/`awk -i inplace`/`perl -pi` edit targets — the last
/// path-like token (skips flags and the substitution script). Lets the Edit tool /
/// ast-meta nudge carry the real path instead of `<file>`. `None` when not derivable.
/// N3a (26/08/2026) — blind inline write: `sed -i`/`sed --in-place`/
/// `awk -i inplace`/`perl -pi` IN COMMAND POSITION (start, `|`, `;`,
/// `&&`/`&`, `$(`, newline). The anchor in command position separates the
/// invocation from the text — `echo "run sed -i here"` is prose, not a write.
/// Known gap (documented, outside the N3a scope): `find -exec sed -i`
/// and `xargs sed -i` are not in command position.
pub(crate) fn is_inline_blind_edit(cmd: &str) -> bool {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"(?:^|[|;&\n]|\$\()\s*(?:sed\s+-[a-zA-Z]*i(?:\.\S*)?(?:\s|$)|sed\s+--in-place\b|awk\s+-i\s+inplace\b|perl\s+-pi\b)",
        )
        .expect("G9 regex compila")
    })
    .is_match(cmd)
}

/// S4 (26/08) — class of the homogeneous executor (R4): `python3`/
/// `.venv/bin/python3`/`pytest` & co — the unrolled burst that no gate
/// named (75 serial calls in session dc87e4e0, 5.8% adoption).
/// `python3 -c` is left out (the CEG advisory owns the inline). Marked
/// mutation is left out by construction (P2.3): shell redirect,
/// installers, git, rm — the marker is the calibration, not proof of purity
/// (the limitation is documented, as in G8).
/// The view of the command that the mutation filter inspects: heredoc bodies
/// removed (they are DATA feeding stdin, not a command) and harmless FD/discard
/// redirects neutralized (`2>&1`, `2>/dev/null`, `>/dev/null`, …) — they
/// write no file at all. Any `>` that remains is a real redirect (a write), and the
/// command stays out of the exec burst, as the tests require. Hole proven
/// live (29/08): the python executions of the 60-turn run of `analise` carried
/// `2>&1`/`2>/dev/null` and the `contains(">")` over the whole blob nullified the
/// class — G10 went mute for the whole turn ("the verifier uses less than the
/// extractor", yet another incarnation).
pub(super) fn mutation_scan_view(cmd: &str) -> String {
    // 1) cut heredoc bodies: from the line after `<<MARKER` to the MARKER line.
    let mut kept: Vec<&str> = Vec::new();
    let mut skip_until: Option<String> = None;
    for line in cmd.lines() {
        if let Some(marker) = &skip_until {
            if line.trim() == marker.as_str() {
                skip_until = None;
            }
            continue;
        }
        kept.push(line);
        if let Some(pos) = line.find("<<")
            && !line[pos..].starts_with("<<<")
        {
            let raw = line[pos + 2..].trim_start_matches('-').trim();
            let marker = raw
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches(|c| c == '\'' || c == '"');
            if !marker.is_empty() {
                skip_until = Some(marker.to_string());
            }
        }
    }
    let mut s = kept.join("\n");
    // 2) neutralize redirects that write nothing (order: the FD-prefixed ones
    // before the generic one, otherwise the partial replace leaves the digit behind).
    for inofensivo in [
        "2>&1",
        "1>&2",
        "2>/dev/null",
        "2> /dev/null",
        "&>/dev/null",
        "&> /dev/null",
        ">/dev/null",
        "> /dev/null",
    ] {
        s = s.replace(inofensivo, " ");
    }
    s
}

pub(super) fn exec_class_of(cmd: &str) -> Option<&'static str> {
    const MUTATING: &[&str] = &[
        ">",
        "| tee",
        "pip install",
        "setup.py install",
        "rm ",
        "mv ",
        "cp ",
        "git ",
        "kill",
        "chmod",
        "chown",
        "curl",
        "wget",
        "ssh",
        "docker",
        "systemctl",
        "touch ",
        "mkdir",
    ];
    let scan = mutation_scan_view(cmd);
    if MUTATING.iter().any(|m| scan.contains(m)) {
        return None;
    }
    let rest = effective_tokens(cmd);
    let verb = rest.first()?;
    let base = verb.rsplit('/').next()?;
    if base == "python" || base == "python3" || base.starts_with("python3.") {
        // Inline (`-c` / `- <<EOF`) entered the burst on 29/08 (tightening by
        // Gabriel) as its OWN class: the historical exclusion existed because
        // R9 would crush a multi-line heredoc in the remedy — python-inline has a
        // 1:1 remedy (the verbatim body in `touring run --lang python`), so
        // the reason fell. The 5 heredocs of the 60-turn run of analise were left out.
        if matches!(rest.get(1), Some(&"-c") | Some(&"-")) {
            return Some("python-inline");
        }
        return Some("python");
    }
    if base.starts_with("pytest") {
        return Some("pytest");
    }
    None
}

pub(super) fn is_script_path(p: &str) -> bool {
    let p = p.trim_matches(|c| c == '\'' || c == '"');
    p.ends_with(".py") || p.ends_with(".sh")
}

pub(super) fn clean_script_path(p: &str) -> String {
    p.trim_matches(|c| c == '\'' || c == '"').to_string()
}

/// The script path (.py/.sh) that this command WRITES via `cat >`/`tee`.
/// Scans the VIEW (heredoc bodies removed): a `cat > y.py` inside the
/// body is data, not a write.
pub(super) fn script_write_target(cmd: &str) -> Option<String> {
    let view = mutation_scan_view(cmd);
    for seg in command_segments(&view) {
        let toks: Vec<&str> = seg.split_whitespace().collect();
        let rest = resolved_tokens(&toks);
        match rest.first() {
            Some(&"cat") => {
                let mut it = rest[1..].iter().copied();
                while let Some(t) = it.next() {
                    if t == ">" || t == ">>" {
                        if let Some(p) = it.next()
                            && is_script_path(p)
                        {
                            return Some(clean_script_path(p));
                        }
                    } else if let Some(p) = t.strip_prefix(">>").or_else(|| t.strip_prefix('>'))
                        && !p.is_empty()
                        && is_script_path(p)
                    {
                        return Some(clean_script_path(p));
                    }
                }
            }
            Some(&"tee") => {
                if let Some(p) = rest[1..].iter().find(|t| !t.starts_with('-'))
                    && is_script_path(p)
                {
                    return Some(clean_script_path(p));
                }
            }
            _ => {}
        }
    }
    None
}

/// The script paths that this command EXECUTES directly (python/bash/sh),
/// in ANY segment — the real pattern of `analise` (24 of 60) wrote AND
/// executed in the SAME multi-line tool_use (`cat > x.py <<EOF…EOF` +
/// `python3 x.py`), and a detector that only looks at the first effective verb sees the
/// `cat` and never the run. Scans the VIEW (heredoc out); a segment with a real
/// output redirect is skipped — the `--file` remedy would not reproduce the write.
pub(super) fn script_run_targets(cmd: &str) -> Vec<String> {
    let view = mutation_scan_view(cmd);
    let mut out: Vec<String> = Vec::new();
    for seg in command_segments(&view) {
        if seg.contains('>') {
            continue;
        }
        let toks: Vec<&str> = seg.split_whitespace().collect();
        let rest = resolved_tokens(&toks);
        let Some(verb) = rest.first() else { continue };
        let base = verb.rsplit('/').next().unwrap_or(verb);
        let interpretador = base == "bash"
            || base == "sh"
            || base == "python"
            || base == "python3"
            || base.starts_with("python3.");
        if !interpretador {
            continue;
        }
        if let Some(arg) = rest[1..].iter().find(|t| !t.starts_with('-'))
            && is_script_path(arg)
            && !out.iter().any(|p| p == &clean_script_path(arg))
        {
            out.push(clean_script_path(arg));
        }
    }
    out
}

/// The body of the inline program (`python3 -c '<code>'` or `python3 - <<'M'`),
/// for the 1:1 remedy of the `python-inline` class.
pub(super) fn python_inline_body(cmd: &str) -> Option<String> {
    // heredoc: body between the line after `<<MARKER` and the MARKER line
    if let Some(pos) = cmd.find("<<")
        && !cmd[pos..].starts_with("<<<")
    {
        let first_line_end = pos + cmd[pos..].find('\n')?;
        let marker = cmd[pos + 2..first_line_end]
            .trim_start_matches('-')
            .split_whitespace()
            .next()?
            .trim_matches(|c| c == '\'' || c == '"')
            .to_string();
        let body: Vec<&str> = cmd[first_line_end + 1..]
            .lines()
            .take_while(|l| l.trim() != marker)
            .collect();
        return Some(body.join("\n"));
    }
    // -c: the literal after the flag (heuristic: the outer quote pair)
    let idx = cmd.find(" -c ")?;
    let rest = cmd[idx + 4..].trim();
    let quote = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let inner = &rest[1..];
    let end = inner.rfind(quote)?;
    Some(inner[..end].to_string())
}

/// The class of READ-ONLY python-inline inside the inspection burst (tightening
/// 29/08, Gabriel's order: *"read-only falls into the inspection burst"*). Distinct
/// in the ledger — the key is per class — and outside `CODE_MODE_COLLAPSED_CLASSES`,
/// which documents the classes that `scan_class_of` emits; this one comes from the BODY of the
/// program, not from the command name.
pub(super) const PY_INLINE_INSPECT_CLASS: &str = "python-inline";

/// Signs that a python body is NOT pure reading — conservative deny-list:
/// FS writes, mutating `open` modes, subprocess, network,
/// dynamic import/exec and DB (opening SQLite changes lock state even when reading).
/// A false "writer" (the mark appears in an innocent string) only hands the
/// command back to G10, which is looser — the safe direction; the reverse would route a
/// writer to the inspection deny.
pub(super) const PY_INLINE_WRITER_MARKS: &[&str] = &[
    ".write(",
    ".writelines(",
    "write_text(",
    "write_bytes(",
    ".unlink(",
    ".touch(",
    ".chmod(",
    ".rename(",
    ".rmdir(",
    "os.remove",
    "os.rename",
    "os.replace",
    "os.rmdir",
    "os.removedirs",
    "os.mkdir",
    "os.makedirs",
    "os.symlink",
    "os.link",
    "os.truncate",
    "os.putenv",
    "shutil.",
    "tempfile.",
    "mode='w",
    "mode=\"w",
    "mode='a",
    "mode=\"a",
    "mode='x",
    "mode=\"x",
    "subprocess",
    "os.system",
    "os.popen",
    "os.exec",
    "os.spawn",
    "pty.",
    "socket",
    "urllib",
    "requests",
    "http.client",
    "httpx",
    "ftplib",
    "smtplib",
    "exec(",
    "eval(",
    "__import__",
    "importlib",
    "sqlite3",
    "dbm.",
    "shelve",
];

/// `Some(PY_INLINE_INSPECT_CLASS)` when the command is a python-inline whose
/// body is pure reading — and therefore INSPECTION, subject to the 2nd/300s burst. A
/// body that cannot be extracted or that has any write mark is left out (G10 is the
/// backstop, 5th/600s).
pub(super) fn python_inline_readonly_class(cmd: &str) -> Option<&'static str> {
    if exec_class_of(cmd) != Some("python-inline") {
        return None;
    }
    let body = python_inline_body(cmd)?;
    if body.is_empty()
        || PY_INLINE_WRITER_MARKS.iter().any(|m| body.contains(m))
        || py_open_mutating_mode(&body)
    {
        return None;
    }
    Some(PY_INLINE_INSPECT_CLASS)
}

/// `open(...)` with a LITERAL 2nd argument containing `w`/`a`/`x`/`+` — the
/// mutating mode. An `open(f)`/`open(f, encoding=…)` is a read and passes; the mark by
/// loose quotes (`'w'`) was tried and bit the test itself (`re.findall(
/// "x", …)` classified as a writer). Residual false positive (a comma from
/// another call followed by a literal with w/a/x) only hands the command back to G10.
pub(super) fn py_open_mutating_mode(body: &str) -> bool {
    let mut rest = body;
    while let Some(i) = rest.find("open(") {
        let after = &rest[i + 5..];
        if let Some(c) = after.find(',') {
            let tail = after[c + 1..].trim_start();
            if let Some(q) = tail.chars().next().filter(|ch| *ch == '\'' || *ch == '"') {
                let inner = &tail[1..];
                if let Some(end) = inner.find(q)
                    && inner[..end].contains(['w', 'a', 'x', '+'])
                {
                    return true;
                }
            }
        }
        rest = after;
    }
    false
}

/// The inspection classes that the burst ledger tracks — ALL those
/// that [`scan_class_of`] recognizes.
///
/// The fixed list that used to be here (`grep`/`cat`/`find`) was wrong in BOTH directions,
/// and the same measurement shows both ends: `find` is 56.2% isolated and has not
/// ONE burst ≥3 in 115 transcripts — denying it by class was pure friction; whereas
/// `sed-n` (408 calls, 81.4% of volume in bursts) and `ls` (338, 71.3%)
/// always passed, and were the largest uncaptured fan-out volume. The burst
/// predicate dispenses with the list: it discriminates by what the class DOES in this window,
/// not by its name.
pub(super) const CODE_MODE_COLLAPSED_CLASSES: &[&str] =
    &["grep", "cat", "find", "ls", "wc", "sed-n"];

pub(crate) fn inline_edit_target(command: &str) -> Option<String> {
    command
        .split_whitespace()
        .map(|t| t.trim_matches(|c| c == '"' || c == '\''))
        .filter(|t| !t.starts_with('-'))
        .rev()
        .find(|t| t.contains('/') || (t.contains('.') && !t.contains('*')))
        .map(std::string::ToString::to_string)
}

/// W1 S-1.2 — live-state commands: identical calls legitimately
/// return different results (health, daemon, working tree, counters).
/// EXPLICIT allowlist, not a heuristic; widen only with data (S-7.2).
pub(super) const G6_LIVE_STATE_ALLOWLIST: &[&str] = &[
    "touring doctor",
    "touring status",
    "touring daemon-ctl",
    "touring gate-metrics",
    "touring kpi",
    "git status",
    "git diff",
];

pub(super) fn g6_allowlisted(cmd: &str) -> bool {
    let trimmed = cmd.trim_start();
    G6_LIVE_STATE_ALLOWLIST
        .iter()
        .any(|p| trimmed.starts_with(p))
}

/// S213 — whether `cmd` APPENDS to a file (`>>` or `tee -a`/`--append`), read after the CEG
/// shell mask so a `>>` inside quotes or a heredoc body is data, not a redirect.
pub(super) fn appends_to_a_file(cmd: &str) -> bool {
    let masked = touring_ceg::gateway::gate::mask_shell_data(cmd);
    masked.contains(">>") || masked.contains("tee -a") || masked.contains("tee --append")
}

/// W2 S-2.1 — class of the atomic inspection (the taxonomy of the simulation). `None`
/// for commands that are not inspection — the burst only counts what inspects.
pub(crate) fn scan_class_of(cmd: &str) -> Option<&'static str> {
    // REAL verb after segments/cd-prefixes/wrappers (S1, 26/08): `time grep`
    // is `grep`, `cd /x && cat f` is `cat` — before, the 1st token decided, and
    // `time`/`nice`/`sudo`/`env`/`cd` nullified the class for the 4 gates that
    // consume this classifier all at once.
    // S213: an APPENDING command is never inspection — the burst remedy replays the
    // commands it fuses, and an append replayed duplicates what it wrote (observed live:
    // `cat t.rs >> src/x.rs && cargo test`). An overwrite redirect stays inspection (P2.3):
    // replaying it is idempotent. Read through the CEG shell mask — quoted/heredoc is data.
    if appends_to_a_file(cmd) {
        return None;
    }
    // S213b (26/09/2026): a SEQUENCE is inspection only if every step inspects — the
    // class came from the first real verb alone, so `grep x f; cargo clippy …` was
    // `grep` and the remedy replayed a 20-minute clippy (observed live); a `touring
    // memory store` in its place would have been a replayed write. Navigation,
    // assignments and `echo`-style separators are neutral; the class is the first
    // reading step's.
    let mut class = None;
    for seg in command_segments(cmd) {
        let toks: Vec<&str> = seg.split_whitespace().collect();
        let rest = resolved_tokens(&toks);
        match rest.first() {
            None => {}
            Some(verb) if NEUTRAL_SEQUENCE_STEPS.contains(verb) => {}
            Some(_) => {
                let step = step_class(rest)?;
                class.get_or_insert(step);
            }
        }
    }
    class
}

/// Steps that neither inspect nor do work — they never break an inspection sequence.
pub(super) const NEUTRAL_SEQUENCE_STEPS: &[&str] = &["cd", "echo", "printf", "pwd", "true"];

/// The inspection class of ONE step (its wrapper-resolved tokens), or `None` when the
/// step does something other than inspect.
pub(super) fn step_class(rest: &[&str]) -> Option<&'static str> {
    let first = *rest.first()?;
    match first {
        "grep" | "rg" => Some("grep"),
        // P2.3 (calibration 25/08, 1,312 real calls): `cat > f`/`cat >> f`
        // (write heredoc) is T4 — a write — not inspection. The prefix alone
        // denied 27 writes (20.6% of what the matrix would catch) under the `code` mode,
        // which denies on the 1st call. If the first non-flag operand is already a
        // redirect, there is no file to read: it is not the `cat` class.
        "cat" | "head" | "tail" | "less" => {
            // The first REAL operand decides: flags (`-n`) — and the argument
            // of those that take a value (`-n 5`, `-c +3`) — are transparent. If what
            // is left already starts with `>`, there is no file to read.
            let mut it = rest[1..].iter().copied().peekable();
            let mut primeiro_operando: Option<&str> = None;
            while let Some(t) = it.next() {
                if t == "-n" || t == "-c" {
                    it.next(); // the flag's argument, not an operand
                    continue;
                }
                if t.starts_with('-') {
                    continue;
                }
                primeiro_operando = Some(t);
                break;
            }
            match primeiro_operando {
                Some(op) if op.starts_with('>') => None,
                _ => Some("cat"),
            }
        }
        "find" | "fd" => Some("find"),
        "ls" => Some("ls"),
        "wc" => Some("wc"),
        // the sed's `-n`, not the wrapper's: `nice -n 5 sed 5p f` is NOT sed-n
        // (with the verb resolved, the `contains` on the whole command would inherit the
        // `nice`'s `-n` — the latent bug that the resolution exposes).
        "sed" if rest[1..].contains(&"-n") => Some("sed-n"),
        _ => None,
    }
}

/// The stamp of `fp` now; `None` when the file does not exist or cannot be measured (G7 then
/// keeps counting as it always counted — without a measurement there is nothing to assert).
pub(super) fn file_stamp(project_root: &Path, fp: &str) -> Option<FileStamp> {
    let path = Path::new(fp);
    let meta = if path.is_absolute() {
        std::fs::metadata(path)
    } else {
        std::fs::metadata(project_root.join(path))
    }
    .ok()?;
    let mtime_ns = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(FileStamp {
        mtime_ns,
        len: meta.len(),
    })
}

/// V0e3 (09/10/2026) — the path fragments of a command: split on every
/// char that is not a path char and keep the fragments that contain `/`. The G7
/// reset never worked in the real case because the edge `trim_matches` covered
/// only `'\"[]{},` — a path quoted INSIDE code (`Path("/…")`, run
/// 1791491632099 of 6b) did not match the recorded key and the next read was
/// denied with a false promise in the message (D8).
pub(super) fn path_fragments(cmd: &str) -> impl Iterator<Item = &str> {
    cmd.split(|c: char| !(c.is_alphanumeric() || matches!(c, '.' | '/' | '_' | '-' | '~')))
        .filter(|frag| frag.len() > 1 && frag.contains('/'))
}

/// W6 S-6.3 — ADDED comment line with a counterfactual modal and no
/// run_id in the vicinity: the never-exercised claim (the worst error of 24/08).
pub(super) fn counterfactual_comment(added: &str) -> bool {
    const MODAIS: &[&str] = &["seria ", "quebraria", "impediria", "faria com que"];
    added.lines().any(|l| {
        let t = l.trim_start();
        let comentario = t.starts_with("//") || t.starts_with('#') || t.starts_with('*');
        comentario && MODAIS.iter().any(|m| t.contains(m)) && !t.contains("run-")
    })
}

/// W2 S-2.1 — the burst gate. `Some(response)` short-circuits; `None` lets
/// the flow (including the legacy advisory of the 3rd search) proceed.
/// Fraction of tokens that two commands share (Jaccard over the multiset
/// of tokens), in `[0.0, 1.0]`.
///
/// Calibration proposed by Gabriel (26/08/2026): *"if the command repeats at least
/// 50% of the previous command"*. Fills a real hole between the gates —
/// G6 catches only the byte-identical, G7 only the re-inspection of the SAME file, and G1
/// only when it accumulates N of the same class. `grep X a.rs` followed by `grep X b.rs` is the
/// canonical signature of serial fan-out and was not caught by any of them.
///
/// Jaccard over tokens, and not a common prefix: `sed -n 1,20p f` vs
/// `sed -n 40,60p f` share almost everything but diverge early in the text, and a
/// prefix would judge them distinct. Order should not matter either — what
/// matters is how much of the work repeats.
pub(crate) fn command_similarity(a: &str, b: &str) -> f64 {
    let toks = |c: &str| -> std::collections::BTreeSet<String> {
        c.split_whitespace().map(str::to_string).collect()
    };
    let (ta, tb) = (toks(a), toks(b));
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let inter = ta.intersection(&tb).count() as f64;
    let uniao = ta.union(&tb).count() as f64;
    inter / uniao
}

/// G8 — the pure-inspection loop becomes ONE sweep in the sandbox.
///
/// Returns the rewritten command, or `None` when the loop is not a candidate. Pure
/// (no I/O) so it can be exercised in a unit test — the predicate that decides has to
/// be easier to test than to work around.
///
/// In: `for`/`while` whose body only inspects (grep/cat/head/tail/ls/find/
/// echo/wc/awk/sed/jq/stat/basename). Out, by construction: everything that
/// has an effect (kill, git, cargo, rm, mv, install, deploy), redirects to a
/// file, uses a heredoc (the body is data, not a program) or is already code mode.
pub(crate) fn loop_rewrite_candidate(cmd: &str) -> Option<String> {
    if std::env::var("TOURING_G8_REWRITE_DISABLED").map(|v| v == "1") == Ok(true) {
        return None;
    }
    let trimmed = cmd.trim();
    // Already code mode / master CLI — nothing to convert.
    if trimmed.contains("touring run") || trimmed.contains("touring exec") {
        return None;
    }
    // Heredoc: the body is DATA (same reason as G2), and the quoting does not survive.
    if trimmed.contains("<<") {
        return None;
    }
    // Unbalanced single quotes would break the `--code '...'`.
    if !trimmed.matches('\'').count().is_multiple_of(2) {
        return None;
    }
    // Needs a real loop — `for x in` / `while `.
    let has_loop = trimmed.split(['\n', ';', '|', '&']).any(|part| {
        let p = part.trim_start();
        p.starts_with("for ") && p.contains(" in ") || p.starts_with("while ")
    });
    if !has_loop {
        return None;
    }
    // Any verb with a side effect takes the command out of scope.
    const EFFECTFUL: &[&str] = &[
        "kill",
        "pkill",
        "git ",
        "cargo",
        "rm ",
        "mv ",
        "cp ",
        "install",
        "update-touring",
        "npm",
        "make ",
        "chmod",
        "chown",
        "mkdir",
        "touch ",
        "tee ",
        "curl",
        "wget",
        "ssh",
        "docker",
        "systemctl",
        "sudo",
        "python3 -c",
        "python3 -m",
        "pip",
        ">>",
        "> /",
    ];
    if EFFECTFUL.iter().any(|verb| trimmed.contains(verb)) {
        return None;
    }
    // Size limit: above it the rewritten command pollutes more than it helps.
    if trimmed.len() > 1200 {
        return None;
    }
    let escaped = trimmed.replace('\'', r"'\''");
    Some(format!("touring run --lang bash --code '{escaped}'"))
}

/// V0e5 (09/10/2026) — entries of ANOTHER process's `/proc/<pid>/` that the sandbox
/// cannot read while a native shell can: guarded by the kernel's ptrace access mode
/// (proc(5)), not by landlock — `/proc` itself is a read root. Measured on 30.4.94
/// against a live daemon, sandbox × native; listed only where the two DIVERGE
/// (`stack`, `syscall`, `pagemap`, `mem` fail on both sides, so they say nothing;
/// `cmdline`, `status`, `stat`, `comm`, `limits` and the `fd/` listing read on both).
pub(super) const PTRACE_GUARDED_PROC_ENTRIES: &[&str] = &[
    "environ",
    "maps",
    "smaps",
    "smaps_rollup",
    "auxv",
    "cwd",
    "exe",
    "root",
    "io",
    "map_files",
];

/// Directories whose LISTING reads in the sandbox but whose entries (`fd/3`) do not.
pub(super) const PTRACE_GUARDED_PROC_DIR_ENTRIES: &[&str] = &["fd", "fdinfo"];

/// The `/proc/<pid>/<entry>` token, when `<pid>` is another process (a literal, `$p`,
/// `${pid}` or a glob — anything but `self`/`thread-self`) and `<entry>` is guarded.
pub(super) fn ptrace_guarded_proc_entry(tok: &str) -> Option<std::path::PathBuf> {
    let (pid, tail) = tok.strip_prefix("/proc/")?.split_once('/')?;
    if pid == "self" || pid == "thread-self" {
        return None;
    }
    let mut parts = tail.split('/');
    let entry = parts.next().unwrap_or("");
    let below = parts.next().is_some_and(|p| !p.is_empty());
    let guarded = PTRACE_GUARDED_PROC_ENTRIES.contains(&entry)
        || (below && PTRACE_GUARDED_PROC_DIR_ENTRIES.contains(&entry));
    guarded.then(|| std::path::PathBuf::from(tok))
}

/// S24 (plan 2026-09-25) — the first path `cmd` names that a sandboxed program cannot
/// reach, judged by the CEG's own landlock table
/// ([`touring_ceg::gateway::sandbox_executor::sandbox_can_reach`] — one predicate, D8).
///
/// A command naming such a path is never rewritten into `touring run` nor denied toward
/// it (G1/G8/G10/T3-B): the route would not execute. Measured 25/09: the gate rewrote a
/// `.touring/bin/touring decompose …` loop into a run that failed with "Permission
/// denied" and still reported `success: true`.
pub(super) fn unreachable_in_sandbox(project_root: &Path, cmd: &str) -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    for raw in cmd.split(|c: char| c.is_whitespace() || ";|&()<>`".contains(c)) {
        let tok = raw.trim_matches(|c: char| "'\"{}[],".contains(c));
        let tok = tok.rsplit_once('=').map_or(tok, |(_, v)| v);
        if let Some(guarded) = ptrace_guarded_proc_entry(tok) {
            return Some(guarded);
        }
        if tok.is_empty() || tok.contains("://") || !(tok.contains('/') || tok.starts_with('~')) {
            continue;
        }
        let expanded = match (
            tok.strip_prefix("~/")
                .or_else(|| tok.strip_prefix("$HOME/")),
            &home,
        ) {
            (Some(rest), Some(h)) => h.join(rest),
            _ => std::path::PathBuf::from(tok),
        };
        let path = if expanded.is_absolute() {
            expanded
        } else {
            project_root.join(expanded)
        };
        // Only an EXISTING path is evidence: a missing one fails inside or outside the
        // sandbox alike, so it says nothing about capability (keeps `/x/a.rs` fixtures fusible).
        if path.exists() && !touring_ceg::gateway::sandbox_executor::sandbox_can_reach(&path) {
            return Some(path);
        }
    }
    None
}
