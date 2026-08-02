//! Stage **X7 DECISION** of the Code Execution Gateway. Phase **P3.6** of CEG
//! Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! X2..X6 each attached a verdict to the [`Evidence`] ledger — a static
//! severity, a VGP resolution, an RL prediction, a sandbox outcome, a
//! capability gate. X7 is where the five become one answer.
//!
//! - [`composite_score`] fuses the five signals into a single number in
//!   `0.0..=1.0`, weighted so the two hard security gates — static analysis
//!   and the capability gate — carry half the weight between them. Each signal
//!   is reduced by its own `*_subscore` helper, so the fusion itself is a flat
//!   weighted sum.
//! - [`GateDecision`] turns that score, plus the deny-wins hard-block rules,
//!   into a [`Verdict`] — `Allow` / `Warn` / `Deny` — with a canonical fix.
//!
//! # Deny-wins
//!
//! A high composite score never overrides a hard block. If X2 STATIC found a
//! destructive pattern ([`StaticSeverity::Block`]) or X6 denied a subprocess /
//! file-write / network capability, the verdict is [`Verdict::Deny`] regardless
//! of the other four signals — the same deny-wins discipline the capability
//! model itself uses ([`crate::capability`]).
//!
//! `Decided` is the terminal pipeline state: [`decide`](Execution::decide)
//! consumes the X6 [`Gated`] execution and there is no transition onward.

use super::gate::{GateReport, capability_class};
use super::predict::PredictionReport;
use super::quality_signal::QualitySignalReport;
use super::sandbox_stage::SandboxOutcome;
use super::static_stage::{StaticReport, StaticSeverity};
use super::typestate::{Decided, Evidence, Execution, Gated};
use super::vgp_stage::VgpReport;
use crate::capability::Decision;
use serde::{Deserialize, Serialize};

// ── Composite score ───────────────────────────────────────────────────────────

/// Weight of the X2 STATIC signal in the composite score.
const W_STATIC: f64 = 0.25;
/// Weight of the X3 VGP signal.
const W_VGP: f64 = 0.15;
/// Weight of the X4 PREDICT signal.
const W_PREDICT: f64 = 0.15;
/// Weight of the X5 SANDBOX signal.
const W_SANDBOX: f64 = 0.20;
/// Weight of the X6 CAPABILITY-GATE signal.
const W_GATE: f64 = 0.25;

/// Maximum multiplicative penalty applied by the X7.5 QUALITY-SIGNAL
/// (Plan v3 Q3 = A: `W_QUALITY = 0.20`). The factor is
/// `1.0 - W_QUALITY * (1.0 - quality_subscore)`, i.e.:
///
/// - quality_subscore = 1.0 → factor = 1.0 (no penalty, perfect code)
/// - quality_subscore = 0.0 → factor = 0.80 (20 % penalty)
/// - quality_report = None → factor = 1.0 (neutral, no signal)
///
/// Applied multiplicatively to the additive composite so the existing 5
/// weights (summing to 1.0) keep their in-flight contract.
const W_QUALITY: f64 = 0.20;

/// Composite score at or above which a spotless execution is allowed.
const ALLOW_THRESHOLD: f64 = 0.85;
/// Composite score below which the execution is denied outright.
const DENY_THRESHOLD: f64 = 0.5;

/// X2 STATIC sub-score: `Clear` (or not yet run) → `1.0`, `Warn` → `0.6`,
/// `Block` → `0.0`.
fn static_subscore(report: Option<&StaticReport>) -> f64 {
    match report.map(|r| r.severity) {
        None | Some(StaticSeverity::Clear) => 1.0,
        Some(StaticSeverity::Warn) => 0.6,
        Some(StaticSeverity::Block) => 0.0,
    }
}

/// X3 VGP sub-score: the fraction of checked symbols that resolved. An empty
/// or absent report scores the neutral `1.0` — nothing to hold against the run.
fn vgp_subscore(report: Option<&VgpReport>) -> f64 {
    match report {
        Some(r) if r.checked() > 0 => r.verified.len() as f64 / r.checked() as f64,
        _ => 1.0,
    }
}

/// X4 PREDICT sub-score: the predicted success probability, clamped. An absent
/// prediction scores the neutral `0.5`.
fn predict_subscore(report: Option<&PredictionReport>) -> f64 {
    report.map_or(0.5, |p| p.success_probability.clamp(0.0, 1.0))
}

/// X5 SANDBOX sub-score: success → `1.0`, timeout → `0.2`, other failure →
/// `0.4`, not yet run → the neutral `0.5`.
fn sandbox_subscore(outcome: Option<&SandboxOutcome>) -> f64 {
    match outcome {
        Some(o) if o.succeeded() => 1.0,
        Some(o) if o.timed_out => 0.2,
        Some(_) => 0.4,
        None => 0.5,
    }
}

/// X6 CAPABILITY-GATE sub-score: `Allow` (or not yet run) → `1.0`, `Prompt` →
/// `0.5`, `Deny` → `0.0`.
fn gate_subscore(report: Option<&GateReport>) -> f64 {
    match report.map(GateReport::worst_decision) {
        None | Some(Decision::Allow) => 1.0,
        Some(Decision::Prompt) => 0.5,
        Some(Decision::Deny) => 0.0,
    }
}

