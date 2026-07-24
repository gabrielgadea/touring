//! Gate trait and core types — the 17-quality-dimension contract.
//!
//! Every gate implements the [`Gate`] trait and returns a [`GateOutcome`]
//! carrying a `score` (0.0-1.0), a `status`, a `severity` (controls whether
//! the change is `BLOCKed`), and a payload of `EvidenceRef`s.
//!
//! The 17 canonical gate IDs are enumerated in [`GateId`]. New gates should
//! be added there (and their `default_weight` set) rather than as ad-hoc string
//! discriminators.

use serde::{Deserialize, Serialize};

/// The 17 canonical quality dimensions of the Touring Elite harness.
///
/// Every value maps 1:1 to a public surface in `touring-cli` (`touring elite
/// gate <id>`), in `touring-server` MCP (`touring_elite_gate`), and in
/// `touring-harness` (`run_harness(change, &default_gates(), ...)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
pub enum GateId {
    /// 01 — Source quality (TDG grade, clippy -D warnings, rustfmt).
    CodeQuality,
    /// 02 — Architecture (cycles=0, blast<10, leaf invariant).
    Architecture,
    /// 03 — Security (CEG capability, deny.toml advisories, landlock V1-V6).
    Security,
    /// 04 — Performance (criterion P99 baseline guard).
    Performance,
    /// 05 — Testing (cargo test, cargo-mutants, llvm-cov gate).
    Testing,
    /// 06 — Documentation (rustdoc -D warnings, `missing_docs`, `sync_metrics`).
    Documentation,
    /// 07 — Best practices (clippy pedantic, cargo semver-checks).
    BestPractices,
    /// 08 — CI/CD & DevOps (ci.yml, release.yml, conventional commits).
    CiCdDevops,
    /// 09 — Modularization (file<300 LOC, fn<100 LOC, leaf invariant).
    Modularization,
    /// 10 — Scalability (no global locks, sharding opportunities).
    Scalability,
    /// 11 — Extensibility (kitchen-sink match, hardcoded branches).
    Extensibility,
    /// 12 — Naming (type-driven, acronym casing, C-CASE/C-CONV).
    Naming,
    /// 13 — Navigability (ast find coverage, broken refs, LSP).
    Navigability,
    /// 14 — Craftsmanship (cognitive <0.7, complexity <15, comment ratio).
    Craftsmanship,
    /// 15 — Dependencies (cargo-deny: advisories, bans, licenses, sources).
    Dependencies,
    /// 16 — UX (`clap_complete` shells, --help coverage, miette errors).
    Ux,
    /// 17 — Product docs (CHANGELOG, ARCHITECTURE.md, skill mirror).
    ProductDocs,
}

impl GateId {
    /// All 17 gate IDs in canonical order.
    pub const ALL: &'static [Self] = &[
        Self::CodeQuality,
        Self::Architecture,
        Self::Security,
        Self::Performance,
        Self::Testing,
        Self::Documentation,
        Self::BestPractices,
        Self::CiCdDevops,
        Self::Modularization,
        Self::Scalability,
        Self::Extensibility,
        Self::Naming,
        Self::Navigability,
        Self::Craftsmanship,
        Self::Dependencies,
        Self::Ux,
        Self::ProductDocs,
    ];

    /// Stable string identifier (`snake_case`) for CLI/MCP/JSON consumers.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::CodeQuality => "code_quality",
            Self::Architecture => "architecture",
            Self::Security => "security",
            Self::Performance => "performance",
            Self::Testing => "testing",
            Self::Documentation => "documentation",
            Self::BestPractices => "best_practices",
            Self::CiCdDevops => "ci_cd_devops",
            Self::Modularization => "modularization",
            Self::Scalability => "scalability",
            Self::Extensibility => "extensibility",
            Self::Naming => "naming",
            Self::Navigability => "navigability",
            Self::Craftsmanship => "craftsmanship",
            Self::Dependencies => "dependencies",
            Self::Ux => "ux",
            Self::ProductDocs => "product_docs",
        }
    }

    /// Reverse lookup from string slug to enum variant.
    /// Returns `None` for unknown slugs.
    #[must_use]
    pub fn from_slug(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|id| id.slug() == s)
    }

    /// Default weight in the composite score. Higher = more impact.
    /// Tuning policy: security/dependencies get 1.5 (P0), craft/extensibility
    /// get 0.6-0.7 (soft), others 0.8-1.0.
    #[must_use]
    pub const fn default_weight(self) -> f32 {
        match self {
            Self::Security | Self::Dependencies => 1.5,
            Self::CodeQuality | Self::Architecture | Self::Testing | Self::Documentation => 1.0,
            Self::CiCdDevops | Self::Modularization => 0.8,
            Self::Performance | Self::Scalability | Self::Craftsmanship => 0.7,
            Self::Extensibility | Self::Ux => 0.6,
            Self::BestPractices | Self::Naming | Self::Navigability | Self::ProductDocs => 0.9,
        }
    }
}

impl std::fmt::Display for GateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.slug())
    }
}

/// Severity classification — controls whether the change is `BLOCKed` or merely
/// flagged in the score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateSeverity {
    /// Failure causes a hard BLOCK in the CEG X7 DECISION stage.
    Block,
    /// Failure downgrades the score but does not BLOCK.
    Warn,
    /// Failure is informational (e.g. cargo bench not yet run).
    Advisory,
}

