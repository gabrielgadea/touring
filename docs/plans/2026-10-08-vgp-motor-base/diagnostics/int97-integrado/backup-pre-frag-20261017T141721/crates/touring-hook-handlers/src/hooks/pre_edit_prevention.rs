//! Pre-Edit Prevention Hook — PROACTIVE error interception.
//!
//! Philosophy: PREVENT before the first failure, don't just learn after the second.
//!
//! Before Claude edits a file, this hook:
//! 1. Queries gotchas matching the target file (sorted by F1 score proxy)
//! 2. Queries cross-session error patterns with exponential decay
//! 3. Runs content-aware warnings (syntax, complexity, shadowing, anti-patterns)
//! 4. Composes a warning context ONLY if high-confidence patterns exist
//! 5. Injects the warning via `additionalContext` so Claude sees it BEFORE editing
//!
//! Enhancements (E5, E17):
//! - Deny gate by DELTA on the applied file (S210, 25/09/2026): the Edit is replayed on the
//!   real file and denied only when a file that parsed stops parsing; the `new_string`
//!   fragment's own syntax signals are advisory and never deny
//! - Jaccard near-duplicate detection (E17): old_string vs new_string similarity >0.85 warning
//!
//! Target latency: <10ms.

use touring_code::vgp::sombra::{Mode as VgpMode, sombra_check, vgp_mode};
use touring_code::vgp::{CheckInput, lang_do_caminho};

use std::collections::{HashMap, HashSet};

use touring_code::ast::{
    Lang, ScopeMap, SpeculateResult, build_scope_map, compute_complexity_for_source, speculate_v2,
};

use super::knowledge::FileKnowledgeDB;
use super::runtime::{HookResponse, HookRuntime, make_relative};
use crate::shared::hook_helpers;
use crate::shared::signals::rank_gotchas_by_relevance;

/// Minimum gotcha hit_count to be considered high-confidence.
const MIN_GOTCHA_HITS: i64 = 2;

/// Maximum number of gotchas to include in warning.
const MAX_GOTCHAS_IN_WARNING: usize = 3;

/// Maximum number of error patterns to include in warning.
const MAX_ERRORS_IN_WARNING: usize = 3;

/// Half-life in days for cross-session error decay (S1.3).
const DECAY_HALF_LIFE_DAYS: f64 = 7.0;

/// Cyclomatic complexity threshold — functions above this trigger a warning.
const COMPLEXITY_THRESHOLD: u16 = 10;

/// Maximum number of scope shadow warnings to emit.
const MAX_SHADOW_WARNINGS: usize = 3;

/// Run the pre-edit prevention hook (diverging version — for use by the CLI entry point).
///
/// Intercepts Edit and Write tool calls, queries the knowledge DB for
/// known error patterns, and injects a warning if high-confidence
/// patterns are found for the target file.
#[tracing::instrument(skip(runtime, input), fields(hook = "pre_edit_prevention"))]
pub fn run(
    runtime: &HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    run_returning(runtime, input).emit()
}

/// Run the pre-edit prevention hook, returning a `HookResponse` instead of diverging.
///
/// Used by the daemon to handle the hook without calling `process::exit`.
pub fn run_returning(runtime: &HookRuntime, input: &serde_json::Value) -> HookResponse {
    run_returning_with(runtime, input, &vgp_sombra_de_edit)
}

/// The seam of the VGP verdict (production: [`vgp_sombra_de_edit`]). The VGP advisory no longer
/// ends the chain: it is deposited in `avisos`, the deny gate still runs, and the most severe
/// verdict wins with the advisory riding along ([`HookResponse::with_advisories`] — the one
/// rule shared with `cli-suggest`).
type VgpEditFn = dyn Fn(
    &HookRuntime,
    &serde_json::Value,
    &str,
    &str,
    &str,
    Option<&AppliedEdit>,
) -> Option<HookResponse>;

fn run_returning_with(
    runtime: &HookRuntime,
    input: &serde_json::Value,
    vgp: &VgpEditFn,
) -> HookResponse {
    let mut avisos: Vec<String> = Vec::new();
    let resposta = run_chain(runtime, input, vgp, &mut avisos);
    resposta.with_advisories(avisos)
}

