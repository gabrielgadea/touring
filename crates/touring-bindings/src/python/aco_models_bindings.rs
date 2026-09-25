//! Python bindings for the ACO model types (Gabriel's order, 24/09/2026:
//! «exporte todos os tipos CLK»).
//!
//! The eight types of `touring_intelligence::rl::aco::models` plus
//! `RlmMemory` and `JaccardComputer`, registered at the level each consumer
//! imports (package for the ACO types and RlmMemory, the `simd` submodule
//! for JaccardComputer).
//!
//! The string contracts are the parity contracts of
//! `analise/scripts/aco/tests/test_aco_rust_parity.py` — measured there, not
//! re-derived: enums stringify lowercase/snake_case (Complexity carries the
//! accent: Critical → "crítica"), `from_str` round-trips them, and an
//! invalid name raises `ValueError` ("Unknown <Type>").

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use touring_intelligence::rl::aco::models as rs;

// ─── The enum macro: one pyclass per ACO enum, with the parity strings ─────

/// Builds a pyo3 enum mirroring an ACO Rust enum: the same variants, the
/// parity `__str__` (measured in the aco parity tests), `from_str`, and
/// bidirectional `From` conversions for future Rust-side use.
macro_rules! aco_enum {
    (
        $(#[$meta:meta])*
        $name:ident, $rs:ty, {
            $( $variant:ident => $string:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        // pyo3 0.29: the automatic `FromPyObject` derive for Clone pyclasses is
        // deprecated — opt in explicitly (the enums are taken as arguments,
        // e.g. `ValidationResult(status=…)`).
        #[pyclass(frozen, eq, eq_int, hash, from_py_object)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $( $variant ),+
        }

        #[pymethods]
        impl $name {
            /// The parity string of the variant (measured in the aco parity
            /// tests) — lowercase/snake_case, never the Rust `Display`.
            fn __str__(&self) -> &'static str {
                match self {
                    $( Self::$variant => $string ),+
                }
            }

            fn __repr__(&self) -> String {
                format!("{}.{}", stringify!($name), self.__str__())
            }

            /// Parse a parity string back into the variant.
            #[classmethod]
            fn from_str(_cls: &Bound<'_, pyo3::types::PyType>, s: &str) -> PyResult<Self> {
                match s {
                    $( $string => Ok(Self::$variant), )+
                    _ => Err(PyValueError::new_err(format!(
                        "Unknown {}: {s:?}", stringify!($name)
                    ))),
                }
            }
        }

        impl From<$rs> for $name {
            fn from(v: $rs) -> Self {
                match v {
                    $( <$rs>::$variant => Self::$variant, )+
                }
            }
        }

        impl From<$name> for $rs {
            fn from(v: $name) -> Self {
                match v {
                    $( $name::$variant => <$rs>::$variant, )+
                }
            }
        }
    };
}

aco_enum! {
    /// Type of a generator node (template, transformer, composer, validator).
    GeneratorType, rs::GeneratorType, {
        Template => "template",
        Transformer => "transformer",
        Composer => "composer",
        Validator => "validator",
    }
}

aco_enum! {
    /// Task complexity tier.
    Complexity, rs::Complexity, {
        Trivial => "trivial",
        Moderate => "moderada",
        High => "alta",
        Critical => "crítica",
    }
}

aco_enum! {
    /// Drift severity relative to the objective.
    DriftLevel, rs::DriftLevel, {
        Ok => "ok",
        Warning => "warning",
        Critical => "critical",
        Halt => "halt",
    }
}

aco_enum! {
    /// Outcome status of a validation check.
    ValidationStatus, rs::ValidationStatus, {
        Pass => "pass",
        Warn => "warn",
        Fail => "fail",
    }
}

aco_enum! {
    /// Execution status of a node in the ACO state machine.
    ExecutionStatus, rs::ExecutionStatus, {
        Pending => "pending",
        Running => "running",
        Success => "success",
        ValidationFailed => "validation_failed",
        ExecutionFailed => "execution_failed",
        PreconditionFailed => "precondition_failed",
        RollbackExecuted => "rollback_executed",
        RollbackFailed => "rollback_failed",
    }
}

// ─── The structs ────────────────────────────────────────────────────────────

/// Scripts of a generator node: execution, validation, rollback.
#[pyclass(name = "GeneratorOutputs", frozen, get_all, eq, from_py_object)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratorOutputs {
    /// Script that performs the generator's action.
    pub execution_script: String,
    /// Script that validates the action's result.
    pub validation_script: String,
    /// Script that rolls the action back on failure.
    pub rollback_script: String,
}

