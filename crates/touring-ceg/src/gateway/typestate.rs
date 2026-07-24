//! `Execution<S>` typestate — the compile-time-enforced spine of the Code
//! Execution Gateway (CEG). Phase **P3.1** of CEG Pln2
//! (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! # Why a typestate
//!
//! The CEG runs every code-bearing tool call through ten ordered stages,
//! `X0`..`X9`. The first eight each produce a distinct *state*:
//!
//! | Stage | Name            | State produced    |
//! |-------|-----------------|-------------------|
//! | X0    | CAPTURE         | [`Captured`]      |
//! | X1    | CLASSIFY        | [`Classified`]    |
//! | X2    | STATIC          | [`Analyzed`]      |
//! | X3    | VGP             | [`Verified`]      |
//! | X4    | PREDICT         | [`Predicted`]     |
//! | X5    | SANDBOX         | [`SandboxTested`] |
//! | X6    | CAPABILITY-GATE | [`Gated`]         |
//! | X7    | DECISION        | [`Decided`]       |
//!
//! `X8` EXECUTE and `X9` LEARN *consume* an [`Execution<Decided>`]; they add no
//! further typestate, so the pipeline has exactly **eight** states.
//!
//! A security pipeline whose stages can be reordered or skipped is not a
//! pipeline — it is a suggestion. Here the order is encoded in the type system:
//! an [`Execution<S>`] only ever moves to `S`'s single successor
//! ([`Advance::Next`]). No method anywhere turns an `Execution<Analyzed>` into
//! an `Execution<Predicted>` — `Predicted` is reachable only *through*
//! `Verified` (X3 VGP), and `Gated` only *through* `SandboxTested` (X5
//! SANDBOX). **X3 and X5 cannot be bypassed**, and attempting it is a compile
//! error, not a runtime check (see the `compile_fail` examples on
//! [`Execution`]).
//!
//! This mirrors the generator's `Draft -> Verified -> Rendered -> Speculated ->
//! Committed` typestate (`touring-generator::executor::typestate`): the CEG
//! applies to code *execution* the discipline the generator applies to code
//! *generation*.
//!
//! # Where the stage data lives
//!
//! The generator carries each stage's data inside that stage's struct. The CEG
//! typestate is instead **distributed across files** — `capture.rs`,
//! `classify.rs`, one module per stage deliverable — so the eight state types
//! are zero-sized **markers** and per-stage results accumulate in the
//! [`Evidence`] ledger that every [`Execution`] carries. Each transition is the
//! *sole* constructor of the state it yields, so an `Execution<Verified>`
//! always carries a VGP result at runtime even though the type tracks only
//! stage order.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::marker::PhantomData;

// ── Sealed trait — the set of pipeline states is closed ───────────────────────

mod sealed {
    /// Sealing trait: only this module's eight markers implement it, so no
    /// downstream crate can introduce a rogue [`super::ExecutionState`].
    pub trait Sealed {}
}

/// Marker trait implemented by exactly the eight CEG pipeline states.
///
/// Sealed — the set of states is closed. Every state carries its canonical
/// [`STAGE`](ExecutionState::STAGE) label and
/// [`ORDINAL`](ExecutionState::ORDINAL) position in `X0..=X7`.
pub trait ExecutionState: sealed::Sealed + Send + Sync + 'static {
    /// Canonical stage label, e.g. `"X3-VGP"`.
    const STAGE: &'static str;
    /// Zero-based position in the `X0..=X7` pipeline.
    const ORDINAL: u8;
}

/// Implemented by every *non-terminal* [`ExecutionState`]: names the single
/// successor state.
///
/// [`Decided`] is terminal and deliberately omits this impl — which is exactly
/// why `Execution<Decided>::advance` does not exist.
pub trait Advance: ExecutionState {
    /// The unique next state in the pipeline.
    type Next: ExecutionState;
}

// ── The eight pipeline states ─────────────────────────────────────────────────

