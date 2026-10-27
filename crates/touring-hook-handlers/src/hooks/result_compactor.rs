//! Result compactor — Output Diet for large Bash results (plan
//! `2026-10-26-touring-janela-eficacia`, S-14 / B1).
//!
//! Above a ceiling, the stdout of a LOG-shaped Bash result becomes: the first lines, every
//! failure line/block, the last lines, and a footer pointing at the raw output saved on disk
//! (content-addressed, mode 0600). The model keeps the signal of a build/test/install log and
//! can read any omitted range back with `Read`/`sed -n`.
//!
//! # Channel (FACT — read from the installed Claude Code 2.1.311 binary, 27/10/2026)
//!
//! `hookSpecificOutput.updatedToolOutput` on `PostToolUse` ("Replaces the tool output before it
//! is sent to the model"; `updatedMCPToolOutput` is the MCP-only legacy twin, "Prefer
//! updatedToolOutput, which works for all tools"). The value is validated against the tool's own
//! output schema; a mismatch logs `PostToolUse hook returned updatedToolOutput that does not
//! match <tool>'s output shape` and the ORIGINAL output is used. For Bash the schema is
//! `{stdout: string, stderr: string, interrupted: boolean, ...optional}`, so this module hands
//! back the WHOLE `tool_response` object with only `stdout` replaced — never a bare string.
//! Hooks of one event run in parallel on the original output and the last rewrite wins: this is
//! the ONLY `updatedToolOutput` producer of the `post-bash` chain (pinned in
//! `touring-dispatch/tests/updated_input_writers.rs`).
//!
//! # Design
//!
//! * OFF by default. `TOURING_RESULT_COMPACTOR=on` arms it; unset (or any other value) the module
//!   costs one environment read and emits nothing. The flag is read by the process that runs the
//!   handler: the daemon reads its boot environment (restart it to flip the flag); the
//!   standalone route reads the session's.
//! * Only `PostToolUse` of `Bash`. `PostToolUseFailure` has no `updatedToolOutput` field, and a
//!   non-zero exit is never compacted (there is no such event to answer).
//! * A failure block is NEVER cut. Failure lines (a case-insensitive vocabulary: error, fail,
//!   panic, traceback, denied, not found, ...) are kept with 2 lines before and 4 after, and the
//!   shapes that span many lines are kept WHOLE: Python tracebacks, rustc diagnostics (to the
//!   blank line), `thread ... panicked` blocks, cargo `failures:` sections (to `test result:`),
//!   pytest `FAILURES`/`ERRORS` sections, Go `--- FAIL` blocks. When in doubt, keep: if keeping
//!   leaves less than ~20% saving, the result passes through untouched.
//! * Lines that are not failures are clipped at 400 chars (a one-line JSON blob would otherwise
//!   defeat the line-based cut); failure lines are never clipped.
//! * Idempotent (never re-compacts its own footer) and fail-open: any I/O error saving the raw
//!   output means the original result goes through.
//!
//! # when_not_to_use (the module makes the call itself)
//!
//! * **The output IS the ask.** The cut is for LOGS, where volume is noise around a few signal
//!   lines. When the LAST stage of the command is a printer or selector — `cat`, `head`, `tail`,
//!   `sed`, `awk`, `grep`/`rg`, `find`, `ls`, `jq`, `cut`, `sort`, `uniq`, `wc`, `diff`, `git
//!   diff|show|log|blame|...`, `touring ...` — the model already shaped the result (a `| tail
//!   -50` is a bound the model chose) and a head/tail cut would drop what it asked for. This is
//!   the same reason `Read` is excluded by design.
//! * **The error is the payload.** `stderr` is passed through untouched; a result that already
//!   carries a persisted-output path, an image, background-task or structured content, an
//!   interrupt, or a `returnCodeInterpretation` (a special exit code) is not ours to rewrite.
//! * Output above 2 MiB is passed through: the 5 ms budget is not worth a partial scan.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{Map, Value};

use super::runtime::HookResponse;