#[pymethods]
impl GeneratorOutputs {
    #[new]
    #[pyo3(signature = (execution_script = String::new(), validation_script = String::new(), rollback_script = String::new()))]
    fn new(
        execution_script: String,
        validation_script: String,
        rollback_script: String,
    ) -> Self {
        Self {
            execution_script,
            validation_script,
            rollback_script,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "GeneratorOutputs(execution_script={:?}, validation_script={:?}, rollback_script={:?})",
            self.execution_script, self.validation_script, self.rollback_script
        )
    }
}

impl From<GeneratorOutputs> for rs::GeneratorOutputs {
    fn from(v: GeneratorOutputs) -> Self {
        Self {
            execution_script: v.execution_script,
            validation_script: v.validation_script,
            rollback_script: v.rollback_script,
        }
    }
}

impl From<rs::GeneratorOutputs> for GeneratorOutputs {
    fn from(v: rs::GeneratorOutputs) -> Self {
        Self {
            execution_script: v.execution_script,
            validation_script: v.validation_script,
            rollback_script: v.rollback_script,
        }
    }
}

/// The contract of a generator: pre/post/invariant conditions.
#[pyclass(name = "GeneratorContract", frozen, get_all, eq, from_py_object)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratorContract {
    /// Condition that must hold before execution.
    pub precondition: String,
    /// Condition guaranteed to hold after execution.
    pub postcondition: String,
    /// Property preserved across execution.
    pub invariant: String,
}

#[pymethods]
impl GeneratorContract {
    #[new]
    #[pyo3(signature = (precondition = String::new(), postcondition = String::new(), invariant = String::new()))]
    fn new(precondition: String, postcondition: String, invariant: String) -> Self {
        Self {
            precondition,
            postcondition,
            invariant,
        }
    }

    /// `true` when pre/post/invariant are all specified (the Rust method).
    fn is_complete(&self) -> bool {
        !self.precondition.is_empty()
            && !self.postcondition.is_empty()
            && !self.invariant.is_empty()
    }

    fn __repr__(&self) -> String {
        format!(
            "GeneratorContract(precondition={:?}, postcondition={:?}, invariant={:?})",
            self.precondition, self.postcondition, self.invariant
        )
    }
}

impl From<GeneratorContract> for rs::GeneratorContract {
    fn from(v: GeneratorContract) -> Self {
        Self {
            precondition: v.precondition,
            postcondition: v.postcondition,
            invariant: v.invariant,
        }
    }
}

impl From<rs::GeneratorContract> for GeneratorContract {
    fn from(v: rs::GeneratorContract) -> Self {
        Self {
            precondition: v.precondition,
            postcondition: v.postcondition,
            invariant: v.invariant,
        }
    }
}

/// The result of a single validation check.
#[pyclass(name = "ValidationResult", frozen, get_all, from_py_object)]
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Orchestration phase the check belongs to.
    pub phase: String,
    /// Name of the validation check.
    pub check_name: String,
    /// Outcome status of the check.
    pub status: ValidationStatus,
    /// Expected value or condition.
    pub expected: String,
    /// Actual observed value or condition.
    pub actual: String,
    /// Human-readable result message.
    pub message: String,
    /// Suggested remediation when the check did not pass.
    pub remediation: Option<String>,
}

#[pymethods]
impl ValidationResult {
    #[new]
    #[pyo3(signature = (phase, check_name, status, expected, actual, message, remediation = None))]
    fn new(
        phase: String,
        check_name: String,
        status: ValidationStatus,
        expected: String,
        actual: String,
        message: String,
        remediation: Option<String>,
    ) -> Self {
        Self {
            phase,
            check_name,
            status,
            expected,
            actual,
            message,
            remediation,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "ValidationResult(phase={:?}, check_name={:?}, status={}, expected={:?}, actual={:?}, message={:?}, remediation={:?})",
            self.phase, self.check_name, self.status.__str__(), self.expected, self.actual, self.message, self.remediation
        )
    }
}

impl From<ValidationResult> for rs::ValidationResult {
    fn from(v: ValidationResult) -> Self {
        Self {
            phase: v.phase,
            check_name: v.check_name,
            status: v.status.into(),
            expected: v.expected,
            actual: v.actual,
            message: v.message,
            remediation: v.remediation,
        }
    }
}

impl From<rs::ValidationResult> for ValidationResult {
    fn from(v: rs::ValidationResult) -> Self {
        Self {
            phase: v.phase,
            check_name: v.check_name,
            status: v.status.into(),
            expected: v.expected,
            actual: v.actual,
            message: v.message,
            remediation: v.remediation,
        }
    }
}

// ─── RlmMemory ─────────────────────────────────────────────────────────────