/// Declare a pipeline-state marker: a zero-sized struct implementing the sealed
/// [`ExecutionState`] trait with a fixed stage label and ordinal.
macro_rules! execution_states {
    ($( $(#[$doc:meta])* $name:ident = ($stage:literal, $ord:literal) ),+ $(,)?) => {
        $(
            $(#[$doc])*
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
            pub struct $name;

            impl sealed::Sealed for $name {}

            impl ExecutionState for $name {
                const STAGE: &'static str = $stage;
                const ORDINAL: u8 = $ord;
            }
        )+
    };
}

execution_states! {
    /// **X0** — the raw code-bearing tool call has been captured verbatim.
    Captured = ("X0-CAPTURE", 0),
    /// **X1** — language, exec-surface and code body extracted.
    Classified = ("X1-CLASSIFY", 1),
    /// **X2** — structural / risk / forbidden-call static analysis attached.
    Analyzed = ("X2-STATIC", 2),
    /// **X3** — VGP symbol verification passed. Unskippable.
    Verified = ("X3-VGP", 3),
    /// **X3.5** — SMT proof attached (X3.5-PROVE, ES1 P3). Optional
    /// `prove_claim` step that produces a `ProofReport` in the
    /// [`Evidence`] ledger. Skipped (`None`) by default; advanced callers
    /// attach a `ClaimKind` and the corresponding backend (Z3/CVC5/Stub)
    /// to obtain a real proof. Stub backend returns `ProofStatus::Void`
    /// (honest no-op contract preserved).
    Proven = ("X3.5-PROVE", 4),
    /// **X4** — RL outcome prediction attached.
    Predicted = ("X4-PREDICT", 5),
    /// **X5** — dry-run sandbox execution observed. Unskippable.
    SandboxTested = ("X5-SANDBOX", 6),
    /// **X6** — required capabilities matched against the granted profile.
    Gated = ("X6-CAPABILITY-GATE", 7),
    /// **X7** — terminal: the Allow / Warn / Deny decision has been computed.
    Decided = ("X7-DECISION", 8),
}

/// Wire the linear successor chain: each `from -> to` pair makes `from`
/// advanceable to `to`. The terminal state is simply absent here.
macro_rules! advance_chain {
    ($( $from:ident -> $to:ident ),+ $(,)?) => {
        $(
            impl Advance for $from {
                type Next = $to;
            }
        )+
    };
}

advance_chain! {
    Captured -> Classified,
    Classified -> Analyzed,
    Analyzed -> Verified,
    Verified -> Proven,
    Proven -> Predicted,
    Predicted -> SandboxTested,
    SandboxTested -> Gated,
    Gated -> Decided,
}

// ── RawInvocation — the X0 capture payload ────────────────────────────────────

/// A raw, unprocessed code-bearing tool call, as captured at stage **X0**.
///
/// This is the verbatim input — no language detection, no code extraction.
/// Stage `X1` CLASSIFY derives the structured surface, language and body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawInvocation {
    /// The tool name, e.g. `"Bash"`, `"ctx_execute"`, `"Write"`.
    pub tool: String,
    /// The code or command text, exactly as the caller supplied it.
    pub payload: String,
    /// The human-stated intent for the call, when one is available.
    pub intent: Option<String>,
}

impl RawInvocation {
    /// A raw invocation with no stated intent.
    pub fn new(tool: impl Into<String>, payload: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            payload: payload.into(),
            intent: None,
        }
    }

    /// Attach a human-stated intent (builder style).
    #[must_use]
    pub fn with_intent(mut self, intent: impl Into<String>) -> Self {
        self.intent = Some(intent.into());
        self
    }
}

// ── ExecutionId — deterministic, content-addressed identity ───────────────────

/// A deterministic, content-addressed identifier for one [`Execution`].
///
/// Derived purely from [`RawInvocation`] content via BLAKE3
/// ([`ExecutionId::derive`]) — never from a timestamp, address or counter. Two
/// executions of byte-identical input share an id; that is intentional, and is
/// what lets [`crate::gateway::staging_classify`] recognise the re-run of a staged script.
/// (REGRA #17 — entity identity is a pure function of canonical content.)
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ExecutionId(String);

impl ExecutionId {
    /// Derive the id from a raw invocation.
    ///
    /// Pure and deterministic: equal `RawInvocation` content always yields an
    /// equal `ExecutionId`, regardless of how the value was constructed. A
    /// `\0` byte separates the fields so that, e.g., `("ab", "c")` and
    /// `("a", "bc")` cannot collide.
    #[must_use]
    pub fn derive(raw: &RawInvocation) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";

        let mut hasher = blake3::Hasher::new();
        hasher.update(raw.tool.as_bytes());
        hasher.update(b"\0");
        hasher.update(raw.payload.as_bytes());
        hasher.update(b"\0");
        if let Some(intent) = &raw.intent {
            hasher.update(intent.as_bytes());
        }
        let digest = hasher.finalize();

        // First 64 bits of the BLAKE3 digest, lower-hex: ample collision
        // resistance for gateway bookkeeping, short enough to read in a log.
        let mut id = String::with_capacity(21);
        id.push_str("exec-");
        for &byte in &digest.as_bytes()[..8] {
            id.push(char::from(HEX[usize::from(byte >> 4)]));
            id.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        Self(id)
    }

    /// The id as a string slice, e.g. `"exec-1a2b3c4d5e6f7a8b"`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ExecutionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ── Evidence — the per-stage audit ledger ─────────────────────────────────────

/// One entry in the [`Evidence`] audit trail: a stage the execution completed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageRecord {
    /// The canonical stage label, e.g. `"X3-VGP"`.
    pub stage: String,
    /// The zero-based pipeline position of the stage (`0..=7`).
    pub ordinal: u8,
}

impl StageRecord {
    /// Build a record for the just-completed stage `S`.
    fn completed<S: ExecutionState>() -> Self {
        Self {
            stage: S::STAGE.to_owned(),
            ordinal: S::ORDINAL,
        }
    }
}

/// The append-only ledger of per-stage results carried by an [`Execution`]
/// through the pipeline.
///
/// In `P3.1` it holds only the [`stage_log`](Evidence::stage_log) audit trail.
/// Later stage deliverables (`X1`..`X7`) extend this struct with the typed
/// result each stage produces; the `X9` LEARN stage consumes the whole ledger.
///
/// `Eq` is deliberately *not* derived: the X4 `prediction` field carries a
/// floating-point probability, for which reflexive equality does not hold.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    /// One [`StageRecord`] per stage the execution has passed through, in
    /// pipeline order — the `X9` LEARN audit trail.
    pub stage_log: Vec<StageRecord>,
    /// The **X1 CLASSIFY** result — execution surface, language and code
    /// body. `None` until the execution reaches [`Classified`]; populated by
    /// `classify` (`crate::gateway::classify::Execution::classify`).
    pub classification: Option<crate::gateway::classify::Classification>,
    /// The **X2 STATIC** result — structural + per-language risk findings.
    /// `None` until the execution reaches [`Analyzed`]; populated by
    /// `static_analyze` (`crate::gateway::static_stage::Execution::static_analyze`).
    pub static_report: Option<crate::gateway::static_stage::StaticReport>,
    /// The **X3 VGP** result — verified vs unresolved symbol references.
    /// `None` until the execution reaches [`Verified`]; populated by
    /// `vgp_verify` (`crate::gateway::vgp_stage::Execution::vgp_verify`).
    pub vgp_report: Option<crate::gateway::vgp_stage::VgpReport>,
    /// The **X3.5 PROVE** result — optional SMT proof attached by
    /// `prove_claim` (ES1 P3). `None` when no claim was asserted (zero Z3
    /// overhead for default callers); `Some(ProofReport)` when a
    /// `ClaimKind` was provided to the gateway. The stub backend returns
    /// `ProofStatus::Void` (honest no-op contract). `None` until the
    /// execution reaches [`Proven`]; populated by
    /// [`prove_claim`](Execution::prove_claim).
    pub proof_report: Option<crate::gateway::offensive_integration::ProofReport>,
    /// The **X4 PREDICT** result — RL-predicted success probability. `None`
    /// until the execution reaches [`Predicted`]; populated by
    /// `predict` (`crate::gateway::predict::Execution::predict`).
    pub prediction: Option<crate::gateway::predict::PredictionReport>,
    /// The **X5 SANDBOX** result — the observed dry-run outcome. `None` until
    /// the execution reaches [`SandboxTested`]; populated by
    /// `sandbox_dry_run` (`crate::gateway::sandbox_stage::Execution::sandbox_dry_run`).
    pub sandbox_outcome: Option<crate::gateway::sandbox_stage::SandboxOutcome>,
    /// The **X6 CAPABILITY-GATE** result — every required capability resolved
    /// against the granted profile. `None` until the execution reaches
    /// [`Gated`]; populated by
    /// `capability_gate` (`crate::gateway::gate::Execution::capability_gate`).
    pub gate_report: Option<crate::gateway::gate::GateReport>,
    /// The **X7.5 QUALITY-SIGNAL** result — 50-dim composite score from
    /// `touring-quality`. Optional; `None` for synthetic / un-scored
    /// executions (treated as neutral in the X7 composite — no penalty).
    /// Populated by `touring-cortex::handlers::quality::PostQualityGate`
    /// (W6) or any other W6 hook integration.
    pub quality_signal: Option<crate::gateway::quality_signal::QualitySignalReport>,
    /// The **X7 DECISION** result — the terminal Allow / Warn / Deny verdict.
    /// `None` until the execution reaches [`Decided`]; populated by
    /// `decide` (`crate::gateway::decision::Execution::decide`).
    pub decision: Option<crate::gateway::decision::GateDecision>,
}