/// Arms the compactor. Anything but `on`/`1`/`true`/`yes` leaves it a strict no-op.
pub const ENV_FLAG: &str = "TOURING_RESULT_COMPACTOR";
/// Ceiling override, in bytes of stdout (an upper bound of chars). Default [`DEFAULT_CEILING`].
pub const ENV_CEILING: &str = "TOURING_RESULT_COMPACTOR_CEILING";
/// Directory override for the raw copies (tests, sandboxes). Default `~/.claude/touring/compacted`.
pub const ENV_DIR: &str = "TOURING_RESULT_COMPACTOR_DIR";
/// Stdout up to this size is never touched (Output Diet: ~2.400 chars).
pub const DEFAULT_CEILING: usize = 2_400;

const MIN_CEILING: usize = 512;
const MAX_CEILING: usize = 1_000_000;
/// Above this the scan is not worth the 5 ms budget: pass through.
const MAX_INPUT_BYTES: usize = 2 * 1024 * 1024;
const HEAD_LINES: usize = 12;
const TAIL_LINES: usize = 15;
/// Failure lines keep this much context around them.
const CONTEXT_BEFORE: usize = 2;
const CONTEXT_AFTER: usize = 4;
/// Non-failure lines longer than this are clipped.
const MAX_KEPT_LINE_CHARS: usize = 400;
/// The footer the compacted text ends with; also the idempotence marker.
const MARKER: &str = "[touring result-compactor]";
/// Room the footer will take, reserved when checking that the cut saves something.
const FOOTER_RESERVE: usize = 420;
/// The compacted text must be at most this percent of the original, or the result is left alone.
const MAX_PERCENT_OF_ORIGINAL: usize = 80;
/// Hex chars of the blake3 digest in the file name (128 bits).
const HASH_CHARS: usize = 32;
/// Raw copies kept on disk; the oldest go first.
const KEEP_FILES: usize = 256;
const POST_TOOL_USE: &str = "PostToolUse";

/// One alternation of the failure vocabulary, scanned once over the whole text.
static FAILURE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)error|fail|panic|fatal|exception|traceback|abort|denied|refused|time(?:d)?[ -]?out|not found|no such file|cannot|can't|couldn't|unable to|segmentation fault|core dumped|killed|assert|exit(?:ed)? (?:code|status)|non-zero|✗|✕|❌|\bE[0-9]{4}\b",
    )
    .expect("static regex is valid")
});

/// Runtime settings, resolved once per result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Config {
    pub(crate) ceiling: usize,
    pub(crate) dir: PathBuf,
}

impl Config {
    /// `None` = compactor off (the default). Reads the flag first and nothing else when it is off.
    pub(crate) fn from_env() -> Option<Self> {
        Self::from_lookup(&|key| std::env::var(key).ok())
    }

    /// [`Config::from_env`] over an injectable lookup, so tests never touch the process environment.
    pub(crate) fn from_lookup(get: &dyn Fn(&str) -> Option<String>) -> Option<Self> {
        if !flag_is_on(get(ENV_FLAG).as_deref()) {
            return None;
        }
        let dir = match get(ENV_DIR).filter(|d| !d.is_empty()) {
            Some(d) => PathBuf::from(d),
            None => Path::new(&get("HOME")?)
                .join(".claude")
                .join("touring")
                .join("compacted"),
        };
        Some(Self {
            ceiling: ceiling_from(get(ENV_CEILING).as_deref()),
            dir,
        })
    }
}

fn flag_is_on(raw: Option<&str>) -> bool {
    raw.is_some_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "on" | "1" | "true" | "yes"
        )
    })
}

fn ceiling_from(raw: Option<&str>) -> usize {
    raw.and_then(|v| v.trim().parse::<usize>().ok())
        .map_or(DEFAULT_CEILING, |c| c.clamp(MIN_CEILING, MAX_CEILING))
}

/// Attach the compaction of this Bash result (when armed and worthwhile) to the handler's
/// `response`. With the flag off this returns `response` untouched after one environment read.
///
/// Only verdicts without a decision ride with a rewritten result: `Allow` becomes the rewrite,
/// `Context` keeps its context next to it; `Deny`/`Block`/`Halt`/input rewrites pass through.
#[must_use]
pub fn attach(response: HookResponse, input: &Value) -> HookResponse {
    attach_via(response, input, Config::from_env().as_ref())
}