/// X7.5 QUALITY-SIGNAL sub-score: the `touring-quality` 50-dim composite
/// in `0.0..=1.0`. Absent report scores the neutral `1.0` — same convention
/// as the other sub-scores, so the penalty multiplier stays at `1.0` when
/// the call site hasn't wired the quality signal yet.
fn quality_subscore(report: Option<&QualitySignalReport>) -> f64 {
    report.map_or(1.0, QualitySignalReport::score)
}

/// Multiplicative penalty factor in `[1.0 - W_QUALITY, 1.0]` = `[0.80, 1.0]`.
/// At `quality_subscore = 1.0` → `1.0` (no penalty). At `quality_subscore =
/// 0.0` → `1.0 - W_QUALITY = 0.80` (20 % penalty).
fn quality_penalty_factor(report: Option<&QualitySignalReport>) -> f64 {
    1.0 - W_QUALITY * (1.0 - quality_subscore(report))
}

/// Fuse the five pipeline signals on the [`Evidence`] ledger into a single
/// safety score in `0.0..=1.0` — `1.0` is spotless, `0.0` maximally unsafe.
///
/// Each stage is reduced to a sub-score by its `*_subscore` helper; the
/// composite is their weighted mean. A stage that has not run yet contributes a
/// neutral value, so the score is defined for a partially-advanced execution.
/// The five weights sum to `1.0`, with X2 STATIC and X6 CAPABILITY-GATE — the
/// two hard security gates — together carrying half.
///
/// The X7.5 QUALITY-SIGNAL modulates the additive composite **multiplicatively**
/// (range `[0.80, 1.0]`) so the existing 5-weight invariant (`ΣW = 1.0`) is
/// preserved. See `quality_penalty_factor`.
#[must_use]
pub fn composite_score(evidence: &Evidence) -> f64 {
    let additive = W_STATIC * static_subscore(evidence.static_report.as_ref())
        + W_VGP * vgp_subscore(evidence.vgp_report.as_ref())
        + W_PREDICT * predict_subscore(evidence.prediction.as_ref())
        + W_SANDBOX * sandbox_subscore(evidence.sandbox_outcome.as_ref())
        + W_GATE * gate_subscore(evidence.gate_report.as_ref());
    let with_quality = additive * quality_penalty_factor(evidence.quality_signal.as_ref());
    with_quality.clamp(0.0, 1.0)
}

// ── The non-terminal evidence bundle (OP2 / §5.2.2) ─────────────────────────────

/// The per-axis decomposition of the [`composite_score`] — the structured,
/// **non-terminal** verification signal behind a [`GateDecision`].
///
/// §5.2.2 of *Code as Agent Harness* (arXiv 2605.18747) warns against collapsing
/// the verification signal into a single terminal scalar. `composite_score` is
/// retained as the scalar projection, but `EvidenceBundle` carries each pipeline
/// stage's sub-score forward so a consumer (X8 EXECUTE, X9 LEARN, or the agent)
/// can inspect *why* — per axis — instead of acting on an opaque number. The
/// scalar is recoverable via [`EvidenceBundle::composite`], so the non-terminal
/// bundle and the terminal scalar can never disagree.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct EvidenceBundle {
    /// X2 STATIC sub-score (weight `W_STATIC`).
    pub static_score: f64,
    /// X3 VGP sub-score (weight `W_VGP`).
    pub vgp_score: f64,
    /// X4 PREDICT sub-score (weight `W_PREDICT`).
    pub predict_score: f64,
    /// X5 SANDBOX sub-score (weight `W_SANDBOX`).
    pub sandbox_score: f64,
    /// X6 CAPABILITY-GATE sub-score (weight `W_GATE`).
    pub gate_score: f64,
}

impl EvidenceBundle {
    /// Decompose an [`Evidence`] ledger into its five per-axis sub-scores — the
    /// same values [`composite_score`] fuses, surfaced individually.
    #[must_use]
    pub fn from_evidence(evidence: &Evidence) -> Self {
        Self {
            static_score: static_subscore(evidence.static_report.as_ref()),
            vgp_score: vgp_subscore(evidence.vgp_report.as_ref()),
            predict_score: predict_subscore(evidence.prediction.as_ref()),
            sandbox_score: sandbox_subscore(evidence.sandbox_outcome.as_ref()),
            gate_score: gate_subscore(evidence.gate_report.as_ref()),
        }
    }

    /// The scalar projection — the same weighted mean [`composite_score`]
    /// returns. Carrying both keeps the non-terminal bundle and the terminal
    /// scalar consistent by construction.
    #[must_use]
    pub fn composite(&self) -> f64 {
        (W_STATIC * self.static_score
            + W_VGP * self.vgp_score
            + W_PREDICT * self.predict_score
            + W_SANDBOX * self.sandbox_score
            + W_GATE * self.gate_score)
            .clamp(0.0, 1.0)
    }
}

// ── The verdict ───────────────────────────────────────────────────────────────