// ── Execution<S> — the typestate token ────────────────────────────────────────

/// A single code-execution request flowing through the CEG pipeline.
///
/// `S` tracks the pipeline stage at compile time. Construct one with
/// [`Execution::capture`]; move it forward exactly one stage at a time with
/// [`Execution::advance`]. An `Execution` is move-only and `#[must_use]` —
/// dropping one mid-pipeline silently discards a pending gate decision, which
/// is always a bug.
///
/// # The happy path
///
/// ```
/// use touring_ceg::gateway::{Execution, RawInvocation};
///
/// let decided = Execution::capture(RawInvocation::new("ctx_execute", "print('hi')"))
///     .advance()  // X1 CLASSIFY
///     .advance()  // X2 STATIC
///     .advance()  // X3 VGP
///     .advance()  // X3.5 PROVE
///     .advance()  // X4 PREDICT
///     .advance()  // X5 SANDBOX
///     .advance()  // X6 CAPABILITY-GATE
///     .advance(); // X7 DECISION
/// assert_eq!(decided.ordinal(), 8);
/// assert_eq!(decided.stage(), "X7-DECISION");
/// assert_eq!(decided.evidence().stage_log.len(), 9);
/// ```
///
/// # X3 (VGP) cannot be skipped
///
/// `advance()` moves exactly one stage, so a state three stages on is
/// unreachable in one hop — this does not compile:
///
/// ```compile_fail
/// use touring_ceg::gateway::{Execution, RawInvocation, Verified};
///
/// let captured = Execution::capture(RawInvocation::new("Bash", "echo hi"));
/// // `Verified` (X3) is three stages on; `advance()` yields `Classified`.
/// let _: Execution<Verified> = captured.advance();
/// ```
///
/// # The terminal state cannot advance
///
/// `Decided` does not implement [`Advance`], so it has no `advance` method —
/// this does not compile:
///
/// ```compile_fail
/// use touring_ceg::gateway::{Execution, RawInvocation};
///
/// let decided = Execution::capture(RawInvocation::new("Bash", "echo hi"))
///     .advance().advance().advance().advance().advance().advance().advance().advance();
/// let _ = decided.advance(); // no method `advance` on `Execution<Decided>`
/// ```
#[must_use = "an Execution carries a pending gate decision — advance or consume it"]
pub struct Execution<S: ExecutionState> {
    id: ExecutionId,
    raw: RawInvocation,
    evidence: Evidence,
    _state: PhantomData<S>,
}