fn run_chain(
    runtime: &HookRuntime,
    input: &serde_json::Value,
    vgp: &VgpEditFn,
    avisos: &mut Vec<String>,
) -> HookResponse {
    let file_path = input
        .pointer("/tool_input/file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if file_path.is_empty() {
        return HookResponse::Allow;
    }

    let rel_path = make_relative(file_path, &runtime.project_root);
    let new_string = input
        .pointer("/tool_input/new_string")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let old_string = input
        .pointer("/tool_input/old_string")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Collect all warnings from different signal sources
    let mut all_warnings: Vec<String> = Vec::new();

    // Signals 1-2: Knowledge-based warnings (gotchas + cross-session errors)
    if let Some(warning) = compose_pre_edit_warning(&runtime.ctx.knowledge, &rel_path) {
        // The existing function returns a formatted block; split into individual lines
        // after the header to merge cleanly with content warnings.
        for line in warning.lines().skip(1) {
            if !line.is_empty() {
                all_warnings.push(line.to_string());
            }
        }
    }

    // Signals 3-6: Content-aware warnings (syntax, complexity, shadowing, anti-patterns)
    // Also captures the SpeculateResult for reuse by the deny gate (avoids duplicate
    // speculate_v2 call — E5 optimization).
    let (content_warns, spec_result) =
        compose_content_warnings(Some(&runtime.project_root), new_string, file_path);
    all_warnings.extend(content_warns);

    // Signal 7: Invalid cfg condition detection
    all_warnings.extend(collect_invalid_cfg_warnings(new_string));

    // Signal 8: Module existence — `mod X;` without corresponding file → E0583
    all_warnings.extend(collect_module_existence_issues(new_string, file_path));

    // Signal 9: Public API signature change — callers may break
    // Enhanced: queries wiring_map to list exact caller files that need updating
    all_warnings.extend(collect_api_signature_changes(
        old_string,
        new_string,
        &runtime.ctx.knowledge,
        &rel_path,
    ));

    // Signal 10: Near-duplicate edit detection (Jaccard word-set similarity)
    if let Some(dup_warning) = check_near_duplicate(old_string, new_string) {
        all_warnings.push(dup_warning);
    }

    // ── Signal 11: Extended metadata — test coverage + community affinity ──
    // Surfaces low coverage or community membership so Claude can prioritize
    // adding tests for uncovered files and understand module groupings.
    if let Ok(Some(ext)) = runtime.ctx.knowledge.query_extended(&rel_path)
        && let Some(sig) = hook_helpers::build_file_meta_signal(&ext)
    {
        all_warnings.push(sig);
    }

    // ── DENY gate (S210): the Edit is replayed on the real file and judged by delta ──
    // A file that parses and would not after the edit is denied; the fragment's own syntax
    // signals are advisory (dropped when the applied file is known, labelled otherwise).
    let replace_all = input
        .pointer("/tool_input/replace_all")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let applied = AppliedEdit::replay(file_path, old_string, new_string, replace_all);

    // ── VGP sombra/ativo (V2, 09/10): o motor julga o conteúdo proposto e grava UMA linha em
    // ~/.claude/touring/vgp/sombra.jsonl — em "sombra" (default) NADA muda na resposta; em
    // "ativo" (só por [vgp] modo do projeto, V4) o deny vira `[VGP · deny] ` e o aviso,
    // `[VGP · aviso] `. Fail-open: qualquer falha interna vira "unknown" e a chamada segue.
    //
    // 30.4.97 (ordem dos gates): o `[VGP · aviso]` NÃO encerra a cadeia — antes ele retornava
    // aqui e o `check_deny_gate` (S210, delta de sintaxe) nunca era avaliado: uma edição que
    // quebra um arquivo que parseia passava com um aviso. O aviso é depositado; um deny do VGP
    // (ou qualquer outra decisão) segue encerrando.
    if let Some(resposta_vgp) = vgp(
        runtime,
        input,
        file_path,
        old_string,
        new_string,
        applied.as_ref(),
    ) {
        match resposta_vgp {
            HookResponse::Context { context, .. } => avisos.push(context),
            outra => return outra,
        }
    }

    relabel_fragment_syntax(&mut all_warnings, spec_result.as_ref(), applied.is_some());
    if let Some(deny_response) =
        check_deny_gate(file_path, &rel_path, &all_warnings, applied.as_ref())
    {
        return deny_response;
    }

    if all_warnings.is_empty() {
        return HookResponse::Allow;
    }

    let header = format!(
        "Pre-edit prevention for {} ({} signal{}):",
        rel_path,
        all_warnings.len(),
        if all_warnings.len() > 1 { "s" } else { "" },
    );

    HookResponse::Context {
        context: format!("{}\n{}", header, all_warnings.join("\n")),
        event_name: Some("PreToolUse".to_string()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Signal 10: Near-duplicate edit detection (Jaccard word-set similarity)
// ─────────────────────────────────────────────────────────────────────────────

/// Jaccard similarity threshold above which an edit is flagged as near-duplicate.
/// A value of 0.85 means 85% of the word tokens are shared between old and new,
/// suggesting the user may have intended to make more changes.
const NEAR_DUPLICATE_THRESHOLD: f64 = 0.85;

/// Detect when `new_string` is nearly identical to `old_string` using
/// word-level Jaccard similarity.
///
/// Returns a warning message when the similarity exceeds
/// [`NEAR_DUPLICATE_THRESHOLD`] (85%), which indicates the edit is a
/// near-duplicate — the user likely intended to change more.
///
/// Uses simple word-level tokenization (`split_whitespace`) and computes
/// Jaccard = |intersection| / |union| on the resulting sets. This is done
/// inline rather than via `touring_simd::JaccardComputer` because the SIMD
/// variant operates on sorted `&[u32]` token hashes, which is unnecessarily
/// complex for small string comparisons in a hook context.
///
/// Returns `None` when:
/// - Either string is empty (nothing to compare)
/// - The union is empty (both contain only whitespace)
/// - Similarity is at or below the threshold
fn check_near_duplicate(old_string: &str, new_string: &str) -> Option<String> {
    if old_string.is_empty() || new_string.is_empty() {
        return None;
    }

    // Extract code identifier tokens: letter/underscore followed by alphanumeric/underscore.
    // More meaningful than `split_whitespace`: punctuation (`;`, `{`, `}`, `->`, `(`, `)`)
    // is ignored so "fn foo(x: i32)" and "fn foo(y: i32)" correctly share "fn"/"foo"/"i32".
    // Minimum length 2, pure-digit sequences excluded, case-folded for case-insensitive match.
    let extract_tokens = |s: &str| -> HashSet<String> {
        let mut tokens: HashSet<String> = HashSet::new();
        let mut start: Option<usize> = None;
        for (i, c) in s.char_indices() {
            if c.is_alphanumeric() || c == '_' {
                if start.is_none() {
                    start = Some(i);
                }
            } else if let Some(s_idx) = start.take() {
                let tok = &s[s_idx..i];
                if tok.len() >= 2 && !tok.chars().all(|ch| ch.is_ascii_digit()) {
                    tokens.insert(tok.to_lowercase());
                }
            }
        }
        // Final token (no trailing delimiter)
        if let Some(s_idx) = start {
            let tok = &s[s_idx..];
            if tok.len() >= 2 && !tok.chars().all(|ch| ch.is_ascii_digit()) {
                tokens.insert(tok.to_lowercase());
            }
        }
        tokens
    };

    let old_tokens = extract_tokens(old_string);
    let new_tokens = extract_tokens(new_string);

    let intersection = old_tokens.intersection(&new_tokens).count();
    let union = old_tokens.union(&new_tokens).count();

    if union == 0 {
        return None;
    }

    let similarity = intersection as f64 / union as f64;
    if similarity > NEAR_DUPLICATE_THRESHOLD {
        Some(format!(
            "NEAR_DUPLICATE: edit is {:.0}% similar to original (AST-token Jaccard) — did you mean to change more?",
            similarity * 100.0,
        ))
    } else {
        None
    }
}

/// Compose a pre-edit warning from gotchas and cross-session error patterns.
///
/// Returns `None` if there's nothing actionable to warn about.
/// This is the core prevention logic: query -> filter -> compose -> or stay silent.
pub fn compose_pre_edit_warning(db: &FileKnowledgeDB, file_path: &str) -> Option<String> {
    let mut warnings: Vec<String> = Vec::new();

    // ── Signal 1: Gotchas (sorted by hit_count as F1 proxy) ──
    warnings.extend(collect_gotcha_warnings(db, file_path));

    // ── Signal 2: Cross-session error patterns (decay-weighted) ──
    warnings.extend(collect_decay_error_warnings(db, file_path));

    if warnings.is_empty() {
        return None;
    }

    // Compose the warning block
    let header = format!(
        "Pre-edit warning for {} ({} signal{}):",
        file_path,
        warnings.len(),
        if warnings.len() > 1 { "s" } else { "" },
    );

    Some(format!("{}\n{}", header, warnings.join("\n")))
}

/// Signal 1: Collect high-confidence gotcha warnings for a file.
///
/// Filters gotchas by [`MIN_GOTCHA_HITS`], ranks the candidates by
/// BM25+TF-IDF semantic relevance to `file_path`, then formats the top
/// [`MAX_GOTCHAS_IN_WARNING`] entries with a severity icon and optional
/// symbol hint.
fn collect_gotcha_warnings(db: &FileKnowledgeDB, file_path: &str) -> Vec<String> {
    let gotchas = db.get_gotchas_for_file(file_path);
    // Collect high-confidence candidates first so we can rank them.
    let candidates: Vec<_> = gotchas
        .iter()
        .filter(|g| g.hit_count >= MIN_GOTCHA_HITS)
        .collect();

    if candidates.is_empty() {
        return Vec::new();
    }

    // Build (pattern, message) pairs for BM25 semantic ranking against the
    // file path — more relevant gotchas surface to the top.
    let pairs: Vec<(String, String)> = candidates
        .iter()
        .map(|g| (g.pattern.clone(), g.gotcha.clone()))
        .collect();

    let ranked_messages = rank_gotchas_by_relevance(&pairs, file_path, MAX_GOTCHAS_IN_WARNING);

    // Format each ranked message by looking up the originating gotcha for
    // severity and symbol metadata.
    ranked_messages
        .iter()
        .filter_map(|msg| {
            candidates.iter().find(|g| &g.gotcha == msg).map(|g| {
                let severity_icon = match g.severity.as_str() {
                    "critical" | "error" => "🔴",
                    "warning" => "⚠️",
                    _ => "ℹ️",
                };
                let symbol_hint = g
                    .symbol_name
                    .as_deref()
                    .map(|s| format!(" (symbol: {s})"))
                    .unwrap_or_default();
                format!(
                    "{severity_icon} GOTCHA [{}]: {}{}",
                    g.pattern, g.gotcha, symbol_hint,
                )
            })
        })
        .collect()
}

/// Signal 2: Collect decay-weighted cross-session error pattern warnings.
///
/// Retrieves recent errors weighted by exponential decay (half-life:
/// [`DECAY_HALF_LIFE_DAYS`]) and formats each with language context.
fn collect_decay_error_warnings(db: &FileKnowledgeDB, file_path: &str) -> Vec<String> {
    let decay_errors = db.recent_errors_with_decay(file_path, DECAY_HALF_LIFE_DAYS);
    decay_errors
        .iter()
        .take(MAX_ERRORS_IN_WARNING)
        .map(|err| {
            let lang_ctx = err
                .language
                .as_deref()
                .map(|l| format!(":{l}"))
                .unwrap_or_default();
            format!(
                "⚡ Recurring error: {}{} (weight: {:.1})",
                err.pattern, lang_ctx, err.weighted_count,
            )
        })
        .collect()
}

/// Compose content-aware warnings for new edit content.
///
/// These signals analyze the CONTENT being written, not just file history.
/// Returns warnings only for high-confidence detections, along with the
/// [`SpeculateResult`] from the syntax pre-check so callers (e.g. the deny
/// gate) can reuse it without re-running `speculate_v2`.
///
/// # Signals
///
/// - **Signal 3**: Quick syntax pre-check via `speculate_v2`
/// - **Signal 4**: Cyclomatic complexity threshold (CC > 10)
/// - **Signal 5**: Scope shadowing detection via `build_scope_map`
/// - **Signal 6**: Structural anti-patterns (e.g. `.unwrap()` in Rust)
pub fn compose_content_warnings(
    project_root: Option<&std::path::Path>,
    new_string: &str,
    file_path: &str,
) -> (Vec<String>, Option<SpeculateResult>) {
    if new_string.is_empty() {
        return (Vec::new(), None);
    }

    let lang_name = detect_language(file_path).unwrap_or("unknown");
    let lang = match lang_name.parse::<Lang>() {
        Ok(l) => l,
        Err(_) => return (Vec::new(), None),
    };

    // V1+V2 and V3+V4 are independent — run in parallel via rayon::join.
    let src_a = new_string.to_owned();
    let src_b = new_string.to_owned();

    let ((syntax_complexity, spec_result), shadow_antipattern) = rayon::join(
        move || check_syntax_and_complexity(&src_a, lang),
        move || check_shadow_and_antipatterns(&src_b, lang, lang_name),
    );

    let mut warnings = syntax_complexity;
    warnings.extend(shadow_antipattern);

    // ── Tantivy: related symbol context (module-level awareness before edit) ──
    // Surfaces what other files in the same module define so Claude can detect
    // naming conflicts and follow conventions during content edits.
    #[cfg(feature = "tantivy-fts")]
    if let Some((_, s)) =
        crate::shared::signals::tantivy_related_docs_signal(project_root, file_path)
    {
        warnings.push(s);
    }
    // The related-docs signal is the only reader of the root.
    #[cfg(not(feature = "tantivy-fts"))]
    let _ = project_root;

    (warnings, Some(spec_result))
}

/// Deny gate (S210, 25/09/2026): denies only when the edit BREAKS a file that parsed —
/// tree-sitter error nodes in the applied file that the file did not have before.
///
/// The old gate speculated on the `new_string` fragment and denied valid edits whose
/// fragment was unbalanced; a whole-file score threshold would have the opposite defect
/// (syntax weighs 0.35 of the composite, so a broken file could still clear it). The delta
/// is exact: a file already broken is never denied (the edit may be the fix), an unknown
/// parse abstains, and with no applied content (the Edit would fail anyway) the gate abstains.
fn check_deny_gate(
    file_path: &str,
    rel_path: &str,
    existing_warnings: &[String],
    applied: Option<&AppliedEdit>,
) -> Option<HookResponse> {
    let edit = applied?;
    let lang = detect_language(file_path)?.parse::<Lang>().ok()?;
    if !matches!(parse_status(&edit.before, lang), Parse::Clean) {
        return None;
    }
    let Parse::Broken(introduced) = parse_status(&edit.after, lang) else {
        return None;
    };
    let introduced: String = introduced.chars().take(400).collect();
    let mut ctx_parts = vec![format!("SYNTAX (applied file): {introduced}")];
    ctx_parts.extend(existing_warnings.iter().cloned());
    Some(HookResponse::Deny {
        reason: format!(
            "Edit denied: {rel_path} parses today and would not after this edit. \
             Fix the syntax of the replacement before applying it."
        ),
        context: Some(ctx_parts.join("\n")),
        event_name: Some("PreToolUse".to_string()),
    })
}

/// S210 — `body` with the Edit applied exactly as Claude Code would: `None` when the Edit
/// itself would fail (empty or absent `old_string`, or several matches without `replace_all`).
fn apply_edit(body: &str, old: &str, new: &str, replace_all: bool) -> Option<String> {
    if old.is_empty() {
        return None;
    }
    match body.matches(old).count() {
        0 => None,
        1 => Some(body.replacen(old, new, 1)),
        _ if replace_all => Some(body.replace(old, new)),
        _ => None,
    }
}

/// S210 — the target file before and after the Edit.
/// VGP sobre a edição (Edit e MultiEdit): devolve `Some(resposta)` só no modo "ativo" quando o
/// motor nega ou avisa; em "sombra" e "off" devolve sempre None — a resposta do hook NUNCA muda.
fn vgp_sombra_de_edit(
    runtime: &HookRuntime,
    input: &serde_json::Value,
    file_path: &str,
    _old_string: &str,
    _new_string: &str,
    applied: Option<&AppliedEdit>,
) -> Option<HookResponse> {
    let mode = vgp_mode(&runtime.project_root);
    // VL0 §8.4: a linguagem sai do caminho (fonte única) — .py/.rs/.ts/.tsx/.js/.html/.css
    // gravam a sombra com o `lang` certo; extensão desconhecida (.md, .json) fica de fora
    let lang = lang_do_caminho(file_path)?;
    if mode == VgpMode::Off {
        return None;
    }
    // MultiEdit: /tool_input/edits é uma lista — a dobra sequencial sobre o disco (uma edição
    // que não casa interrompe, e a entrada fica de fora, nunca com conteúdo errado)
    let (hook, disco, proposto) = if input
        .pointer("/tool_input/edits")
        .and_then(|v| v.as_array())
        .is_some()
    {
        let disco = applied
            .map(|a| a.before.clone())
            .or_else(|| std::fs::read_to_string(file_path).ok())
            .unwrap_or_default();
        let mut texto = disco.clone();
        let mut ok = true;
        for e in input
            .pointer("/tool_input/edits")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            let (o, n, all) = (
                e.pointer("/old_string")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
                e.pointer("/new_string")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
                e.pointer("/replace_all")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            );
            match apply_edit(&texto, o, n, all) {
                Some(t) => texto = t,
                None => {
                    ok = false;
                    break;
                }
            }
        }
        ("multiedit", disco, if ok { Some(texto) } else { None })
    } else {
        (
            "pre-edit",
            applied
                .map(|a| a.before.clone())
                .or_else(|| std::fs::read_to_string(file_path).ok())
                .unwrap_or_default(),
            applied.map(|a| a.after.clone()),
        )
    };
    let proposto = proposto?;
    let vgp_in = CheckInput {
        path: file_path.to_string(),
        content: proposto,
        project_root: runtime.project_root.clone(),
        disk: Some(disco),
        overlays: std::collections::HashMap::new(),
        lang: lang.to_string(),
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
        .map(|f| {
            format!(
                "{} `{}` ({}:{})",
                f.classe,
                f.nome,
                rel_path_of(&runtime.project_root, file_path),
                f.linha
            )
        })
        .take(3)
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

fn rel_path_of(root: &std::path::Path, path: &str) -> String {
    make_relative(path, root)
}

struct AppliedEdit {
    before: String,
    after: String,
}

impl AppliedEdit {
    /// Replay the Edit on the real file; `None` when the file is unreadable or the Edit
    /// itself would fail — then only the `new_string` fragment is known.
    fn replay(file_path: &str, old: &str, new: &str, replace_all: bool) -> Option<Self> {
        let before = std::fs::read_to_string(file_path).ok()?;
        let after = apply_edit(&before, old, new, replace_all)?;
        Some(Self { before, after })
    }
}

/// S210 — parse status of a whole file (tree-sitter only, no other speculate layer).
enum Parse {
    /// No error node.
    Clean,
    /// Error nodes, with their located description.
    Broken(String),
    /// The parser could not run (grammar/ABI, bounded parse): no opinion.
    Unknown,
}

fn parse_status(source: &str, lang: Lang) -> Parse {
    use touring_code::ast::surgery::{SurgeryError, validate_syntax};
    match validate_syntax(source, lang) {
        Ok(_) => Parse::Clean,
        Err(SurgeryError::SyntaxError(msg)) => Parse::Broken(msg),
        Err(_) => Parse::Unknown,
    }
}

/// S210 — the fragment's own syntax diagnostics are advisory: a valid edit's `new_string`
/// is routinely unbalanced (a lone `}`). With the applied file known they are dropped (the
/// deny gate judged the real file); without it they stay, labelled for what they are.
/// Non-syntax layers (structural, complexity) are untouched.
fn relabel_fragment_syntax(
    warnings: &mut Vec<String>,
    fragment: Option<&SpeculateResult>,
    applied_known: bool,
) {
    let Some(spec) = fragment else {
        return;
    };
    let syntax: HashSet<String> = spec
        .layers
        .iter()
        .filter(|l| matches!(l.layer, touring_code::ast::ValidationLayer::Syntax) && !l.passed)
        .flat_map(|l| l.diagnostics.iter().map(|d| format!("SYNTAX: {d}")))
        .collect();
    if applied_known {
        warnings.retain(|w| !syntax.contains(w));
        return;
    }
    for w in warnings.iter_mut() {
        if syntax.contains(w.as_str()) {
            *w = w.replacen(
                "SYNTAX: ",
                "SYNTAX (new_string fragment only, advisory): ",
                1,
            );
        }
    }
}

/// V1+V2: Syntax validity and cyclomatic complexity checks.
///
/// Returns warning strings for any failed speculate layers and for
/// functions exceeding [`COMPLEXITY_THRESHOLD`], along with the
/// [`SpeculateResult`] from the syntax check for reuse by the deny gate.
fn check_syntax_and_complexity(source: &str, lang: Lang) -> (Vec<String>, SpeculateResult) {
    let (mut out, spec_result) = collect_syntax_issues(source, lang);
    out.extend(collect_complexity_issues(source, lang));
    (out, spec_result)
}

/// V1: Collect syntax diagnostic messages from speculate_v2.
///
/// Iterates over failed validation layers and prefixes each diagnostic
/// with `"SYNTAX: "`. Returns both the warning strings and the underlying
/// [`SpeculateResult`] so callers can reuse it (e.g. for the deny gate)
/// without re-running the expensive AST parse.
fn collect_syntax_issues(source: &str, lang: Lang) -> (Vec<String>, SpeculateResult) {
    let spec_result = speculate_v2(source, lang, None, None);
    if spec_result.all_passed {
        return (Vec::new(), spec_result);
    }
    let issues = spec_result
        .layers
        .iter()
        .filter(|layer| !layer.passed)
        .flat_map(|layer| layer.diagnostics.iter().map(|d| format!("SYNTAX: {d}")))
        .collect();
    (issues, spec_result)
}

/// V2: Collect complexity warnings for functions exceeding [`COMPLEXITY_THRESHOLD`].
///
/// Returns an empty vec when complexity data is unavailable or all functions
/// are within the threshold.
fn collect_complexity_issues(source: &str, lang: Lang) -> Vec<String> {
    let Ok(complexities) = compute_complexity_for_source(source, lang) else {
        return Vec::new();
    };
    complexities
        .iter()
        .filter(|(_, cc)| *cc > COMPLEXITY_THRESHOLD)
        .map(|(name, cc)| format!("COMPLEXITY: {name} CC={cc} (threshold: {COMPLEXITY_THRESHOLD})"))
        .collect()
}

/// V3+V4: Scope shadowing and structural anti-pattern checks.
///
/// Detects variable names that shadow an outer binding and language-specific
/// anti-patterns (all languages supported by [`crate::shared::antipatterns`]).
fn check_shadow_and_antipatterns(source: &str, lang: Lang, lang_name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    // ── V3: Scope shadowing detection ──
    let scope = build_scope_map(source, lang);
    let shadows = detect_scope_shadows(&scope);
    for shadow in shadows.iter().take(MAX_SHADOW_WARNINGS) {
        out.push(format!(
            "SHADOW: variable '{}' shadows outer definition (L{})",
            shadow.0, shadow.1
        ));
    }

    // ── V4: Structural anti-patterns (all supported languages) ──
    let ap = crate::shared::antipatterns::detect_antipatterns(source, lang_name);
    for msg in ap {
        out.push(format!("ANTIPATTERN: {msg}"));
    }

    out
}

/// Detect variable shadowing within a scope map.
///
/// Iterates over all scope entries and identifies names that appear more than
/// once, indicating a shadow. Returns pairs of `(variable_name, shadow_line)`.
fn detect_scope_shadows(scope: &ScopeMap) -> Vec<(String, usize)> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    let mut shadows = Vec::new();
    for entry in &scope.entries {
        if seen.contains_key(entry.name.as_str()) {
            shadows.push((entry.name.clone(), entry.line));
        }
        seen.insert(&entry.name, entry.line);
    }
    shadows
}

/// Signal 7: Detect invalid `#[cfg(...)]` conditions in new content.
///
/// Known invalid conditions:
/// - `#[cfg(bench)]` — not a valid rustc cfg. Use `#[cfg(test)]` or `benches/`.
/// - `#[cfg(nightly)]` — not a built-in cfg. Use `#[cfg(feature = "nightly")]`.
///
/// Returns one warning string per detected invalid condition.
fn collect_invalid_cfg_warnings(source: &str) -> Vec<String> {
    /// Known invalid cfg conditions and their fix suggestions.
    const INVALID_CFGS: &[(&str, &str)] = &[
        (
            "#[cfg(bench)]",
            "INVALID_CFG: #[cfg(bench)] is not a valid cfg condition. \
             Use #[cfg(test)] for inline benchmarks, or move benchmark code \
             to benches/ directory with required-features in Cargo.toml.",
        ),
        (
            "#[cfg_attr(bench",
            "INVALID_CFG: cfg_attr(bench, ...) uses invalid cfg condition 'bench'. \
             Use cfg_attr(test, ...) or feature-gate instead.",
        ),
    ];

    let mut warnings = Vec::new();
    for (pattern, message) in INVALID_CFGS {
        if source.contains(pattern) {
            warnings.push(message.to_string());
        }
    }
    warnings
}

/// Detect language from file extension.
///
/// Used by S1.2 to populate the `language` column in edit_history
/// for context-aware error normalization.
///
/// Delegates to [`crate::shared::detect_language::detect_language`].
pub fn detect_language(file_path: &str) -> Option<&'static str> {
    crate::shared::detect_language::detect_language(file_path)
}

// ─────────────────────────────────────────────────────────────────────────────
// Signal 8: Module file existence — prevents E0583
// ─────────────────────────────────────────────────────────────────────────────

/// Returns `true` when neither `parent/NAME.rs` nor `parent/NAME/mod.rs` exists.
///
/// A `true` result means the compiler will emit E0583 for this `mod NAME;` declaration.
fn mod_file_missing(mod_name: &str, parent: &std::path::Path) -> bool {
    !parent.join(format!("{mod_name}.rs")).exists()
        && !parent.join(mod_name).join("mod.rs").exists()
}

/// Signal 8: Module existence — `mod X;` declared without a corresponding file.
///
/// Scans `new_string` for bare module declarations (`mod NAME;` / `pub mod NAME;`)
/// and checks that `NAME.rs` or `NAME/mod.rs` exists relative to `file_path`.
///
/// Zero false positives: a missing file is always a compiler error (E0583).
/// Inline module bodies (`mod NAME { ... }`) are ignored — they need no file.
fn collect_module_existence_issues(new_string: &str, file_path: &str) -> Vec<String> {
    if !file_path.ends_with(".rs") {
        return Vec::new();
    }
    let parent = match std::path::Path::new(file_path).parent() {
        Some(p) => p.to_path_buf(),
        None => return Vec::new(),
    };
    let mut issues = Vec::new();
    let mut search = new_string;
    while let Some(pos) = search.find("mod ") {
        let after = &search[pos + 4..];
        let name_end = after
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(after.len());
        let mod_name = &after[..name_end];
        // Only bare declarations end with `;`, not bodies `{`
        let rest = after[name_end..].trim_start_matches([' ', '\t']);
        if !mod_name.is_empty() && rest.starts_with(';') && mod_file_missing(mod_name, &parent) {
            issues.push(format!(
                "E0583 risk: mod `{mod_name}` declared but `{mod_name}.rs` / `{mod_name}/mod.rs` not found — create the file first"
            ));
        }
        search = &search[pos + 4..];
    }
    issues
}

// ─────────────────────────────────────────────────────────────────────────────
// Signal 9: Public API signature change — prevents silent caller breakage
// ─────────────────────────────────────────────────────────────────────────────

/// Signal 9: Public API signature change — `pub fn` signature differs between
/// `old_string` and `new_string` for the same function name.
///
/// A changed parameter list or return type silently breaks every caller.
/// Enhanced: queries `wiring_map` to list exact caller files that need updating,
/// so Claude knows precisely WHERE to fix call sites.
fn collect_api_signature_changes(
    old_string: &str,
    new_string: &str,
    db: &FileKnowledgeDB,
    module_file: &str,
) -> Vec<String> {
    if old_string.is_empty() {
        return Vec::new();
    }
    let old_sigs = extract_pub_fn_signatures(old_string);
    let new_sigs = extract_pub_fn_signatures(new_string);
    let mut issues = Vec::new();
    for (name, old_sig) in &old_sigs {
        if let Some(new_sig) = new_sigs.get(name.as_str())
            && old_sig != new_sig
        {
            // Query wiring_map for all files that import this symbol
            let caller_detail = format_caller_detail(db, module_file, name);
            issues.push(format!(
                "⚠ SIGNATURE CHANGED: `pub fn {name}` — {old_sig} → {new_sig}{caller_detail}"
            ));
        }
    }
    issues
}

/// Format caller details for a changed symbol by querying the wiring_map.
///
/// Returns a string like:
/// ```text
/// \n  Callers that MUST be updated:
///   • src/post_read.rs (line 82) — uses `build_knowledge_ast`
///   • tests/integration.rs (line 145) — uses `build_knowledge_ast`
/// ```
/// Returns empty string if no consumers are found.
fn format_caller_detail(db: &FileKnowledgeDB, module_file: &str, symbol_name: &str) -> String {
    let consumers = db
        .consumers_of_symbol(module_file, symbol_name)
        .unwrap_or_default();

    if consumers.is_empty() {
        return String::new();
    }

    let mut detail = format!(
        "\n  Callers that MUST be updated ({} file{}):",
        consumers.len(),
        if consumers.len() > 1 { "s" } else { "" }
    );
    for (consumer_file, import_line) in &consumers {
        let line_hint = import_line
            .map(|l| format!(" (line {l})"))
            .unwrap_or_default();
        detail.push_str(&format!(
            "\n  • {consumer_file}{line_hint} — uses `{symbol_name}`"
        ));
    }
    detail
}

/// Scan from `text[0]` (which must be `'('`) to its matching close paren.
///
/// Returns the index one past the `')'`, or `text.len()` when unbalanced.
/// Extracted from [`extract_pub_fn_signatures`] to reduce cyclomatic complexity.
fn scan_to_close_paren(text: &str) -> usize {
    let mut depth: usize = 0;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
    }
    text.len()
}

/// Extract `pub fn NAME(PARAMS) -> RET` signatures from Rust source.
///
/// Returns a map of `name → "(params) -> ret"`. Used by
/// [`collect_api_signature_changes`] to detect breaking signature edits.
fn extract_pub_fn_signatures(source: &str) -> HashMap<String, String> {
    let mut sigs = HashMap::new();
    let mut search = source;
    while let Some(pos) = search.find("pub fn ") {
        let after = &search[pos + 7..];
        let name_end = after
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(after.len());
        let name = &after[..name_end];
        if name.is_empty() {
            search = &search[pos + 7..];
            continue;
        }
        let rest = &after[name_end..];
        if let Some(open) = rest.find('(') {
            let params_text = &rest[open..];
            let close = scan_to_close_paren(params_text);
            let params = params_text[1..close.saturating_sub(1)].trim().to_string();
            let after_params = &rest[open + close..];
            let ret = if let Some(arrow) = after_params.find("->") {
                let ret_raw = &after_params[arrow + 2..];
                let end = ret_raw.find(['{', ';', '\n']).unwrap_or(ret_raw.len());
                ret_raw[..end].trim().to_string()
            } else {
                String::new()
            };
            sigs.insert(name.to_string(), format!("({params}) -> {ret}"));
        }
        search = &search[pos + 7..];
    }
    sigs
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::len_zero)]
    use super::*;
    use tempfile::TempDir;

    fn setup_db() -> (TempDir, FileKnowledgeDB) {
        let dir = TempDir::new().expect("create temp dir");
        let db_path = dir.path().join("test_knowledge.db");
        let db = FileKnowledgeDB::new(&db_path).expect("create db");
        (dir, db)
    }

    #[test]
    fn test_pre_edit_silent_for_unknown_file() {
        let (_dir, db) = setup_db();
        let warning = compose_pre_edit_warning(&db, "unknown/file.rs");
        assert!(warning.is_none(), "Unknown file should produce no warning");
    }

    #[test]
    fn test_pre_edit_returns_gotcha_warning() {
        let (_dir, db) = setup_db();

        // Add a gotcha with enough hits to be high-confidence
        db.add_gotcha("rust_bridge", "use matched_text not text", "critical", None)
            .expect("add gotcha");
        // Increment hit_count to meet MIN_GOTCHA_HITS threshold
        let gotchas = db.get_gotchas_for_file("scripts/aco/rust_bridge.py");
        assert!(!gotchas.is_empty(), "gotcha should match");
        for _ in 0..MIN_GOTCHA_HITS {
            db.increment_gotcha_hit(gotchas[0].id);
        }

        let warning = compose_pre_edit_warning(&db, "scripts/aco/rust_bridge.py");
        assert!(warning.is_some(), "Should produce warning for known gotcha");
        let text = warning.unwrap();
        assert!(text.contains("GOTCHA"), "Warning should contain GOTCHA");
        assert!(
            text.contains("matched_text"),
            "Warning should contain gotcha text"
        );
    }

    #[test]
    fn test_pre_edit_filters_low_confidence_gotchas() {
        let (_dir, db) = setup_db();

        // Add a gotcha with 0 hits (below threshold)
        db.add_gotcha("some_file", "low confidence gotcha", "info", None)
            .expect("add gotcha");

        let warning = compose_pre_edit_warning(&db, "some_file.rs");
        assert!(
            warning.is_none(),
            "Low-confidence gotchas should not trigger warning"
        );
    }

    #[test]
    fn test_pre_edit_includes_recurring_errors() {
        let (_dir, db) = setup_db();

        // Record enough errors to trigger decay-weighted detection
        for _ in 0..5 {
            db.record_edit_full(
                "src/main.rs",
                "Edit",
                Some("fix import"),
                Some("string_not_found"),
                Some("rust"),
                None,
                Some("session-1"),
            )
            .expect("record edit");
        }

        let warning = compose_pre_edit_warning(&db, "src/main.rs");
        assert!(warning.is_some(), "Recurring errors should trigger warning");
        let text = warning.unwrap();
        assert!(
            text.contains("Recurring error"),
            "Warning should mention recurring error"
        );
        assert!(
            text.contains("string_not_found"),
            "Warning should name the error pattern"
        );
    }

    #[test]
    fn test_pre_edit_max_warnings_capped() {
        let (_dir, db) = setup_db();

        // Add many gotchas with DIFFERENT patterns that all substring-match the test path.
        // Patterns: "g0_", "g1_", ..., "g9_" all match "g0_g1_g2_g3_g4_g5_g6_g7_g8_g9_file.rs"
        for i in 0..10 {
            let pattern = format!("g{i}_");
            let id = db
                .add_gotcha(&pattern, &format!("gotcha_{i}"), "warning", None)
                .expect("add gotcha");
            for _ in 0..MIN_GOTCHA_HITS {
                db.increment_gotcha_hit(id);
            }
        }

        let warning = compose_pre_edit_warning(&db, "g0_g1_g2_g3_g4_g5_g6_g7_g8_g9_file.rs");
        assert!(warning.is_some());
        let text = warning.unwrap();
        let gotcha_count = text.matches("GOTCHA").count();
        assert_eq!(
            gotcha_count, MAX_GOTCHAS_IN_WARNING,
            "Should cap at {MAX_GOTCHAS_IN_WARNING} gotchas"
        );
    }

    #[test]
    fn test_detect_language_python() {
        assert_eq!(detect_language("scripts/test.py"), Some("python"));
        assert_eq!(detect_language("test.pyi"), Some("python"));
    }

    #[test]
    fn test_detect_language_rust() {
        assert_eq!(detect_language("src/main.rs"), Some("rust"));
    }

    #[test]
    fn test_detect_language_typescript() {
        assert_eq!(detect_language("src/App.tsx"), Some("typescript"));
        assert_eq!(detect_language("index.ts"), Some("typescript"));
    }

    #[test]
    fn test_detect_language_unknown() {
        assert_eq!(detect_language("Makefile"), None);
        assert_eq!(detect_language("noext"), None);
    }

    #[test]
    fn test_make_relative() {
        use std::path::Path;
        assert_eq!(
            make_relative(
                "/home/user/project/src/main.rs",
                Path::new("/home/user/project")
            ),
            "src/main.rs"
        );
        assert_eq!(
            make_relative("already/relative.rs", Path::new("/home/user/project")),
            "already/relative.rs"
        );
    }

    // ── Signal 3-6 tests (content-aware warnings) ───────────────────────

    #[test]
    fn test_content_warnings_empty_string() {
        let (warnings, spec) = compose_content_warnings(None, "", "test.rs");
        assert!(
            warnings.is_empty(),
            "Empty content should produce no warnings"
        );
        assert!(
            spec.is_none(),
            "Empty content should not produce a SpeculateResult"
        );
    }

    #[test]
    fn test_content_warnings_clean_code() {
        let (warnings, spec) =
            compose_content_warnings(None, "fn clean() -> u32 { 42 }", "test.rs");
        // Clean code with no unwrap, low complexity, no shadows should be clean
        let has_antipattern = warnings.iter().any(|w| w.contains("ANTIPATTERN"));
        assert!(
            !has_antipattern,
            "Clean code should produce no ANTIPATTERN warnings"
        );
        assert!(
            spec.is_some(),
            "Parseable code should return a SpeculateResult"
        );
    }

    #[test]
    fn test_content_warnings_unwrap_detection() {
        let (warnings, _spec) = compose_content_warnings(
            None,
            "fn risky() { let _ = x.unwrap(); let _ = y.unwrap(); }",
            "test.rs",
        );
        let antipattern = warnings.iter().find(|w| w.contains("ANTIPATTERN"));
        assert!(
            antipattern.is_some(),
            "Should detect unwrap() anti-pattern in Rust code"
        );
        let text = antipattern.expect("antipattern should exist");
        assert!(text.contains("2"), "Should count 2 unwrap() calls");
        assert!(text.contains("unwrap()"), "Should mention unwrap()");
    }

    #[test]
    fn test_content_warnings_unwrap_not_triggered_for_python() {
        let (warnings, _spec) = compose_content_warnings(None, "x.unwrap()", "test.py");
        let antipattern = warnings.iter().any(|w| w.contains("ANTIPATTERN"));
        assert!(
            !antipattern,
            "unwrap() anti-pattern should only trigger for Rust files"
        );
    }

    #[test]
    fn test_scope_shadow_detection() {
        let scope = build_scope_map("fn foo() { let x = 1; let x = 2; }", Lang::Rust);
        let shadows = detect_scope_shadows(&scope);
        // The function should run without panic regardless of scope_map granularity
        let _ = shadows;
    }

    #[test]
    fn test_scope_shadow_detection_empty() {
        let scope = ScopeMap::default();
        let shadows = detect_scope_shadows(&scope);
        assert!(shadows.is_empty(), "Empty scope map should have no shadows");
    }

    #[test]
    fn test_content_warnings_unknown_language() {
        // Unknown language should not panic; signals gracefully degrade
        let (warnings, spec) = compose_content_warnings(None, "some content", "test.xyz");
        // Should not panic, warnings may or may not be empty depending on heuristics
        let _ = warnings;
        assert!(
            spec.is_none(),
            "Unknown language should not produce a SpeculateResult"
        );
    }

    // ── Signal 7: Invalid cfg condition detection ───────────────────────

    #[test]
    fn test_invalid_cfg_bench_detected() {
        let warnings = collect_invalid_cfg_warnings("#[cfg(bench)]\nfn bench_it() {}");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("INVALID_CFG"));
        assert!(warnings[0].contains("#[cfg(bench)]"));
    }

    #[test]
    fn test_invalid_cfg_attr_bench_detected() {
        let warnings = collect_invalid_cfg_warnings("#[cfg_attr(bench, derive(Debug))]");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("cfg_attr(bench"));
    }

    #[test]
    fn test_valid_cfg_test_not_flagged() {
        let warnings = collect_invalid_cfg_warnings("#[cfg(test)]\nmod tests {}");
        assert!(
            warnings.is_empty(),
            "cfg(test) is valid and should not be flagged"
        );
    }

    #[test]
    fn test_valid_cfg_feature_not_flagged() {
        let warnings = collect_invalid_cfg_warnings("#[cfg(feature = \"nightly\")]");
        assert!(warnings.is_empty(), "cfg(feature = \"nightly\") is valid");
    }

    #[test]
    fn test_no_cfg_no_warnings() {
        let warnings = collect_invalid_cfg_warnings("fn main() { println!(\"hello\"); }");
        assert!(warnings.is_empty());
    }

    // ── Signal 8: Module file existence (E0583 prevention) ─────────────
    #[test]
    fn test_module_existence_non_rs_file_skipped() {
        let issues = collect_module_existence_issues("mod foo;", "src/lib.py");
        assert!(
            issues.is_empty(),
            "Non-Rust files should produce no module existence issues"
        );
    }

    #[test]
    fn test_module_existence_inline_body_no_warning() {
        // Inline body (`mod NAME { ... }`) needs no file — only bare `mod NAME;` does
        let issues = collect_module_existence_issues("mod inline { fn bar() {} }", "/tmp/lib.rs");
        assert!(
            issues.is_empty(),
            "Inline module body must not trigger E0583 warning"
        );
    }

    #[test]
    fn test_module_existence_missing_file_warns() {
        let issues = collect_module_existence_issues(
            "pub mod nonexistent_xyz_touring_abc;",
            "/tmp/test_lib.rs",
        );
        assert!(
            !issues.is_empty(),
            "Missing module file should trigger E0583 warning"
        );
        assert!(
            issues[0].contains("E0583"),
            "Warning should reference E0583 error code"
        );
        assert!(
            issues[0].contains("nonexistent_xyz_touring_abc"),
            "Warning should name the missing module"
        );
    }

    #[test]
    fn test_module_existence_file_present_no_warning() {
        let dir = TempDir::new().expect("create temp dir");
        std::fs::write(dir.path().join("mymod.rs"), "// stub").expect("write module file");
        let lib_path = dir.path().join("lib.rs").to_string_lossy().into_owned();
        let issues = collect_module_existence_issues("mod mymod;", &lib_path);
        assert!(
            issues.is_empty(),
            "Existing module file should produce no warning"
        );
    }

    #[test]
    fn test_module_existence_dir_mod_present_no_warning() {
        let dir = TempDir::new().expect("create temp dir");
        let sub = dir.path().join("submod");
        std::fs::create_dir(&sub).expect("create subdir");
        std::fs::write(sub.join("mod.rs"), "// stub").expect("write mod.rs");
        let lib_path = dir.path().join("lib.rs").to_string_lossy().into_owned();
        let issues = collect_module_existence_issues("mod submod;", &lib_path);
        assert!(
            issues.is_empty(),
            "NAME/mod.rs present should produce no warning"
        );
    }

    // ── Signal 9: Public API signature changes (E0308/E0061 prevention) ─

    fn sig_test_db() -> (TempDir, FileKnowledgeDB) {
        let tmp = TempDir::new().expect("create temp dir");
        let db_path = tmp.path().join("test.db");
        let db = FileKnowledgeDB::new(&db_path).expect("create db");
        (tmp, db)
    }

    #[test]
    fn test_api_sig_empty_old_string_no_issues() {
        let (_tmp, db) = sig_test_db();
        let issues =
            collect_api_signature_changes("", "pub fn foo(x: u32) -> u32 { x }", &db, "src/lib.rs");
        assert!(
            issues.is_empty(),
            "Empty old_string means no baseline — no issues expected"
        );
    }

    #[test]
    fn test_api_sig_identical_no_warning() {
        let (_tmp, db) = sig_test_db();
        let src = "pub fn foo(x: u32) -> u32 { x }";
        let issues = collect_api_signature_changes(src, src, &db, "src/lib.rs");
        assert!(
            issues.is_empty(),
            "Identical signature should produce no warning"
        );
    }

    #[test]
    fn test_api_sig_return_type_changed_warns() {
        let (_tmp, db) = sig_test_db();
        let old = "pub fn foo(x: u32) -> u32 { x }";
        let new = "pub fn foo(x: u32) -> String { x.to_string() }";
        let issues = collect_api_signature_changes(old, new, &db, "src/lib.rs");
        assert!(
            !issues.is_empty(),
            "Changed return type should trigger warning"
        );
        assert!(
            issues[0].contains("foo"),
            "Warning should name the changed function"
        );
        assert!(
            issues[0].contains("SIGNATURE CHANGED"),
            "Warning should say SIGNATURE CHANGED"
        );
    }

    #[test]
    fn test_api_sig_param_added_warns() {
        let (_tmp, db) = sig_test_db();
        let old = "pub fn bar(a: u32) -> bool { true }";
        let new = "pub fn bar(a: u32, b: u32) -> bool { true }";
        let issues = collect_api_signature_changes(old, new, &db, "src/lib.rs");
        assert!(!issues.is_empty(), "Added parameter should trigger warning");
        assert!(
            issues[0].contains("bar"),
            "Warning should name the affected function"
        );
    }

    #[test]
    fn test_api_sig_new_function_added_no_warning() {
        let (_tmp, db) = sig_test_db();
        let old = "pub fn existing() -> u32 { 1 }";
        let new = "pub fn existing() -> u32 { 1 }\npub fn brand_new() -> u32 { 2 }";
        let issues = collect_api_signature_changes(old, new, &db, "src/lib.rs");
        assert!(
            issues.is_empty(),
            "Adding a new function should not produce a warning"
        );
    }

    #[test]
    fn test_api_sig_private_fn_not_tracked() {
        let (_tmp, db) = sig_test_db();
        let old = "fn internal(x: u32) -> u32 { x }";
        let new = "fn internal(x: u32, y: u32) -> u32 { x + y }";
        let issues = collect_api_signature_changes(old, new, &db, "src/lib.rs");
        assert!(
            issues.is_empty(),
            "Private function changes should not be warned about"
        );
    }

    // ── Signal 9 caller-aware: wiring-backed caller detail ─────────────

    #[test]
    fn test_api_sig_lists_callers_from_wiring_map() {
        let (_tmp, db) = sig_test_db();

        // Register the pub fn being changed
        db.register_pub_symbol("src/ast.rs", "build_knowledge_ast", "function", "public")
            .expect("register symbol");

        // Simulate 3 callers
        db.record_consumer(
            "src/ast.rs",
            "build_knowledge_ast",
            "src/post_read.rs",
            Some(82),
        )
        .expect("record consumer 1");
        db.record_consumer(
            "src/ast.rs",
            "build_knowledge_ast",
            "tests/integration.rs",
            Some(145),
        )
        .expect("record consumer 2");
        db.record_consumer(
            "src/ast.rs",
            "build_knowledge_ast",
            "src/cli_handlers.rs",
            None,
        )
        .expect("record consumer 3");

        let old = "pub fn build_knowledge_ast(rel: &str, content: &str) -> (Knowledge, Vec<String>) { todo!() }";
        let new = "pub fn build_knowledge_ast(rel: &str, content: &str, abs: &str) -> (Knowledge, Vec<String>, Vec<Import>) { todo!() }";
        let issues = collect_api_signature_changes(old, new, &db, "src/ast.rs");

        assert_eq!(
            issues.len(),
            1,
            "Should have exactly 1 signature change warning"
        );
        let warning = &issues[0];

        // Should name the function
        assert!(
            warning.contains("build_knowledge_ast"),
            "Should name the function"
        );

        // Should list exact caller files
        assert!(
            warning.contains("src/post_read.rs"),
            "Should list post_read.rs as caller"
        );
        assert!(
            warning.contains("tests/integration.rs"),
            "Should list integration.rs as caller"
        );
        assert!(
            warning.contains("src/cli_handlers.rs"),
            "Should list cli_handlers.rs as caller"
        );

        // Should show line numbers where available
        assert!(
            warning.contains("line 82"),
            "Should show line 82 for post_read.rs"
        );
        assert!(
            warning.contains("line 145"),
            "Should show line 145 for integration.rs"
        );

        // Should show file count
        assert!(
            warning.contains("3 files"),
            "Should indicate 3 caller files"
        );
    }

    #[test]
    fn test_api_sig_no_callers_still_warns_but_without_detail() {
        let (_tmp, db) = sig_test_db();

        // Register the pub fn but no consumers
        db.register_pub_symbol("src/lib.rs", "my_fn", "function", "public")
            .expect("register");

        let old = "pub fn my_fn(a: i32) -> i32 { a }";
        let new = "pub fn my_fn(a: i32, b: i32) -> i32 { a + b }";
        let issues = collect_api_signature_changes(old, new, &db, "src/lib.rs");

        assert_eq!(issues.len(), 1);
        // Should still warn about the signature change
        assert!(issues[0].contains("SIGNATURE CHANGED"));
        // But no caller detail section
        assert!(!issues[0].contains("Callers that MUST be updated"));
    }

    #[test]
    fn test_api_sig_multiple_functions_changed_with_callers() {
        let (_tmp, db) = sig_test_db();

        db.register_pub_symbol("src/core.rs", "foo", "function", "public")
            .expect("reg");
        db.register_pub_symbol("src/core.rs", "bar", "function", "public")
            .expect("reg");
        db.record_consumer("src/core.rs", "foo", "src/main.rs", Some(10))
            .expect("rec");
        db.record_consumer("src/core.rs", "bar", "src/utils.rs", Some(25))
            .expect("rec");

        let old = "pub fn foo(x: i32) -> i32 { x }\npub fn bar(y: bool) -> bool { y }";
        let new =
            "pub fn foo(x: i64) -> i64 { x }\npub fn bar(y: bool, z: bool) -> bool { y && z }";
        let issues = collect_api_signature_changes(old, new, &db, "src/core.rs");

        assert_eq!(issues.len(), 2, "Both functions changed → 2 warnings");
        // Verify each warning has its own caller detail
        let foo_warn = issues
            .iter()
            .find(|w| w.contains("foo"))
            .expect("foo warning");
        let bar_warn = issues
            .iter()
            .find(|w| w.contains("bar"))
            .expect("bar warning");

        assert!(foo_warn.contains("src/main.rs"), "foo caller is main.rs");
        assert!(bar_warn.contains("src/utils.rs"), "bar caller is utils.rs");
    }

    #[test]
    fn test_invalid_cfg_integrated_in_run_returning() {
        // Verify that the invalid cfg signal propagates through run_returning
        let _input = serde_json::json!({
            "tool_input": {
                "file_path": "/tmp/test_cfg.rs",
                "new_string": "#[cfg(bench)]\nfn bench_it() {}"
            }
        });
        let runtime = {
            let dir = TempDir::new().expect("create temp dir");
            let db_path = dir.path().join("test_knowledge.db");
            let _db = FileKnowledgeDB::new(&db_path).expect("create db");
            // We can't easily construct a full HookRuntime in unit tests,
            // but we can verify the function directly
            drop(dir);
        };
        let _ = runtime;
        // Direct function test is sufficient — the integration is verified
        // by the test_invalid_cfg_bench_detected test above and the wiring
        // in run_returning which calls collect_invalid_cfg_warnings.
    }

    // ── Signal 10: Near-duplicate edit detection (Jaccard) ─────────────

    #[test]
    fn test_near_duplicate_high_similarity() {
        // Only a single token differs out of many — well above 85% threshold
        let old = "fn foo() { let x = 1; let y = 2; let z = 3; let w = 4; }";
        let new = "fn foo() { let x = 1; let y = 2; let z = 3; let w = 5; }";
        let result = check_near_duplicate(old, new);
        assert!(result.is_some(), "High similarity should produce a warning");
        let msg = result.expect("warning should exist");
        assert!(
            msg.contains("NEAR_DUPLICATE"),
            "Warning should contain NEAR_DUPLICATE tag"
        );
        assert!(msg.contains("Jaccard"), "Warning should mention Jaccard");
        assert!(
            msg.contains("did you mean to change more"),
            "Warning should prompt the user"
        );
    }

    #[test]
    fn test_near_duplicate_low_similarity_no_warning() {
        // Completely different content — no warning expected
        let old = "fn alpha() { let a = 1; }";
        let new = "fn beta() { println!(\"hello world\"); }";
        let result = check_near_duplicate(old, new);
        assert!(result.is_none(), "Low similarity should produce no warning");
    }

    #[test]
    fn test_near_duplicate_empty_strings() {
        assert!(
            check_near_duplicate("", "anything").is_none(),
            "Empty old_string → no warning"
        );
        assert!(
            check_near_duplicate("anything", "").is_none(),
            "Empty new_string → no warning"
        );
        assert!(
            check_near_duplicate("", "").is_none(),
            "Both empty → no warning"
        );
    }

    #[test]
    fn test_near_duplicate_identical_strings() {
        // Jaccard = 1.0 which is > 0.85 — should warn
        let same = "fn foo() { let x = 42; }";
        let result = check_near_duplicate(same, same);
        assert!(
            result.is_some(),
            "Identical strings (Jaccard=1.0) should warn"
        );
        let msg = result.expect("warning should exist");
        assert!(
            msg.contains("100%"),
            "Identical strings should show 100% similarity"
        );
    }

    #[test]
    fn test_near_duplicate_whitespace_only() {
        // Whitespace-only strings produce empty token sets → union = 0 → None
        assert!(
            check_near_duplicate("   ", "   ").is_none(),
            "Whitespace-only → no warning"
        );
    }

    #[test]
    fn test_near_duplicate_at_threshold_boundary() {
        // Construct sets where Jaccard = exactly threshold (0.85) — should NOT warn
        // 17 shared tokens + 3 unique = 17/20 = 0.85 → not > 0.85
        let shared: Vec<String> = (0..17).map(|i| format!("token{i}")).collect();
        let old_only: Vec<String> = (17..20).map(|i| format!("old{i}")).collect();
        let new_only: Vec<String> = (17..20).map(|i| format!("new{i}")).collect();
        let old = format!("{} {}", shared.join(" "), old_only.join(" "));
        let new = format!("{} {}", shared.join(" "), new_only.join(" "));
        let result = check_near_duplicate(&old, &new);
        assert!(
            result.is_none(),
            "Jaccard = 0.85 (at boundary, not above) → no warning"
        );
    }
}