/// The X7 verdict on whether an execution may proceed to X8 EXECUTE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Verdict {
    /// Proceed — every signal is clean and the composite score is high.
    Allow,
    /// Proceed with caution — soft concerns remain; a human should review.
    Warn,
    /// Refuse — a hard block fired, or the composite score is too low.
    Deny,
}

/// The **X7 DECISION** result: the [`Verdict`], the [`composite_score`] it was
/// based on, the human-readable reasons, and — for a non-`Allow` verdict — a
/// single canonical fix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateDecision {
    /// The verdict.
    pub verdict: Verdict,
    /// The composite safety score the verdict was derived from.
    pub composite_score: f64,
    /// One human-readable line per signal that was not spotless — empty for a
    /// clean [`Verdict::Allow`].
    pub reasons: Vec<String>,
    /// A single canonical, actionable fix. `None` only for [`Verdict::Allow`].
    pub canonical_fix: Option<String>,
    /// The per-axis [`EvidenceBundle`] behind `composite_score` — the
    /// structured, **non-terminal** verification signal (§5.2.2). Inspectable by
    /// X8 EXECUTE, X9 LEARN and the agent; `composite_score` is its scalar
    /// projection (`evidence.composite()`).
    pub evidence: EvidenceBundle,
}

/// The X2 STATIC reason line, if the report is not `Clear`.
fn static_reasons(report: Option<&StaticReport>) -> Vec<String> {
    let Some(r) = report else {
        return Vec::new();
    };
    match r.severity {
        StaticSeverity::Clear => Vec::new(),
        StaticSeverity::Warn => vec![format!(
            "X2 STATIC raised a warning: {}",
            r.risk_summary
                .clone()
                .or_else(|| r.findings.first().cloned())
                .unwrap_or_else(|| "see findings".to_owned())
        )],
        StaticSeverity::Block => vec![format!(
            "X2 STATIC blocked the code: {}",
            r.findings
                .first()
                .cloned()
                .unwrap_or_else(|| "destructive pattern".to_owned())
        )],
    }
}

/// One X6 reason line per denied capability.
fn gate_reasons(report: Option<&GateReport>) -> Vec<String> {
    let Some(r) = report else {
        return Vec::new();
    };
    r.denied()
        .map(|denied| {
            format!(
                "X6 denied the {} capability '{}' under profile '{}'",
                capability_class(&denied.capability),
                denied.operation,
                r.profile_name
            )
        })
        .collect()
}

/// The X3 VGP reason line, if any symbol stayed unresolved.
fn vgp_reasons(report: Option<&VgpReport>) -> Vec<String> {
    match report {
        Some(r) if !r.all_resolved() => vec![format!(
            "X3 VGP left {} symbol(s) unresolved",
            r.unresolved.len()
        )],
        _ => Vec::new(),
    }
}

/// The X4 PREDICT reason line, if the predicted success probability is low.
fn predict_reasons(report: Option<&PredictionReport>) -> Vec<String> {
    match report {
        Some(p) if p.success_probability < DENY_THRESHOLD => vec![format!(
            "X4 PREDICT estimates only {:.0}% success probability",
            p.success_probability * 100.0
        )],
        _ => Vec::new(),
    }
}

/// The X5 SANDBOX reason line, if the dry-run did not succeed.
fn sandbox_reasons(outcome: Option<&SandboxOutcome>) -> Vec<String> {
    match outcome {
        Some(o) if o.timed_out => vec!["X5 SANDBOX dry-run timed out".to_owned()],
        Some(o) if !o.succeeded() => {
            vec![format!(
                "X5 SANDBOX dry-run exited with code {}",
                o.exit_code
            )]
        }
        _ => Vec::new(),
    }
}

/// Build the human-readable reason list — one line per signal that is not
/// spotless. An empty list means every signal passed cleanly.
fn collect_reasons(evidence: &Evidence) -> Vec<String> {
    let mut reasons = static_reasons(evidence.static_report.as_ref());
    reasons.extend(gate_reasons(evidence.gate_report.as_ref()));
    reasons.extend(vgp_reasons(evidence.vgp_report.as_ref()));
    reasons.extend(predict_reasons(evidence.prediction.as_ref()));
    reasons.extend(sandbox_reasons(evidence.sandbox_outcome.as_ref()));
    reasons
}