/// RLM memory storage (SQLite), as the analise's `learning_service` consumes
/// it: `RlmMemory(); mem.read("skill_ranking")`.
///
/// Constructor contract (measured in `touring_intelligence::rl::memory::rlm`):
/// an optional path — omitted, it resolves the current project's canonical
/// memory db (`.claude/touring/memory.db`, walk-up); a path that does not
/// exist is CREATED empty. `read` of an absent key returns None, never
/// raises (the backend's fallback keeps working).
#[pyclass(name = "RlmMemory", unsendable)]
pub struct RlmMemory {
    inner: touring_intelligence::rl::memory::rlm::RlmMemory,
}

/// Resolve the canonical memory db of the current project (the same walk-up
/// the daemon uses: first ancestor with `.claude/` … `.git`, `.touring/`, or
/// `.claude/` itself).
fn default_db_path() -> std::path::PathBuf {
    let start = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    for dir in start.ancestors() {
        let cand = dir.join(".claude").join("touring").join("memory.db");
        if cand.is_file() || dir.join(".touring").is_dir() || dir.join(".git").exists() {
            return cand;
        }
    }
    start.join(".claude").join("touring").join("memory.db")
}

#[pymethods]
impl RlmMemory {
    /// Open (or create) the memory db. `db_path` is optional: omitted, the
    /// current project's canonical `.claude/touring/memory.db`.
    #[new]
    #[pyo3(signature = (db_path = None))]
    fn new(db_path: Option<String>) -> PyResult<Self> {
        let path = db_path
            .map(std::path::PathBuf::from)
            .unwrap_or_else(default_db_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| PyValueError::new_err(format!("cannot create {}: {e}", parent.display())))?;
        }
        let inner = touring_intelligence::rl::memory::rlm::RlmMemory::new(&path)
            .map_err(|e| PyValueError::new_err(format!("cannot open {}: {e}", path.display())))?;
        Ok(Self { inner })
    }

    /// Read a key. Absent → None (never raises). `tier` is optional: given,
    /// it filters exactly; omitted, the tiers are searched in persistence
    /// order (core → reference → working → ephemeral) and the first hit wins.
    #[pyo3(signature = (key, tier = None))]
    fn read(&self, key: &str, tier: Option<String>) -> PyResult<Option<String>> {
        let tiers = [
            touring_intelligence::rl::memory::rlm::MemoryTier::Core,
            touring_intelligence::rl::memory::rlm::MemoryTier::Reference,
            touring_intelligence::rl::memory::rlm::MemoryTier::Working,
            touring_intelligence::rl::memory::rlm::MemoryTier::Ephemeral,
        ];
        let wanted = tier.and_then(|t| tiers.iter().find(|x| x.as_str() == t).copied());
        match wanted {
            Some(t) => self
                .inner
                .get(key, t)
                .map_err(|e| PyValueError::new_err(e.to_string())),
            None => {
                for t in tiers {
                    if let Ok(Some(v)) = self.inner.get(key, t) {
                        return Ok(Some(v));
                    }
                }
                Ok(None)
            }
        }
    }
}

// ─── JaccardComputer (lives in the simd submodule) ──────────────────────────

/// Jaccard similarity computer (set overlap), with batch support.
#[pyclass(name = "JaccardComputer", from_py_object)]
#[derive(Debug, Clone, Default)]
pub struct JaccardComputer {
    inner: touring_simd::JaccardComputer,
}

#[pymethods]
impl JaccardComputer {
    #[new]
    fn new() -> Self {
        Self {
            inner: touring_simd::JaccardComputer::new(),
        }
    }

    /// Jaccard similarity of two sets: |A ∩ B| / |A ∪ B|. Both empty → 1.0;
    /// exactly one empty → 0.0.
    fn jaccard(&self, a: Vec<u32>, b: Vec<u32>) -> f64 {
        use touring_simd::JaccardSimilarity as _;
        self.inner.jaccard(&a, &b)
    }
}

// ─── Registration ───────────────────────────────────────────────────────────

/// Register every ACO model type at package level, and JaccardComputer in
/// the `simd` submodule (created by `simd_bindings::register`, called first).
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<GeneratorType>()?;
    m.add_class::<Complexity>()?;
    m.add_class::<DriftLevel>()?;
    m.add_class::<ValidationStatus>()?;
    m.add_class::<ExecutionStatus>()?;
    m.add_class::<GeneratorOutputs>()?;
    m.add_class::<GeneratorContract>()?;
    m.add_class::<ValidationResult>()?;
    m.add_class::<RlmMemory>()?;

    let simd_any = m.getattr("simd")?;
    let simd = simd_any.cast::<pyo3::types::PyModule>()?;
    simd.add_class::<JaccardComputer>()?;
    Ok(())
}
