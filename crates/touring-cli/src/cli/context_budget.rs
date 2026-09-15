//! P0/S-0.1 (2026-09-04) — the ruler for what the context window actually costs.
//!
//! Everything else in `touring kpi` measures whether the right ROUTE was taken
//! (`code_mode_adherence`, `code_mode_reuse`, `hooks_complement`). Nothing
//! measured the WINDOW, so the most expensive resource in the system was the one
//! resource with no instrument — and every attempt to tune it was faith.
//!
//! Baseline measured 04/09/2026 over 10 transcripts / 406 user turns of this
//! project, which is the number the ceilings below are set against:
//!
//! | origin | share of the window |
//! |---|---:|
//! | tool results | 51,3 % |
//! | **hook injection** | **26,6 %** (6 650 B/turn, ~1 662 tokens) |
//! | user text | 14,5 % |
//! | assistant text | 7,6 % |
//!
//! and, inside the injection: **35,1 % is a block byte-IDENTICAL to one already
//! in the window** — a second copy of the same bytes carries no proposition the
//! first did not, so its information density is zero by construction.
//!
//! Source: the Claude Code transcript itself (`~/.claude/projects/<slug>/*.jsonl`).
//! No new instrumentation, and it works retroactively — the same choice the
//! `touring.memory.*` family made for the same reason. Hook injections live in
//! `attachment` records of type `hook_success`, with the text under
//! `stdout.hookSpecificOutput.additionalContext`.
//!
//! Absence is DISPLAYED, never silent (E4): no transcript directory, no readable
//! file, or a scan that hit the byte cap all say so in the payload.

use serde_json::{Value, json};
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// How many transcripts to scan, most recent first. A transcript is tens of MB;
/// the cap keeps a `touring kpi` call cheap and is REPORTED, so a partial scan
/// can never be read as a complete one.
const MAX_TRANSCRIPTS: usize = 3;
/// Total bytes to read across transcripts before stopping and saying so.
const MAX_BYTES: u64 = 48 * 1024 * 1024;

/// Per-family accounting. The family taxonomy lives in [`injection_family`] —
/// ONE source, so the payload and the dedup accounting cannot disagree about
/// which emitter a block belongs to.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FamilyStats {
    /// Bytes this emitter put into the window.
    pub bytes: u64,
    /// How many separate blocks those bytes arrived in.
    pub blocks: u64,
    /// Of `bytes`, how many were a byte-identical repeat of an earlier block.
    pub duplicate_bytes: u64,
    /// Of `blocks`, how many were such a repeat.
    pub duplicate_blocks: u64,
    /// MUST / SHOULD / MAY directive lines carried — the unit of instruction.
    pub propositions: u64,
    /// Blocks carrying no directive at all: cost with nothing to act on.
    pub zero_proposition_blocks: u64,
}

/// Per-tool accounting for the largest single share of the window (tool results,
/// 51,3% measured 04/09/2026).
///
/// `reused_*` answers the only question that decides a digest. "Did the model
/// read this?" is not measurable; "how many bytes would a digest have to carry
/// to preserve every fact the model demonstrably USED?" is — the lines and
/// identifiers of the result that reappear in what the assistant wrote, or in
/// the arguments of its next calls, before the next human turn.
///
/// It is a FLOOR on use (content can inform without being quoted) and therefore
/// a CEILING on what a digest may safely discard, which is the conservative
/// direction for this decision.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ToolStats {
    /// Results this tool returned.
    pub results: u64,
    /// Bytes it returned.
    pub bytes: u64,
    /// Of those bytes, the ones in lines that reappear downstream.
    pub reused_bytes: u64,
    /// Informative lines it returned (see `REUSE_MIN_LINE`).
    pub lines: u64,
    /// Of those lines, the ones that reappear downstream.
    pub reused_lines: u64,
    /// Results with no observable reuse at all — neither a line nor a token.
    pub zero_reuse_results: u64,
}

/// The pure aggregate behind the payload — everything the ruler counts, with no
/// filesystem in sight, so the arithmetic is testable on synthetic lines.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ContextBudgetAggregate {
    /// Transcript files actually read.
    pub transcripts: u64,
    /// Bytes of transcript read — the denominator of how much was inspected.
    pub bytes_scanned: u64,
    /// The scan stopped at `MAX_BYTES`: this is a PARTIAL measurement.
    pub capped: bool,
    /// User messages carrying real text — the unit the per-turn ceiling divides by.
    pub user_turns: u64,
    /// Bytes injected by hooks.
    pub injected_bytes: u64,
    /// Blocks those bytes arrived in.
    pub injected_blocks: u64,
    /// Injected bytes that repeated a block already in the window (density zero).
    pub duplicate_bytes: u64,
    /// Injected blocks that were such a repeat.
    pub duplicate_blocks: u64,
    /// Bytes returned by tools — the largest single share of the window.
    pub tool_result_bytes: u64,
    /// Bytes the assistant wrote as prose.
    pub assistant_bytes: u64,
    /// Bytes the user wrote.
    pub user_bytes: u64,
    /// Injected blocks that asked for something with a `MUST` line.
    pub must_emitted: u64,
    /// Of those, how many the next Bash call actually ran — an UPPER bound on
    /// the nudge's effect, since the model may have run it regardless.
    pub must_followed: u64,
    /// Tool calls the assistant made — the declared proxy denominator for TpCD.
    pub tool_calls: u64,
    /// The same accounting, split by emitter (see [`injection_family`]).
    pub by_family: BTreeMap<String, FamilyStats>,
    /// Tool results split by the tool that produced them, with how much of each
    /// is observably reused (see [`ToolStats`]).
    pub by_tool: BTreeMap<String, ToolStats>,
}

/// Shortest line that can carry a fact worth keeping in a digest. Below this a
/// match is noise — `}` and `---` appear in every file and would inflate reuse.
const REUSE_MIN_LINE: usize = 8;

/// Which emitter a block of injected context came from.
///
/// ONE source for the taxonomy: the payload, the per-family ratios and the
/// duplicate accounting all call this, so a family can never mean one thing in
/// the totals and another in the breakdown.
#[must_use]
pub fn injection_family(text: &str) -> &'static str {
    const TABLE: &[(&str, &[&str])] = &[
        (
            "code-mode-deny",
            &[
                "CODE MODE",
                "G7 re-inspe",
                "G10 write",
                "G1 rajada",
                "G11 ",
                "G3 edit",
            ],
        ),
        ("touring-suggest", &["TOURING SUGGEST"]),
        (
            "loop-outer",
            &["LOOP RESUME", "work-outer", "explore-ledger", "OUTER ·"],
        ),
        (
            "session-start",
            &[
                "Touring Knowledge",
                "ATZILUTH",
                "SCOUT PERP",
                "TOURING (slim)",
                "Painel",
            ],
        ),
        ("prompt-enhance", &["PROMPT ENHANCEMENT"]),
        (
            "past-lessons",
            &["lições de erros passados", "Similar command failed"],
        ),
        (
            "post-tool-echo",
            &["temporal: reliability", "last fail:", "post-tool-batch"],
        ),
    ];
    for (name, needles) in TABLE {
        if needles.iter().any(|n| text.contains(n)) {
            return name;
        }
    }
    "other"
}