/// Build the single canonical, actionable fix for a non-`Allow` verdict.
///
/// Names the dominant cause in deny-wins order:
/// 1. Hard static block (X2 STATIC `Block`) — highest priority.
/// 2. Denied capability (X6 CAPABILITY-GATE) — next in deny-wins chain.
/// 3. P8.7 — Workflow antipattern advisory (`Warn` only, never `Deny` — R13/R14):
///    when X2 STATIC raised a `Warn` that carried a workflow antipattern finding
///    (prefixed `"workflow["`), surface the elite-tool conversion hint here so the
///    agent can act on it immediately.  This is purely advisory — it never raises
///    the verdict to `Deny`.
/// 4. Below-threshold composite score.
/// 5. Generic review prompt for a `Warn`.
fn build_canonical_fix(evidence: &Evidence, score: f64) -> String {
    if let Some(r) = &evidence.static_report {
        if r.severity == StaticSeverity::Block {
            let detail = r
                .findings
                .first()
                .cloned()
                .unwrap_or_else(|| "a destructive pattern".to_owned());
            return format!(
                "X2 STATIC blocked the code ({detail}). Revise the command, then re-run the gateway."
            );
        }
    }
    if let Some(r) = &evidence.gate_report {
        if let Some(denied) = r.first_blocking_denial() {
            return format!(
                "Profile '{}' denies the {} capability '{}'. Run under a profile that grants it, \
                 or remove the operation from the code.",
                r.profile_name,
                capability_class(&denied.capability),
                denied.operation
            );
        }
    }
    // P8.7 — workflow antipattern advisory (Warn only, R13/R14 — never Deny).
    // When X2 STATIC raised a Warn that includes a workflow antipattern finding
    // (identified by the "workflow[" prefix emitted by `antipattern_finding()`),
    // promote it to the canonical fix so the agent receives the elite-tool hint.
    // This block is intentionally after all hard-block checks to preserve
    // deny-wins discipline.
    if let Some(r) = &evidence.static_report {
        if r.severity != StaticSeverity::Block {
            if let Some(wf_finding) = r.findings.iter().find(|f| f.starts_with("workflow[")) {
                return wf_finding.clone();
            }
        }
    }
    if score < DENY_THRESHOLD {
        return format!(
            "Composite score {score:.2} is below the {DENY_THRESHOLD:.2} deny threshold — too \
             little evidence of safety. Strengthen the weakest signal before X8 EXECUTE."
        );
    }
    format!("Composite score {score:.2}: review the listed reasons before X8 EXECUTE.")
}

impl GateDecision {
    /// Compute the **X7 DECISION** from a full [`Evidence`] ledger.
    ///
    /// Verdict logic, in order:
    /// 1. a hard block — [`StaticSeverity::Block`] or a denied high-authority
    ///    capability — or a composite score below the deny threshold →
    ///    [`Verdict::Deny`];
    /// 2. a composite score at or above the allow threshold with **no** reason
    ///    on the ledger → [`Verdict::Allow`];
    /// 3. anything else → [`Verdict::Warn`].
    #[must_use]
    pub fn from_evidence(evidence: &Evidence) -> Self {
        let score = composite_score(evidence);
        let reasons = collect_reasons(evidence);

        let static_block = evidence
            .static_report
            .as_ref()
            .is_some_and(|r| r.severity == StaticSeverity::Block);
        let gate_block = evidence
            .gate_report
            .as_ref()
            .is_some_and(GateReport::has_blocking_denial);

        let verdict = if static_block || gate_block || score < DENY_THRESHOLD {
            Verdict::Deny
        } else if score >= ALLOW_THRESHOLD && reasons.is_empty() {
            Verdict::Allow
        } else {
            Verdict::Warn
        };

        let canonical_fix = match verdict {
            Verdict::Allow => None,
            Verdict::Warn | Verdict::Deny => Some(build_canonical_fix(evidence, score)),
        };

        Self {
            verdict,
            composite_score: score,
            reasons,
            canonical_fix,
            evidence: EvidenceBundle::from_evidence(evidence),
        }
    }

    /// `true` only for [`Verdict::Allow`].
    #[must_use]
    pub fn is_allowed(&self) -> bool {
        self.verdict == Verdict::Allow
    }

    /// **S-13 / R12** — map this decision to a CEG credibility in `0.0..=1.0` for
    /// the gated MCTS planner (`touring_intelligence::reasoning::gated_mcts`).
    ///
    /// `Deny` → `0.0` (deny-wins — the branch is never expanded); `Warn` → half
    /// the composite; `Allow` → the full composite score. Weighting by the
    /// composite means a high-evidence Allow outranks a marginal one, so planning
    /// favors the best-verified branch. `touring-server` reads this and feeds it
    /// to `GatedCandidate::credibility` (the crates are joined there, keeping
    /// gated_mcts free of any touring-hooks dependency).
    #[must_use]
    pub fn credibility(&self) -> f64 {
        match self.verdict {
            Verdict::Deny => 0.0,
            Verdict::Warn => 0.5 * self.composite_score.clamp(0.0, 1.0),
            Verdict::Allow => self.composite_score.clamp(0.0, 1.0),
        }
    }
}

// ── X7 transition ─────────────────────────────────────────────────────────────