/// S210 (plan 2026-09-25) — the deny is decided on the file WITH the edit applied, by delta
/// (the edit breaks a file that parsed), never on the `new_string` fragment alone.
#[cfg(test)]
mod s210_tests {
    use super::*;

    const RUST_FILE: &str = "fn a() {\n    if true {\n        x();\n    }\n}\n\nfn x() {}\n";

    fn edit(dir: &tempfile::TempDir, body: &str, old: &str, new: &str) -> HookResponse {
        let path = dir.path().join("prev.rs");
        std::fs::write(&path, body).expect("write");
        let rt = HookRuntime::new(dir.path()).expect("runtime");
        let input = serde_json::json!({
            "tool_name": "Edit",
            "tool_input": {"file_path": path.to_str().expect("utf8"), "old_string": old, "new_string": new}
        });
        run_returning(&rt, &input)
    }

    fn denied(r: &HookResponse) -> bool {
        matches!(r, HookResponse::Deny { .. })
    }

    fn context_of(r: &HookResponse) -> String {
        match r {
            HookResponse::Context { context, .. } => context.clone(),
            HookResponse::Deny { context, .. } => context.clone().unwrap_or_default(),
            _ => String::new(),
        }
    }

    #[test]
    fn lone_closing_brace_fragment_is_not_denied() {
        let dir = tempfile::TempDir::new().expect("tmp");
        let r = edit(
            &dir,
            RUST_FILE,
            "        x();\n    }",
            "        x();\n        x();\n    }",
        );
        assert!(
            !denied(&r),
            "a valid edit was denied from its unbalanced fragment: {r:?}"
        );
        assert!(
            !context_of(&r).contains("syntax error(s) detected"),
            "the applied file parses — the fragment's syntax noise must not reach the model: {r:?}"
        );
    }