pub(crate) fn attach_via(
    response: HookResponse,
    input: &Value,
    cfg: Option<&Config>,
) -> HookResponse {
    let Some(cfg) = cfg else {
        return response;
    };
    if !matches!(response, HookResponse::Allow | HookResponse::Context { .. }) {
        return response;
    }
    let Some(updated_tool_output) = compact_tool_response(input, cfg) else {
        return response;
    };
    let context = match response {
        HookResponse::Context { context, .. } => context,
        _ => String::new(),
    };
    HookResponse::ContextWithUpdatedToolOutput {
        context,
        event_name: Some(POST_TOOL_USE.to_string()),
        updated_tool_output,
    }
}

/// The replacement `tool_response` (the whole object, `stdout` swapped), or `None` when the
/// result is not ours to touch.
fn compact_tool_response(input: &Value, cfg: &Config) -> Option<Value> {
    if input.get("hook_event_name").and_then(Value::as_str) != Some(POST_TOOL_USE) {
        return None;
    }
    if input
        .get("tool_name")
        .and_then(Value::as_str)
        .is_some_and(|t| t != "Bash")
    {
        return None;
    }
    let response = input.get("tool_response")?.as_object()?;
    let stdout = response.get("stdout")?.as_str()?;
    if stdout.len() <= cfg.ceiling || !result_is_ours_to_rewrite(response, input) {
        return None;
    }
    let compacted = compact_text(stdout, cfg.ceiling)?;
    let (path, hash) = match persist_raw(&cfg.dir, stdout) {
        Ok(saved) => saved,
        Err(e) => {
            tracing::warn!(
                "result_compactor: raw copy not saved — passing the result through: {e}"
            );
            return None;
        }
    };
    let footer = footer_line(&compacted, stdout.len(), &path, &hash);
    tracing::info!(
        raw_bytes = stdout.len(),
        kept_lines = compacted.kept_lines,
        total_lines = compacted.total_lines,
        "result_compactor: stdout compacted"
    );
    let mut replaced = response.clone();
    replaced.insert(
        "stdout".to_string(),
        Value::String(format!("{}{footer}", compacted.body)),
    );
    Some(Value::Object(replaced))
}

/// Everything that makes a Bash result something other than plain log text, plus the command
/// gate (`when_not_to_use`).
fn result_is_ours_to_rewrite(response: &Map<String, Value>, input: &Value) -> bool {
    let flag = |k: &str| response.get(k).and_then(Value::as_bool).unwrap_or(false);
    if flag("interrupted") || flag("isImage") {
        return false;
    }
    // CC already persisted the output / returned structured content / backgrounded the call, or
    // the exit code carried a special meaning: not plain log text.
    for k in [
        "persistedOutputPath",
        "backgroundTaskId",
        "returnCodeInterpretation",
    ] {
        if response.get(k).is_some_and(|v| !v.is_null()) {
            return false;
        }
    }
    if response
        .get("structuredContent")
        .and_then(Value::as_array)
        .is_some_and(|a| !a.is_empty())
    {
        return false;
    }
    if response
        .get("exit_code")
        .or_else(|| response.get("returnCode"))
        .and_then(Value::as_i64)
        .is_some_and(|c| c != 0)
    {
        return false;
    }
    input
        .pointer("/tool_input/command")
        .and_then(Value::as_str)
        .is_some_and(|c| !is_payload_command(c))
}

// ── the command gate ────────────────────────────────────────────────────

/// First words (after wrappers) whose output is the thing the model asked for.
const PAYLOAD_TOOLS: &[&str] = &[
    "cat", "head", "tail", "bat", "nl", "tac", "less", "more", "sed", "awk", "grep", "egrep",
    "fgrep", "rg", "ag", "find", "fd", "ls", "tree", "jq", "yq", "cut", "sort", "uniq", "wc",
    "diff", "stat", "du", "echo", "printf", "column", "touring",
];

/// `git` subcommands that print the requested content.
const GIT_PAYLOAD: &[&str] = &[
    "diff",
    "show",
    "log",
    "blame",
    "grep",
    "ls-files",
    "ls-tree",
    "cat-file",
    "shortlog",
    "reflog",
    "status",
    "branch",
    "rev-parse",
];