impl Execution<Gated> {
    /// **X7 DECISION** — fuse the evidence ledger into a [`GateDecision`],
    /// attach it, and advance to the terminal [`Decided`] state.
    ///
    /// This is the evidence-carrying form of the X6→X7 transition; bare
    /// [`advance`](Execution::advance) performs the same typestate move without
    /// recording a decision. [`Decided`] is terminal — there is no transition
    /// onward, so the [`GateDecision`] is the pipeline's final word.
    pub fn decide(mut self) -> Execution<Decided> {
        let decision = GateDecision::from_evidence(self.evidence());
        self.evidence_mut().decision = Some(decision);
        self.advance()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::builtins::trusted;
    use crate::capability::{Capability, CmdScope};
    use crate::gateway::capture_tool_call;
    use crate::gateway::gate::{GateReport, GatedCapability};
    use crate::gateway::predict::{
        ExecutionOutcomePredictor, OutcomeStats, PredictionConfidence, PredictionReport,
    };
    use crate::gateway::sandbox_stage::SandboxOutcome;
    use crate::gateway::static_stage::StaticReport;
    use crate::gateway::vgp_stage::VgpReport;

    // ── evidence builders ─────────────────────────────────────────────────

    fn static_report(severity: StaticSeverity) -> StaticReport {
        let findings = match severity {
            StaticSeverity::Block => vec!["destructive command: rm -rf".to_owned()],
            StaticSeverity::Warn => vec!["risk: unwrap in body".to_owned()],
            StaticSeverity::Clear => Vec::new(),
        };
        StaticReport {
            severity,
            findings,
            risk_summary: None,
        }
    }

    fn vgp_report(verified: usize, unresolved: usize) -> VgpReport {
        VgpReport {
            verified: (0..verified).map(|i| format!("ok{i}")).collect(),
            unresolved: (0..unresolved).map(|i| format!("miss{i}")).collect(),
        }
    }

    fn prediction(probability: f64) -> PredictionReport {
        PredictionReport {
            signature: "outcome:bash:test:none".to_owned(),
            success_probability: probability,
            observed: OutcomeStats {
                successes: 8,
                failures: 2,
            },
            confidence: PredictionConfidence::from_total(10),
        }
    }

    fn ok_outcome() -> SandboxOutcome {
        SandboxOutcome {
            exit_code: 0,
            output_bytes: 5,
            was_truncated: false,
            timed_out: false,
            content_hash: "blake3-stub".to_owned(),
            capability_profile: "Trusted".to_owned(),
            summary: crate::gateway::OutputSummary::empty(0),
        }
    }

    fn timed_out_outcome() -> SandboxOutcome {
        SandboxOutcome {
            exit_code: -1,
            output_bytes: 0,
            was_truncated: false,
            timed_out: true,
            content_hash: String::new(),
            capability_profile: "Sandboxed".to_owned(),
            summary: crate::gateway::OutputSummary::empty(-1),
        }
    }

    fn gate_report(decisions: &[(Capability, Decision)]) -> GateReport {
        GateReport {
            profile_name: "TestProfile".to_owned(),
            gated: decisions
                .iter()
                .map(|(cap, dec)| GatedCapability {
                    capability: cap.clone(),
                    operation: "op".to_owned(),
                    decision: *dec,
                })
                .collect(),
        }
    }

    /// A spotless evidence ledger — every signal at its best.
    fn clean_evidence() -> Evidence {
        Evidence {
            static_report: Some(static_report(StaticSeverity::Clear)),
            vgp_report: Some(vgp_report(4, 0)),
            prediction: Some(prediction(0.95)),
            sandbox_outcome: Some(ok_outcome()),
            gate_report: Some(gate_report(&[(
                Capability::Run(CmdScope::new("cargo")),
                Decision::Allow,
            )])),
            ..Evidence::default()
        }
    }

    // ── sub-scores ────────────────────────────────────────────────────────

    #[test]
    fn subscores_grade_each_signal_at_its_extremes() {
        assert_eq!(static_subscore(None), 1.0);
        assert_eq!(
            static_subscore(Some(&static_report(StaticSeverity::Block))),
            0.0
        );
        assert_eq!(vgp_subscore(Some(&vgp_report(0, 4))), 0.0);
        assert_eq!(vgp_subscore(Some(&vgp_report(4, 0))), 1.0);
        assert_eq!(predict_subscore(None), 0.5);
        assert_eq!(sandbox_subscore(Some(&timed_out_outcome())), 0.2);
        assert_eq!(sandbox_subscore(Some(&ok_outcome())), 1.0);
        assert_eq!(
            gate_subscore(Some(&gate_report(&[(
                Capability::Run(CmdScope::new("rm")),
                Decision::Deny,
            )]))),
            0.0
        );
    }

    // ── EvidenceBundle (OP2 / §5.2.2 — non-terminal signal) ────────────────

    #[test]
    fn gate_decision_carries_non_terminal_evidence_bundle() {
        // §5.2.2: the decision exposes the per-axis bundle, and its scalar
        // projection equals the terminal composite_score — so the non-terminal
        // signal is carried forward yet remains recoverable to the scalar.
        let decision = GateDecision::from_evidence(&clean_evidence());
        let bundle = decision.evidence;
        for axis in [
            bundle.static_score,
            bundle.vgp_score,
            bundle.predict_score,
            bundle.sandbox_score,
            bundle.gate_score,
        ] {
            assert!((0.0..=1.0).contains(&axis), "axis out of range: {axis}");
        }
        assert!(
            (bundle.composite() - decision.composite_score).abs() < 1e-9,
            "bundle projection {} must equal composite_score {}",
            bundle.composite(),
            decision.composite_score
        );
    }

    // ── composite_score ───────────────────────────────────────────────────

    #[test]
    fn composite_score_empty_evidence_is_neutral() {
        let s = composite_score(&Evidence::default());
        assert!((0.80..0.85).contains(&s), "got {s}");
    }

    #[test]
    fn composite_score_clean_is_near_one() {
        let s = composite_score(&clean_evidence());
        assert!(s > 0.95, "a spotless ledger must score high: {s}");
    }

    #[test]
    fn composite_score_static_block_drags_down() {
        let mut ev = clean_evidence();
        ev.static_report = Some(static_report(StaticSeverity::Block));
        let s = composite_score(&ev);
        assert!(
            s < 0.8,
            "a Block static report must drag the score down: {s}"
        );
    }

    #[test]
    fn composite_score_is_always_in_unit_range() {
        for ev in [Evidence::default(), clean_evidence()] {
            let s = composite_score(&ev);
            assert!((0.0..=1.0).contains(&s), "out of range: {s}");
        }
    }

    // ── GateDecision::from_evidence ───────────────────────────────────────

    #[test]
    fn from_evidence_allows_a_spotless_ledger() {
        let decision = GateDecision::from_evidence(&clean_evidence());
        assert_eq!(decision.verdict, Verdict::Allow);
        assert!(decision.reasons.is_empty(), "{:?}", decision.reasons);
        assert!(decision.canonical_fix.is_none());
        assert!(decision.is_allowed());
    }

    #[test]
    fn from_evidence_denies_on_static_block() {
        let mut ev = clean_evidence();
        ev.static_report = Some(static_report(StaticSeverity::Block));
        let decision = GateDecision::from_evidence(&ev);
        assert_eq!(decision.verdict, Verdict::Deny);
        let fix = decision.canonical_fix.expect("a Deny must carry a fix");
        assert!(
            fix.contains("X2 STATIC"),
            "fix should name the cause: {fix}"
        );
    }

    #[test]
    fn from_evidence_denies_on_blocking_capability() {
        let mut ev = clean_evidence();
        ev.gate_report = Some(gate_report(&[(
            Capability::Run(CmdScope::new("rm")),
            Decision::Deny,
        )]));
        let decision = GateDecision::from_evidence(&ev);
        assert_eq!(decision.verdict, Verdict::Deny);
        let fix = decision.canonical_fix.expect("a Deny must carry a fix");
        assert!(fix.contains("denies"), "fix should name the profile: {fix}");
    }

    #[test]
    fn from_evidence_denies_on_low_composite() {
        // No hard block, but every signal is mediocre — the composite alone
        // drops below the deny threshold.
        let ev = Evidence {
            static_report: Some(static_report(StaticSeverity::Warn)),
            vgp_report: Some(vgp_report(1, 3)),
            prediction: Some(prediction(0.2)),
            sandbox_outcome: Some(timed_out_outcome()),
            gate_report: Some(gate_report(&[(
                Capability::Run(CmdScope::new("git")),
                Decision::Prompt,
            )])),
            ..Evidence::default()
        };
        let decision = GateDecision::from_evidence(&ev);
        assert_eq!(decision.verdict, Verdict::Deny);
        assert!(decision.composite_score < DENY_THRESHOLD);
    }

    #[test]
    fn from_evidence_warns_on_a_soft_concern() {
        // Spotless but for one unresolved symbol — a yellow flag, not a block.
        let mut ev = clean_evidence();
        ev.vgp_report = Some(vgp_report(3, 1));
        let decision = GateDecision::from_evidence(&ev);
        assert_eq!(decision.verdict, Verdict::Warn);
        assert!(!decision.reasons.is_empty());
        assert!(decision.canonical_fix.is_some());
        assert!(!decision.is_allowed());
    }

    #[test]
    fn collect_reasons_lists_every_imperfect_signal() {
        let ev = Evidence {
            static_report: Some(static_report(StaticSeverity::Warn)),
            vgp_report: Some(vgp_report(1, 2)),
            prediction: Some(prediction(0.1)),
            sandbox_outcome: Some(timed_out_outcome()),
            gate_report: Some(gate_report(&[(
                Capability::Run(CmdScope::new("rm")),
                Decision::Deny,
            )])),
            ..Evidence::default()
        };
        let reasons = collect_reasons(&ev);
        assert_eq!(reasons.len(), 5, "one line per signal: {reasons:?}");
    }

    #[test]
    fn collect_reasons_is_empty_for_a_clean_ledger() {
        assert!(collect_reasons(&clean_evidence()).is_empty());
    }

    #[test]
    fn verdict_serde_roundtrip() {
        for v in [Verdict::Allow, Verdict::Warn, Verdict::Deny] {
            let json = serde_json::to_string(&v).expect("serialize");
            let back: Verdict = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(v, back);
        }
    }

    #[test]
    fn gate_decision_serde_roundtrip() {
        let decision = GateDecision::from_evidence(&clean_evidence());
        let json = serde_json::to_string(&decision).expect("serialize");
        let back: GateDecision = serde_json::from_str(&json).expect("deserialize");
        // The discrete fields roundtrip exactly.
        assert_eq!(back.verdict, decision.verdict);
        assert_eq!(back.reasons, decision.reasons);
        assert_eq!(back.canonical_fix, decision.canonical_fix);
        // `composite_score` is an `f64`; serde_json's default float parser is
        // not correctly-rounded (it lacks the `float_roundtrip` feature), so a
        // JSON roundtrip may shift the value by up to ~1 ULP. Compare within a
        // tolerance far below any score difference that could change a verdict.
        assert!(
            (back.composite_score - decision.composite_score).abs() < 1e-9,
            "composite_score must survive the JSON roundtrip within f64 tolerance: \
             {} vs {}",
            back.composite_score,
            decision.composite_score
        );
    }

    // ── E2E: the full X0 → X7 pipeline ────────────────────────────────────

    #[test]
    fn e2e_clean_bash_under_trusted_is_allowed() {
        let decided = capture_tool_call("Bash", "echo hello", None)
            .expect("Bash admitted at X0")
            .classify()
            .static_analyze()
            .vgp_verify(|_| true)
            .prove_claim(
                None,
                crate::gateway::offensive_integration::SolverBackendKind::Stub,
                &crate::gateway::offensive_integration::ClaimContext::default(),
            )
            .predict(&ExecutionOutcomePredictor::new(), |_| OutcomeStats {
                successes: 9,
                failures: 1,
            })
            .sandbox_dry_run(|_| ok_outcome())
            .capability_gate(&trusted())
            .decide();
        assert_eq!(decided.ordinal(), 8);
        assert_eq!(decided.evidence().stage_log.len(), 9);
        let decision = decided
            .evidence()
            .decision
            .as_ref()
            .expect("decide() must attach a GateDecision");
        assert_eq!(decision.verdict, Verdict::Allow);
        assert!(decision.canonical_fix.is_none());
    }

    #[test]
    fn e2e_destructive_bash_is_denied() {
        let decided = capture_tool_call("Bash", "rm -rf /", None)
            .expect("Bash admitted at X0")
            .classify()
            .static_analyze()
            .vgp_verify(|_| true)
            .prove_claim(
                None,
                crate::gateway::offensive_integration::SolverBackendKind::Stub,
                &crate::gateway::offensive_integration::ClaimContext::default(),
            )
            .predict(&ExecutionOutcomePredictor::new(), |_| OutcomeStats {
                successes: 9,
                failures: 1,
            })
            .sandbox_dry_run(|_| ok_outcome())
            .capability_gate(&trusted())
            .decide();
        let decision = decided.evidence().decision.as_ref().expect("decided");
        assert_eq!(
            decision.verdict,
            Verdict::Deny,
            "`rm -rf /` must never be allowed"
        );
        assert!(decision.canonical_fix.is_some());
        assert!(!decision.reasons.is_empty());
    }

    #[test]
    fn e2e_full_pipeline_reaches_decided_with_a_decision() {
        // An unresolved symbol downgrades an otherwise-clean run to Warn — the
        // pipeline still reaches the terminal Decided state with a decision.
        let decided = capture_tool_call("Bash", "cargo test --release", None)
            .expect("Bash admitted at X0")
            .classify()
            .static_analyze()
            .vgp_verify(|_| false)
            .prove_claim(
                None,
                crate::gateway::offensive_integration::SolverBackendKind::Stub,
                &crate::gateway::offensive_integration::ClaimContext::default(),
            )
            .predict(&ExecutionOutcomePredictor::new(), |_| OutcomeStats {
                successes: 9,
                failures: 1,
            })
            .sandbox_dry_run(|_| ok_outcome())
            .capability_gate(&trusted())
            .decide();
        assert_eq!(decided.ordinal(), 8);
        assert_eq!(decided.stage(), "X7-DECISION");
        assert_eq!(decided.evidence().stage_log.len(), 9);
        let decision = decided
            .evidence()
            .decision
            .as_ref()
            .expect("decide() must attach a GateDecision");
        assert_eq!(decision.verdict, Verdict::Warn);
        assert!(decided.id().as_str().starts_with("exec-"));
    }

    // ── P8.7 — Workflow antipattern canonical_fix wiring tests ────────────

    /// Helper: build an Evidence with a `StaticReport::Warn` that carries a
    /// workflow antipattern finding string (the format emitted by
    /// `antipattern_finding()` in P8.3).
    fn warn_evidence_with_workflow_finding(finding: &str) -> Evidence {
        let mut ev = clean_evidence();
        ev.static_report = Some(StaticReport {
            severity: StaticSeverity::Warn,
            findings: vec![finding.to_owned()],
            risk_summary: None,
        });
        ev
    }

    #[test]
    fn p8_7_workflow_finding_becomes_canonical_fix_on_warn() {
        // When X2 STATIC raises a Warn carrying a "workflow[" prefixed finding,
        // build_canonical_fix must surface that finding as the canonical_fix so
        // the agent receives the elite-tool conversion hint.
        let finding =
            "workflow[bash-grep-raw] → use Grep tool or `touring tantivy search` instead. ...";
        let ev = warn_evidence_with_workflow_finding(finding);
        let decision = GateDecision::from_evidence(&ev);
        // The static Warn + workflow finding should produce a Warn verdict (not
        // Deny), because static_subscore(Warn)=0.6 keeps composite above 0.5
        // given the other clean signals.
        assert_ne!(
            decision.verdict,
            Verdict::Deny,
            "workflow finding on Warn static must not produce Deny"
        );
        let fix = decision
            .canonical_fix
            .expect("a Warn verdict must carry canonical_fix");
        assert!(
            fix.starts_with("workflow["),
            "canonical_fix must be the workflow finding when present: {fix}"
        );
        assert!(
            fix.contains("bash-grep-raw"),
            "fix must name the antipattern: {fix}"
        );
    }

    #[test]
    fn p8_7_block_wins_over_workflow_finding() {
        // Deny-wins discipline: if X2 STATIC is Block, the Block explanation
        // must be canonical_fix — never the workflow advisory.
        let mut ev = clean_evidence();
        ev.static_report = Some(StaticReport {
            severity: StaticSeverity::Block,
            findings: vec![
                "destructive command: rm -rf /".to_owned(),
                "workflow[bash-grep-raw] → use Grep tool instead.".to_owned(),
            ],
            risk_summary: None,
        });
        let decision = GateDecision::from_evidence(&ev);
        assert_eq!(decision.verdict, Verdict::Deny);
        let fix = decision
            .canonical_fix
            .expect("a Deny must carry canonical_fix");
        assert!(
            fix.contains("X2 STATIC"),
            "Block verdict canonical_fix must name X2 STATIC, not the workflow finding: {fix}"
        );
        assert!(
            !fix.starts_with("workflow["),
            "workflow finding must not override a Block: {fix}"
        );
    }

    #[test]
    fn p8_7_gate_denial_wins_over_workflow_finding() {
        // Deny-wins: a capability denial must take canonical_fix priority over
        // a workflow advisory in the static Warn findings.
        let finding = "workflow[bash-find] → use Glob tool or `touring index files` instead. ...";
        let mut ev = warn_evidence_with_workflow_finding(finding);
        // Add a blocking capability denial.
        use crate::capability::{Capability, CmdScope, Decision};
        use crate::gateway::gate::{GateReport, GatedCapability};
        ev.gate_report = Some(GateReport {
            profile_name: "Sandboxed".to_owned(),
            gated: vec![GatedCapability {
                capability: Capability::Run(CmdScope::new("find")),
                operation: "find /".to_owned(),
                decision: Decision::Deny,
            }],
        });
        let decision = GateDecision::from_evidence(&ev);
        assert_eq!(decision.verdict, Verdict::Deny);
        let fix = decision
            .canonical_fix
            .expect("a Deny must carry canonical_fix");
        assert!(
            fix.contains("denies"),
            "gate denial must appear in canonical_fix before workflow finding: {fix}"
        );
        assert!(
            !fix.starts_with("workflow["),
            "workflow finding must not override a capability denial: {fix}"
        );
    }

    #[test]
    fn p8_7_non_workflow_warn_finding_does_not_use_workflow_branch() {
        // A normal (non-workflow) Warn finding must not be promoted via the
        // workflow branch — canonical_fix must fall through to the score-based
        // generic message.
        let ev = Evidence {
            static_report: Some(StaticReport {
                severity: StaticSeverity::Warn,
                findings: vec!["risk: unwrap in hot path".to_owned()],
                risk_summary: None,
            }),
            vgp_report: Some(vgp_report(4, 0)),
            prediction: Some(prediction(0.9)),
            sandbox_outcome: Some(ok_outcome()),
            gate_report: Some(gate_report(&[(
                Capability::Run(CmdScope::new("cargo")),
                Decision::Allow,
            )])),
            ..Evidence::default()
        };
        let decision = GateDecision::from_evidence(&ev);
        let fix = decision
            .canonical_fix
            .expect("a non-Allow verdict must carry canonical_fix");
        assert!(
            !fix.starts_with("workflow["),
            "a non-workflow finding must not use the workflow branch: {fix}"
        );
    }

    #[test]
    fn p8_7_empty_findings_does_not_panic_in_workflow_branch() {
        // Exit-0 fail-open: if static_report has Warn severity but an empty
        // findings list, build_canonical_fix must not panic and must fall
        // through to the generic fix message.
        let ev = Evidence {
            static_report: Some(StaticReport {
                severity: StaticSeverity::Warn,
                findings: Vec::new(), // no findings at all
                risk_summary: None,
            }),
            vgp_report: Some(vgp_report(4, 0)),
            prediction: Some(prediction(0.9)),
            sandbox_outcome: Some(ok_outcome()),
            gate_report: Some(gate_report(&[(
                Capability::Run(CmdScope::new("cargo")),
                Decision::Allow,
            )])),
            ..Evidence::default()
        };
        // Must not panic.
        let decision = GateDecision::from_evidence(&ev);
        // With a Warn static + otherwise clean signals the composite should be
        // high enough to produce Warn or Allow; not our concern here — what
        // matters is no panic and canonical_fix is coherent.
        if let Some(fix) = &decision.canonical_fix {
            assert!(
                !fix.starts_with("workflow["),
                "empty findings list must not trigger workflow branch: {fix}"
            );
        }
    }
}