impl Execution<Captured> {
    /// Capture a raw code-bearing tool call — stage **X0**, the pipeline entry.
    ///
    /// The [`ExecutionId`] is derived deterministically from `raw`; the
    /// [`Evidence`] ledger is seeded with the `X0` stage record.
    pub fn capture(raw: RawInvocation) -> Self {
        let id = ExecutionId::derive(&raw);
        let evidence = Evidence {
            stage_log: vec![StageRecord::completed::<Captured>()],
            ..Evidence::default()
        };
        Self {
            id,
            raw,
            evidence,
            _state: PhantomData,
        }
    }
}

impl<S: ExecutionState> Execution<S> {
    /// The deterministic id of this execution — stable across every stage.
    #[must_use]
    pub fn id(&self) -> &ExecutionId {
        &self.id
    }

    /// The raw tool call captured at `X0` — verbatim, never mutated.
    #[must_use]
    pub fn raw(&self) -> &RawInvocation {
        &self.raw
    }

    /// The accumulating per-stage [`Evidence`] ledger.
    #[must_use]
    pub fn evidence(&self) -> &Evidence {
        &self.evidence
    }

    /// Mutable access to the [`Evidence`] ledger — for the per-stage
    /// transition deliverables (`X1`..`X7`) to attach their results.
    /// Crate-internal by design: only the pipeline itself writes evidence.
    pub(crate) fn evidence_mut(&mut self) -> &mut Evidence {
        &mut self.evidence
    }