/// Words that only wrap the real command.
const WRAPPERS: &[&str] = &[
    "sudo", "time", "env", "nice", "nohup", "command", "exec", "builtin", "xargs",
];

/// `true` when the LAST stage of `command` prints something the model selected or shaped.
fn is_payload_command(command: &str) -> bool {
    let stage = last_stage(command);
    let mut words = stage.split_whitespace().peekable();
    while let Some(w) = words.peek().copied() {
        let is_env_assignment = w.contains('=')
            && !w.starts_with('-')
            && w.chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_');
        if is_env_assignment || WRAPPERS.contains(&w) {
            words.next();
        } else if w == "timeout" {
            words.next();
            words.next(); // the duration
        } else {
            break;
        }
    }
    let Some(first) = words.next() else {
        return false;
    };
    let name = first
        .trim_start_matches(['(', '{'])
        .rsplit('/')
        .next()
        .unwrap_or(first);
    if name == "git" {
        return git_subcommand(words).is_some_and(|sub| GIT_PAYLOAD.contains(&sub));
    }
    PAYLOAD_TOOLS.contains(&name)
}

/// The git subcommand among `words`, skipping global options (`-C <dir>`, `-c <k=v>`, `--x`).
fn git_subcommand<'a>(words: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let mut skip_next = false;
    for w in words {
        if skip_next {
            skip_next = false;
        } else if w == "-C" || w == "-c" {
            skip_next = true;
        } else if !w.starts_with('-') {
            return Some(w);
        }
    }
    None
}

/// The last simple command of a shell line, honouring quotes. Boundaries are the unquoted
/// `| || && & ; newline`; a redirection such as `2>&1` / `&>` is not one; an unquoted heredoc
/// (`<<`) ends the scan, because what follows is data, not commands.
fn last_stage(command: &str) -> &str {
    let bytes = command.as_bytes();
    let mut stage_start = 0;
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let (next_quote, escapes_next) = quote_step(bytes[i], quote);
        let plain = quote.is_none() && next_quote.is_none() && !escapes_next;
        if plain && bytes[i..].starts_with(b"<<") {
            return command.get(stage_start..i).unwrap_or("");
        }
        if plain && is_stage_boundary(bytes, i) {
            stage_start = i + 1;
        }
        quote = next_quote;
        i += if escapes_next { 2 } else { 1 };
    }
    command.get(stage_start..).unwrap_or("")
}

/// One byte of quote tracking: the quote state after `c`, and whether `c` is a backslash that
/// escapes the next byte (outside quotes or inside double quotes).
fn quote_step(c: u8, quote: Option<u8>) -> (Option<u8>, bool) {
    match (quote, c) {
        (None | Some(b'"'), b'\\') => (quote, true),
        (Some(q), _) if c == q => (None, false),
        (None, b'\'' | b'"') => (Some(c), false),
        _ => (quote, false),
    }
}

/// An unquoted `|`, `;`, newline or `&` that separates commands (not the `&` of `2>&1`/`&>`).
fn is_stage_boundary(bytes: &[u8], i: usize) -> bool {
    match bytes[i] {
        b'|' | b';' | b'\n' => true,
        b'&' => !((i > 0 && bytes[i - 1] == b'>') || bytes.get(i + 1) == Some(&b'>')),
        _ => false,
    }
}

// ── the cut ─────────────────────────────────────────────────────────────

/// What [`compact_text`] kept.
#[derive(Debug)]
pub(crate) struct Compacted {
    /// Kept lines with `[... raw lines a-b omitted ...]` markers; no footer.
    pub(crate) body: String,
    pub(crate) total_lines: usize,
    pub(crate) kept_lines: usize,
    /// Lines kept because they belong to a failure (any position).
    pub(crate) failure_lines: usize,
}