/// Actionable propositions carried by a block: lines in the MUST / SHOULD / MAY
/// shape the injection-density invariant mandates
/// (`~/.claude/rules/touring-4-pillars.md`).
///
/// A block with ZERO of them is not "small", it is empty of instruction — the
/// measured population that motivates P2: `[gabrielgadea]` 384×, `[generic]`
/// 188×, `post-tool-batch: batch of 1 tools …` 422×. Counting them is what turns
/// that rule from a declaration into something an executor can check (D8).
#[must_use]
pub fn count_propositions(text: &str) -> u64 {
    text.lines()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("MUST") || t.starts_with("SHOULD") || t.starts_with("MAY")
        })
        .count() as u64
}

/// The transcript directory Claude Code uses for a project root: every byte that
/// is not alphanumeric becomes `-` (so `/home/x/p` → `-home-x-p`).
#[must_use]
pub fn transcript_dir_for(home: &Path, project_root: &Path) -> PathBuf {
    let slug: String = project_root
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    home.join(".claude/projects").join(slug)
}

/// Every text a hook injected in one transcript record, or an empty vec.
fn hook_contexts(rec: &Value) -> Vec<String> {
    let Some(att) = rec.get("attachment") else {
        return Vec::new();
    };
    if att.get("type").and_then(Value::as_str) != Some("hook_success") {
        return Vec::new();
    }
    let mut out = Vec::new();
    for key in ["stdout", "content"] {
        let Some(raw) = att.get(key).and_then(Value::as_str) else {
            continue;
        };
        if raw.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(raw) {
            Ok(parsed) => {
                if let Some(hs) = parsed.get("hookSpecificOutput") {
                    for k in ["additionalContext", "systemMessage"] {
                        if let Some(s) = hs.get(k).and_then(Value::as_str) {
                            out.push(s.to_string());
                        }
                    }
                }
                if let Some(s) = parsed.get("systemMessage").and_then(Value::as_str) {
                    out.push(s.to_string());
                }
            }
            // Not JSON — the hook printed plain text, which still costs the window.
            Err(_) => out.push(raw.to_string()),
        }
    }
    out
}

fn hash_of(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// The first `MUST <target>` a suggestion block asks for, used to see whether the
/// next Bash call actually did it.
fn must_target(text: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let t = l.trim_start();
        let rest = t.strip_prefix("MUST")?;
        let mut words = rest.split_whitespace();
        let first = words.next()?;
        Some(match words.next() {
            Some(second) => format!("{first} {second}"),
            None => first.to_string(),
        })
    })
}

/// Shortest token that identifies something rather than being a common word.
const TOKEN_MIN: usize = 6;

/// Marks a `user` record that the HARNESS wrote, not the person.
///
/// Claude Code files several machine-authored things as user records: the output
/// of a slash command, its caveat banner, and the summary injected when a
/// context is compacted. They carry no `tool_result`, so the obvious predicate
/// ("a user record without tool results is a human turn") counts them — and
/// `user_turns` is the DENOMINATOR of the ruler's headline metric.
///
/// Measured 04/09/2026 over 5 real transcripts: **35 of 172** counted turns
/// (20,3%) were one of these — 13 caveats, 13 command outputs, 9 compaction
/// summaries. The denominator was inflated 25,5%, so `injected_bytes_per_turn`
/// read ~25% LOWER than the truth: the ruler understated the very cost it
/// exists to expose.
///
/// This is the complement of the defect the bundle already records for this
/// file (a user message whose `content` is a bare STRING was not counted at
/// all, and the rate read 12x high). One correction stopped dropping real
/// turns; this one stops adding fake ones. A denominator has two ways to be
/// wrong and both were live.
///
/// A genuine turn may carry an appended `<system-reminder>`, so that is
/// deliberately NOT a marker — the predicate stays narrow, and a doubtful
/// record counts as human (erring toward a larger denominator understates the
/// problem, which is the conservative direction for a cost metric).
// Private since 2026-09-13: every reader lives in this file (five references, none
// outside), so `pub` only made it an orphan in the wiring audit (REGRA #0).
const NON_HUMAN_TURN_MARKERS: [&str; 4] = [
    "This session is being continued from a previous conversation",
    "<local-command-stdout>",
    "<local-command-caveat>",
    "Caveat: The messages below were generated by the user while running local commands",
];

/// Did a person write this, or the harness? See `NON_HUMAN_TURN_MARKERS`.
#[must_use]
pub fn is_human_turn_text(text: &str) -> bool {
    !NON_HUMAN_TURN_MARKERS.iter().any(|m| text.contains(m))
}