    /// The canonical stage label of the current state, e.g. `"X3-VGP"`.
    #[must_use]
    pub fn stage(&self) -> &'static str {
        S::STAGE
    }

    /// The zero-based pipeline position of the current state (`0..=7`).
    #[must_use]
    pub fn ordinal(&self) -> u8 {
        S::ORDINAL
    }
}

impl<S: Advance> Execution<S> {
    /// Advance exactly one pipeline stage: `Execution<S>` → `Execution<S::Next>`.
    ///
    /// This is the foundational transition. It consumes `self`, appends the
    /// successor stage to the [`Evidence`] audit trail, and re-brands the
    /// typestate. Per-stage deliverables (`X1`..`X7`) layer evidence-carrying
    /// transitions on top of this one — but every path forward advances by
    /// exactly one stage, so no stage is skippable.
    pub fn advance(mut self) -> Execution<S::Next> {
        self.evidence
            .stage_log
            .push(StageRecord::completed::<S::Next>());
        Execution {
            id: self.id,
            raw: self.raw,
            evidence: self.evidence,
            _state: PhantomData,
        }
    }
}

// ── X3.5 PROVE — optional SMT proof (ES1 P3, 2026-06-01) ─────────────────────

impl Execution<crate::gateway::Verified> {
    /// **X3.5 PROVE** — attach an optional SMT proof to the execution
    /// evidence, advancing `Execution<Verified>` → `Execution<Proven>`.
    ///
    /// `claim = None` is a deliberate no-op for the SMT solver (zero Z3
    /// overhead for default callers); the state still advances to
    /// `Proven` so the `X3.5-PROVE` stage record is appended to the
    /// audit trail — defense-in-depth: the stage log records the
    /// transition even when no claim is asserted.
    ///
    /// `claim = Some(_)` invokes the configured backend
    /// (`Z3`/`Cvc5`/`Stub`) via `prove_claim` and attaches the resulting
    /// `ProofReport` to `evidence.proof_report`. The stub backend
    /// returns `ProofStatus::Void` (honest no-op contract preserved);
    /// real Z3/Cvc5 invocation is feature-gated in `touring-offensive`.
    ///
    /// This is the SOLE constructor of [`Execution<Proven>`]: the marker
    /// type can only be reached through this method, so the `X3.5`
    /// stage is compile-time unskippable from `Verified`.
    pub fn prove_claim(
        mut self,
        claim: Option<crate::gateway::offensive_integration::ClaimKind>,
        backend: crate::gateway::offensive_integration::SolverBackendKind,
        ctx: &crate::gateway::offensive_integration::ClaimContext,
    ) -> Execution<crate::gateway::Proven> {
        if let Some(c) = claim.as_ref() {
            // Real proof attempt — backend may be Z3/Cvc5 (feature-gated)
            // or Stub (always available, returns ProofStatus::Void).
            let report = crate::gateway::offensive_integration::prove_claim(c, ctx, backend);
            self.evidence_mut().proof_report = Some(report);
        } else {
            // No claim asserted: state still advances; proof_report is
            // explicitly None (the default) so callers can distinguish
            // "no claim made" from "claim resolved to Void".
            self.evidence_mut().proof_report = None;
        }
        // Re-brand typestate: Execution<Verified> → Execution<Proven>.
        // The stage_log gets X3.5-PROVE appended via the Advance impl.
        self.advance()
    }
}

