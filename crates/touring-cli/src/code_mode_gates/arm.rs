//! The A/B arm and its evidence: route offers, durable counts, and the policy that — only when
//! ARMED, and never over a human declaration — picks the presentation the evidence favours.

use serde_json::Value;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use super::*;

pub(in crate::code_mode_gates) mod r3_a2;

/// The written route that the presentation handed to the model, with the arm that produced it.
///
/// FREQUENCY counters say how often a gate acts and nothing about
/// whether acting worked — telemetry without a learning consumer (measured
/// 25/08/2026: zero policies read them; the T3-B counters were the
/// exemplary case, and the whole gate left in S10). This is the missing half: the
/// presentation in force becomes the arm, and the next command becomes the reward.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteOffer {
    /// `native` | `code` | `both` — the presentation in force when the route went out.
    pub mode: String,
    /// When it was offered (secs since UNIX_EPOCH).
    pub offered_secs: u64,
}

pub(super) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// What a code mode program BOUGHT in round-trips.
///
/// `adoption_ratio` answers "did it use the right tool?" and nothing about "was the call
/// worth a round-trip?" — it measures the CHANNEL, not the ECONOMY (finding by Gabriel,
/// 26/08/2026: `scan_class_of` does not recognize `touring run`, so N different
/// and trivial programs add up to N adoptions and zero warnings).
///
/// A program that inspects ONE target with ONE operation fused nothing: it is an
/// atomic call with a shell around it. That is not indiscipline of the model — under the
/// `code` presentation the atomic call is DENIED, so the shell is mandatory.
/// That is why the economy is read as a COST OF THE PRESENTATION and not as a fault of the
/// model: it is the number that keeps the `code` arm from looking great just because
/// everyone obeys, when each obedience costs a round-trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgramShape {
    /// One operation on one target — the shell bought nothing.
    Trivial,
    /// Fused `n` operations/targets into a single call.
    Fused(usize),
}

/// Inspection verbs recognized in the body, in any language.
pub(super) const INSPECTION_VERBS: [&str; 12] = [
    "grep", "rg", "cat", "head", "tail", "sed", "awk", "ls", "wc", "find", "stat", "jq",
];

/// Extracts the body of `touring run … --code '<body>'`.
///
/// Without the body there is nothing to classify, and `None` keeps the previous
/// behavior — never guess the shape of a program that was not read.
pub(crate) fn extract_run_body(cmd: &str) -> Option<&str> {
    let idx = cmd.find("--code")?;
    let resto = &cmd[idx + "--code".len()..];
    let resto = resto.trim_start();
    let aspa = resto.chars().next()?;
    if aspa != '\'' && aspa != '"' {
        return None;
    }
    let corpo = &resto[aspa.len_utf8()..];
    // The FIRST matching quote closes, not the last. `rfind` captured beyond the
    // body when the outer command carried more quotes afterwards — measured live
    // on 26/08: `… --code 'cat X' 2>/dev/null | python3 -c "…print('…')"`
    // returned a body that included the whole pipe, and the `/dev/null` inside
    // it counted as a second target: a trivial program was classified
    // as a fusion. In shell, a single-quoted string does not contain its own
    // quote, so the first one IS the closing one.
    let fim = corpo.find(aspa)?;
    Some(&corpo[..fim])
}

/// Classifies the body by what it fuses. Pure — easier to test than
/// to work around.
///
/// Two axes, and the LARGER decides: how many inspection operations the body executes,
/// and how many distinct targets it touches. A `grep` over three files fused
/// three; three `sed` over the same file fused three. One of each fused
/// nothing.
pub(crate) fn program_shape(body: &str) -> ProgramShape {
    // Match a WORD, not a substring: `body.matches("rg")` counts 1 inside
    // "Cargo.toml", and `cat /a/Cargo.toml` — one file, one operation — was
    // classified as a fusion of two. The test caught it; live measurement would have
    // taken weeks to reveal it, because the wrong number is plausible.
    let ops = body
        .split(|c: char| c.is_whitespace() || matches!(c, ';' | '|' | '&' | '(' | ')' | '"' | '\''))
        .filter(|t| INSPECTION_VERBS.contains(&t.trim_start_matches("$(")))
        .count()
        // `open(` is a function call (Python), not an isolated token.
        + body.matches("open(").count();
    let alvos: std::collections::BTreeSet<&str> = body
        .split(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '(' || c == ')')
        .filter(|t| t.contains('/') && t.len() > 3 && !t.starts_with('-'))
        // Devices and descriptors are not inspection targets: `2>/dev/null`
        // is shell plumbing, and counting it made a one-file program
        // look like it touched two (measured live on 26/08).
        .filter(|t| {
            !t.trim_start_matches(['>', '<', '&', '1', '2'])
                .starts_with("/dev/")
        })
        .collect();
    let fundido = ops.max(alvos.len());
    if fundido <= 1 {
        ProgramShape::Trivial
    } else {
        ProgramShape::Fused(fundido)
    }
}

