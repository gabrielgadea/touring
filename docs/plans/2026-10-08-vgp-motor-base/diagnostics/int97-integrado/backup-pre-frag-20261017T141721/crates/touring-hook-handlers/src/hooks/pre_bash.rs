//! Pre-Bash Hook — CONTEXTUAL outcome recall.
//!
//! Philosophy: SILENCE IS THE DEFAULT.
//! Only inject when a RELEVANT past failure exists:
//!
//! Relevance tiers (highest first):
//! 1. Same command + same file → "ruff failed on THIS file: E501"
//! 2. Same command + same directory → "ruff failed in this dir"
//! 3. Everything else → SILENCE (generic failure stats are noise)
//!
//! Enhancements (E15, E19):
//! - Pensieve lookup (E15): cosine similarity to known failed command embeddings
//! - StableSessionContext (E19): cached CILA level avoids redundant DB queries
//!
//! Target latency: <10ms.

use touring_code::vgp::CheckInput;
use touring_code::vgp::sombra::{Mode as VgpMode, sombra_check, vgp_mode};

use super::knowledge::FileKnowledgeDB;
use super::runtime::{HookResponse, HookRuntime};
use super::shared::hook_helpers;
use super::shared::patterns::FILE_PATH_RE;
use super::shared::result_ext::ResultExt;
use crate::schemas::{validate_payload, validation_deny};
use touring_foundation::truncate_str;
use touring_intelligence::reasoning::BM25TfIdfVectorizer;

/// Returns the context budget (in characters) for a given CILA level.
///
/// Pre-bash context is typically short (command history), so we use the
/// read-tier budget which is smaller than the edit-tier.
/// Delegates to [`crate::shared::cila::cila_budget_read`].
fn cila_budget(cila_level: u8) -> usize {
    crate::shared::cila::cila_budget_read(cila_level)
}

/// Run the pre-bash hook (diverging version — for use by the CLI entry point).
#[tracing::instrument(skip(runtime, input), fields(hook = "pre_bash"))]
pub fn run(
    runtime: &HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    run_returning(runtime, input).emit()
}

/// Run the pre-bash hook, returning a `HookResponse` instead of diverging.
///
/// Used by the daemon to handle the hook without calling `process::exit`.
pub fn run_returning(runtime: &HookRuntime, input: &serde_json::Value) -> HookResponse {
    run_returning_with(runtime, input, &vgp_sombra_de_bash)
}

/// The VGP verdict enters through `vgp` (the production seam is [`vgp_sombra_de_bash`]), so the
/// precedence of the chain is testable without the motor. The chain deposits advisories in
/// `avisos` instead of returning them; the most severe verdict wins and the advisories ride
/// along ([`HookResponse::with_advisories`] — the one rule shared with `cli-suggest`).
fn run_returning_with(
    runtime: &HookRuntime,
    input: &serde_json::Value,
    vgp: &dyn Fn(&HookRuntime, &serde_json::Value, &str) -> Option<HookResponse>,
) -> HookResponse {
    let mut avisos: Vec<String> = Vec::new();
    let resposta = run_chain(runtime, input, vgp, &mut avisos);
    resposta.with_advisories(avisos)
}