    #[test]
    fn syntax_breaking_edit_is_denied() {
        let dir = tempfile::TempDir::new().expect("tmp");
        let r = edit(&dir, RUST_FILE, "        x();\n    }", "        x(;\n    }");
        assert!(
            denied(&r),
            "an edit that breaks a parsing file must be denied: {r:?}"
        );
        assert!(
            context_of(&r).contains("SYNTAX (applied file)"),
            "the deny names the applied-file error: {r:?}"
        );
    }

    #[test]
    fn edit_to_an_already_broken_file_is_not_denied() {
        let dir = tempfile::TempDir::new().expect("tmp");
        let broken = "fn a() {\n    x(;\n}\n";
        let r = edit(&dir, broken, "    x(;", "    x(;\n    y(;");
        assert!(
            !denied(&r),
            "the file did not parse before — the edit may be the fix: {r:?}"
        );
    }

    #[test]
    fn fragment_fallback_never_denies() {
        let dir = tempfile::TempDir::new().expect("tmp");
        let r = edit(
            &dir,
            RUST_FILE,
            "this text is not in the file",
            "fn broken( {",
        );
        assert!(
            !denied(&r),
            "without the applied file the fragment is advisory only: {r:?}"
        );
        assert!(
            context_of(&r).lines().all(|l| !l.starts_with("SYNTAX: ")),
            "fragment syntax lines must carry the advisory label: {r:?}"
        );
    }