/// What the model did with the route it received — pure, so that it is easier to
/// test than to work around.
///
/// `Some(1.0)` ran the program; `Some(0.0)` refused the route with a
/// relaxation token; `None` did something else — and that is NOT a verdict. Absence of
/// signal is unknown, never zero: the same fail-closed reading that the `loop` node
/// applies to `NEW_FINDINGS`.
///
/// Order matters: `TOURING_CODE_MODE=native touring run …` **ran** the
/// program. Having relaxed the gate on the way does not undo the fact that the route
/// was taken.
pub fn classify_route_outcome(cmd: &str) -> Option<f64> {
    // `TOURING_T3_FUSE_DISABLED=1` left with T3-B (S10, 27/08): recognizing a
    // kill switch that turns nothing off would teach the model to type it.
    const BYPASS: [&str; 3] = [
        "TOURING_CODE_MODE=native",
        "TOURING_GATE_OK=1",
        "TOURING_CODE_GATES_DISABLED=1",
    ];
    if cmd.contains("touring run ") || cmd.contains("touring exec ") {
        // The route was taken — but did it BUY anything? A program that
        // inspects one target with one operation is an atomic call with a shell around it:
        // the channel is right and the economy is null. Crediting 1.0 here would teach
        // the arm that `code` is great because everyone obeys, when each
        // obedience cost a round-trip (finding by Gabriel, 26/08).
        //
        // 0.5 and not 0.0: under `code` the atomic call is DENIED, so the shell is
        // mandatory. Punishing it as a refusal would blame the model for the policy — the
        // cost belongs to the presentation, and it is the presentation the number must speak about.
        return Some(match extract_run_body(cmd).map(program_shape) {
            Some(ProgramShape::Trivial) => 0.5,
            _ => 1.0,
        });
    }
    if BYPASS.iter().any(|t| cmd.contains(t)) {
        return Some(0.0);
    }
    None
}

/// Stable name of the arm. It is the key of the counters and of the memory — changing it
/// renames the arm and zeroes the accumulated evidence without telling anyone.
pub(crate) fn presentation_label(p: CodeModePresentation) -> &'static str {
    p.label()
}

/// Records that the presentation just handed over a written route.
///
/// SINGLE writer of the `route` field: two sites writing the same offer would be
/// two versions of the same decision, and the first divergence between them would
/// only show up as an arm learning the opposite of what happened — the class of
/// `comentario-afirma-simetria-inexistente`.
pub(crate) fn record_route_offer(project_root: &Path, session: &str, mode: CodeModePresentation) {
    let label = presentation_label(mode);
    let key = format!("{}\u{2}{session}", project_root.display());
    let offered_secs = now_secs();
    route_offers().insert(
        key,
        RouteOffer {
            mode: label.to_string(),
            offered_secs,
        },
    );
    // `bump_arm` is the single writer: it writes the durable view AND the volatile one.
    bump_arm(project_root, label, ArmAxis::Offered);
    // r3 shadow (observe-only, default OFF): closes as `replaced` the offer that was still
    // pending in this session and opens the new one. Nothing here alters what was written above.
    r3_a2::on_offer(project_root, session, label, offered_secs);
}

