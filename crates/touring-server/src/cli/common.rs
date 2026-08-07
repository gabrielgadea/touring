//! Shared CLI infrastructure for all touring subcommands.
//!
//! Provides argument parsing, output formatting, daemon communication helpers,
//! and a command descriptor system for table-driven dispatch.

use super::daemon_query;
use std::sync::atomic::{AtomicBool, Ordering};

/// Process-global mirror of the `--brief` flag, set by [`parse_global_flags`] so
/// daemon-backed handlers that do not thread [`GlobalFlags`] (e.g. the `wiring`
/// subcommands, which print directly) can still honour it via
/// [`brief_output_enabled`]. Mirrors the `DAEMON_READ_TIMEOUT_SECS` pattern.
static BRIEF_OUTPUT: AtomicBool = AtomicBool::new(false);

// ─────────────────────────────────────────────────────────────────────────────
// Global flags parsed from any position in the argument list
// ─────────────────────────────────────────────────────────────────────────────

/// Global CLI flags extracted from args before subcommand-specific parsing.
#[derive(Debug, Clone)]
pub struct GlobalFlags {
    /// Output raw JSON (no pretty-printing). Activated by `-j` or `--json`.
    pub json: bool,
    /// Daemon socket timeout in seconds. Default: 10.
    pub timeout_secs: u64,
    /// Enable verbose tracing output to stderr.
    pub verbose: bool,
    /// Emit a lean output shape: elide large arrays (e.g. the ~170 K wiring
    /// orphans) to a count, keeping the LLM context budget small. Activated by
    /// `--brief`. Scalars and small arrays are preserved — truncation-with-count,
    /// never a silent cut. See [`slim_large_arrays`] / [`shape_daemon_output`].
    pub brief: bool,
}

impl Default for GlobalFlags {
    fn default() -> Self {
        Self {
            json: false,
            timeout_secs: 10,
            verbose: false,
            brief: false,
        }
    }
}

/// Parse global flags from an argument slice, returning the flags and
/// a filtered `Vec<String>` with global flags removed.
pub fn parse_global_flags(args: &[String]) -> (GlobalFlags, Vec<String>) {
    // Seed `brief` from the process mirror so a heavy-command default set by
    // `apply_heavy_brief_default` (in the main dispatch) reaches handlers that
    // thread `GlobalFlags`, not only those reading `brief_output_enabled()`. (A1)
    let mut flags = GlobalFlags {
        brief: BRIEF_OUTPUT.load(Ordering::Relaxed),
        ..GlobalFlags::default()
    };
    let mut filtered = Vec::with_capacity(args.len());
    let mut skip_next = false;
    let mut full = false;

    for (i, arg) in args.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        match arg.as_str() {
            "-j" | "--json" => flags.json = true,
            "-v" | "--verbose" => flags.verbose = true,
            "--brief" => {
                flags.brief = true;
                BRIEF_OUTPUT.store(true, Ordering::Relaxed);
            }
            // `--full` opts out of any brief default (heavy-command auto-brief or
            // the atomic mirror) and restores the complete output. (A1)
            "--full" => full = true,
            "--timeout" => {
                if let Some(val) = args.get(i + 1) {
                    let t = val.parse().unwrap_or(10);
                    flags.timeout_secs = t;
                    super::DAEMON_READ_TIMEOUT_SECS.store(t, std::sync::atomic::Ordering::Relaxed);
                    skip_next = true;
                }
            }
            _ => filtered.push(arg.clone()),
        }
    }

    // `--full` wins over any brief default — restore the complete output. (A1)
    if full {
        flags.brief = false;
        BRIEF_OUTPUT.store(false, Ordering::Relaxed);
    }

    (flags, filtered)
}

// ─────────────────────────────────────────────────────────────────────────────
// Subcommand argument helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Extract a single positional arg at `index`, or return `default`.
pub fn arg_or<'a>(args: &'a [String], index: usize, default: &'a str) -> &'a str {
    args.get(index).map(|s| s.as_str()).unwrap_or(default)
}

/// Join all args from `start` onward into a single space-separated string.
pub fn args_joined(args: &[String], start: usize) -> String {
    args.get(start..)
        .map(|slice| slice.join(" "))
        .unwrap_or_default()
}

/// Look for a `--flag value` pair in `args` and return the value.
pub fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|w| w.first().map(|s| s.as_str()) == Some(flag))
        .and_then(|w| w.get(1).map(|s| s.as_str()))
}