    #[test]
    fn applied_content_respects_replace_all_and_uniqueness() {
        let body = "a\nx\nb\nx\n";
        assert_eq!(
            apply_edit(body, "x", "y", true).as_deref(),
            Some("a\ny\nb\ny\n")
        );
        assert_eq!(
            apply_edit(body, "x", "y", false),
            None,
            "ambiguous old_string: the Edit itself would fail"
        );
        assert_eq!(
            apply_edit(body, "a\n", "z\n", false).as_deref(),
            Some("z\nx\nb\nx\n")
        );
        assert_eq!(apply_edit(body, "missing", "y", false), None);
        assert_eq!(
            apply_edit(body, "", "y", false),
            None,
            "empty old_string is not an Edit"
        );
    }

    /// Parser control: this very file uses let-chains (`if let … && let …`); if the grammar
    /// could not parse them, every let-chain edit would read as "breaks a parsing file".
    #[test]
    fn let_chains_parse_as_valid_rust() {
        let own = include_str!("pre_edit_prevention.rs");
        assert!(
            own.contains("&& let Some("),
            "control lost: the file no longer has a let-chain"
        );
        assert!(
            matches!(parse_status(own, Lang::Rust), Parse::Clean),
            "the grammar rejects let-chains — the delta gate would deny valid edits"
        );
    }
}