/// Where the DURABLE evidence of the arm lives, per project.
///
/// Measured on 26/08/2026: `t3_turn_first_passed_count` dropped from 2 to 0 in two
/// minutes, with nothing happening besides a daemon restart. The `gate-metrics`
/// counters are process telemetry — and a policy with a sample floor that
/// read from there would NEVER reach the floor between deploys: it would exist,
/// be armed, and choose nothing. Exactly the failure mode this entire
/// plan catalogs. The decision reads disk; the telemetry keeps observing.
/// Pending route offers, per (project, session).
///
/// Its OWN cache and not a field of `TurnBurst`: keeping it there made
/// `record_route_offer` call `get(...).unwrap_or_default()` in a code-mode
/// deny where the turn often does NOT exist — and the default has
/// `first_passed = false`, so the offer inserted a false turn and the next
/// T3 fusion never fired. The tests caught this as a victim alternating
/// between two neighbors, the signature of shared global state.
///
/// The separation is also semantic: the offer survives `turn_gate_close` on
/// purpose (the end of the turn is when the outcome becomes observable, not when it is
/// forgotten) — a sign that it was never turn state.
pub(super) fn route_offers() -> &'static moka::sync::Cache<String, RouteOffer> {
    static C: OnceLock<moka::sync::Cache<String, RouteOffer>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(1024)
            .time_to_live(Duration::from_secs(60))
            .build()
    })
}

pub(super) fn arm_store_path(project_root: &Path) -> std::path::PathBuf {
    project_root.join(".claude/touring/code_mode_arm.json")
}

/// DURABLE evidence of the measurement families without their own file — S3 (inspection
/// burst) and pillar-induction. R2 (29/08, Gabriel's order): the KPIs
/// `inspect_burst_share` and `pillar_induction_ratio` read process counters
/// that reset on every daemon restart (3 deploys in a day = 3 blackouts
/// of the sample) — the same class that made the arm decision read
/// `code_mode_arm.json`. Same contract as the arm: unreadable read ⇒ zeros (the
/// consumer stays quiet), write failure ⇒ warn, never silence.
pub(super) fn durable_evidence_path(project_root: &Path) -> std::path::PathBuf {
    project_root.join(".claude/touring/durable_gate_evidence.json")
}

/// The whole file as JSON; absent/corrupt ⇒ empty object (evidence
/// that cannot be read is evidence that does not exist).
pub(crate) fn read_durable_evidence(project_root: &Path) -> serde_json::Value {
    std::fs::read_to_string(durable_evidence_path(project_root))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}))
}

/// `+1` in `<family>.<field>`, read-modify-write on the durable file. Called
/// beside the corresponding volatile counter — the live one feeds the session's
/// `gate-metrics`, the durable one feeds the KPI that needs DAYS.
pub(crate) fn bump_durable_evidence(project_root: &Path, family: &str, field: &str) {
    let mut v = read_durable_evidence(project_root);
    let Some(obj) = v.as_object_mut() else { return };
    let fam = obj
        .entry(family.to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !fam.is_object() {
        *fam = serde_json::json!({});
    }
    let Some(f) = fam.as_object_mut() else { return };
    let n = f.get(field).and_then(Value::as_u64).unwrap_or(0);
    f.insert(field.to_string(), serde_json::json!(n + 1));
    let path = durable_evidence_path(project_root);
    // The write below reports the failure; this names its cause (the directory).
    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        tracing::warn!(target: "touring::kpi", "cannot create {}: {e}", dir.display());
    }
    if let Err(e) = std::fs::write(&path, v.to_string()) {
        tracing::warn!(
            target: "touring::kpi",
            "durable gate evidence write failed at {}: {e}",
            path.display()
        );
    }
}

/// Short TTL: `code_mode_presentation` runs on every PreToolUse, and reading the file
/// on every call would trade a durability problem for a latency one.
pub(super) fn arm_cache() -> &'static moka::sync::Cache<String, ArmCounts> {
    static C: OnceLock<moka::sync::Cache<String, ArmCounts>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(64)
            .time_to_live(Duration::from_secs(5))
            .build()
    })
}

pub(super) const ARM_NAMES: [&str; 3] = ["native", "both", "code"];

pub(super) fn arm_index(mode: &str) -> Option<usize> {
    ARM_NAMES.iter().position(|n| *n == mode)
}