fn run_chain(
    runtime: &HookRuntime,
    input: &serde_json::Value,
    vgp: &dyn Fn(&HookRuntime, &serde_json::Value, &str) -> Option<HookResponse>,
    avisos: &mut Vec<String>,
) -> HookResponse {
    // D9: Validate payload with typed schema — fail fast on malformed input.
    let tool_input = match input.get("tool_input") {
        Some(v) => v,
        None => return HookResponse::Allow,
    };
    let validated = match validate_payload::<crate::schemas::PreBashPayload>(tool_input) {
        Ok(v) => v,
        Err(errors) => return validation_deny(&errors, "pre_bash"),
    };
    let command = validated.command.as_str();

    if command.is_empty() {
        return HookResponse::Allow;
    }

    // ── VGP inline (V5, 09/10): `python3 - <<EOF` / `python3 -c` / `touring run --code|--file`
    // passam pelo MESMO motor com path <string> e a raiz da sessão — "sombra" (default) só
    // registra; "ativo" é só por [vgp] modo do projeto (V4). Fail-open de ponta a ponta.
    //
    // 30.4.97 (ordem dos gates): o `[VGP · aviso]` NÃO encerra a cadeia — antes ele retornava
    // aqui e escondia o Block do `bash_ast_validator`, o Deny do `PreToolValidator` e o do
    // W-540 de um comando perigoso na mesma linha. O aviso é depositado; um deny do VGP
    // (ou qualquer outra decisão) segue encerrando.
    if let Some(resposta_vgp) = vgp(runtime, input, command) {
        match resposta_vgp {
            HookResponse::Context { context, .. } => avisos.push(context),
            outra => return outra,
        }
    }

    // Wave v4.29.0 — S2: structural bash validation. Strips quoted strings
    // and # comments before rule evaluation, so `echo "rm -rf"` and
    // `# rm -rf` correctly do NOT block. Runs BEFORE PreToolValidator so
    // its precise reason text wins on overlap.
    match crate::shared::bash_ast_validator::validate_command(command) {
        crate::shared::bash_ast_validator::Verdict::Block { reason } => {
            tracing::warn!(
                "bash_ast_validator blocked: command={} reason={}",
                command,
                reason
            );
            return HookResponse::Deny {
                reason,
                context: None,
                event_name: Some("PreToolUse".to_string()),
            };
        }
        crate::shared::bash_ast_validator::Verdict::Warn { .. }
        | crate::shared::bash_ast_validator::Verdict::Allow => {
            // Warn is left to flow through — the existing PreToolValidator
            // path may produce a richer warning context, and we don't want
            // to surface two different warnings for the same command.
        }
    }

    // SECURITY: PreToolValidator gate — block dangerous commands before execution.
    // Validate the tool invocation against known dangerous patterns (rm -rf, git reset --hard, etc.).
    // Every simple command of the line, each by its own words (18/09/2026): the
    // old split validated ONE tool with the text after it at the start of the
    // line — empty for `cd x && git push -f`, raw text for everything else.
    let validation = runtime.ctx.pre_tool_validator.validate_command(command);
    if validation.is_blocked() {
        tracing::warn!(
            "PreToolValidator blocked: command={} reason={}",
            command,
            validation.reason.as_deref().unwrap_or("unknown")
        );
        return HookResponse::Deny {
            reason: validation
                .reason
                .unwrap_or_else(|| "Dangerous tool blocked".to_string()),
            context: None,
            event_name: Some("PreToolUse".to_string()),
        };
    }

    // S-01 (elite-harness 2026-05-29) — CEG observability is now re-homed to
    // the dedicated, matcher-wide `ceg-observe` hook
    // (`crate::ceg_adapter::run_observe_only`), wired in settings.json against
    // *every* Bash (not just the cargo/rust/touring subset this `pre-bash`
    // `if` matcher covers). That makes `touring gate-metrics -j` reflect the
    // whole action stream and avoids double-counting: `observe()` runs exactly
    // once per Bash, in the universal hook. (Wave 7.A originally called
    // `observe("Bash", command)` here.)

    // W-540 — Memory pressure gate (resource-monitor feature).
    //
    // Blocks heavy spawns (cargo test/build, npm install, pip install) when
    // MemoryGuard reports Pressure::Red. Linux-only at the platform layer;
    // on non-Linux targets the MemoryGuard::pressure() call returns Green so
    // this branch is never reached in production. Advisory only — exits 0 so
    // Claude Code's hook fail-open invariant is preserved; the Deny reason is
    // surfaced as a context string rather than a hard block.
    #[cfg(feature = "resource-monitor")]
    {
        use touring_resilience::sentinel::Pressure;
        let is_heavy_spawn = {
            let c = command.trim();
            c.contains("cargo test")
                || c.contains("cargo build")
                || c.contains("cargo check")
                || c.contains("npm install")
                || c.contains("pip install")
                || c.contains("pip3 install")
        };
        if is_heavy_spawn {
            let pressure = touring_resilience::sentinel::MemoryGuard::global().pressure();
            match pressure {
                Pressure::Red => {
                    crate::shared::gate_metrics::record_cargo_test_paused();
                    tracing::warn!(
                        command = command,
                        "[W-540] Memory pressure RED — blocking heavy spawn"
                    );
                    return HookResponse::Deny {
                        reason: "[W-540] Memory pressure RED — cargo command paused (heavy spawn would risk OOM)".to_string(),
                        context: None,
                        event_name: Some("PreToolUse".to_string()),
                    };
                }
                Pressure::Yellow => {
                    tracing::info!(
                        command = command,
                        "[W-541] Memory pressure YELLOW — heavy spawn allowed but monitor active"
                    );
                }
                Pressure::Green => {}
            }
        }
    }

    // Wave v4.29.0 — S3: command-shape clustering. Use the structural shape
    // (`"cargo test"` for any `cargo … test … flags`) as the Pensieve cluster
    // key when available; fall back to the legacy `extract_command_short`.
    let short_key = crate::shared::bash_ast_validator::command_shape(command)
        .unwrap_or_else(|| extract_command_short(command));
    if short_key.is_empty() {
        return HookResponse::Allow;
    }

    // S6/E19: CILA-aware budget — prefer stable session context,
    // fall back to result_cache for standalone/cold-start mode.
    let cila_level: u8 = hook_helpers::cila_level_from_runtime(runtime, 3);
    let max_chars = cila_budget(cila_level);

    let file_ctx = extract_file_context(command);

    // E15: Check Pensieve for similar past command failures (ANN penalty lookup).
    let pensieve_warning = {
        let states = crate::shared::command_hash::command_to_states(&short_key);
        if states.is_empty() {
            None
        } else {
            match runtime.learning.pensieve.try_borrow() {
                Ok(pensieve) => {
                    // Use check_known_failure_seq for multi-token commands, single-state for one token.
                    let penalty = match states.first() {
                        Some(&single) if states.len() == 1 => pensieve.check_known_failure(single),
                        _ => pensieve.check_known_failure_seq(&states),
                    };
                    penalty.map(|sim| {
                        format!(
                            "Similar command failed recently (similarity={:.0}%)",
                            sim * 100.0
                        )
                    })
                }
                _ => {
                    tracing::debug!("pensieve: skipped check (already borrowed)");
                    None
                }
            }
        }
    };

    let context = compose_relevant_context(&runtime.ctx.knowledge, &short_key, file_ctx.as_deref());

    // Merge Pensieve warning with DB-sourced context.
    let merged = match (pensieve_warning, context) {
        (Some(pw), Some(ctx)) => Some(format!("{pw}\n{ctx}")),
        (Some(pw), None) => Some(pw),
        (None, ctx) => ctx,
    };

    // EC36: Cognitive enrichment — inject file risk and bash failure signals into pre-bash context.
    // Mirrors pre_edit.rs:EC32. file_ctx provides file-specific risk when a path is detected;
    // "" fires recent_bash_outcomes for bare commands (cargo, pytest, etc.) even without a file target.
    let merged = if let Some(ref cognitive) = runtime.cognitive {
        let path_ref = file_ctx.as_deref().unwrap_or("");
        let enriched = crate::shared::signals::enrich_with_cognitive(
            cognitive,
            path_ref,
            false,
            crate::shared::signals::payload_session(input),
        );
        if enriched.is_empty() {
            merged
        } else {
            match merged {
                Some(m) => Some(format!("{m}\n{enriched}")),
                None => Some(enriched),
            }
        }
    } else {
        merged
    };

    // S-8 (plan r2): semantic bash gate — o DESCONHECIDO recebe um reversibility
    // Score do aluno local como advisory (enriquece, nunca nega por palpite).
    let merged = {
        let short = extract_command_short(command);
        if !short.is_empty() && !touring_hook_runtime::prompt_enhance::is_known_program(&short) {
            match touring_hook_runtime::prompt_enhance::semantic_reversibility(command) {
                Some((score, wire_ms)) => {
                    let note = format!(
                        "ceg_semantic_reversibility: {score:.2}/2 (aluno local, {wire_ms}ms) — advisory, não bloqueia"
                    );
                    Some(match merged {
                        Some(m) => format!("{m}\n{note}"),
                        None => note,
                    })
                }
                None => merged,
            }
        } else {
            merged
        }
    };

    match merged {
        Some(ctx) if !ctx.is_empty() => {
            // Truncate to CILA budget — prevents oversized context at low levels.
            let ctx = truncate_str(&ctx, max_chars).to_string();
            // S-9 (r3 shadow): observe the injection before it reaches the model.
            // Observe-only — never alters `ctx` or the returned response.
            {
                let sid = crate::shared::signals::payload_session(input).unwrap_or("");
                touring_hook_runtime::r3_shadow::maybe_emit_a1_block(
                    &runtime.project_root,
                    sid,
                    "pre-bash",
                    &ctx,
                );
            }
            HookResponse::Context {
                context: ctx,
                event_name: Some("PreToolUse".to_string()),
            }
        }
        _ => HookResponse::Allow,
    }
}