/// Check if a boolean flag (e.g. `--definitions-only`) is present.
pub fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

// ─────────────────────────────────────────────────────────────────────────────
// Output helpers — stdout/stderr separation
// ─────────────────────────────────────────────────────────────────────────────

/// Write JSON value to stdout (for machine-readable output).
pub fn json_to_stdout(result: &str) {
    print!("{result}");
}

/// Write human-readable message to stderr (for user-facing output).
pub fn human_to_stderr(msg: &str) {
    eprintln!("{msg}");
}

/// Write the YAML document carried by an export response to `path`.
///
/// The export hooks answer with a JSON envelope — `{"success": true, "<field>":
/// "<yaml>"}` — and writing that envelope verbatim is what broke the
/// export→validate round-trip: `validate` re-read the file, parsed the envelope
/// as YAML (JSON *is* valid YAML) and rejected it with `missing field
/// 'version'`, because it was reading the wrapper instead of the document.
/// Only the YAML string belongs in the file.
pub fn write_yaml_export(
    output: &str,
    field: &str,
    path: &std::path::Path,
) -> anyhow::Result<()> {
    let parsed: serde_json::Value = serde_json::from_str(output)
        .map_err(|e| anyhow::anyhow!("export returned malformed JSON: {e}"))?;
    if let Some(err) = parsed.get("error").and_then(|v| v.as_str()) {
        anyhow::bail!("export failed: {err}");
    }
    let yaml = parsed
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("export response carries no '{field}' field"))?;
    std::fs::write(path, yaml)
        .map_err(|e| anyhow::anyhow!("Failed to write '{}': {}", path.display(), e))
}

// ─────────────────────────────────────────────────────────────────────────────
// Daemon query with output formatting
// ─────────────────────────────────────────────────────────────────────────────

/// Recursively elide JSON arrays larger than 512 bytes to
/// `{"_elided_array_len": N}`, preserving sibling scalars and small arrays. This
/// is truncation-*with-count* (never a silent cut) — e.g. the `wiring` snapshot's
/// ~170 K-entry orphan array collapses to its length while `count`/`orphan_count`
/// scalars survive untouched. Objects are recursed so a large array nested under
/// a key is elided in place without dropping its siblings.
pub fn slim_large_arrays(v: &serde_json::Value) -> serde_json::Value {
    const ELIDE_THRESHOLD_BYTES: usize = 512;
    match v {
        serde_json::Value::Array(items) => {
            let too_big =
                serde_json::to_string(v).map(|s| s.len()).unwrap_or(0) > ELIDE_THRESHOLD_BYTES;
            if too_big {
                serde_json::json!({ "_elided_array_len": items.len() })
            } else {
                v.clone()
            }
        }
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, val) in map {
                out.insert(k.clone(), slim_large_arrays(val));
            }
            serde_json::Value::Object(out)
        }
        other => other.clone(),
    }
}

/// Shape a daemon JSON response string for output per the global flags.
///
/// `--brief` elides large arrays (`slim_large_arrays`) so the LLM context stays
/// lean — e.g. `wiring orphans` collapses its ~170 K-entry array to a count.
/// Fast path: `-j` without `--brief` returns the exact daemon bytes unchanged
/// (zero re-serialization). Human mode pretty-prints; invalid JSON passes through.
pub fn shape_daemon_output(output: &str, flags: &GlobalFlags) -> String {
    // Preserve the exact daemon bytes when no shaping is requested.
    if flags.json && !flags.brief {
        return output.to_string();
    }
    match serde_json::from_str::<serde_json::Value>(output) {
        Ok(val) => {
            let shaped = if flags.brief {
                slim_large_arrays(&val)
            } else {
                val
            };
            let rendered = if flags.json {
                serde_json::to_string(&shaped)
            } else {
                serde_json::to_string_pretty(&shaped)
            };
            rendered.unwrap_or_else(|_| output.to_string())
        }
        Err(_) => output.to_string(),
    }
}

/// Returns whether `--brief` output shaping is active for this process (mirror
/// of the parsed flag — see `BRIEF_OUTPUT`). Used by daemon-backed handlers
/// that print directly without threading [`GlobalFlags`].
pub fn brief_output_enabled() -> bool {
    BRIEF_OUTPUT.load(Ordering::Relaxed)
}

/// Command families whose default output is large enough to blow the LLM context
/// budget — `viz`/`wiring`/`graph` snapshots reach megabytes (e.g. `viz workspace`
/// ~3.5 MB, `wiring audit` ~1.2 MB). These default to `--brief` (large arrays
/// elided to counts) unless the caller opts back in with `--full`. (A1)
const HEAVY_BRIEF_COMMANDS: &[&str] = &["wiring", "viz", "graph"];