impl<S: ExecutionState> fmt::Debug for Execution<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Execution")
            .field("id", &self.id)
            .field("stage", &S::STAGE)
            .field("ordinal", &S::ORDINAL)
            .field("raw", &self.raw)
            .field("evidence", &self.evidence)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stage label + ordinal for a state, via its [`ExecutionState`] impl.
    fn stage_of<S: ExecutionState>() -> (&'static str, u8) {
        (S::STAGE, S::ORDINAL)
    }

    fn sample() -> RawInvocation {
        RawInvocation::new("ctx_execute", "print('hello')")
    }

    fn full_pipeline() -> Execution<Decided> {
        // ES1 P3 (2026-06-01): X3.5 PROVE inserted, so 7 advances become 8.
        Execution::capture(sample())
            .advance()
            .advance()
            .advance()
            .advance()
            .advance()
            .advance()
            .advance()
            .advance()
    }

    #[test]
    fn nine_markers_have_ordinals_zero_to_eight() {
        // ES1 P3 (2026-06-01): Proven inserted at ordinal 4 between
        // Verified (3) and Predicted (5); subsequent states renumber.
        let ords = [
            stage_of::<Captured>().1,
            stage_of::<Classified>().1,
            stage_of::<Analyzed>().1,
            stage_of::<Verified>().1,
            stage_of::<Proven>().1,
            stage_of::<Predicted>().1,
            stage_of::<SandboxTested>().1,
            stage_of::<Gated>().1,
            stage_of::<Decided>().1,
        ];
        assert_eq!(ords, [0, 1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn nine_markers_have_distinct_stage_names() {
        let names = [
            stage_of::<Captured>().0,
            stage_of::<Classified>().0,
            stage_of::<Analyzed>().0,
            stage_of::<Verified>().0,
            stage_of::<Proven>().0,
            stage_of::<Predicted>().0,
            stage_of::<SandboxTested>().0,
            stage_of::<Gated>().0,
            stage_of::<Decided>().0,
        ];
        let unique: std::collections::BTreeSet<&str> = names.iter().copied().collect();
        assert_eq!(
            unique.len(),
            9,
            "all stage names must be distinct: {names:?}"
        );
        assert_eq!(names[0], "X0-CAPTURE");
        assert_eq!(names[3], "X3-VGP");
        assert_eq!(names[4], "X3.5-PROVE");
        assert_eq!(names[5], "X4-PREDICT");
        assert_eq!(names[6], "X5-SANDBOX");
        assert_eq!(names[8], "X7-DECISION");
    }

    #[test]
    fn capture_starts_at_x0() {
        let e = Execution::capture(sample());
        assert_eq!(e.ordinal(), 0);
        assert_eq!(e.stage(), "X0-CAPTURE");
    }

    #[test]
    fn advance_moves_exactly_one_stage() {
        let e = Execution::capture(sample()).advance();
        assert_eq!(e.ordinal(), 1);
        assert_eq!(e.stage(), "X1-CLASSIFY");
    }

    #[test]
    fn full_pipeline_reaches_decided() {
        let decided = full_pipeline();
        // ES1 P3 (2026-06-01): Decided ordinal is now 8 (was 7).
        assert_eq!(decided.ordinal(), 8);
        assert_eq!(decided.stage(), "X7-DECISION");
    }

    #[test]
    fn capture_logs_x0_only() {
        let e = Execution::capture(sample());
        assert_eq!(e.evidence().stage_log.len(), 1);
        assert_eq!(e.evidence().stage_log[0].stage, "X0-CAPTURE");
        assert_eq!(e.evidence().stage_log[0].ordinal, 0);
    }

    #[test]
    fn stage_log_records_all_nine_stages_in_order() {
        let decided = full_pipeline();
        let log = &decided.evidence().stage_log;
        assert_eq!(log.len(), 9);
        let ordinals: Vec<u8> = log.iter().map(|r| r.ordinal).collect();
        assert_eq!(ordinals, vec![0, 1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(log[0].stage, "X0-CAPTURE");
        assert_eq!(log[3].stage, "X3-VGP");
        assert_eq!(log[4].stage, "X3.5-PROVE");
        assert_eq!(log[8].stage, "X7-DECISION");
    }

    #[test]
    fn execution_id_is_deterministic() {
        assert_eq!(
            ExecutionId::derive(&sample()),
            ExecutionId::derive(&sample())
        );
    }

    #[test]
    fn execution_id_differs_on_payload() {
        let a = ExecutionId::derive(&RawInvocation::new("Bash", "ls"));
        let b = ExecutionId::derive(&RawInvocation::new("Bash", "rm -rf /"));
        assert_ne!(a, b);
    }

    #[test]
    fn execution_id_differs_on_tool() {
        let a = ExecutionId::derive(&RawInvocation::new("Bash", "x"));
        let b = ExecutionId::derive(&RawInvocation::new("ctx_execute", "x"));
        assert_ne!(a, b);
    }

    #[test]
    fn execution_id_differs_on_intent() {
        let a = ExecutionId::derive(&RawInvocation::new("Bash", "x"));
        let b = ExecutionId::derive(&RawInvocation::new("Bash", "x").with_intent("clean build"));
        assert_ne!(a, b);
    }

    #[test]
    fn execution_id_field_separator_prevents_collision() {
        // ("ab","c") and ("a","bc") must not collide — the \0 separator guards this.
        let a = ExecutionId::derive(&RawInvocation::new("ab", "c"));
        let b = ExecutionId::derive(&RawInvocation::new("a", "bc"));
        assert_ne!(a, b);
    }

    #[test]
    fn execution_id_independent_of_construction_path() {
        // REGRA #17: the id is a pure function of canonical content, not of how
        // the RawInvocation value happened to be built.
        let built = RawInvocation::new("Bash", "echo hi").with_intent("greet");
        let literal = RawInvocation {
            tool: "Bash".to_owned(),
            payload: "echo hi".to_owned(),
            intent: Some("greet".to_owned()),
        };
        assert_eq!(ExecutionId::derive(&built), ExecutionId::derive(&literal));
    }

    #[test]
    fn execution_id_format_is_exec_prefixed_hex() {
        let id = ExecutionId::derive(&sample());
        assert!(id.as_str().starts_with("exec-"), "got {id}");
        assert_eq!(id.as_str().len(), 21, "exec- + 16 hex chars");
        assert!(
            id.as_str()["exec-".len()..]
                .chars()
                .all(|c| c.is_ascii_hexdigit())
        );
    }

    #[test]
    fn raw_invocation_preserved_through_pipeline() {
        let raw = RawInvocation::new("Bash", "echo hi").with_intent("greet");
        // ES1 P3 (2026-06-01): 7 advances → 8.
        let decided = Execution::capture(raw.clone())
            .advance()
            .advance()
            .advance()
            .advance()
            .advance()
            .advance()
            .advance()
            .advance();
        assert_eq!(decided.raw(), &raw);
    }

    #[test]
    fn execution_id_stable_through_pipeline() {
        let captured = Execution::capture(sample());
        let id_at_x0 = captured.id().clone();
        // ES1 P3 (2026-06-01): 7 advances → 8.
        let decided = captured
            .advance()
            .advance()
            .advance()
            .advance()
            .advance()
            .advance()
            .advance()
            .advance();
        assert_eq!(decided.id(), &id_at_x0);
    }

    #[test]
    fn raw_invocation_with_intent_builder() {
        let raw = RawInvocation::new("Write", "fn main() {}").with_intent("scaffold");
        assert_eq!(raw.tool, "Write");
        assert_eq!(raw.intent.as_deref(), Some("scaffold"));
        assert_eq!(RawInvocation::new("Write", "fn main() {}").intent, None);
    }

    #[test]
    fn evidence_default_is_empty() {
        assert!(Evidence::default().stage_log.is_empty());
    }

    // ── ES1 P3 (2026-06-01) — X3.5 PROVE new unit tests ─────────────────

    /// X3.5 helper: build an `Execution<Verified>` for the prove_claim tests.
    fn verified_for_prove() -> Execution<Verified> {
        Execution::capture(sample())
            .advance() // X1
            .advance() // X2
            .advance() // X3
    }

    #[test]
    fn prove_claim_with_none_attaches_no_report_and_advances_to_proven() {
        // ES1 P3 (2026-06-01): no claim → no Z3 cost; state still
        // advances to Proven so the X3.5-PROVE record is appended.
        let proven = verified_for_prove().prove_claim(
            None,
            crate::gateway::offensive_integration::SolverBackendKind::Stub,
            &crate::gateway::offensive_integration::ClaimContext::default(),
        );
        assert_eq!(proven.ordinal(), 4);
        assert_eq!(proven.stage(), "X3.5-PROVE");
        assert!(
            proven.evidence().proof_report.is_none(),
            "no claim asserted ⇒ proof_report must be None"
        );
        // The stage_log gains the X3.5-PROVE record (4 records: X0 + 3 + X3.5).
        let log = &proven.evidence().stage_log;
        assert_eq!(log.len(), 5);
        assert_eq!(log[4].stage, "X3.5-PROVE");
    }

    #[test]
    fn prove_claim_with_some_claim_runs_prove_claim_and_attaches_report() {
        // Some(claim) ⇒ real solve via the configured backend. With
        // Stub the solver returns ProofStatus::Void (honest no-op),
        // but the report is still attached so callers can introspect.
        let proven = verified_for_prove().prove_claim(
            Some(
                crate::gateway::offensive_integration::ClaimKind::Postcondition {
                    predicate: "x > 0".to_owned(),
                },
            ),
            crate::gateway::offensive_integration::SolverBackendKind::Stub,
            &crate::gateway::offensive_integration::ClaimContext::default(),
        );
        assert_eq!(proven.ordinal(), 4);
        let report = proven
            .evidence()
            .proof_report
            .as_ref()
            .expect("prove_claim must attach a ProofReport when called with Some(_)");
        assert_eq!(
            report.status,
            crate::gateway::offensive_integration::ProofStatus::Void
        );
        assert_eq!(
            report.backend_used,
            crate::gateway::offensive_integration::SolverBackendKind::Stub
        );
    }

    #[test]
    fn prove_claim_with_stub_backend_returns_void_status() {
        // The Stub→Void contract is the P1 honest scope invariant: the
        // stub is NOT a real solver, so it must NOT report Sat/Unsat.
        let proven = verified_for_prove().prove_claim(
            Some(
                crate::gateway::offensive_integration::ClaimKind::Postcondition {
                    predicate: "x > 0".to_owned(),
                },
            ),
            crate::gateway::offensive_integration::SolverBackendKind::Stub,
            &crate::gateway::offensive_integration::ClaimContext::default(),
        );
        let report = proven
            .evidence()
            .proof_report
            .as_ref()
            .expect("report attached");
        assert_eq!(
            report.status,
            crate::gateway::offensive_integration::ProofStatus::Void
        );
        // CRITICAL: must NOT be Sat/Unsat (which would be a dishonest report).
        assert!(
            !matches!(
                report.status,
                crate::gateway::offensive_integration::ProofStatus::Sat
                    | crate::gateway::offensive_integration::ProofStatus::Unsat
            ),
            "Stub backend must NEVER report Sat/Unsat: got {:?}",
            report.status
        );
    }

    #[test]
    fn proven_state_advances_to_predicted_via_predict() {
        // After prove_claim (X3.5), the next stage is X4 PREDICT. The
        // typestate MUST compile — `Execution<Proven>::predict` is the
        // sole constructor of `Execution<Predicted>` from this state.
        let proven = verified_for_prove().prove_claim(
            None,
            crate::gateway::offensive_integration::SolverBackendKind::Stub,
            &crate::gateway::offensive_integration::ClaimContext::default(),
        );
        let predicted = proven.predict(
            &crate::gateway::predict::ExecutionOutcomePredictor::default(),
            |_| crate::gateway::predict::OutcomeStats::default(),
        );
        assert_eq!(predicted.ordinal(), 5);
        assert_eq!(predicted.stage(), "X4-PREDICT");
        assert!(predicted.evidence().prediction.is_some());
    }

    #[test]
    fn stage_record_completed_constructor() {
        let r = StageRecord::completed::<Verified>();
        assert_eq!(r.stage, "X3-VGP");
        assert_eq!(r.ordinal, 3);
    }

    #[test]
    fn serde_roundtrip_raw_invocation() {
        let raw = RawInvocation::new("Bash", "ls -la").with_intent("list");
        let json = serde_json::to_string(&raw).expect("serialize");
        let back: RawInvocation = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(raw, back);
    }

    #[test]
    fn serde_roundtrip_evidence_and_records() {
        let decided = full_pipeline();
        let ev = decided.evidence();
        let json = serde_json::to_string(ev).expect("serialize");
        let back: Evidence = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(ev, &back);
    }

    #[test]
    fn serde_roundtrip_execution_id() {
        let id = ExecutionId::derive(&sample());
        let json = serde_json::to_string(&id).expect("serialize");
        let back: ExecutionId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    #[test]
    fn execution_debug_shows_stage() {
        let dbg = format!("{:?}", Execution::capture(sample()));
        assert!(
            dbg.contains("X0-CAPTURE"),
            "Debug must show the stage: {dbg}"
        );
        assert!(dbg.contains("Execution"));
    }
}