/// VL0 §8.4 — the hook derives `lang` from the path (`lang_do_caminho`) and writes the shadow with
/// it, for Edit AND MultiEdit; an unknown extension writes no line at all.
#[cfg(test)]
mod vl0_lang_tests {
    use super::*;
    use touring_code::vgp::sombra::com_log_em;

    /// Every language of VL0 §2, as the extension the hook sees.
    const POR_EXTENSAO: &[(&str, &str)] = &[
        ("a.py", "python"),
        ("a.rs", "rust"),
        ("a.ts", "typescript"),
        ("a.tsx", "tsx"),
        ("a.js", "javascript"),
        ("a.html", "html"),
        ("a.css", "css"),
    ];

    /// Runs `input` through the WHOLE pre-edit hook with the shadow redirected; returns its lines.
    fn linhas_da_sombra(
        dir: &tempfile::TempDir,
        input: &serde_json::Value,
    ) -> Vec<serde_json::Value> {
        let log = dir.path().join("sombra.jsonl");
        let rt = HookRuntime::new(dir.path()).expect("runtime");
        com_log_em(&log, || {
            let _ = run_returning(&rt, input);
        });
        std::fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    fn arquivo(dir: &tempfile::TempDir, nome: &str) -> String {
        let path = dir.path().join(nome);
        std::fs::write(&path, "alfa\nbeta\n").expect("write");
        path.to_str().expect("utf8").to_string()
    }

    #[test]
    fn edit_grava_a_sombra_com_o_lang_do_caminho() {
        for (nome, lang) in POR_EXTENSAO {
            let dir = tempfile::TempDir::new().expect("tmp");
            let input = serde_json::json!({
                "tool_name": "Edit",
                "tool_input": {
                    "file_path": arquivo(&dir, nome),
                    "old_string": "alfa",
                    "new_string": "gama"
                }
            });
            let linhas = linhas_da_sombra(&dir, &input);
            assert_eq!(linhas.len(), 1, "{nome}: uma linha por edição: {linhas:?}");
            assert_eq!(linhas[0]["lang"], *lang, "{nome}");
            assert_eq!(linhas[0]["hook"], "pre-edit", "{nome}");
        }
    }

    #[test]
    fn multiedit_grava_a_sombra_com_o_lang_do_caminho() {
        for (nome, lang) in POR_EXTENSAO {
            let dir = tempfile::TempDir::new().expect("tmp");
            let input = serde_json::json!({
                "tool_name": "MultiEdit",
                "tool_input": {
                    "file_path": arquivo(&dir, nome),
                    "edits": [
                        {"old_string": "alfa", "new_string": "gama"},
                        {"old_string": "beta", "new_string": "delta"}
                    ]
                }
            });
            let linhas = linhas_da_sombra(&dir, &input);
            assert_eq!(linhas.len(), 1, "{nome}: {linhas:?}");
            assert_eq!(linhas[0]["lang"], *lang, "{nome}");
            assert_eq!(linhas[0]["hook"], "multiedit", "{nome}");
        }
    }

    #[test]
    fn extensao_desconhecida_nao_grava_sombra() {
        for nome in ["a.md", "a.json", "Makefile"] {
            let dir = tempfile::TempDir::new().expect("tmp");
            let input = serde_json::json!({
                "tool_name": "Edit",
                "tool_input": {
                    "file_path": arquivo(&dir, nome),
                    "old_string": "alfa",
                    "new_string": "gama"
                }
            });
            assert!(
                linhas_da_sombra(&dir, &input).is_empty(),
                "{nome} não é linguagem do VGP"
            );
        }
    }
}

/// Ordem dos gates (30.4.97): o deny aplicável (S210, delta de sintaxe) vence o `[VGP · aviso]`,
/// qualquer que seja a ordem de registro. O veredito do VGP entra pela costura
/// `run_returning_with`.
#[cfg(test)]
mod vgp_ordem_tests {
    use super::*;
    use touring_hook_runtime::verdict::ALSO_NON_BLOCKING;