/// Enable `--brief` by default for a heavy-output command family unless the caller
/// passed `--full`. Sets the process `BRIEF_OUTPUT` mirror — which
/// [`parse_global_flags`] seeds into threaded `GlobalFlags` — and returns whether
/// brief was enabled so the decision stays unit-testable. Small outputs are
/// unaffected: only arrays over the [`slim_large_arrays`] threshold are elided. (A1)
pub fn apply_heavy_brief_default(command: &str, args: &[String]) -> bool {
    let opted_out = args.iter().any(|a| a == "--full");
    let enable = HEAVY_BRIEF_COMMANDS.contains(&command) && !opted_out;
    if enable {
        BRIEF_OUTPUT.store(true, Ordering::Relaxed);
    }
    enable
}

/// Slim a daemon JSON response string when `brief` is set: parse, elide large
/// arrays via [`slim_large_arrays`], and pretty-print. When `brief` is false the
/// `output` is returned unchanged. Invalid JSON passes through verbatim. Pure in
/// `brief` so it stays unit-testable without touching the process atomic.
pub fn maybe_slim_json(output: &str, brief: bool) -> String {
    if !brief {
        return output.to_string();
    }
    match serde_json::from_str::<serde_json::Value>(output) {
        Ok(val) => serde_json::to_string_pretty(&slim_large_arrays(&val))
            .unwrap_or_else(|_| output.to_string()),
        Err(_) => output.to_string(),
    }
}

/// Send a daemon query and print the result, respecting global flags.
///
/// This is the single entry point for all daemon-backed CLI commands.
/// It handles JSON pretty-printing, error formatting, and the exit contract.
pub fn run_daemon_cmd(
    hook: &str,
    payload: serde_json::Value,
    flags: &GlobalFlags,
) -> anyhow::Result<()> {
    let output = daemon_query(hook, payload)?;
    json_to_stdout(&shape_daemon_output(&output, flags));
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Command descriptor for table-driven dispatch
// ─────────────────────────────────────────────────────────────────────────────

/// How a command handles errors in main.rs dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorPolicy {
    /// Hook contract: exit 0 even on error (never block user operations).
    HookSilent,
    /// Non-hook: exit 1 on error, report to stderr.
    ExitOnError,
}

/// A registered CLI command with its handler and metadata.
pub struct CommandDescriptor {
    /// Primary command name (e.g. "ast", "wiring", "classify-intent").
    pub name: &'static str,
    /// Short one-line description for `--help`.
    pub description: &'static str,
    /// Error handling policy.
    pub error_policy: ErrorPolicy,
    /// The handler function. Receives the full args slice.
    pub handler: fn(&[String]) -> anyhow::Result<()>,
}

// NOTE: no `pub use super::command_table::command_table;` re-export here — it
// formed a `common ↔ command_table` module import cycle (command_table already
// uses common's descriptor types). Callers reach it via the canonical
// `cli::command_table::command_table` path directly (F1.8 decoupling, 2026-07-02).