/// One result line, in the form reuse is matched on.
///
/// `Read` returns `cat -n` output, so every line arrives prefixed with `<n>\t`
/// and NEVER matches a quotation of it. The first run of this measurement
/// reported 0,0% reuse for every tool because of exactly that; probed against a
/// known case, the same result gave 0 raw hits and 17 after stripping the
/// prefix. The normalization is not cosmetic — without it the metric reads zero
/// and the zero looks like a finding.
fn normalize_result_line(line: &str) -> String {
    let body = match line.find('\t') {
        Some(at) if line[..at].trim().chars().all(|c| c.is_ascii_digit()) && at > 0 => {
            &line[at + 1..]
        }
        _ => line,
    };
    body.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Identifier-ish tokens — the unit a fact travels in when the model uses a
/// result without quoting the whole line (a symbol name lifted out of a 3 KB
/// file). Collected into `out` so one window is scanned once.
fn distinctive_tokens(text: &str, out: &mut BTreeSet<String>) {
    let bytes = text.as_bytes();
    let mut start: Option<usize> = None;
    for (i, &c) in bytes.iter().enumerate() {
        let head = c.is_ascii_alphabetic() || c == b'_';
        let body = head || c.is_ascii_digit() || matches!(c, b'.' | b'/' | b':' | b'-' | b'_');
        match start {
            None if head => start = Some(i),
            Some(s) if !body => {
                if i - s >= TOKEN_MIN {
                    out.insert(text[s..i].to_string());
                }
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start
        && bytes.len() - s >= TOKEN_MIN
    {
        out.insert(text[s..].to_string());
    }
}

/// One tool result waiting for the window that will say how much of it was used.
struct PendingResult {
    tool: String,
    bytes: u64,
    lines: BTreeSet<String>,
    tokens: BTreeSet<String>,
    /// How much assistant output already existed when this result arrived.
    ///
    /// Without it the window was everything the assistant produced in the WHOLE
    /// turn, so a result arriving late was measured against text written before
    /// it existed — co-occurrence, not use. Proven 04/09/2026 by
    /// `sonda_texto_anterior_ao_resultado_nao_pode_contar_como_uso`, which read
    /// `1` where the contract demands `0`. The metric called itself a FLOOR on
    /// use while over-counting; a bound that is wrong in the direction it
    /// claims to be safe is worse than no bound.
    after: usize,
}

/// Measures how much of each tool result reappears downstream.
///
/// The window is everything the assistant produced — prose and the arguments of
/// its next calls — until the next HUMAN turn. A fact used three calls later
/// still counts; one quoted after the human speaks again does not, because by
/// then it belongs to a different task.
///
/// A line counts as reused when the window carries it verbatim, or when every
/// distinctive token it holds reached the window — the second arm is what
/// catches a line quoted inside a sentence, which exact matching alone misses.
#[derive(Default)]
struct ReuseTracker {
    tool_of: BTreeMap<String, String>,
    pending: Vec<PendingResult>,
    /// The assistant's output for this turn, IN ORDER — prose and call
    /// arguments, one entry per block. Order is the whole point: a result is
    /// measured only against the entries that come after it.
    window: Vec<String>,
}

impl ReuseTracker {
    fn observe_call(&mut self, id: &str, name: &str) {
        self.tool_of.insert(id.to_string(), name.to_string());
    }

    fn extend_window(&mut self, text: &str) {
        self.window.push(text.to_string());
    }

    fn observe_result(&mut self, id: Option<&str>, text: &str) {
        let tool = id
            .and_then(|i| self.tool_of.get(i))
            .cloned()
            .unwrap_or_else(|| "unattributed".to_string());
        let mut lines = BTreeSet::new();
        for line in text.lines() {
            let n = normalize_result_line(line);
            if n.len() >= REUSE_MIN_LINE {
                lines.insert(n);
            }
        }
        let mut tokens = BTreeSet::new();
        distinctive_tokens(text, &mut tokens);
        let after = self.window.len();
        self.pending.push(PendingResult {
            tool,
            bytes: text.len() as u64,
            lines,
            tokens,
            after,
        });
    }

    /// Close every pending result against the output that came AFTER it.
    ///
    /// Walks the pending list backwards, growing the suffix sets as it goes: the
    /// last result sees only the tail, the one before it sees that tail plus its
    /// own, and so on. One pass over the window in total — the naive form (a
    /// fresh set per result) is quadratic on a turn with many calls, and this
    /// ruler runs inside `touring kpi`.
    fn flush(&mut self, agg: &mut ContextBudgetAggregate) {
        let window = std::mem::take(&mut self.window);
        let mut pending = std::mem::take(&mut self.pending);
        let mut suffix_lines: BTreeSet<String> = BTreeSet::new();
        let mut suffix_tokens: BTreeSet<String> = BTreeSet::new();
        let mut boundary = window.len();
        while let Some(p) = pending.pop() {
            let start = p.after.min(window.len());
            for chunk in &window[start..boundary] {
                for line in chunk.lines() {
                    let n = normalize_result_line(line);
                    if n.len() >= REUSE_MIN_LINE {
                        suffix_lines.insert(n);
                    }
                }
                distinctive_tokens(chunk, &mut suffix_tokens);
            }
            boundary = start;

            let mut reused_bytes = 0u64;
            let mut reused_lines = 0u64;
            for line in &p.lines {
                if line_reached(line, &suffix_lines, &suffix_tokens) {
                    reused_bytes += line.len() as u64;
                    reused_lines += 1;
                }
            }
            let reused_tokens = p
                .tokens
                .iter()
                .filter(|t| suffix_tokens.contains(*t))
                .count();
            let e = agg.by_tool.entry(p.tool).or_default();
            e.results += 1;
            e.bytes += p.bytes;
            e.lines += p.lines.len() as u64;
            e.reused_bytes += reused_bytes;
            e.reused_lines += reused_lines;
            if reused_lines == 0 && reused_tokens == 0 {
                e.zero_reuse_results += 1;
            }
        }
    }
}

/// Is this line's content in the window — verbatim, or by all of its tokens?
///
/// The second arm is what catches a line quoted inside a sentence, which exact
/// matching alone misses; two distinctive tokens is the floor at which the match
/// stops being a coincidence.
fn line_reached(line: &str, lines: &BTreeSet<String>, tokens: &BTreeSet<String>) -> bool {
    if lines.contains(line) {
        return true;
    }
    let mut toks = BTreeSet::new();
    distinctive_tokens(line, &mut toks);
    toks.len() >= 2 && toks.iter().all(|t| tokens.contains(t))
}

/// Aggregate one transcript's lines. Pure: the caller supplies the lines.
///
/// `seen` is threaded ACROSS transcripts by the caller, because the duplication
/// that matters is "the same bytes reached the model again", which does not
/// respect file boundaries.
pub fn aggregate_lines<'a>(
    lines: impl Iterator<Item = &'a str>,
    agg: &mut ContextBudgetAggregate,
    seen: &mut BTreeMap<u64, u64>,
) {
    let mut pending_must: Option<String> = None;
    let mut reuse = ReuseTracker::default();
    for line in lines {
        let Ok(rec) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let kind = rec.get("type").and_then(Value::as_str).unwrap_or_default();

        for text in hook_contexts(&rec) {
            let family = injection_family(&text);
            let len = text.len() as u64;
            let props = count_propositions(&text);
            agg.injected_bytes += len;
            agg.injected_blocks += 1;
            let entry = agg.by_family.entry(family.to_string()).or_default();
            entry.bytes += len;
            entry.blocks += 1;
            entry.propositions += props;
            if props == 0 {
                entry.zero_proposition_blocks += 1;
            }
            let count = seen.entry(hash_of(&text)).or_insert(0);
            *count += 1;
            if *count > 1 {
                agg.duplicate_bytes += len;
                agg.duplicate_blocks += 1;
                entry.duplicate_bytes += len;
                entry.duplicate_blocks += 1;
            }
            if let Some(target) = must_target(&text) {
                agg.must_emitted += 1;
                pending_must = Some(target);
            }
            continue;
        }

        let Some(content) = rec.pointer("/message/content") else {
            continue;
        };
        // `content` is EITHER an array of blocks OR a bare string — and a bare
        // string is the common shape of a real user prompt. Requiring an array
        // dropped those turns entirely: the live probe read 22 turns where the
        // same corpus holds ~120, and `injected_bytes_per_turn` came out 12x too
        // high because the denominator was missing most of its rows. A ruler
        // whose denominator silently drops the common case reports a number that
        // is confidently wrong, which is worse than reporting none.
        if let Some(text) = content.as_str() {
            if kind == "assistant" {
                agg.assistant_bytes += text.len() as u64;
                reuse.extend_window(text);
            } else if kind == "user" {
                // Os bytes contam sempre — eles ocuparam a janela. O TURNO só
                // conta se uma pessoa o escreveu (ver `NON_HUMAN_TURN_MARKERS`).
                agg.user_bytes += text.len() as u64;
                if is_human_turn_text(text) {
                    agg.user_turns += 1;
                    reuse.flush(agg);
                }
            }
            continue;
        }
        let Some(blocks) = content.as_array() else {
            continue;
        };
        let mut counted_turn = false;
        let mut carried_result = false;
        for b in blocks {
            match b.get("type").and_then(Value::as_str) {
                Some("tool_result") => {
                    carried_result = true;
                    if let Some(c) = b.get("content") {
                        let text = match c.as_str() {
                            Some(s) => s.to_string(),
                            None => c.to_string(),
                        };
                        agg.tool_result_bytes += text.len() as u64;
                        reuse.observe_result(b.get("tool_use_id").and_then(Value::as_str), &text);
                    }
                }
                Some("text") => {
                    let text = b.get("text").and_then(Value::as_str).unwrap_or("");
                    let len = text.len() as u64;
                    if kind == "assistant" {
                        agg.assistant_bytes += len;
                        reuse.extend_window(text);
                    } else if kind == "user" {
                        agg.user_bytes += len;
                        counted_turn = counted_turn || is_human_turn_text(text);
                    }
                }
                Some("tool_use") if kind == "assistant" => {
                    agg.tool_calls += 1;
                    if let Some(id) = b.get("id").and_then(Value::as_str) {
                        reuse
                            .observe_call(id, b.get("name").and_then(Value::as_str).unwrap_or("?"));
                    }
                    if let Some(input) = b.get("input") {
                        reuse.extend_window(&input.to_string());
                    }
                    if b.get("name").and_then(Value::as_str) == Some("Bash")
                        && let Some(target) = pending_must.take()
                    {
                        let cmd = b
                            .pointer("/input/command")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        // Match the whole two-word target, else its head — the
                        // same rule the 04/09 measurement used, kept verbatim so
                        // the baseline and the KPI are the same number.
                        let head = target.split_whitespace().next().unwrap_or(&target);
                        if cmd.contains(&target) || cmd.contains(head) {
                            agg.must_followed += 1;
                        }
                    }
                }
                _ => {}
            }
        }
        if counted_turn {
            agg.user_turns += 1;
        }
        // A human turn closes the reuse window: a fact quoted after the person
        // speaks again belongs to the next task, not to this result. A record
        // that carries results is never a human turn, however much text rides
        // along with them.
        if counted_turn && !carried_result {
            reuse.flush(agg);
        }
    }
    // The transcript ends: whatever is still pending is measured against the
    // window it did get, never dropped — a result with no window is a result
    // with zero observed reuse, which is a measurement, not a gap.
    reuse.flush(agg);
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// The payload, from the pure aggregate. Testable without a filesystem.
#[must_use]
pub fn budget_from_aggregate(agg: &ContextBudgetAggregate) -> Value {
    let window_total =
        agg.injected_bytes + agg.tool_result_bytes + agg.assistant_bytes + agg.user_bytes;
    let available = agg.user_turns > 0 && agg.injected_blocks > 0;

    let per_turn = if agg.user_turns == 0 {
        0.0
    } else {
        agg.injected_bytes as f64 / agg.user_turns as f64
    };
    let dup_ratio = if agg.injected_bytes == 0 {
        0.0
    } else {
        agg.duplicate_bytes as f64 / agg.injected_bytes as f64
    };
    // A floor over zero emissions would read as a failure of adherence when it
    // is an absence of data — `null`, never 0.0 (the `ratio_absent_reads_as_null`
    // rule this file's neighbours already follow).
    let follow_ratio =
        (agg.must_emitted > 0).then(|| round3(agg.must_followed as f64 / agg.must_emitted as f64));

    let families: BTreeMap<String, Value> = agg
        .by_family
        .iter()
        .map(|(name, f)| {
            (
                name.clone(),
                json!({
                    "bytes": f.bytes,
                    "blocks": f.blocks,
                    "mean_bytes": f.bytes.checked_div(f.blocks).unwrap_or(0),
                    "duplicate_bytes": f.duplicate_bytes,
                    "duplicate_ratio": if f.bytes == 0 { 0.0 } else { round3(f.duplicate_bytes as f64 / f.bytes as f64) },
                    "propositions": f.propositions,
                    "zero_proposition_blocks": f.zero_proposition_blocks,
                    // The density the injection invariant asks for, in the unit a
                    // reader can act on: propositions per kilobyte of window.
                    "propositions_per_kb": if f.bytes == 0 { 0.0 } else { round3(f.propositions as f64 * 1024.0 / f.bytes as f64) },
                }),
            )
        })
        .collect();

    // Tool results, attributed. Ordered by bytes so the dominant emitter is the
    // first row a reader meets, which is the whole point of the attribution.
    let mut tools: Vec<(&String, &ToolStats)> = agg.by_tool.iter().collect();
    tools.sort_by(|a, b| b.1.bytes.cmp(&a.1.bytes).then(a.0.cmp(b.0)));
    let tool_bytes: u64 = agg.by_tool.values().map(|t| t.bytes).sum();
    let tool_reused: u64 = agg.by_tool.values().map(|t| t.reused_bytes).sum();
    let by_tool: Vec<Value> = tools
        .iter()
        .map(|(name, t)| {
            json!({
                "tool": name,
                "results": t.results,
                "bytes": t.bytes,
                "share": if tool_bytes == 0 { 0.0 } else { round3(t.bytes as f64 / tool_bytes as f64) },
                "mean_bytes": t.bytes.checked_div(t.results).unwrap_or(0),
                "reused_bytes": t.reused_bytes,
                "reused_line_ratio": if t.lines == 0 { Value::Null } else { json!(round3(t.reused_lines as f64 / t.lines as f64)) },
                // A result nothing downstream touched — the only population a
                // digest could drop outright. Measured at 3–6%: almost every
                // result contributes something, so truncation is the wrong tool.
                "zero_reuse_results": t.zero_reuse_results,
            })
        })
        .collect();

    let mut failing: Vec<&str> = Vec::new();
    if per_turn > touring_foundation::cila::INJECTION_CEIL_BYTES_PER_TURN as f64 {
        failing.push("injected_bytes_per_turn");
    }
    if dup_ratio > touring_foundation::cila::INJECTION_DUPLICATE_RATIO_CEIL {
        failing.push("duplicate_injection_ratio");
    }
    if let Some(r) = follow_ratio
        && r < touring_foundation::cila::INJECTION_FOLLOW_RATIO_FLOOR
    {
        failing.push("injection_follow_ratio");
    }

    json!({
        "available": available,
        "reason": if available { Value::Null } else { json!("no transcript with user turns and hook injection yet") },
        "transcripts_scanned": agg.transcripts,
        "bytes_scanned": agg.bytes_scanned,
        // A capped scan is a PARTIAL measurement and says so, so a number from a
        // truncated read is never mistaken for a number from a whole one.
        "capped": agg.capped,
        "user_turns": agg.user_turns,
        "window_bytes": {
            "injection": agg.injected_bytes,
            "tool_results": agg.tool_result_bytes,
            "assistant": agg.assistant_bytes,
            "user": agg.user_bytes,
            "injection_share": if window_total == 0 { 0.0 } else { round3(agg.injected_bytes as f64 / window_total as f64) },
            "tool_result_share": if window_total == 0 { 0.0 } else { round3(agg.tool_result_bytes as f64 / window_total as f64) },
        },
        "injected_bytes_per_turn": round3(per_turn),
        "injected_bytes_per_turn_ceil": touring_foundation::cila::INJECTION_CEIL_BYTES_PER_TURN as f64,
        "duplicate_injection_ratio": round3(dup_ratio),
        "duplicate_injection_ratio_ceil": touring_foundation::cila::INJECTION_DUPLICATE_RATIO_CEIL,
        "duplicate_blocks": agg.duplicate_blocks,
        "injection_follow_ratio": follow_ratio,
        "injection_follow_ratio_floor": touring_foundation::cila::INJECTION_FOLLOW_RATIO_FLOOR,
        "must_emitted": agg.must_emitted,
        "must_followed": agg.must_followed,
        // TpCD (Eixo 4) with a DECLARED proxy for "consolidated action": one tool
        // call. It is a proxy and is named as one — a real correct-decision count
        // needs an outcome the transcript does not carry.
        "tpcd_proxy_bytes_per_tool_call": if agg.tool_calls == 0 { Value::Null } else { json!(round3(window_total as f64 / agg.tool_calls as f64)) },
        "tool_calls": agg.tool_calls,
        // The 51,3% of the window nobody was measuring. `reuse_ratio` is the
        // number that decides a digest: the share of returned bytes that
        // demonstrably reached the model's next words or arguments. A FLOOR on
        // use, therefore a CEILING on what a digest may discard.
        "tool_output": {
            "bytes": tool_bytes,
            "reused_bytes": tool_reused,
            "reuse_ratio": if tool_bytes == 0 { Value::Null } else { json!(round3(tool_reused as f64 / tool_bytes as f64)) },
            "by_tool": by_tool,
        },
        "by_family": families,
        "status": if !available { "STUB" } else if failing.is_empty() { "PASS" } else { "FAIL" },
        "failing": failing,
    })
}

/// The IO shell: locate this project's transcripts and aggregate the most recent
/// ones, newest first, stopping at the byte cap and reporting that it did.
#[must_use]
pub fn context_budget(project_root: &Path) -> Value {
    let Some(home) = std::env::var_os("HOME") else {
        return json!({"available": false, "reason": "HOME unset"});
    };
    let dir = transcript_dir_for(Path::new(&home), project_root);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return json!({
            "available": false,
            "reason": format!("no transcript directory at {}", dir.display()),
        });
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|p| {
            let mtime = std::fs::metadata(&p).ok()?.modified().ok()?;
            Some((mtime, p))
        })
        .collect();
    // Mais recente primeiro: a janela que interessa e a de agora.
    files.sort_by_key(|a| std::cmp::Reverse(a.0));

    let mut agg = ContextBudgetAggregate::default();
    let mut seen: BTreeMap<u64, u64> = BTreeMap::new();
    for (_, path) in files.into_iter().take(MAX_TRANSCRIPTS) {
        if agg.bytes_scanned >= MAX_BYTES {
            agg.capped = true;
            break;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        agg.bytes_scanned += content.len() as u64;
        agg.transcripts += 1;
        aggregate_lines(content.lines(), &mut agg, &mut seen);
    }
    budget_from_aggregate(&agg)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal transcript: one hook injection repeated, one user turn, one
    /// assistant tool call. Every number below is arithmetic a reader can redo
    /// by hand — which is the point of keeping the aggregation pure.
    fn synthetic() -> Vec<String> {
        let inj = |text: &str| {
            let inner = json!({"hookSpecificOutput": {"additionalContext": text}}).to_string();
            json!({"type": "attachment", "attachment": {"type": "hook_success", "stdout": inner}})
                .to_string()
        };
        vec![
            inj("[TOURING SUGGEST]\n  MUST touring doctor -j\n"),
            json!({"type": "assistant", "message": {"content": [
                {"type": "tool_use", "name": "Bash", "input": {"command": "touring doctor -j"}}
            ]}})
            .to_string(),
            // byte-identical repeat — the whole point of the duplicate ruler
            inj("[TOURING SUGGEST]\n  MUST touring doctor -j\n"),
            json!({"type": "assistant", "message": {"content": [
                {"type": "tool_use", "name": "Bash", "input": {"command": "ls"}}
            ]}})
            .to_string(),
            inj("[generic]"),
            json!({"type": "user", "message": {"content": [
                {"type": "text", "text": "faca isso"}
            ]}})
            .to_string(),
            json!({"type": "user", "message": {"content": [
                {"type": "tool_result", "content": "saida grande"}
            ]}})
            .to_string(),
        ]
    }

    /// A call and the result that answers it, as Claude Code writes them.
    fn call(id: &str, tool: &str, input: Value) -> String {
        json!({"type": "assistant", "message": {"content": [
            {"type": "tool_use", "id": id, "name": tool, "input": input}
        ]}})
        .to_string()
    }
    fn result(id: &str, text: &str) -> String {
        json!({"type": "user", "message": {"content": [
            {"type": "tool_result", "tool_use_id": id, "content": text}
        ]}})
        .to_string()
    }
    fn says(text: &str) -> String {
        json!({"type": "assistant", "message": {"content": [{"type": "text", "text": text}]}})
            .to_string()
    }
    fn human(text: &str) -> String {
        json!({"type": "user", "message": {"content": [{"type": "text", "text": text}]}})
            .to_string()
    }

    /// O denominador da métrica-manchete tem DUAS formas de errar, e as duas
    /// estiveram vivas neste arquivo: deixar de contar turno real (a mensagem
    /// cujo `content` é string crua — a taxa lia 12× alto) e contar o que não é
    /// turno (saída de slash command, banner de caveat, resumo de compactação —
    /// 35 de 172 medidos, 20,3%). Este teste fixa os dois lados de uma vez.
    #[test]
    fn so_a_fala_humana_conta_como_turno() {
        // O que a MÁQUINA escreveu, arquivado como record de usuário.
        for marker in NON_HUMAN_TURN_MARKERS {
            assert!(
                !is_human_turn_text(&format!("prefixo {marker} sufixo")),
                "{marker} e' texto do harness, nao turno humano"
            );
        }
        // Um turno legítimo, inclusive carregando um system-reminder anexado —
        // o predicado fica ESTREITO de propósito: a dúvida conta como humana,
        // que é a direção que subestima o problema em vez de inflá-lo.
        assert!(is_human_turn_text("faca a auditoria"));
        assert!(is_human_turn_text(
            "faca a auditoria\n<system-reminder>contexto</system-reminder>"
        ));

        let agg = run(&[
            human("tarefa de verdade"),
            human("<local-command-stdout>[2mCompacted[22m</local-command-stdout>"),
            human("This session is being continued from a previous conversation. O resumo segue."),
        ]);
        assert_eq!(
            agg.user_turns, 1,
            "so o primeiro e' fala humana; os outros dois sao do harness"
        );
        assert!(
            agg.user_bytes > 0,
            "os BYTES contam sempre — eles ocuparam a janela; so o TURNO nao"
        );
    }

    /// SONDA DE AUDITORIA (04/09): o contrato diz que uma linha conta como
    /// reusada quando ela REAPARECE DEPOIS. A janela, porém, acumula desde o
    /// início do turno — então um resultado que chega TARDE é medido contra
    /// texto escrito ANTES de ele existir. Se este teste vir reuso, o número
    /// não é o piso que a doc promete: é co-ocorrência no turno.
    #[test]
    fn sonda_texto_anterior_ao_resultado_nao_pode_contar_como_uso() {
        let agg = run(&[
            human("faca"),
            says("vou olhar alvo_especifico_xyz no arquivo caminho/do/modulo.rs"),
            call("c1", "Bash", json!({"command": "ls"})),
            result("c1", "alvo_especifico_xyz caminho/do/modulo.rs"),
            human("outra tarefa"),
        ]);
        assert_eq!(
            agg.by_tool["Bash"].reused_lines, 0,
            "texto que PRECEDE o resultado nao pode contar como uso dele"
        );
    }

    /// `distinctive_tokens` slices by BYTE index, so a boundary landing inside a
    /// multi-byte char would panic — on a codebase whose comments and doc
    /// strings are in Portuguese, that is the common case, not the exotic one.
    /// The reasoning says it is safe (a token only starts at an ASCII letter and
    /// only ends at a non-ASCII-body byte, and a UTF-8 lead byte is neither), but
    /// reasoning is not a test.
    #[test]
    fn acentuacao_nao_quebra_a_fatia_por_byte() {
        let mut out = BTreeSet::new();
        distinctive_tokens("função contexto_orçamento é medição — ré", &mut out);
        assert!(
            out.contains("contexto_or"),
            "parou no acento, sem cortá-lo: {out:?}"
        );
        let mut out2 = BTreeSet::new();
        distinctive_tokens("日本語 identifier_longo 中文", &mut out2);
        assert!(out2.contains("identifier_longo"));
        // O caminho inteiro, com um resultado acentuado de ponta a ponta.
        let agg = run(&[
            call("c1", "Bash", json!({"command": "grep função"})),
            result("c1", "  1\tfn medição_do_orçamento() {\n  2\t    ação();"),
            says("a função e\nfn medição_do_orçamento() {"),
        ]);
        assert_eq!(agg.by_tool["Bash"].reused_lines, 1);
    }

    /// The plan's P5 contract: the 51,3% of the window that tool results occupy
    /// is attributed to the tool that produced it, or the dominant emitter
    /// cannot be named — and an unnamed dominant emitter cannot be acted on.
    #[test]
    fn tool_output_volume_is_attributed_per_tool() {
        let big = "a".repeat(300);
        let agg = run(&[
            call("c1", "Bash", json!({"command": "ls"})),
            result("c1", &big),
            call("c2", "Read", json!({"file_path": "/x.rs"})),
            result("c2", "curto"),
        ]);
        assert_eq!(agg.by_tool["Bash"].bytes, 300, "Bash carrega os 300 B");
        assert_eq!(agg.by_tool["Bash"].results, 1);
        assert_eq!(agg.by_tool["Read"].bytes, 5, "Read carrega os seus 5 B");
        assert_eq!(
            agg.tool_result_bytes, 305,
            "a soma por ferramenta e o total: uma fonte, nao duas"
        );
    }

    /// The defect the live probe caught, kept as a test because it made the
    /// whole metric read 0,0% and the zero looked like a finding: `Read` returns
    /// `cat -n`, so every line carries a `<n>\t` prefix and never matches the
    /// quotation of it. Reverting `normalize_result_line` to the identity turns
    /// this assertion red.
    #[test]
    fn o_prefixo_de_numeracao_do_read_nao_zera_o_reuso() {
        assert_eq!(
            normalize_result_line("  12\tfn alvo_da_medicao() {"),
            "fn alvo_da_medicao() {",
            "o prefixo do cat -n sai; sem isso nada casa"
        );
        assert_eq!(
            normalize_result_line("abc\tdef"),
            "abc def",
            "so numero antes da tabulacao e prefixo — texto e conteudo"
        );
        let agg = run(&[
            call("c1", "Read", json!({"file_path": "/x.rs"})),
            result(
                "c1",
                "  12\tfn alvo_da_medicao() {\n  13\t    outra_coisa();",
            ),
            says("a funcao e\nfn alvo_da_medicao() {\ne resolve"),
        ]);
        let read = &agg.by_tool["Read"];
        assert_eq!(read.lines, 2, "duas linhas informativas voltaram");
        assert_eq!(read.reused_lines, 1, "a citada conta, a outra nao");
        assert_eq!(read.zero_reuse_results, 0);
    }

    /// The window closes at the human turn: a fact quoted after the person
    /// speaks again belongs to the next task, and counting it would inflate
    /// reuse with work this result never informed.
    #[test]
    fn um_fato_citado_depois_do_turno_humano_nao_conta() {
        let agg = run(&[
            call("c1", "Read", json!({"file_path": "/x.rs"})),
            result("c1", "  12\tfn alvo_da_medicao() {"),
            human("outra tarefa"),
            says("fn alvo_da_medicao() {"),
        ]);
        assert_eq!(
            agg.by_tool["Read"].reused_lines, 0,
            "a janela fechou antes da citacao"
        );
        assert_eq!(agg.by_tool["Read"].zero_reuse_results, 1);
    }

    /// The only population a digest could drop outright, counted separately —
    /// measured at 3–6% live, which is why truncation is the wrong instrument
    /// and spill-with-retrieval is the right one.
    #[test]
    fn resultado_que_ninguem_tocou_conta_como_zero_reuso() {
        let agg = run(&[
            call("c1", "Bash", json!({"command": "ls"})),
            result("c1", "zzqqx_inexistente_um\nzzqqx_inexistente_dois"),
            says("segui por outro caminho"),
        ]);
        let bash = &agg.by_tool["Bash"];
        assert_eq!(bash.reused_lines, 0);
        assert_eq!(bash.reused_bytes, 0);
        assert_eq!(bash.zero_reuse_results, 1);
        let payload = budget_from_aggregate(&agg);
        assert_eq!(payload["tool_output"]["reuse_ratio"], json!(0.0));
        assert_eq!(payload["tool_output"]["by_tool"][0]["tool"], "Bash");
    }

    fn run(lines: &[String]) -> ContextBudgetAggregate {
        let mut agg = ContextBudgetAggregate::default();
        let mut seen = BTreeMap::new();
        aggregate_lines(lines.iter().map(String::as_str), &mut agg, &mut seen);
        agg
    }

    #[test]
    fn the_aggregate_counts_injection_turns_and_the_byte_identical_repeat() {
        let agg = run(&synthetic());
        assert_eq!(agg.injected_blocks, 3, "3 hook blocks");
        assert_eq!(
            agg.duplicate_blocks, 1,
            "the 2nd identical block is the repeat"
        );
        assert_eq!(
            agg.user_turns, 1,
            "only the text-carrying user message is a turn"
        );
        assert_eq!(agg.tool_calls, 2);
        assert_eq!(agg.must_emitted, 2);
        assert_eq!(agg.must_followed, 1, "1st MUST matched, 2nd did not");
        assert_eq!(agg.tool_result_bytes, "saida grande".len() as u64);
        assert!(agg.by_family.contains_key("touring-suggest"));
        assert!(
            agg.by_family.contains_key("other"),
            "[generic] has no family"
        );
    }

    #[test]
    fn a_block_with_no_directive_line_is_counted_as_zero_proposition() {
        let agg = run(&synthetic());
        let other = &agg.by_family["other"];
        assert_eq!(other.propositions, 0);
        assert_eq!(
            other.zero_proposition_blocks, 1,
            "[generic] carries no instruction"
        );
        let sug = &agg.by_family["touring-suggest"];
        assert_eq!(sug.propositions, 2, "one MUST line per block");
        assert_eq!(sug.zero_proposition_blocks, 0);
    }

    /// Each ceiling is asserted by an aggregate that violates THAT ONE, so a
    /// failure names which ruler moved. The synthetic transcript is small on
    /// purpose — 95 injected bytes over one turn is nowhere near the per-turn
    /// ceiling, and a test that claimed otherwise would be asserting arithmetic
    /// it had not done.
    #[test]
    fn each_ceiling_fails_on_its_own_violation_and_none_on_a_clean_aggregate() {
        let base = ContextBudgetAggregate {
            user_turns: 10,
            injected_bytes: 1000,
            injected_blocks: 5,
            must_emitted: 4,
            must_followed: 4,
            ..Default::default()
        };
        let clean = budget_from_aggregate(&base);
        assert_eq!(clean["status"], "PASS", "{clean}");
        assert_eq!(clean["injected_bytes_per_turn"], 100.0);
        assert_eq!(clean["injection_follow_ratio"], 1.0);

        let fat = budget_from_aggregate(&ContextBudgetAggregate {
            injected_bytes: 40_000, // 4 000 B/turn > 3 000
            ..base.clone()
        });
        assert_eq!(fat["failing"], json!(["injected_bytes_per_turn"]), "{fat}");

        let repetitive = budget_from_aggregate(&ContextBudgetAggregate {
            duplicate_bytes: 500, // metade da injecao > teto 0,05
            ..base.clone()
        });
        assert_eq!(
            repetitive["failing"],
            json!(["duplicate_injection_ratio"]),
            "{repetitive}"
        );

        let ignored = budget_from_aggregate(&ContextBudgetAggregate {
            must_followed: 1, // 0,25 < piso 0,60
            ..base.clone()
        });
        assert_eq!(
            ignored["failing"],
            json!(["injection_follow_ratio"]),
            "{ignored}"
        );
    }

    /// The synthetic transcript end-to-end: a 1-in-3 byte-identical repeat and a
    /// MUST followed once out of twice are BOTH over their rulers, and the
    /// per-turn ceiling is correctly NOT among them.
    #[test]
    fn the_synthetic_transcript_fails_exactly_the_rulers_it_violates() {
        let v = budget_from_aggregate(&run(&synthetic()));
        assert_eq!(v["available"], true);
        assert_eq!(
            v["failing"],
            json!(["duplicate_injection_ratio", "injection_follow_ratio"]),
            "{v}"
        );
        assert_eq!(v["injection_follow_ratio"], 0.5);
        assert_eq!(v["must_emitted"], 2);
    }

    #[test]
    fn an_empty_scan_is_a_stub_not_a_zero() {
        let v = budget_from_aggregate(&ContextBudgetAggregate::default());
        assert_eq!(v["status"], "STUB");
        assert_eq!(v["available"], false);
        assert!(
            !v["reason"].is_null(),
            "absence must be displayed, not silent"
        );
        // Never a fabricated 0.0 for a ratio nobody measured.
        assert!(v["injection_follow_ratio"].is_null());
        assert!(v["tpcd_proxy_bytes_per_tool_call"].is_null());
    }

    #[test]
    fn the_family_taxonomy_has_one_source_and_covers_the_measured_population() {
        // The families the 04/09 census actually found, each pinned to the text
        // that identifies it. A block that matches nothing is "other" — never
        // silently attributed to a neighbour.
        for (text, want) in [
            ("[TOURING SUGGEST · code-mode-loop]", "touring-suggest"),
            ("[CODE MODE · rajada] 2a inspecao", "code-mode-deny"),
            ("[G7 re-inspeção] 2a leitura", "code-mode-deny"),
            ("[LOOP RESUME] flow 'work-outer'", "loop-outer"),
            ("Touring Knowledge: 5156 files known", "session-start"),
            ("[PROMPT ENHANCEMENT -- DEBUG MODE]", "prompt-enhance"),
            ("⚠ lições de erros passados para esta ação:", "past-lessons"),
            ("temporal: reliability 96% (7d)", "post-tool-echo"),
            ("[gabrielgadea]", "other"),
        ] {
            assert_eq!(injection_family(text), want, "for {text:?}");
        }
    }

    #[test]
    fn the_transcript_directory_matches_the_one_claude_code_actually_uses() {
        let d = transcript_dir_for(
            Path::new("/home/gabrielgadea"),
            Path::new("/home/gabrielgadea/projects/touring"),
        );
        assert!(
            d.ends_with("-home-gabrielgadea-projects-touring"),
            "slug drifted: {}",
            d.display()
        );
        assert!(d.starts_with("/home/gabrielgadea/.claude/projects"));
    }

    #[test]
    fn a_plain_text_hook_still_costs_the_window_and_is_counted() {
        // A hook that printed prose instead of JSON consumed the window all the
        // same; counting only well-formed JSON would under-report the cost.
        let line = json!({"type": "attachment", "attachment":
            {"type": "hook_success", "stdout": "aviso solto sem json"}})
        .to_string();
        let agg = run(&[line]);
        assert_eq!(agg.injected_blocks, 1);
        assert_eq!(agg.injected_bytes, "aviso solto sem json".len() as u64);
    }
    /// Sonda VIVA da régua contra os transcripts reais deste projeto.
    ///
    /// `#[ignore]` porque depende do HOME de quem roda — mas é o único controle
    /// que prova que a régua enxerga dados de verdade, e não só as linhas
    /// sintéticas acima. Sem ela, "a régua compila e passa nos testes" seria
    /// exatamente a asserção negativa satisfeita por cegueira que esta sessão
    /// inteira tem perseguido.
    ///
    /// Rodar: `cargo test -p touring-cli --lib live_probe -- --ignored --nocapture`
    #[test]
    #[ignore = "depende dos transcripts reais do HOME"]
    fn live_probe_the_ruler_reads_real_transcripts() {
        let root = std::path::Path::new("/home/gabrielgadea/projects/touring");
        if !root.is_dir() {
            eprintln!("skip: projeto ausente");
            return;
        }
        let v = context_budget(root);
        eprintln!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        // Um ambiente SEM corpus (CI limpo, container) não é um instrumento
        // quebrado — é um ambiente sem dado, e a resposta honesta é pular
        // dizendo isso. Sem esta saída a sonda não podia entrar em CI, e por
        // isso não entrava: `--ignored` não aparecia em nenhum workflow nem
        // script, então o único instrumento que já pegou um erro de 12x nunca
        // rodava sozinho. Guard que existe e não roda é certificado, não guard.
        if v["available"] != true {
            eprintln!(
                "SKIP live_probe: sem corpus de transcript neste ambiente ({})",
                v["reason"]
            );
            return;
        }
        assert_eq!(v["available"], true, "a regua nao achou transcript: {v}");
        assert!(
            v["transcripts_scanned"].as_u64().unwrap_or(0) > 0,
            "zero transcripts lidos — instrumento mudo"
        );
        assert!(
            v["user_turns"].as_u64().unwrap_or(0) > 0,
            "zero turnos — o denominador do teto por turno nao existiria"
        );
        assert!(
            v["window_bytes"]["injection"].as_u64().unwrap_or(0) > 0,
            "zero injecao medida — o predicado de bloco de hook nao casou nada"
        );
        // P5: the tool-result attribution, against real data. Nine synthetic
        // tests were green while the ruler still reported 12x the true rate;
        // only the live probe caught it. The same discipline applies here — a
        // reuse ratio of exactly zero over real transcripts is the signature of
        // a broken matcher, not of a model that ignores what it asks for.
        let tools = v["tool_output"]["by_tool"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(
            !tools.is_empty(),
            "zero ferramentas atribuidas — a atribuicao nao casou nada: {}",
            v["tool_output"]
        );
        assert!(
            v["tool_output"]["reuse_ratio"].as_f64().unwrap_or(0.0) > 0.0,
            "reuso exatamente zero sobre dados reais e assinatura de matcher \
             quebrado (foi o que o prefixo do cat -n causou): {}",
            v["tool_output"]
        );
        assert!(
            tools
                .iter()
                .any(|t| t["zero_reuse_results"].as_u64().unwrap_or(u64::MAX)
                    < t["results"].as_u64().unwrap_or(0)),
            "toda ferramenta com 100% de zero-reuso: o instrumento, nao o sistema"
        );
    }
    /// O defeito que a sonda viva pegou: `content` como STRING crua e' a forma
    /// comum de um prompt de usuario real, e exigir um array descartava esses
    /// turnos. O denominador do teto por turno perdia a maioria das linhas e o
    /// numero saia 12x alto — confiantemente errado, que e' pior que ausente.
    #[test]
    fn a_user_message_whose_content_is_a_bare_string_still_counts_as_a_turn() {
        let lines = vec![
            json!({"type": "user", "message": {"content": "faca a coisa"}}).to_string(),
            json!({"type": "user", "message": {"content": [
                {"type": "text", "text": "e depois esta"}
            ]}})
            .to_string(),
            json!({"type": "assistant", "message": {"content": "prosa do modelo"}}).to_string(),
        ];
        let agg = run(&lines);
        assert_eq!(agg.user_turns, 2, "as DUAS formas de conteudo sao turnos");
        assert_eq!(
            agg.user_bytes,
            "faca a coisa".len() as u64 + "e depois esta".len() as u64
        );
        assert_eq!(agg.assistant_bytes, "prosa do modelo".len() as u64);
    }
}