/// Reads the file. Absent, unreadable or corrupt ⇒ zeros: evidence that cannot
/// be read is evidence that does not exist, and zeros keep the policy quiet (which is
/// the safe behavior), instead of inventing a choice.
/// `(offered, taken, economical)` per arm.
///
/// Three numbers and not two because obedience and economy are DIFFERENT axes:
/// `taken/offered` answers "did they follow the route?" (the channel) and
/// `economical/taken` answers "did the route buy round-trips?" (the economy).
/// Merging them into one would make an obeyed and expensive arm look excellent — the
/// defect Gabriel pointed out in `adoption_ratio`.
pub(crate) type ArmCounts = [(u64, u64, u64); 3];

pub(super) fn read_arm_file(project_root: &Path) -> ArmCounts {
    let mut out = [(0u64, 0u64, 0u64); 3];
    let Ok(txt) = std::fs::read_to_string(arm_store_path(project_root)) else {
        return out;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) else {
        return out;
    };
    for (i, nome) in ARM_NAMES.iter().enumerate() {
        if let Some(par) = v.get(*nome) {
            out[i].0 = par.get("offered").and_then(Value::as_u64).unwrap_or(0);
            out[i].1 = par.get("followed").and_then(Value::as_u64).unwrap_or(0);
            out[i].2 = par.get("economical").and_then(Value::as_u64).unwrap_or(0);
        }
    }
    out
}

pub(super) fn write_arm_file(project_root: &Path, counts: &ArmCounts) {
    let path = arm_store_path(project_root);
    // The write below reports the failure; this names its cause (the directory).
    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        tracing::warn!(target: "touring::kpi", "cannot create {}: {e}", dir.display());
    }
    let mut obj = serde_json::Map::new();
    for (i, nome) in ARM_NAMES.iter().enumerate() {
        obj.insert(
            (*nome).to_string(),
            serde_json::json!({
                "offered": counts[i].0,
                "followed": counts[i].1,
                "economical": counts[i].2,
            }),
        );
    }
    // A write failure CANNOT be silent. If the directory is not
    // writable, the evidence stays at zero forever and the policy goes quiet —
    // indistinguishable from "no sample yet". A `warn` is the difference between
    // a diagnosable bug and yet another mechanism that exists and never fires.
    if let Err(e) = std::fs::write(&path, serde_json::Value::Object(obj).to_string()) {
        tracing::warn!(
            path = %path.display(),
            error = %e,
            "P2: evidência do braço code-mode não pôde ser persistida — a política \
             ficará calada e isso NÃO é ausência de amostra"
        );
    }
}

/// Durable evidence of the project, with a short cache.
pub(crate) fn arm_counts_full(project_root: &Path) -> ArmCounts {
    let key = project_root.display().to_string();
    if let Some(c) = arm_cache().get(&key) {
        return c;
    }
    let counts = read_arm_file(project_root);
    arm_cache().insert(key, counts);
    counts
}

/// The `(offered, taken)` pair that the POLICY reads. The economy is reported
/// (KPI) but does not yet choose an arm: changing the choice criterion is Gabriel's
/// decision, not a side effect of having started to measure.
pub(crate) fn arm_counts_durable(project_root: &Path) -> [(u64, u64); 3] {
    let full = arm_counts_full(project_root);
    [
        (full[0].0, full[0].1),
        (full[1].0, full[1].1),
        (full[2].0, full[2].1),
    ]
}

/// Increments an axis of the arm and persists it.
///
/// **Budget**: this file declares `< 1ms (byte scan + moka)` for the
/// hot path, and here there is disk I/O. The only two callers are
/// INSIDE deny branches (the T3 fusion and the code-mode deny), so the read +
/// write of ~200 bytes is paid only when a gate acts — never on calls
/// that pass, which are the overwhelming majority. The budget of the common path stays
/// intact; whoever pays is the rare event that produces the evidence.
///
/// Re-reads the file before adding (not the cache): two per-project daemons or
/// two sessions in the same project would add on top of a stale copy and one of the two
/// counts would vanish. There is still a race window between the read and
/// the write — acceptable for an evidence counter, and declared here instead
/// of discovered later as a number that does not add up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArmAxis {
    /// The presentation handed over a route.
    Offered,
    /// The model took the route (the channel).
    Followed,
    /// The route taken fused round-trips (the economy).
    Economical,
}