/// The pure cut: `None` when `raw` is under the ceiling, over the scan cap, already compacted,
/// or when keeping every failure leaves too little saving to be worth the indirection.
pub(crate) fn compact_text(raw: &str, ceiling: usize) -> Option<Compacted> {
    if raw.len() <= ceiling || raw.len() > MAX_INPUT_BYTES || raw.contains(MARKER) {
        return None;
    }
    let (lines, starts) = split_lines(raw);
    let n = lines.len();
    let mut protect = vec![false; n];
    mark_failures(raw, &lines, &starts, &mut protect);
    let mut keep = protect.clone();
    keep[..HEAD_LINES.min(n)].fill(true);
    keep[n.saturating_sub(TAIL_LINES)..].fill(true);
    let (body, kept_lines) = render(&lines, &keep, &protect);
    if (body.len() + FOOTER_RESERVE) * 100 > raw.len() * MAX_PERCENT_OF_ORIGINAL {
        return None;
    }
    Some(Compacted {
        body,
        total_lines: n,
        kept_lines,
        failure_lines: protect.iter().filter(|p| **p).count(),
    })
}

/// Lines (each with its terminator) and the byte offset where each starts.
fn split_lines(raw: &str) -> (Vec<&str>, Vec<usize>) {
    let mut lines = Vec::new();
    let mut starts = Vec::new();
    let mut at = 0;
    for line in raw.split_inclusive('\n') {
        starts.push(at);
        at += line.len();
        lines.push(line);
    }
    (lines, starts)
}

/// Mark every line that belongs to a failure: vocabulary hits with a context window, plus the
/// multi-line shapes whole.
fn mark_failures(raw: &str, lines: &[&str], starts: &[usize], protect: &mut [bool]) {
    let n = lines.len();
    let mut last_hit = usize::MAX;
    for m in FAILURE_RE.find_iter(raw) {
        let idx = starts
            .partition_point(|&s| s <= m.start())
            .saturating_sub(1);
        if idx == last_hit {
            continue;
        }
        last_hit = idx;
        let lo = idx.saturating_sub(CONTEXT_BEFORE);
        let hi = (idx + CONTEXT_AFTER + 1).min(n);
        protect[lo..hi].fill(true);
    }
    let mut i = 0;
    while i < n {
        match block_end(lines, i) {
            Some(end) => {
                protect[i..end].fill(true);
                i = end.max(i + 1);
            }
            None => i += 1,
        }
    }
}