/// Extract the primary command name: the last `&&` step, its first `;` segment that runs
/// something, past env assignments (`FOO=1`) and wrappers (`sudo`, `env -u X`,
/// `timeout -s KILL 30`, `nice -n 5`, `nohup`, `time`, `command`, `exec`, `stdbuf`).
///
/// S21: it returned `TOURING_CODE_MODE=native` and `S=/tmp/…;` as the "command" of real
/// failures, so the lessons named assignments instead of programs.
pub fn extract_command_short(command: &str) -> String {
    let step = command.trim().rsplit("&&").next().unwrap_or("").trim();
    step.split(';').find_map(first_program).unwrap_or_default()
}

/// First word of `segment` that is a program, skipping assignments and wrapper prefixes.
fn first_program(segment: &str) -> Option<String> {
    let mut words = segment.split_whitespace().peekable();
    while let Some(w) = words.next() {
        if is_env_assignment(w) {
            continue;
        }
        let takes_value: &[&str] = match w {
            "sudo" => &["-u", "-g", "-C", "-D", "-h", "-p", "-r", "-t", "-U"],
            "env" => &["-u", "-C", "-S"],
            "timeout" => &["-s", "-k", "--signal", "--kill-after"],
            "nice" => &["-n", "--adjustment"],
            "stdbuf" => &["-i", "-o", "-e"],
            "nohup" | "time" | "command" | "exec" => &[],
            other => return Some(other.to_string()),
        };
        while let Some(next) = words.peek().copied() {
            if takes_value.contains(&next) {
                words.next();
                words.next();
            } else if next.starts_with('-')
                || is_env_assignment(next)
                || (w == "timeout" && next.parse::<f64>().is_ok())
            {
                words.next();
            } else {
                break;
            }
        }
    }
    None
}

fn is_env_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(k, _)| {
        !k.is_empty()
            && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !k.starts_with(|c: char| c.is_ascii_digit())
    })
}