/// Print auto-generated help text from the command table.
pub fn print_help(commands: &[CommandDescriptor]) {
    eprintln!("touring — Unified Rust MCP Mega-Server + Neural Hook Accelerator");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("  touring <command> [subcommand] [flags]");
    eprintln!();
    eprintln!("GLOBAL FLAGS:");
    eprintln!("  -j, --json       Raw JSON output (machine-readable)");
    eprintln!("  -v, --verbose    Verbose tracing to stderr");
    eprintln!("  --brief          Elide large arrays to counts (lean LLM-context output)");
    eprintln!(
        "  --full           Force complete output (opt out of the heavy-command brief default)"
    );
    eprintln!("  --timeout <N>    Daemon socket timeout in seconds (default: 10)");
    eprintln!();

    // Group commands by policy
    let hooks: Vec<_> = commands
        .iter()
        .filter(|c| c.error_policy == ErrorPolicy::HookSilent)
        .collect();
    let tools: Vec<_> = commands
        .iter()
        .filter(|c| c.error_policy == ErrorPolicy::ExitOnError)
        .collect();

    eprintln!("HOOK COMMANDS (exit 0 always — never block user operations):");
    for cmd in &hooks {
        eprintln!("  {:<24} {}", cmd.name, cmd.description);
    }

    eprintln!();
    eprintln!("TOOL COMMANDS (exit 1 on error):");
    for cmd in &tools {
        eprintln!("  {:<24} {}", cmd.name, cmd.description);
    }

    eprintln!();
    eprintln!("BUILT-IN:");
    eprintln!("  serve                    Start MCP server (stdio)");
    eprintln!("  --help, -h, help         Show this help");
    eprintln!("  --version, -V, version   Show version");
    eprintln!();
    eprintln!("Hooks read JSON from stdin (pipe). If stdin is a terminal, empty {{}} is used.");
    eprintln!("Use -j for machine-readable JSON output suitable for piping to jq.");
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::command_table::command_table;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    /// Regression guard for the export→validate round-trip (2026-08-02).
    ///
    /// `tasksfile export --file` used to write the JSON envelope, so
    /// `tasksfile validate` rejected the very file the export had just
    /// produced (`missing field 'version'` — it was parsing the wrapper).
    /// The file must hold the YAML document and nothing else.
    #[test]
    fn write_yaml_export_writes_the_document_not_the_envelope() {
        let dir = std::env::temp_dir().join("touring_write_yaml_export_doc");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("out.yml");
        let yaml = "version: '1.0'\nmetadata:\n  name: t1\ntasks: {}\n";
        let envelope = serde_json::json!({ "success": true, "tasksfile_yaml": yaml }).to_string();

        write_yaml_export(&envelope, "tasksfile_yaml", &path).expect("write");

        let written = std::fs::read_to_string(&path).expect("read back");
        assert_eq!(written, yaml, "the file must carry the YAML document");
        assert!(
            !written.contains("\"success\""),
            "the JSON envelope must not reach the file: {written}"
        );
        // And the document round-trips as YAML, which the envelope never did.
        let parsed: serde_json::Value = serde_yaml::from_str(&written).expect("valid YAML");
        assert_eq!(parsed.get("version").and_then(|v| v.as_str()), Some("1.0"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_yaml_export_surfaces_the_handler_error() {
        let path = std::env::temp_dir().join("touring_write_yaml_export_err.yml");
        let _ = std::fs::remove_file(&path);
        let envelope = serde_json::json!({ "success": false, "error": "No task_id provided" })
            .to_string();

        let err = write_yaml_export(&envelope, "tasksfile_yaml", &path)
            .expect_err("a failed export must not be written as if it succeeded");

        assert!(
            err.to_string().contains("No task_id provided"),
            "the daemon's reason must survive: {err}"
        );
        assert!(!path.exists(), "nothing may be written on a failed export");
    }

    #[test]
    fn write_yaml_export_rejects_a_missing_field_and_bad_json() {
        let path = std::env::temp_dir().join("touring_write_yaml_export_missing.yml");
        let _ = std::fs::remove_file(&path);

        let no_field = serde_json::json!({ "success": true }).to_string();
        let err = write_yaml_export(&no_field, "tasksfile_yaml", &path).expect_err("no field");
        assert!(err.to_string().contains("tasksfile_yaml"), "{err}");

        let err = write_yaml_export("not json at all", "tasksfile_yaml", &path)
            .expect_err("malformed JSON");
        assert!(err.to_string().contains("malformed JSON"), "{err}");
        assert!(!path.exists(), "no file on either failure");
    }

    #[test]
    fn parse_global_flags_json_short() {
        let (flags, filtered) = parse_global_flags(&s(&["touring", "index", "-j", "status"]));
        assert!(flags.json);
        assert_eq!(filtered, s(&["touring", "index", "status"]));
    }

    #[test]
    fn parse_global_flags_json_long() {
        let (flags, filtered) = parse_global_flags(&s(&["touring", "--json", "wiring", "orphans"]));
        assert!(flags.json);
        assert_eq!(filtered, s(&["touring", "wiring", "orphans"]));
    }

    #[test]
    fn parse_global_flags_verbose() {
        let (flags, _) = parse_global_flags(&s(&["touring", "-v", "status"]));
        assert!(flags.verbose);
    }

    #[test]
    fn parse_global_flags_brief() {
        let (flags, filtered) =
            parse_global_flags(&s(&["touring", "--brief", "wiring", "orphans"]));
        assert!(flags.brief);
        // `--brief` is consumed like the other global flags.
        assert_eq!(filtered, s(&["touring", "wiring", "orphans"]));
    }

    #[test]
    fn slim_large_arrays_elides_big_array_keeps_scalars() {
        // Mirror the real `wiring.hypergraph_cycles` shape: a scalar count plus a
        // huge detail array. The detail collapses to a length stub; the sibling
        // scalars survive (the composite_health_score inputs).
        let big: Vec<i32> = (0..500).collect();
        let input = serde_json::json!({
            "orphan_count": 4823,
            "total_pub_symbols": 10000,
            "hypergraph_cycles": { "count": 1929, "detail": big },
        });
        let slim = slim_large_arrays(&input);
        assert_eq!(slim["orphan_count"], serde_json::json!(4823));
        assert_eq!(slim["hypergraph_cycles"]["count"], serde_json::json!(1929));
        assert_eq!(
            slim["hypergraph_cycles"]["detail"]["_elided_array_len"],
            serde_json::json!(500)
        );
        let before = serde_json::to_string(&input)
            .expect("serialize input")
            .len();
        let after = serde_json::to_string(&slim).expect("serialize slim").len();
        assert!(
            after < before / 4,
            "expected >4x shrink: {before} -> {after}"
        );
    }

    #[test]
    fn slim_large_arrays_keeps_small_arrays_and_scalars() {
        // Arrays under the 512-byte threshold are untouched; plain scalars too.
        let input = serde_json::json!({ "tags": ["a", "b", "c"], "n": 42 });
        let slim = slim_large_arrays(&input);
        assert_eq!(slim["tags"], serde_json::json!(["a", "b", "c"]));
        assert_eq!(slim["n"], serde_json::json!(42));
    }

    #[test]
    fn shape_daemon_output_json_without_brief_is_byte_exact() {
        // Fast path: `-j` without `--brief` preserves the daemon bytes verbatim.
        let raw = r#"{"a":1,"big":[0,1,2,3]}"#;
        let flags = GlobalFlags {
            json: true,
            brief: false,
            ..Default::default()
        };
        assert_eq!(shape_daemon_output(raw, &flags), raw);
    }

    #[test]
    fn shape_daemon_output_brief_elides_large_arrays() {
        let big: Vec<i32> = (0..500).collect();
        let raw = serde_json::to_string(&serde_json::json!({ "n": 7, "detail": big }))
            .expect("serialize input");
        let flags = GlobalFlags {
            json: true,
            brief: true,
            ..Default::default()
        };
        let out = shape_daemon_output(&raw, &flags);
        assert!(out.contains("_elided_array_len"), "brief must elide: {out}");
        assert!(out.contains("\"n\":7"), "scalar must survive: {out}");
        assert!(
            out.len() < raw.len() / 4,
            "expected shrink: {} -> {}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn shape_daemon_output_invalid_json_passes_through() {
        // Non-JSON daemon output is never mangled — returned verbatim.
        let raw = "not json at all";
        let flags = GlobalFlags {
            brief: true,
            ..Default::default()
        };
        assert_eq!(shape_daemon_output(raw, &flags), raw);
    }

    #[test]
    fn maybe_slim_json_respects_brief_param() {
        let big: Vec<i32> = (0..500).collect();
        let raw = serde_json::to_string(&serde_json::json!({ "n": 7, "detail": big }))
            .expect("serialize input");
        // brief=false → byte-exact passthrough (the daemon bytes are preserved).
        assert_eq!(maybe_slim_json(&raw, false), raw);
        // brief=true → large array elided, scalar preserved.
        let slim = maybe_slim_json(&raw, true);
        assert!(slim.contains("_elided_array_len"), "must elide: {slim}");
        assert!(slim.contains("\"n\""), "scalar must survive: {slim}");
        assert!(
            slim.len() < raw.len() / 4,
            "expected shrink: {} -> {}",
            raw.len(),
            slim.len()
        );
        // Invalid JSON is returned verbatim even with brief on.
        assert_eq!(maybe_slim_json("not json", true), "not json");
    }

    #[test]
    fn parse_global_flags_timeout() {
        let (flags, filtered) =
            parse_global_flags(&s(&["touring", "--timeout", "30", "index", "find", "Foo"]));
        assert_eq!(flags.timeout_secs, 30);
        assert_eq!(filtered, s(&["touring", "index", "find", "Foo"]));
    }

    #[test]
    fn parse_global_flags_timeout_invalid_fallback() {
        let (flags, _) = parse_global_flags(&s(&["touring", "--timeout", "abc"]));
        assert_eq!(flags.timeout_secs, 10);
    }

    #[test]
    fn parse_global_flags_combined() {
        let (flags, filtered) = parse_global_flags(&s(&[
            "touring",
            "-j",
            "-v",
            "--timeout",
            "5",
            "ast",
            "find",
            "Foo",
        ]));
        assert!(flags.json);
        assert!(flags.verbose);
        assert_eq!(flags.timeout_secs, 5);
        assert_eq!(filtered, s(&["touring", "ast", "find", "Foo"]));
    }

    #[test]
    fn parse_global_flags_no_flags() {
        let (flags, filtered) = parse_global_flags(&s(&["touring", "index", "status"]));
        assert!(!flags.json);
        assert!(!flags.verbose);
        assert_eq!(flags.timeout_secs, 10);
        assert_eq!(filtered, s(&["touring", "index", "status"]));
    }

    #[test]
    fn arg_or_returns_value_when_present() {
        let args = s(&["touring", "ast", "find", "MyStruct"]);
        assert_eq!(arg_or(&args, 3, ""), "MyStruct");
    }

    #[test]
    fn arg_or_returns_default_when_missing() {
        let args = s(&["touring", "ast"]);
        assert_eq!(arg_or(&args, 3, "find"), "find");
    }

    #[test]
    fn args_joined_collects_tail() {
        let args = s(&["touring", "session", "start", "my-session", "bug", "fix"]);
        assert_eq!(args_joined(&args, 3), "my-session bug fix");
    }

    #[test]
    fn args_joined_empty_when_out_of_range() {
        let args = s(&["touring", "session"]);
        assert_eq!(args_joined(&args, 5), "");
    }

    #[test]
    fn flag_value_finds_pair() {
        let args = s(&["touring", "index", "rebuild", "--dir", "src/"]);
        assert_eq!(flag_value(&args, "--dir"), Some("src/"));
    }

    #[test]
    fn flag_value_none_when_missing() {
        let args = s(&["touring", "index", "rebuild"]);
        assert_eq!(flag_value(&args, "--dir"), None);
    }

    #[test]
    fn has_flag_detects_presence() {
        let args = s(&["touring", "index", "find", "Foo", "--definitions-only"]);
        assert!(has_flag(&args, "--definitions-only"));
        assert!(!has_flag(&args, "--json"));
    }

    #[test]
    fn command_table_has_entries() {
        let table = command_table();
        assert!(
            table.len() >= 30,
            "expected at least 30 commands, got {}",
            table.len()
        );
    }

    #[test]
    fn command_table_names_are_unique() {
        let table = command_table();
        let mut names: Vec<&str> = table.iter().map(|c| c.name).collect();
        let original_len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(
            names.len(),
            original_len,
            "duplicate command names in table"
        );
    }

    #[test]
    fn command_table_has_hooks_and_tools() {
        let table = command_table();
        let hooks = table
            .iter()
            .filter(|c| c.error_policy == ErrorPolicy::HookSilent)
            .count();
        let tools = table
            .iter()
            .filter(|c| c.error_policy == ErrorPolicy::ExitOnError)
            .count();
        assert!(hooks >= 10, "expected >= 10 hooks, got {hooks}");
        assert!(tools >= 15, "expected >= 15 tools, got {tools}");
    }

    #[test]
    fn apply_heavy_brief_default_enables_for_heavy_families() {
        // Heavy-output families opt into brief by default; others do not. (A1)
        assert!(apply_heavy_brief_default("wiring", &[]));
        assert!(apply_heavy_brief_default("viz", &[]));
        assert!(apply_heavy_brief_default("graph", &[]));
        assert!(!apply_heavy_brief_default("index", &[]));
        assert!(!apply_heavy_brief_default("ast", &[]));
        BRIEF_OUTPUT.store(false, Ordering::Relaxed); // reset process mirror
    }

    #[test]
    fn apply_heavy_brief_default_full_opts_out() {
        // `--full` keeps the complete output even for a heavy family. (A1)
        let full = s(&["wiring", "audit", "--full"]);
        assert!(!apply_heavy_brief_default("wiring", &full));
        BRIEF_OUTPUT.store(false, Ordering::Relaxed);
    }

    #[test]
    fn parse_global_flags_full_overrides_brief() {
        // `--full` wins over `--brief` and both are stripped from filtered args. (A1)
        let (flags, filtered) = parse_global_flags(&s(&["wiring", "audit", "--brief", "--full"]));
        assert!(!flags.brief, "--full must clear brief");
        assert_eq!(filtered, s(&["wiring", "audit"]));
        BRIEF_OUTPUT.store(false, Ordering::Relaxed);
    }
}