fn starts_indented(line: &str) -> bool {
    line.starts_with([' ', '\t'])
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// The multi-line failure shapes kept whole.
#[derive(Clone, Copy)]
enum Block {
    /// Python: indented frames, then the exception line.
    Traceback,
    /// rustc diagnostics and panic messages: up to the blank line.
    Diagnostic,
    /// cargo `failures:` section: through the `test result:` line.
    CargoFailures,
    /// pytest `FAILURES`/`ERRORS` section: up to the next `=====` header.
    PytestSection,
    /// Go `--- FAIL` header and its indented output.
    GoFail,
}

fn is_pytest_failure_header(line: &str) -> bool {
    line.starts_with('=') && (line.contains(" FAILURES ") || line.contains(" ERRORS "))
}

fn block_kind(line: &str) -> Option<Block> {
    if line.starts_with("Traceback (most recent call last):") {
        Some(Block::Traceback)
    } else if line.starts_with("error[")
        || line.starts_with("error:")
        || line.contains("panicked at")
    {
        Some(Block::Diagnostic)
    } else if line == "failures:" {
        Some(Block::CargoFailures)
    } else if is_pytest_failure_header(line) {
        Some(Block::PytestSection)
    } else if line.starts_with("--- FAIL") {
        Some(Block::GoFail)
    } else {
        None
    }
}

/// If a multi-line failure block STARTS at line `i`, the exclusive index where it ends.
fn block_end(lines: &[&str], i: usize) -> Option<usize> {
    let kind = block_kind(lines[i].trim_end())?;
    let n = lines.len();
    let first = |stop: &dyn Fn(&str) -> bool| (i + 1..n).find(|&j| stop(lines[j])).unwrap_or(n);
    Some(match kind {
        Block::Traceback => (first(&|l| !starts_indented(l)) + 1).min(n),
        Block::Diagnostic => first(&is_blank),
        Block::CargoFailures => (first(&|l| l.starts_with("test result:")) + 1).min(n),
        Block::PytestSection => first(&|l| l.starts_with('=') && !is_pytest_failure_header(l)),
        Block::GoFail => first(&|l| !starts_indented(l)),
    })
}

/// Kept lines in order, `[... raw lines a-b omitted ...]` for each gap. Returns the text and
/// the number of lines kept.
fn render(lines: &[&str], keep: &[bool], protect: &[bool]) -> (String, usize) {
    let mut out = String::new();
    let mut kept = 0;
    let mut i = 0;
    while i < lines.len() {
        if keep[i] {
            push_line(&mut out, lines[i], !protect[i]);
            kept += 1;
            i += 1;
        } else {
            let gap_start = i;
            while i < lines.len() && !keep[i] {
                i += 1;
            }
            let _ = writeln!(
                out,
                "[... raw lines {}-{} omitted ({} lines) ...]",
                gap_start + 1,
                i,
                i - gap_start
            );
        }
    }
    (out, kept)
}

/// Push `line` with a terminating newline, clipping it when `clip` and it is too long.
fn push_line(out: &mut String, line: &str, clip: bool) {
    if clip
        && line.len() > MAX_KEPT_LINE_CHARS
        && let Some((cut, _)) = line.char_indices().nth(MAX_KEPT_LINE_CHARS)
    {
        out.push_str(&line[..cut]);
        let _ = writeln!(out, " [... {} bytes clipped]", line.trim_end().len() - cut);
        return;
    }
    out.push_str(line);
    if !line.ends_with('\n') {
        out.push('\n');
    }
}

fn footer_line(c: &Compacted, raw_bytes: usize, path: &Path, hash: &str) -> String {
    let p = path.display();
    format!(
        "{MARKER} stdout was {raw_bytes} B / {total} lines; kept {kept} (first {HEAD_LINES}, last {TAIL_LINES}, {fail} failure lines always kept). \
         Full output saved at {p} (blake3 {hash}); the omitted ranges above use ITS line numbers. \
         Read them with `Read {p}` (offset/limit) or `sed -n 'A,Bp' {p}`; search with `grep -n PATTERN {p}`.\n",
        total = c.total_lines,
        kept = c.kept_lines,
        fail = c.failure_lines,
    )
}

// ── the raw copy ────────────────────────────────────────────────────────

/// Save `raw` under `dir` as `<blake3-128>.txt` (mode 0600, directory 0700), written to a
/// temporary file and renamed. Content-addressed: the same output saves once. Returns the
/// path and the hash. Keeps at most [`KEEP_FILES`] copies.
fn persist_raw(dir: &Path, raw: &str) -> std::io::Result<(PathBuf, String)> {
    let digest = blake3::hash(raw.as_bytes()).to_hex();
    let hash = digest
        .as_str()
        .get(..HASH_CHARS)
        .unwrap_or_default()
        .to_string();
    let path = dir.join(format!("{hash}.txt"));
    let mut dirs = std::fs::DirBuilder::new();
    dirs.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        dirs.mode(0o700);
    }
    dirs.create(dir)?;
    if path
        .metadata()
        .is_ok_and(|m| m.len() == u64::try_from(raw.len()).unwrap_or(u64::MAX))
    {
        return Ok((path, hash));
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let tmp = dir.join(format!(".{hash}.{}.{nanos}.tmp", std::process::id()));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    // No fsync: the copy is a convenience cache, not durable state, and a sync would spend
    // the 5 ms budget on journaling filesystems. The rename keeps readers from seeing half a file.
    let written = opts
        .open(&tmp)
        .and_then(|mut f| f.write_all(raw.as_bytes()));
    if let Err(e) = written.and_then(|()| std::fs::rename(&tmp, &path)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    prune(dir);
    Ok((path, hash))
}

/// Keep the newest [`KEEP_FILES`] copies (and any stale temp file counts as one).
fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x == "txt" || x == "tmp")
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    if files.len() <= KEEP_FILES {
        return;
    }
    files.sort();
    let excess = files.len() - KEEP_FILES;
    for (_, path) in files.into_iter().take(excess) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
#[path = "result_compactor_tests.rs"]
mod tests;