/// Per-gate execution result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateStatus {
    /// All checks passed (score in [0.95, 1.0]).
    Pass,
    /// Soft failure — score downgraded but change allowed.
    Warn,
    /// Informational — tool unavailable, score 0.5.
    Advisory,
    /// Hard failure — score 0.0, may BLOCK if severity=Block.
    Fail,
    /// External gate (e.g. cargo-deny) — assumed pass.
    External,
    /// Implementation missing for this gate.
    Missing,
}

/// A piece of evidence supporting a gate's verdict. The `path` is relative
/// to the workspace root; `line` is 1-based.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRef {
    /// What kind of evidence this is.
    pub kind: EvidenceKind,
    /// Workspace-relative path (file or symbol).
    pub path: String,
    /// 1-based line number (for `EvidenceKind::File`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// Excerpt (truncated to 200 chars).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    /// Shell command that produced the evidence (for `EvidenceKind::Command`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

/// The provenance category of an [`EvidenceRef`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceKind {
    /// A source file location (path plus 1-based `line`).
    File,
    /// A code symbol resolved via the Touring index.
    Symbol,
    /// The output of a shell command recorded in `command`.
    Command,
    /// A documentation snippet fetched from Context7.
    Context7,
    /// A lesson or fact recalled from Touring memory.
    Memory,
}

/// Per-gate outcome, returned by `Gate::check(&Change)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateOutcome {
    /// The canonical quality dimension this outcome belongs to.
    pub gate_id: GateId,
    /// Whether a failure hard-blocks the change or only flags it.
    pub severity: GateSeverity,
    /// The verdict reached by the gate (pass, warn, fail, etc.).
    pub status: GateStatus,
    /// 0.0-1.0 score.
    pub score: f32,
    /// Effective weight in the composite.
    pub weight: f32,
    /// Human-readable message.
    pub message: String,
    /// Free-form payload (gate-specific structured data).
    pub payload: serde_json::Value,
    /// Evidence supporting this verdict.
    pub evidence: Vec<EvidenceRef>,
    /// Wall-clock time the gate took.
    pub elapsed_ms: u64,
}

impl GateOutcome {
    /// Construct a `Pass` outcome at score 1.0 with no message.
    #[must_use]
    pub fn pass(gate_id: GateId, severity: GateSeverity) -> Self {
        Self {
            gate_id,
            severity,
            status: GateStatus::Pass,
            score: 1.0,
            weight: gate_id.default_weight(),
            message: String::new(),
            payload: serde_json::Value::Null,
            evidence: Vec::new(),
            elapsed_ms: 0,
        }
    }

    /// Construct a `Fail` outcome at score 0.0 with a message.
    #[must_use]
    pub fn fail(gate_id: GateId, severity: GateSeverity, message: impl Into<String>) -> Self {
        Self {
            gate_id,
            severity,
            status: GateStatus::Fail,
            score: 0.0,
            weight: gate_id.default_weight(),
            message: message.into(),
            payload: serde_json::Value::Null,
            evidence: Vec::new(),
            elapsed_ms: 0,
        }
    }

    /// Construct a `Warn` outcome at score 0.5.
    #[must_use]
    pub fn warn(gate_id: GateId, severity: GateSeverity, message: impl Into<String>) -> Self {
        Self {
            gate_id,
            severity,
            status: GateStatus::Warn,
            score: 0.5,
            weight: gate_id.default_weight(),
            message: message.into(),
            payload: serde_json::Value::Null,
            evidence: Vec::new(),
            elapsed_ms: 0,
        }
    }

    /// Construct an `Advisory` outcome at score 0.5.
    #[must_use]
    pub fn advisory(gate_id: GateId, severity: GateSeverity, message: impl Into<String>) -> Self {
        Self {
            gate_id,
            severity,
            status: GateStatus::Advisory,
            score: 0.5,
            weight: gate_id.default_weight(),
            message: message.into(),
            payload: serde_json::Value::Null,
            evidence: Vec::new(),
            elapsed_ms: 0,
        }
    }

    /// Construct an `External` outcome (e.g. cargo-deny assumed pass).
    #[must_use]
    pub fn external(gate_id: GateId, severity: GateSeverity) -> Self {
        Self {
            gate_id,
            severity,
            status: GateStatus::External,
            score: 1.0,
            weight: gate_id.default_weight(),
            message: "external CI step (assumed PASS)".into(),
            payload: serde_json::Value::Null,
            evidence: Vec::new(),
            elapsed_ms: 0,
        }
    }

    /// Attach evidence to this outcome (builder pattern).
    #[must_use]
    pub fn with_evidence(mut self, ev: EvidenceRef) -> Self {
        self.evidence.push(ev);
        self
    }

    /// Attach a structured payload (builder pattern).
    #[must_use]
    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = payload;
        self
    }

    /// Set elapsed time in milliseconds (builder pattern).
    #[must_use]
    pub fn with_elapsed(mut self, ms: u64) -> Self {
        self.elapsed_ms = ms;
        self
    }
}

/// The contract every gate must implement.
///
/// 17 implementations live in `builtins::*`; third parties can add new gates
/// by implementing this trait and passing their `Box<dyn Gate>` to
/// `run_harness`. Use [`default_gates`](crate::builtins::default_gates) for
/// the canonical 17-gate set.
pub trait Gate: Send + Sync {
    /// The gate's canonical ID.
    fn id(&self) -> GateId;

    /// Severity classification.
    fn severity(&self) -> GateSeverity;

    /// Run the gate against a proposed change and return the outcome.
    fn check(&self, change: &crate::change::Change) -> GateOutcome;
}