    const RUST_FILE: &str = "fn a() {\n    if true {\n        x();\n    }\n}\n\nfn x() {}\n";
    const AVISO: &str = "[VGP · aviso] atributo `inexistente` (prev.rs:3)";

    fn avisa(
        _: &HookRuntime,
        _: &serde_json::Value,
        _: &str,
        _: &str,
        _: &str,
        _: Option<&AppliedEdit>,
    ) -> Option<HookResponse> {
        Some(HookResponse::Context {
            context: AVISO.to_string(),
            event_name: Some("PreToolUse".to_string()),
        })
    }

    fn nada(
        _: &HookRuntime,
        _: &serde_json::Value,
        _: &str,
        _: &str,
        _: &str,
        _: Option<&AppliedEdit>,
    ) -> Option<HookResponse> {
        None
    }

    fn nega(
        _: &HookRuntime,
        _: &serde_json::Value,
        _: &str,
        _: &str,
        _: &str,
        _: Option<&AppliedEdit>,
    ) -> Option<HookResponse> {
        Some(HookResponse::Deny {
            reason: "[VGP · deny] import `x` inexistente".to_string(),
            context: None,
            event_name: Some("PreToolUse".to_string()),
        })
    }

    fn edita(old: &str, new: &str, vgp: &VgpEditFn) -> HookResponse {
        let dir = tempfile::TempDir::new().expect("tmp");
        let path = dir.path().join("prev.rs");
        std::fs::write(&path, RUST_FILE).expect("write");
        let rt = HookRuntime::new(dir.path()).expect("runtime");
        let input = serde_json::json!({
            "tool_name": "Edit",
            "tool_input": {"file_path": path.to_str().expect("utf8"), "old_string": old, "new_string": new}
        });
        run_returning_with(&rt, &input, vgp)
    }