/// Extract file paths mentioned in the command.
pub fn extract_file_context(command: &str) -> Option<String> {
    FILE_PATH_RE
        .captures(command)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

/// Extract directory from a file path.
fn extract_dir(path: &str) -> &str {
    std::path::Path::new(path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("")
}

/// Compose context ONLY when relevant past failures exist.
pub fn compose_relevant_context(
    db: &FileKnowledgeDB,
    short_key: &str,
    current_file: Option<&str>,
) -> Option<String> {
    // Get recent outcomes for this command type
    let outcomes = db.find_bash_outcomes(short_key, 10).ok()?;
    if outcomes.is_empty() {
        return None;
    }

    let failures: Vec<_> = outcomes.iter().filter(|o| !o.success).collect();
    if failures.is_empty() {
        return None; // All recent runs succeeded — silence
    }

    // ── Tier 1 + 2: file-scoped signals (highest signal) ──
    if let Some(msg) = collect_file_scoped_failure(short_key, current_file, &failures) {
        return Some(msg);
    }

    // ── Systemic failure rate: no file context, many failures ──
    collect_systemic_failure(short_key, current_file, &outcomes, &failures)
}

/// Rank error patterns by BM25 relevance to the current command text.
///
/// Uses a `BM25TfIdfVectorizer` to score each failure's `error_pattern` against
/// the command short key as the query. Falls back to the chronologically latest
/// failure if BM25 produces no results (e.g., very short tokens, empty patterns).
///
/// # Why BM25 here?
/// When the same command fails multiple times on the same file with different
/// error messages (e.g., "E501 line too long" vs "import not found"), the most
/// relevant error for the *current* invocation surfaces first instead of always
/// showing the most recent.
fn rank_errors_by_relevance(
    short_key: &str,
    failures: &[&super::knowledge::BashOutcome],
) -> String {
    // Build a small BM25 index over the error patterns.
    let mut vectorizer = BM25TfIdfVectorizer::new();
    for (i, outcome) in failures.iter().enumerate() {
        let text = outcome.error_pattern.as_deref().unwrap_or("unknown error");
        vectorizer.add_document(i.to_string(), text);
    }

    // Query by the command name — returns ranked results with bm25_score.
    let results = vectorizer.query(short_key, failures.len());

    // Pick the top-ranked result; fall back to failures[0] (most recent) on tie/empty.
    // EC57: ResultExt::unwrap_or_debug — logs if BM25 returns a non-numeric rank id.
    let best_idx = results
        .first()
        .map(|r| {
            r.id.parse::<usize>()
                .unwrap_or_debug(0, "pre_bash: BM25 rank id is non-numeric")
        })
        .unwrap_or(0);

    failures
        .get(best_idx)
        .and_then(|o| o.error_pattern.as_deref())
        .unwrap_or("unknown error")
        .to_string()
}

/// Check Tier-1 (same file) and Tier-2 (same directory) failure signals.
///
/// Returns `Some(message)` if a file- or directory-scoped failure was found,
/// `None` otherwise (caller continues to systemic check).
fn collect_file_scoped_failure(
    short_key: &str,
    current_file: Option<&str>,
    failures: &[&super::knowledge::BashOutcome],
) -> Option<String> {
    let target_file = current_file?;
    collect_tier1_same_file(short_key, target_file, failures)
        .or_else(|| collect_tier2_same_dir(short_key, target_file, failures))
}

/// Tier-1 signal: same command failed on the exact same file.
///
/// BM25-ranks error patterns so the most relevant past error surfaces first
/// when multiple failures exist for the same file.
fn collect_tier1_same_file(
    short_key: &str,
    target_file: &str,
    failures: &[&super::knowledge::BashOutcome],
) -> Option<String> {
    let same_file_failures: Vec<&super::knowledge::BashOutcome> = failures
        .iter()
        .filter(|o| {
            o.file_context
                .as_deref()
                .map(|fc| fc.contains(target_file) || target_file.contains(fc))
                .unwrap_or(false)
        })
        .copied()
        .collect();

    if same_file_failures.is_empty() {
        return None;
    }

    // BM25-rank error patterns by relevance to the current command.
    // When multiple failures exist, the most relevant error surfaces first.
    let best_err = rank_errors_by_relevance(short_key, &same_file_failures);
    let short_err = truncate_str(&best_err, 100);
    Some(format!(
        "`{short_key}` failed on this file previously: {short_err}"
    ))
}

/// Tier-2 signal: same command failed ≥2 times in the same directory.
///
/// Only fires when Tier-1 produces no match, providing a weaker but still
/// useful directory-level signal.
fn collect_tier2_same_dir(
    short_key: &str,
    target_file: &str,
    failures: &[&super::knowledge::BashOutcome],
) -> Option<String> {
    let target_dir = extract_dir(target_file);
    if target_dir.is_empty() {
        return None;
    }

    let same_dir_count = failures
        .iter()
        .filter(|o| {
            o.file_context
                .as_deref()
                .map(|fc| extract_dir(fc) == target_dir)
                .unwrap_or(false)
        })
        .count();

    if same_dir_count >= 2 {
        Some(format!(
            "`{short_key}` has {same_dir_count} recent failures in this directory"
        ))
    } else {
        None
    }
}

/// Check systemic failure rate when no file context is present.
///
/// Only fires when failure rate ≥ 80% and at least 3 failures — indicates a
/// command-level issue not tied to any specific file.
fn collect_systemic_failure(
    short_key: &str,
    current_file: Option<&str>,
    outcomes: &[super::knowledge::BashOutcome],
    failures: &[&super::knowledge::BashOutcome],
) -> Option<String> {
    // Only alert if no file context (file-scoped already handled above)
    if current_file.is_some() {
        return None;
    }

    let total = outcomes.len();
    let fail_count = failures.len();
    let fail_rate = fail_count as f64 / total as f64;

    if fail_rate >= 0.8 && fail_count >= 3 {
        // SAFETY: fail_count >= 3 guarantees failures is non-empty
        #[allow(clippy::indexing_slicing)]
        let latest = failures[0];
        let err = latest.error_pattern.as_deref().unwrap_or("recurring error");
        let short_err = truncate_str(err, 80);
        return Some(format!(
            "`{short_key}` has {fail_count}/{total} recent failures: {short_err}"
        ));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::{BashOutcome, FileKnowledgeDB};
    use tempfile::TempDir;

    fn setup() -> (TempDir, FileKnowledgeDB) {
        let tmp = TempDir::new().unwrap();
        let db = FileKnowledgeDB::new(&tmp.path().join("test.db")).unwrap();
        (tmp, db)
    }

    /// Guard: the call site in `run_returning` exists and fires.
    ///
    /// Mutant killed: `call_site_removed_from_pre_bash` — removing the
    /// `maybe_emit_a1_block(…)` call from `run_returning` causes 0 lines to be
    /// emitted even when `HookResponse::Context` is returned; this assertion fails.
    ///
    /// The thread-local [`touring_hook_runtime::r3_shadow::with_events_path`] is used
    /// so parallel tests never touch the production events file and never race on
    /// `TOURING_R3_EVENTS_FILE`.
    #[test]
    fn a1_block_emitted_at_context_injection() {
        use sha2::{Digest as _, Sha256};
        use touring_hook_runtime::r3_shadow;

        assert!(
            std::env::var_os(r3_shadow::ENV_FLAG).is_none(),
            "run without TOURING_R3_SHADOW"
        );

        let tmp = TempDir::new().expect("tempdir");

        // Enable shadow via toml for this project_root.
        let toml_dir = tmp.path().join(".touring");
        std::fs::create_dir_all(&toml_dir).expect("mkdir .touring");
        std::fs::write(toml_dir.join("touring.toml"), "[shadow]\nr3 = true\n")
            .expect("write touring.toml");

        // Build runtime first, then insert a failed outcome directly into
        // runtime.knowledge — avoids any DB path or schema-version mismatch.
        let runtime = HookRuntime::new(tmp.path()).expect("HookRuntime::new");
        // Compute short_key the SAME way run_returning does so the DB lookup
        // matches the stored record (command_shape may return "ruff check", not "ruff").
        let cmd = "ruff check src/target_file.py";
        let short_key = crate::shared::bash_ast_validator::command_shape(cmd)
            .unwrap_or_else(|| extract_command_short(cmd));
        runtime
            .ctx
            .knowledge
            .record_bash_outcome(&BashOutcome {
                command: cmd.to_string(),
                command_short: short_key,
                exit_code: 1,
                success: false,
                error_pattern: Some("E501: line too long".to_string()),
                file_context: Some("src/target_file.py".to_string()),
                ..Default::default()
            })
            .ok();

        // ── Case 1: input that produces HookResponse::Context ────────────────
        let input_ctx = serde_json::json!({
            "session_id": "test-session-callsite",
            "tool_input": { "command": "ruff check src/target_file.py" }
        });
        let events = tmp.path().join("callsite_events.jsonl");
        let response = r3_shadow::with_events_path(&events, || run_returning(&runtime, &input_ctx));

        match response {
            HookResponse::Context { ref context, .. } => {
                // Call site present → exactly 1 a1_block line.
                let text = std::fs::read_to_string(&events).expect("read events");
                let lines: Vec<_> = text.lines().collect();
                assert_eq!(lines.len(), 1, "exactly 1 a1_block per Context injection");

                let ev: serde_json::Value = serde_json::from_str(lines[0]).expect("valid JSON");
                assert_eq!(ev["kind"], "a1_block");
                assert_eq!(
                    ev["bytes"].as_u64().unwrap(),
                    context.len() as u64,
                    "bytes must equal the RETURNED context length"
                );
                // first_line_sha must be sha256 of first line — computed independently.
                let first_line = context.lines().next().unwrap_or("");
                let expected_sha: String = {
                    let d = Sha256::digest(first_line.as_bytes());
                    d.iter().take(8).map(|b| format!("{b:02x}")).collect()
                };
                assert_eq!(
                    ev["first_line_sha"], expected_sha,
                    "first_line_sha must be sha256 of first line of returned context"
                );
            }
            _ => {
                // Non-Context response → 0 lines expected (Allow/Deny don't inject).
                let has_lines = events
                    .exists()
                    .then(|| std::fs::read_to_string(&events).unwrap_or_default())
                    .unwrap_or_default();
                assert!(
                    has_lines.is_empty(),
                    "non-Context response must not emit a1_block; got: {has_lines}"
                );
                panic!("expected HookResponse::Context — check knowledge DB pre-population");
            }
        }

        // ── Case 2: no tool_input → Allow, 0 lines ───────────────────────────
        let events2 = tmp.path().join("callsite_events2.jsonl");
        let resp2 = r3_shadow::with_events_path(&events2, || {
            run_returning(&runtime, &serde_json::json!({}))
        });
        assert!(
            matches!(resp2, HookResponse::Allow),
            "no tool_input must return Allow"
        );
        assert!(!events2.exists(), "Allow response must not emit a1_block");
    }

    #[test]
    fn test_extract_command_short() {
        assert_eq!(extract_command_short("ruff check src/"), "ruff");
        assert_eq!(extract_command_short("pytest -v tests/"), "pytest");
        assert_eq!(extract_command_short("cargo test"), "cargo");
        assert_eq!(
            extract_command_short("cd /tmp && python script.py"),
            "python"
        );
        assert_eq!(extract_command_short(""), "");
    }

    #[test]
    fn test_silence_no_history() {
        let (_tmp, db) = setup();
        let ctx = compose_relevant_context(&db, "ruff", Some("src/main.py"));
        assert!(ctx.is_none(), "No history should produce silence");
    }

    #[test]
    fn test_silence_all_success() {
        let (_tmp, db) = setup();
        db.record_bash_outcome(&BashOutcome {
            command: "ruff check src/main.py".to_string(),
            command_short: "ruff".to_string(),
            exit_code: 0,
            success: true,
            error_pattern: None,
            file_context: Some("src/main.py".to_string()),
            command_hash: String::new(),
            executed_at: String::new(),
        })
        .unwrap();

        let ctx = compose_relevant_context(&db, "ruff", Some("src/main.py"));
        assert!(ctx.is_none(), "All success should produce silence");
    }

    #[test]
    fn test_silence_failure_on_different_file() {
        let (_tmp, db) = setup();
        // ruff failed on main.py, but we're about to run on utils.py
        db.record_bash_outcome(&BashOutcome {
            command: "ruff check src/main.py".to_string(),
            command_short: "ruff".to_string(),
            exit_code: 1,
            success: false,
            error_pattern: Some("E501 line too long".to_string()),
            file_context: Some("src/main.py".to_string()),
            command_hash: String::new(),
            executed_at: String::new(),
        })
        .unwrap();

        let ctx = compose_relevant_context(&db, "ruff", Some("src/utils.py"));
        assert!(
            ctx.is_none(),
            "Failure on DIFFERENT file should produce SILENCE"
        );
    }

    #[test]
    fn test_alert_failure_on_same_file() {
        let (_tmp, db) = setup();
        db.record_bash_outcome(&BashOutcome {
            command: "ruff check src/main.py".to_string(),
            command_short: "ruff".to_string(),
            exit_code: 1,
            success: false,
            error_pattern: Some("E501 line too long at line 42".to_string()),
            file_context: Some("src/main.py".to_string()),
            command_hash: String::new(),
            executed_at: String::new(),
        })
        .unwrap();

        let ctx = compose_relevant_context(&db, "ruff", Some("src/main.py")).unwrap();
        assert!(ctx.contains("failed on this file"));
        assert!(ctx.contains("E501"));
    }

    #[test]
    fn test_alert_multiple_failures_same_dir() {
        let (_tmp, db) = setup();
        for file in &["src/a.py", "src/b.py", "src/c.py"] {
            db.record_bash_outcome(&BashOutcome {
                command: format!("ruff check {file}"),
                command_short: "ruff".to_string(),
                exit_code: 1,
                success: false,
                error_pattern: Some("E501".to_string()),
                file_context: Some(file.to_string()),
                command_hash: String::new(),
                executed_at: String::new(),
            })
            .unwrap();
        }

        let ctx = compose_relevant_context(&db, "ruff", Some("src/new.py")).unwrap();
        assert!(ctx.contains("failures in this directory"));
    }

    #[test]
    fn test_alert_systemic_failure_no_file_context() {
        let (_tmp, db) = setup();
        // 4 out of 5 cargo tests failed (80%) — systemic
        for i in 0..5 {
            db.record_bash_outcome(&BashOutcome {
                command: "cargo test".to_string(),
                command_short: "cargo".to_string(),
                exit_code: if i < 4 { 1 } else { 0 },
                success: i >= 4,
                error_pattern: if i < 4 {
                    Some("test failed".to_string())
                } else {
                    None
                },
                file_context: None,
                command_hash: String::new(),
                executed_at: String::new(),
            })
            .unwrap();
        }

        // No file context in current command — but 80% failure rate
        let ctx = compose_relevant_context(&db, "cargo", None).unwrap();
        assert!(ctx.contains("4/5 recent failures"));
    }

    #[test]
    fn test_silence_low_failure_rate_no_file_context() {
        let (_tmp, db) = setup();
        // 1 out of 5 failed (20%) — not systemic, silence
        for i in 0..5 {
            db.record_bash_outcome(&BashOutcome {
                command: "cargo test".to_string(),
                command_short: "cargo".to_string(),
                exit_code: if i == 0 { 1 } else { 0 },
                success: i != 0,
                error_pattern: if i == 0 {
                    Some("test failed".to_string())
                } else {
                    None
                },
                file_context: None,
                command_hash: String::new(),
                executed_at: String::new(),
            })
            .unwrap();
        }

        let ctx = compose_relevant_context(&db, "cargo", None);
        assert!(
            ctx.is_none(),
            "20% failure rate without file context should be silence"
        );
    }

    #[test]
    fn test_extract_dir_standard_path() {
        assert_eq!(extract_dir("src/main.py"), "src");
        assert_eq!(extract_dir("crates/hooks/src/lib.rs"), "crates/hooks/src");
        assert_eq!(extract_dir("/abs/path/file.rs"), "/abs/path");
    }

    #[test]
    fn test_extract_dir_no_parent() {
        // A bare filename with no directory component returns "".
        assert_eq!(extract_dir("file.rs"), "");
    }

    #[test]
    fn test_cila_budget_scales_with_level() {
        // Higher CILA levels should give more context budget.
        let l0 = cila_budget(0);
        let l3 = cila_budget(3);
        let l5 = cila_budget(5);
        assert!(l3 >= l0, "L3 budget should be >= L0 budget");
        assert!(l5 >= l3, "L5 budget should be >= L3 budget");
        // Budget should always be positive.
        assert!(l0 > 0, "L0 budget must be positive");
    }

    #[test]
    fn test_bm25_ranking_selects_relevant_error() {
        let (_tmp, db) = setup();
        // Two failures on the same file: one about "import" (less relevant to
        // "cargo") and one about "test failed" (more relevant to "cargo test").
        for (err, file) in &[
            ("import not found in module", "src/lib.rs"),
            ("test failed: assertion mismatch", "src/lib.rs"),
        ] {
            db.record_bash_outcome(&BashOutcome {
                command: format!("cargo test {file}"),
                command_short: "cargo".to_string(),
                exit_code: 1,
                success: false,
                error_pattern: Some(err.to_string()),
                file_context: Some(file.to_string()),
                command_hash: String::new(),
                executed_at: String::new(),
            })
            .unwrap();
        }

        let ctx = compose_relevant_context(&db, "cargo", Some("src/lib.rs")).unwrap();
        // The context should mention one of the known error patterns.
        assert!(
            ctx.contains("import") || ctx.contains("test failed"),
            "expected one of the known error patterns, got: {ctx}"
        );
        // The BM25 ranking should prefer the "test" keyword match for "cargo".
        assert!(
            ctx.contains("test"),
            "BM25 should rank 'test failed' higher for 'cargo' query, got: {ctx}"
        );
    }
}

// ─────────────────────────── V5 — inline Python no pre-bash ───────────────────────────

/// Extrai o programa Python de um comando, quando há um: `touring run --code|--file` (hook
/// "touring-run"), `python3 - <<'TAG' … TAG` ou `python3 -c '…'` (hook "pre-bash", path <string>).
fn extrai_inline_python(command: &str) -> Option<(&'static str, String, String)> {
    if command.contains("touring run") {
        if let Some(code) = valor_de_flag_shell(command, "--code") {
            return Some(("touring-run", code, "<string>".into()));
        }
        if let Some(f) = valor_de_flag_shell(command, "--file") {
            let prog = std::fs::read_to_string(&f).unwrap_or_default();
            return Some(("touring-run", prog, "<string>".into()));
        }
        return None;
    }
    if !command.contains("python") {
        return None;
    }
    if let Some(code) = valor_de_flag_shell(command, "-c") {
        return Some(("pre-bash", code, "<string>".into()));
    }
    heredoc_python(command).map(|prog| ("pre-bash", prog, "<stdin>".into()))
}

/// `python3 - <<'PY' … PY` / `python3 - <<PY … PY`: o corpo do heredoc, como o traceback o nomeia.
fn heredoc_python(command: &str) -> Option<String> {
    let pos = command.find("<<")?;
    let rest = &command[pos + 2..];
    let rest = rest.trim_start_matches(['-', ' ']);
    let (tag, quoted) = match rest.chars().next() {
        Some(q @ ('\'' | '"')) => (&rest[1..rest[1..].find(q).map(|i| i + 1)?], true),
        _ => {
            let end = rest.find([' ', '\n'])?;
            (&rest[..end], false)
        }
    };
    let _ = quoted;
    let corpo = &rest[rest.find('\n')? + 1..];
    let fim = corpo.find(&format!("\n{tag}"))?;
    Some(format!("{}\n", &corpo[..fim]))
}

/// O valor de uma flag num comando shell, com aspas simples/duplas — mínimo e fail-open (a V5 é
/// sombra: uma aspa mal balanceada vira None, nunca um erro).
fn valor_de_flag_shell(command: &str, flag: &str) -> Option<String> {
    let mut rest = command;
    while let Some(pos) = rest.find(flag) {
        rest = &rest[pos + flag.len()..];
        let rest = rest.trim_start();
        if flag == "-c" && !rest.starts_with(['\'', '"']) {
            // -c exige o programa logo após — senão era outra flag (-cp, -count…)
            continue;
        }
        return match rest.chars().next() {
            Some('\'') => rest[1..].find('\'').map(|i| rest[1..i + 1].to_string()),
            Some('"') => rest[1..].find('"').map(|i| rest[1..i + 1].to_string()),
            _ => rest.split([' ', '\t', '\n']).next().map(str::to_string),
        };
    }
    None
}

/// A raiz dos nomes do inline: o segmento `-projects-<nome>` do path (como vivos.project_root),
/// senão CLAUDE_PROJECT_DIR, senão a raiz do hook.
fn raiz_inline(path: &str, runtime: &HookRuntime) -> std::path::PathBuf {
    for (nome, raiz) in [("analise", "analise"), ("touring", "touring")] {
        if path.contains(&format!("-projects-{nome}/"))
            || path.ends_with(&format!("-projects-{nome}"))
        {
            let home = std::env::var("HOME").unwrap_or_default();
            return std::path::PathBuf::from(format!("{home}/projects/{raiz}"));
        }
    }
    if let Ok(dir) = std::env::var("CLAUDE_PROJECT_DIR")
        && !dir.is_empty()
    {
        return std::path::PathBuf::from(dir);
    }
    runtime.project_root.clone()
}

/// VGP sobre o programa inline (V5): `Some(resposta)` só em "ativo"; "sombra"/"off" → sempre None.
fn vgp_sombra_de_bash(
    runtime: &HookRuntime,
    input: &serde_json::Value,
    command: &str,
) -> Option<HookResponse> {
    let mode = vgp_mode(&runtime.project_root);
    if mode == VgpMode::Off {
        return None;
    }
    let (hook, programa, path) = extrai_inline_python(command)?;
    if programa.trim().is_empty() {
        return None;
    }
    let vgp_in = CheckInput {
        path: path.clone(),
        content: programa,
        project_root: raiz_inline(&path, runtime),
        disk: None,
        overlays: std::collections::HashMap::new(),
        lang: "python".into(),
    };
    let out = sombra_check(
        hook,
        &vgp_in,
        input
            .pointer("/session_id")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        input
            .pointer("/tool_use_id")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        mode,
        crate::vgp_l2::extra(&vgp_in),
    );
    if mode != VgpMode::Ativo {
        return None;
    }
    let resumo = out
        .findings
        .iter()
        .take(3)
        .map(|f| format!("{} `{}` ({}:{})", f.classe, f.nome, path, f.linha))
        .collect::<Vec<_>>()
        .join(" · ");
    match out.verdict.as_str() {
        "deny" => Some(HookResponse::Deny {
            reason: format!(
                "[VGP · deny] {resumo}{}",
                out.findings
                    .first()
                    .map(|f| format!(" — {}", f.sugestao))
                    .unwrap_or_default()
            ),
            context: None,
            event_name: Some("PreToolUse".to_string()),
        }),
        "warn" => Some(HookResponse::Context {
            context: format!("[VGP · aviso] {resumo}"),
            event_name: Some("PreToolUse".to_string()),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod vgp_inline_tests {
    use super::*;

    #[test]
    fn extrai_python_c() {
        let (hook, prog, path) = extrai_inline_python("python3 -c 'print(1)' 2>&1 | head").unwrap();
        assert_eq!(
            (hook, prog.as_str(), path.as_str()),
            ("pre-bash", "print(1)", "<string>")
        );
    }

    #[test]
    fn extrai_touring_run_code() {
        let (_, prog, _) = extrai_inline_python(
            "touring run --lang python --timeout-ms 30000 --code '\nimport os\nprint(os)'",
        )
        .unwrap();
        assert!(prog.contains("import os"));
    }

    #[test]
    fn extrai_heredoc() {
        let (_, prog, path) = extrai_inline_python("python3 - <<'PY'\nprint(2)\nPY\n").unwrap();
        assert_eq!((prog.as_str(), path.as_str()), ("print(2)\n", "<stdin>"));
    }

    #[test]
    fn comando_sem_python_e_none() {
        assert!(extrai_inline_python("ls -la /tmp").is_none());
    }
}

/// Ordem dos gates (30.4.97): o deny aplicável vence o `[VGP · aviso]`, qualquer que seja a
/// ordem de registro. O veredito do VGP entra pela costura `run_returning_with`.
#[cfg(test)]
mod vgp_ordem_tests {
    use super::*;
    use tempfile::TempDir;
    use touring_hook_runtime::verdict::ALSO_NON_BLOCKING;

    const AVISO: &str = "[VGP · aviso] atributo `inexistente` (<string>:2)";
    const PERIGOSO: &str = "rm -rf /tmp/work";

    fn avisa(_: &HookRuntime, _: &serde_json::Value, _: &str) -> Option<HookResponse> {
        Some(HookResponse::Context {
            context: AVISO.to_string(),
            event_name: Some("PreToolUse".to_string()),
        })
    }

    fn nada(_: &HookRuntime, _: &serde_json::Value, _: &str) -> Option<HookResponse> {
        None
    }

    fn nega(_: &HookRuntime, _: &serde_json::Value, _: &str) -> Option<HookResponse> {
        Some(HookResponse::Deny {
            reason: "[VGP · deny] import `x` inexistente".to_string(),
            context: None,
            event_name: Some("PreToolUse".to_string()),
        })
    }

    fn roda(
        cmd: &str,
        vgp: &dyn Fn(&HookRuntime, &serde_json::Value, &str) -> Option<HookResponse>,
    ) -> HookResponse {
        let tmp = TempDir::new().expect("tempdir");
        let runtime = HookRuntime::new(tmp.path()).expect("HookRuntime::new");
        let input = serde_json::json!({
            "session_id": "g97-vgp-ordem-bash",
            "tool_input": { "command": cmd }
        });
        run_returning_with(&runtime, &input, vgp)
    }

    /// O caso: python inline com aviso do VGP E um comando perigoso na mesma linha. Antes, o
    /// aviso retornava na hora e o `rm -rf` nunca passava pelo `bash_ast_validator`.
    #[test]
    fn aviso_do_vgp_nao_esconde_o_bloqueio_de_comando_perigoso() {
        match roda(PERIGOSO, &avisa) {
            HookResponse::Deny { reason, .. } => {
                assert!(
                    reason.contains("rm -rf"),
                    "o bloqueio é o do validador: {reason}"
                );
                assert!(
                    reason.contains(AVISO),
                    "o aviso vai junto no texto: {reason}"
                );
                assert!(reason.contains(ALSO_NON_BLOCKING), "{reason}");
            }
            outra => panic!("o deny aplicável vence o aviso do VGP: {outra:?}"),
        }
    }

    /// Controle positivo do caso acima: SEM aviso do VGP o mesmo comando já é negado — o gate
    /// existe e a diferença é só a ordem.
    #[test]
    fn sem_aviso_o_bloqueio_segue_sem_texto_do_vgp() {
        match roda(PERIGOSO, &nada) {
            HookResponse::Deny { reason, .. } => {
                assert!(reason.contains("rm -rf"), "{reason}");
                assert!(!reason.contains("VGP"), "{reason}");
            }
            outra => panic!("o validador nega o comando perigoso: {outra:?}"),
        }
    }

    /// Controle: comando inofensivo com aviso do VGP segue com o aviso (Context, nunca deny).
    #[test]
    fn comando_inofensivo_com_aviso_do_vgp_segue_com_o_aviso() {
        match roda("ls docs", &avisa) {
            HookResponse::Context { context, .. } => {
                assert!(context.contains(AVISO), "{context}");
            }
            outra => panic!("só o aviso: {outra:?}"),
        }
    }

    /// O deny do VGP continua encerrando a cadeia (é decisão, não aviso).
    #[test]
    fn deny_do_vgp_continua_encerrando_a_cadeia() {
        match roda("ls docs", &nega) {
            HookResponse::Deny { reason, .. } => {
                assert!(reason.contains("[VGP · deny]"), "{reason}");
            }
            outra => panic!("o deny do VGP segue deny: {outra:?}"),
        }
    }
}