pub(crate) fn bump_arm(project_root: &Path, mode: &str, eixo: ArmAxis) {
    let Some(i) = arm_index(mode) else { return };
    let mut counts = read_arm_file(project_root);
    // SINGLE writer of the arm count. The DURABLE evidence (the file, which
    // survives the restart and feeds the policy) and the VOLATILE one (the atoms that
    // `gate-metrics -j` publishes) are two VIEWS OF THE SAME decision — not two
    // independent records. Until 26/08 they were kept by two adjacent call
    // sites: it worked by neighborhood, not by construction, and a
    // third offer point that called only one of the two would make the views
    // diverge silently (the policy learning from a number the
    // operator does not see in `gate-metrics`, or the reverse). It is the same rule that the
    // docstring of `record_route_offer` already imposed on the `route` field, applied
    // to the neighbor it did not cover. Proven by
    // `bump_arm_mantem_duravel_e_volatil_em_sincronia`.
    match eixo {
        ArmAxis::Offered => {
            counts[i].0 = counts[i].0.saturating_add(1);
            touring_foundation::gate_metrics_snapshot::record_code_mode_arm_offered(mode);
        }
        ArmAxis::Followed => {
            counts[i].1 = counts[i].1.saturating_add(1);
            touring_foundation::gate_metrics_snapshot::record_code_mode_arm_followed(mode);
        }
        // Economy has no corresponding atomic counter: `gate-metrics`
        // publishes the channel (offered/taken), and the economy is derived in the KPI from
        // the shape of the program. With no volatile pair, nothing to synchronize.
        ArmAxis::Economical => counts[i].2 = counts[i].2.saturating_add(1),
    }
    write_arm_file(project_root, &counts);
    arm_cache().insert(project_root.display().to_string(), counts);
}

/// P2 (decision (b), 26/08/2026) — the policy only acts when ARMED.
///
/// Default-OFF, like `f7_actuator_armed`: without the env, no signal is read and the
/// behavior is byte-identical to the previous one. Arming is a human decision, and it is the
/// promotion gate that the MSR survey requires beyond the verifier's frontier —
/// here the signal (did the model follow the route?) is produced by the directed
/// system itself, so autonomy without a human would degrade with iteration.
pub(super) fn code_mode_arm_armed() -> bool {
    std::env::var("TOURING_CODE_MODE_ARM_ARMED").is_ok_and(|v| v != "0")
}

/// The final precedence, PURE: human declaration > armed policy > default.
///
/// Extracted from `code_mode_presentation` because the invariant that matters most
/// here — "the policy never overrides what the human declared" — was only
/// testable by mutating `TOURING_CODE_MODE_ARM_ARMED` in the process. And the var came to
/// be read also by the fusion path, so that mutation leaked into
/// concurrent tests: `turn_gate_ignora_o_que_nao_e_fanout` failed in parallel and
/// passed with `--test-threads=1` (the signature of global state). A pure predicate
/// is easier to test than to work around, and has no victim.
pub(crate) fn resolve_with_policy(
    declarado: Option<CodeModePresentation>,
    armada: bool,
    counts: [(u64, u64); 3],
) -> CodeModePresentation {
    if let Some(d) = declarado {
        return d;
    }
    if armada && let Some(escolhido) = arm_choice_from_counts(counts) {
        return escolhido;
    }
    CodeModePresentation::Both
}

/// Sample floor per arm — SHARED with the KPI that reports the evidence.
///
/// A copy of the number on each side would diverge at the first adjustment, and then the
/// dashboard would say "ready to promote" while the policy is still quiet (or the
/// reverse). The judge and the meter must read the same predicate.
pub(crate) const ARM_MIN_SAMPLE: u64 = 20;