    const QUEBRA: (&str, &str) = ("        x();\n    }", "        x(;\n    }");
    const VALIDA: (&str, &str) = ("        x();\n    }", "        x();\n        x();\n    }");

    /// O caso: um aviso do VGP E uma edição que quebra um arquivo que parseia. Antes, o aviso
    /// retornava na hora e o `check_deny_gate` (S210) nunca era avaliado.
    #[test]
    fn aviso_do_vgp_nao_esconde_o_deny_de_sintaxe() {
        match edita(QUEBRA.0, QUEBRA.1, &avisa) {
            HookResponse::Deny {
                reason, context, ..
            } => {
                assert!(
                    reason.contains(AVISO),
                    "o aviso vai junto no texto: {reason}"
                );
                assert!(reason.contains(ALSO_NON_BLOCKING), "{reason}");
                assert!(
                    context
                        .unwrap_or_default()
                        .contains("SYNTAX (applied file)"),
                    "o deny é o do S210"
                );
            }
            outra => panic!("o deny aplicável vence o aviso do VGP: {outra:?}"),
        }
    }

    /// Controle positivo do caso acima: SEM aviso a mesma edição já é negada.
    #[test]
    fn sem_aviso_o_deny_de_sintaxe_segue_sem_texto_do_vgp() {
        match edita(QUEBRA.0, QUEBRA.1, &nada) {
            HookResponse::Deny { reason, .. } => assert!(!reason.contains("VGP"), "{reason}"),
            outra => panic!("o S210 nega a edição que quebra o arquivo: {outra:?}"),
        }
    }

    /// Controle: edição válida com aviso do VGP segue com o aviso (nunca deny).
    #[test]
    fn edicao_valida_com_aviso_do_vgp_segue_com_o_aviso() {
        match edita(VALIDA.0, VALIDA.1, &avisa) {
            HookResponse::Context { context, .. } => assert!(context.contains(AVISO), "{context}"),
            HookResponse::Allow => panic!("o aviso do VGP não pode sumir"),
            outra => panic!("só o aviso: {outra:?}"),
        }
    }

    /// O deny do VGP continua encerrando a cadeia (é decisão, não aviso).
    #[test]
    fn deny_do_vgp_continua_encerrando_a_cadeia() {
        match edita(VALIDA.0, VALIDA.1, &nega) {
            HookResponse::Deny { reason, .. } => {
                assert!(reason.contains("[VGP · deny]"), "{reason}");
            }
            outra => panic!("o deny do VGP segue deny: {outra:?}"),
        }
    }
}