/// Chooses the arm by the measured evidence — or `None` when it is thin.
///
/// Pure, so that it is easier to test than to work around. Two rules:
///
/// * **Sample floor** (`ARM_MIN_SAMPLE`): below it "the best rate" is
///   sampling noise, and promoting noise is exactly what `default-OFF +
///   measured promotion` exists to prevent.
/// * **Comparison requires at least TWO arms** with a sample. A single arm
///   above the floor is not a choice: it is the only one that exists, and "choosing" the
///   only one observed would be confirming the current configuration and calling it
///   learning.
///
/// Exploration — deliberately offering an under-sampled arm to learn
/// about it — is NOT here on purpose: it would mean degrading the
/// presentation to harvest data, which is another human decision.
pub(crate) fn arm_choice_from_counts(counts: [(u64, u64); 3]) -> Option<CodeModePresentation> {
    let arms = [
        CodeModePresentation::Native,
        CodeModePresentation::Both,
        CodeModePresentation::Code,
    ];
    let mut elegiveis: Vec<(f64, CodeModePresentation)> = Vec::new();
    for (i, (offered, followed)) in counts.iter().enumerate() {
        if *offered >= ARM_MIN_SAMPLE {
            elegiveis.push((*followed as f64 / *offered as f64, arms[i]));
        }
    }
    if elegiveis.len() < 2 {
        return None;
    }
    elegiveis.sort_by(|a, b| b.0.total_cmp(&a.0));
    let (melhor, vice) = (elegiveis[0], elegiveis[1]);
    // Minimum margin: a difference within the noise is NOT evidence. Without it,
    // `max_by` would break the tie by vector order and the presentation would change by
    // chance — switching a project's gate on a tiebreak is the opposite of "promotion
    // by measured evidence". Tie ⇒ silence ⇒ the declared default prevails.
    const ARM_MIN_MARGIN: f64 = 0.05;
    if melhor.0 - vice.0 < ARM_MIN_MARGIN {
        return None;
    }
    Some(melhor.1)
}

// ── The feedback adapters: from the post-tool payload to the arm ─────────────────────────

/// Claims the pending route of this (project, session) — ONCE.
///
/// Same discipline as the decision and case ledgers: a claimed offer
/// disappears, so that two reads never credit the same decision twice. The
/// session comes from the SAME `session_key` as the pre and the close, or pre and post would be
/// talking about different turns that look the same.
pub fn take_route_offer(project_root: &Path, payload: &Value) -> Option<RouteOffer> {
    let key = format!("{}\u{2}{}", project_root.display(), session_key(payload));
    let offer = route_offers().get(&key)?;
    route_offers().invalidate(&key);
    Some(offer)
}

/// Seeds a route offer — for tests only, through the SAME key as the runtime.
///
/// A test that built the key on its own would prove that its formula
/// works, not that the gate's does.
#[cfg(test)]
pub fn turn_ledger_insert_for_test(project_root: &Path, payload: &Value, offer: RouteOffer) {
    let key = format!("{}\u{2}{}", project_root.display(), session_key(payload));
    route_offers().insert(key, offer);
}

/// Claims the pending route **only when there is a verdict** — the
/// (arm, reward) pair ready for deposit.
///
/// Classifying before claiming is not a style preference: a claimed offer
/// disappears. With the order inverted, the first PostToolUse of ANY
/// tool — a `Read`, which carries no command — consumed the offer without a
/// verdict, and the `touring run` right after found nothing to credit.
/// Leaving the rule to the caller would lose it at the second caller; here it is
/// structural.
pub fn claim_route_reward(
    project_root: &Path,
    payload: &Value,
    cmd: &str,
) -> Option<(RouteOffer, f64)> {
    let decision = claim_route_reward_decision(project_root, payload, cmd);
    // r3 shadow (observe-only, default OFF): the decision above is already made and is returned
    // by value, untouched — the shadow only OBSERVES it to close the offer's lifecycle.
    r3_a2::on_outcome(
        project_root,
        payload,
        cmd,
        decision.as_ref().map(|(_, v)| *v),
    );
    decision
}

/// The production decision of [`claim_route_reward`] — the original body, without the shadow.
fn claim_route_reward_decision(
    project_root: &Path,
    payload: &Value,
    cmd: &str,
) -> Option<(RouteOffer, f64)> {
    let value = classify_route_outcome(cmd)?;
    let offer = take_route_offer(project_root, payload)?;
    if value > 0.0 {
        bump_arm(project_root, &offer.mode, ArmAxis::Followed);
        // 1.0 is the program that FUSED; 0.5 is the shell around a single call.
        if value >= 1.0 {
            bump_arm(project_root, &offer.mode, ArmAxis::Economical);
        }
    }
    Some((offer, value))
}
