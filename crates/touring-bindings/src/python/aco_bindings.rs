//! PyO3 bindings for ACO (Agentic Code Orchestrator) types.
//!
//! Wraps `touring_intelligence::rl::aco` types with PyO3 attributes to expose them
//! to Python. Preserves the EXACT API surface that `scripts/aco/rust_bridge.py`
//! imports from `claude_learning_kernel`.
//!
//! # Backward Compatibility Contract
//!
//! The following symbols MUST be available at module level:
//! - Constants: CRITICAL_WEIGHT, HALT_ITERATIONS, HALT_THRESHOLD, NORMAL_WEIGHT, VETO_THRESHOLD
//! - Classes: AcoGraph, DimResult, TrackerReport, EventProjector
//! - Functions: py_build_report, py_compute_composite, py_determine_status, verify_chain_parallel

use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use touring_intelligence::rl::aco::{
    self as tl_aco,
    models::{
        GeneratorContract as TlGeneratorContract, GeneratorNode as TlGeneratorNode,
        GeneratorOutputs as TlGeneratorOutputs, GeneratorType as TlGeneratorType,
    },
    tracker::{
        CRITICAL_DIMS, CRITICAL_WEIGHT, HALT_ITERATIONS, HALT_THRESHOLD, NORMAL_WEIGHT,
        VETO_THRESHOLD,
    },
};

// ═══════════════════════════════════════════════════════════════════════════
// Tracker types — DimResult, TrackerReport, TrackerStatus
// ═══════════════════════════════════════════════════════════════════════════

/// Tracker status (mirrors touring_intelligence::rl::aco::tracker::TrackerStatus).
#[pyclass(eq, frozen, skip_from_py_object)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackerStatus {
    Pass,
    Veto,
    Halt,
}

#[pymethods]
impl TrackerStatus {
    fn __str__(&self) -> &str {
        match self {
            Self::Pass => "PASS",
            Self::Veto => "VETO",
            Self::Halt => "HALT",
        }
    }
}

impl From<tl_aco::TrackerStatus> for TrackerStatus {
    fn from(s: tl_aco::TrackerStatus) -> Self {
        match s {
            tl_aco::TrackerStatus::Pass => Self::Pass,
            tl_aco::TrackerStatus::Veto => Self::Veto,
            tl_aco::TrackerStatus::Halt => Self::Halt,
        }
    }
}

/// Result for a single dimension (9 per report).
#[pyclass(frozen, get_all, from_py_object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimResult {
    /// Dimension identifier (e.g. "D1"..."D9").
    pub dim_id: String,
    /// Human-readable dimension name.
    pub name: String,
    /// Score in [0.0, 1.0].
    pub score: f64,
    /// Number of checks that passed.
    pub checks_passed: u32,
    /// Total number of checks evaluated.
    pub checks_total: u32,
    /// Detail strings per check.
    pub details: Vec<String>,
}

#[pymethods]
impl DimResult {
    #[new]
    #[pyo3(signature = (dim_id, name, score, checks_passed, checks_total, details=vec![]))]
    fn new(
        dim_id: String,
        name: String,
        score: f64,
        checks_passed: u32,
        checks_total: u32,
        details: Vec<String>,
    ) -> Self {
        Self {
            dim_id,
            name,
            score,
            checks_passed,
            checks_total,
            details,
        }
    }

    /// Weight for this dimension (1.5x for critical, 1.0x otherwise).
    #[getter]
    fn weight_py(&self) -> f64 {
        self.weight()
    }

    /// Weighted score = score * weight.
    #[getter]
    fn weighted_score_py(&self) -> f64 {
        self.weighted_score()
    }

    /// Is this dimension passing the VETO threshold?
    fn is_passing_py(&self) -> bool {
        self.is_passing()
    }

    fn __repr__(&self) -> String {
        format!(
            "DimResult(id='{}', name='{}', score={:.4})",
            self.dim_id, self.name, self.score
        )
    }

    /// Convert to a Python dict with all fields.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
        let dict = pyo3::types::PyDict::new(py);
        dict.set_item("dim_id", &self.dim_id)?;
        dict.set_item("name", &self.name)?;
        dict.set_item("score", self.score)?;
        dict.set_item("checks_passed", self.checks_passed)?;
        dict.set_item("checks_total", self.checks_total)?;
        dict.set_item("details", &self.details)?;
        dict.set_item("weight", self.weight())?;
        dict.set_item("weighted_score", self.weighted_score())?;
        dict.set_item("is_passing", self.is_passing())?;
        Ok(dict)
    }
}

impl DimResult {
    fn weight(&self) -> f64 {
        if CRITICAL_DIMS.contains(&self.dim_id.as_str()) {
            CRITICAL_WEIGHT
        } else {
            NORMAL_WEIGHT
        }
    }

    fn weighted_score(&self) -> f64 {
        self.score * self.weight()
    }

    fn is_passing(&self) -> bool {
        self.score >= VETO_THRESHOLD
    }

    fn to_tl(&self) -> tl_aco::tracker::DimResult {
        tl_aco::tracker::DimResult {
            dim_id: self.dim_id.clone(),
            name: self.name.clone(),
            score: self.score,
            checks_passed: self.checks_passed,
            checks_total: self.checks_total,
            details: self.details.clone(),
        }
    }

    fn from_tl(d: &tl_aco::tracker::DimResult) -> Self {
        Self {
            dim_id: d.dim_id.clone(),
            name: d.name.clone(),
            score: d.score,
            checks_passed: d.checks_passed,
            checks_total: d.checks_total,
            details: d.details.clone(),
        }
    }
}

/// Full 9-dimension tracker report.
#[pyclass(frozen, get_all, skip_from_py_object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerReport {
    /// All 9 dimension results.
    pub dims: Vec<DimResult>,
    /// Weighted composite score.
    pub composite: f64,
    /// Overall tracker status.
    pub status: TrackerStatus,
    /// Current iteration number.
    pub iteration: u32,
}

#[pymethods]
impl TrackerReport {
    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!(
            "TrackerReport(status={}, composite={:.4}, dims={})",
            self.status.__str__(),
            self.composite,
            self.dims.len()
        )
    }

    fn to_dict_json(&self) -> PyResult<String> {
        let mut map = serde_json::Map::new();
        map.insert(
            "status".into(),
            serde_json::Value::String(self.status.__str__().to_string()),
        );
        map.insert("composite".into(), serde_json::json!(self.composite));
        map.insert("iteration".into(), serde_json::json!(self.iteration));
        let dims: Vec<serde_json::Value> = self
            .dims
            .iter()
            .map(|d| {
                serde_json::json!({
                    "id": d.dim_id,
                    "name": d.name,
                    "score": d.score,
                    "checks_passed": d.checks_passed,
                    "checks_total": d.checks_total,
                    "weight": d.weight(),
                })
            })
            .collect();
        map.insert("dims".into(), serde_json::json!(dims));
        serde_json::to_string(&map)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tracker free functions
// ═══════════════════════════════════════════════════════════════════════════

/// Compute composite score from Python DimResult list.
#[pyfunction]
pub fn py_compute_composite(dims: Vec<DimResult>) -> f64 {
    let tl_dims: Vec<tl_aco::tracker::DimResult> = dims.iter().map(|d| d.to_tl()).collect();
    tl_aco::compute_composite(&tl_dims)
}

/// Determine status from Python DimResult list.
#[pyfunction]
pub fn py_determine_status(dims: Vec<DimResult>, composite: f64) -> TrackerStatus {
    let tl_dims: Vec<tl_aco::tracker::DimResult> = dims.iter().map(|d| d.to_tl()).collect();
    tl_aco::determine_status(&tl_dims, composite).into()
}

/// Build full report from Python DimResult list.
#[pyfunction]
pub fn py_build_report(dims: Vec<DimResult>, iteration: u32) -> TrackerReport {
    let tl_dims: Vec<tl_aco::tracker::DimResult> = dims.iter().map(|d| d.to_tl()).collect();
    let tl_report = tl_aco::build_report(tl_dims, iteration);
    TrackerReport {
        dims: tl_report.dims.iter().map(DimResult::from_tl).collect(),
        composite: tl_report.composite,
        status: tl_report.status.into(),
        iteration: tl_report.iteration,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// AcoGraph — PyO3 wrapper around MutableGeneratorGraph
// ═══════════════════════════════════════════════════════════════════════════

/// PyO3 wrapper for GeneratorNode (used as input to AcoGraph.add_node).
#[pyclass(frozen, get_all, from_py_object)]
#[derive(Debug, Clone)]
pub struct GeneratorNode {
    pub id: String,
    pub description: String,
    pub generator_type: String,
    pub inputs_data: Vec<String>,
    pub inputs_templates: Vec<String>,
    pub inputs_constraints: Vec<String>,
    pub execution_script: String,
    pub validation_script: String,
    pub rollback_script: String,
    pub precondition: String,
    pub postcondition: String,
    pub invariant: String,
    pub acceptance_criteria: String,
    pub depends_on: Vec<String>,
}

#[pymethods]
impl GeneratorNode {
    #[new]
    #[pyo3(signature = (id, description, generator_type, inputs_data, inputs_templates, inputs_constraints, execution_script, validation_script, rollback_script, precondition, postcondition, invariant, acceptance_criteria, depends_on))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        id: String,
        description: String,
        generator_type: String,
        inputs_data: Vec<String>,
        inputs_templates: Vec<String>,
        inputs_constraints: Vec<String>,
        execution_script: String,
        validation_script: String,
        rollback_script: String,
        precondition: String,
        postcondition: String,
        invariant: String,
        acceptance_criteria: String,
        depends_on: Vec<String>,
    ) -> Self {
        Self {
            id,
            description,
            generator_type,
            inputs_data,
            inputs_templates,
            inputs_constraints,
            execution_script,
            validation_script,
            rollback_script,
            precondition,
            postcondition,
            invariant,
            acceptance_criteria,
            depends_on,
        }
    }
}

impl GeneratorNode {
    fn to_tl(&self) -> TlGeneratorNode {
        let gt = match self.generator_type.as_str() {
            "transformer" => TlGeneratorType::Transformer,
            "composer" => TlGeneratorType::Composer,
            "validator" => TlGeneratorType::Validator,
            _ => TlGeneratorType::Template,
        };
        TlGeneratorNode {
            id: self.id.clone(),
            description: self.description.clone(),
            generator_type: gt,
            inputs_data: self.inputs_data.clone(),
            inputs_templates: self.inputs_templates.clone(),
            inputs_constraints: self.inputs_constraints.clone(),
            outputs: TlGeneratorOutputs {
                execution_script: self.execution_script.clone(),
                validation_script: self.validation_script.clone(),
                rollback_script: self.rollback_script.clone(),
            },
            contract: TlGeneratorContract {
                precondition: self.precondition.clone(),
                postcondition: self.postcondition.clone(),
                invariant: self.invariant.clone(),
            },
            acceptance_criteria: self.acceptance_criteria.clone(),
            depends_on: self.depends_on.clone(),
        }
    }
}

/// PyO3 wrapper around MutableGeneratorGraph — exposed as `AcoGraph` to Python.
#[pyclass(name = "AcoGraph")]
#[derive(Debug)]
pub struct PyAcoGraph {
    inner: tl_aco::MutableGeneratorGraph,
}

#[pymethods]
impl PyAcoGraph {
    #[new]
    fn new() -> Self {
        Self {
            inner: tl_aco::MutableGeneratorGraph::new(),
        }
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn __repr__(&self) -> String {
        format!("AcoGraph(nodes={})", self.inner.len())
    }

    /// Add a generator node.
    fn add_node(&mut self, node: GeneratorNode) -> PyResult<()> {
        self.inner
            .add_node(node.to_tl())
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Remove a generator node.
    fn remove_node(&mut self, node_id: &str) -> PyResult<()> {
        self.inner
            .remove_node(node_id)
            .map(|_| ())
            .map_err(|e| pyo3::exceptions::PyKeyError::new_err(e.to_string()))
    }

    /// Topological sort — deterministic execution order.
    fn topological_sort(&mut self) -> PyResult<Vec<String>> {
        self.inner
            .topological_sort()
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Groups that can run in parallel (2+ nodes per group).
    fn detect_parallelizable(&mut self) -> PyResult<Vec<Vec<String>>> {
        self.inner
            .detect_parallelizable()
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// All execution levels (including single-node groups).
    fn get_all_levels(&mut self) -> PyResult<Vec<Vec<String>>> {
        self.inner
            .get_all_levels()
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Longest dependency chain.
    fn get_critical_path(&mut self) -> PyResult<Vec<String>> {
        self.inner
            .get_critical_path()
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Validate DAG invariant (no cycles).
    fn validate_acyclic(&mut self) -> PyResult<bool> {
        self.inner
            .validate_acyclic()
            .map(|()| true)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Validate all contracts are complete.
    fn validate_contracts(&self) -> Vec<String> {
        self.inner.validate_contracts()
    }

    /// Validate all dependencies reference existing nodes.
    fn validate_dependencies(&self) -> Vec<String> {
        self.inner.validate_dependencies()
    }

    /// Freeze to immutable graph model (returns JSON).
    fn freeze(&mut self, objective_hash: Option<String>) -> PyResult<String> {
        let model = self
            .inner
            .freeze(objective_hash.as_deref())
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        serde_json::to_string(&model)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ESAA — EventProjector + verify_chain_parallel
// ═══════════════════════════════════════════════════════════════════════════

/// Stored domain event — mirrors SQLite `events` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredEvent {
    seq: i64,
    event_id: String,
    agent_id: String,
    event_type: String,
    timestamp: f64,
    payload: serde_json::Value,
    prev_hash: String,
    event_hash: String,
    #[serde(default)]
    correlation_id: Option<String>,
    #[serde(default)]
    causation_id: Option<String>,
}

/// Canonical JSON: deterministic key ordering, compact separators.
///
/// INVARIANT: `serde_json::Value::Object` uses `BTreeMap` internally,
/// which produces sorted keys by default. This is REQUIRED for
/// reproducible SHA-256 hash chain verification. If `serde_json`'s
/// `preserve_order` feature is ever enabled (which switches to `IndexMap`),
/// hash chain verification will silently break. Do NOT enable that feature.
fn canonical_json(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// SHA-256 event hash.
fn compute_hash_rs(prev_hash: &str, event_id: &str, canonical_payload: &str) -> String {
    let content = format!("{prev_hash}:{event_id}:{canonical_payload}");
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Python-exposed event projector (D3).
#[pyclass(name = "EventProjector")]
#[derive(Debug)]
pub struct PyEventProjector {
    events: Vec<StoredEvent>,
}

impl Default for PyEventProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[pymethods]
impl PyEventProjector {
    #[new]
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Load events from a list of JSON strings, replacing any existing events.
    pub fn load_events(&mut self, events_json: Vec<String>) -> PyResult<()> {
        let mut parsed = Vec::with_capacity(events_json.len());
        for (i, s) in events_json.iter().enumerate() {
            let ev: StoredEvent = serde_json::from_str(s).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "Invalid event JSON at index {i}: {e}"
                ))
            })?;
            parsed.push(ev);
        }
        self.events = parsed;
        Ok(())
    }

    /// Project the cumulative state for `agent_id` by merging payloads in seq order.
    pub fn project_agent_state(&self, agent_id: &str) -> PyResult<Option<String>> {
        let mut agent_events: Vec<&StoredEvent> = self
            .events
            .iter()
            .filter(|e| e.agent_id == agent_id)
            .collect();
        if agent_events.is_empty() {
            return Ok(None);
        }
        agent_events.sort_by_key(|e| e.seq);
        let mut state = serde_json::Map::new();
        for ev in &agent_events {
            if let serde_json::Value::Object(ref map) = ev.payload {
                for (k, v) in map {
                    state.insert(k.clone(), v.clone());
                }
            }
        }
        let result = serde_json::to_string(&serde_json::Value::Object(state))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(Some(result))
    }

    /// Verify SHA-256 hash chain for all events of `agent_id`.
    pub fn verify_hash_chain(&self, py: Python<'_>, agent_id: &str) -> bool {
        let mut agent_events: Vec<&StoredEvent> = self
            .events
            .iter()
            .filter(|e| e.agent_id == agent_id)
            .collect();
        if agent_events.is_empty() {
            return true;
        }
        agent_events.sort_by_key(|e| e.seq);
        let events_owned: Vec<StoredEvent> = agent_events.into_iter().cloned().collect();
        py.detach(|| {
            for ev in &events_owned {
                let canonical = canonical_json(&ev.payload);
                let expected = compute_hash_rs(&ev.prev_hash, &ev.event_id, &canonical);
                if ev.event_hash != expected {
                    return false;
                }
            }
            for w in events_owned.windows(2) {
                // SAFETY: windows(2) guarantees each slice w has exactly 2 elements.
                #[allow(clippy::indexing_slicing)]
                if w[1].prev_hash != w[0].event_hash {
                    return false;
                }
            }
            true
        })
    }

    /// Return events for `agent_id` with `seq >= since_seq` as JSON strings.
    pub fn replay_since(&self, agent_id: &str, since_seq: i64) -> PyResult<Vec<String>> {
        let mut agent_events: Vec<&StoredEvent> = self
            .events
            .iter()
            .filter(|e| e.agent_id == agent_id && e.seq >= since_seq)
            .collect();
        agent_events.sort_by_key(|e| e.seq);
        agent_events
            .iter()
            .map(|e| {
                serde_json::to_string(e)
                    .map_err(|err| pyo3::exceptions::PyRuntimeError::new_err(err.to_string()))
            })
            .collect()
    }

    pub fn __len__(&self) -> usize {
        self.events.len()
    }
}

/// Parallel SHA-256 hash chain verifier using Rayon (D4).
#[pyfunction]
pub fn verify_chain_parallel(py: Python<'_>, events_json: Vec<String>) -> PyResult<bool> {
    if events_json.is_empty() {
        return Ok(true);
    }
    let events: Vec<serde_json::Value> = events_json
        .iter()
        .enumerate()
        .map(|(i, s)| {
            serde_json::from_str::<serde_json::Value>(s).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "Invalid event JSON at index {i}: {e}"
                ))
            })
        })
        .collect::<PyResult<_>>()?;
    let valid = py.detach(|| {
        use rayon::prelude::*;
        // Linkage check (sequential)
        let linkage_ok = events.windows(2).all(|w| {
            let (h0, p1) = match w {
                [a, b] => (
                    a.get("event_hash").and_then(|v| v.as_str()).unwrap_or(""),
                    b.get("prev_hash").and_then(|v| v.as_str()).unwrap_or(""),
                ),
                _ => return true, // windows(2) guarantees len==2; unreachable
            };
            h0 == p1
        });
        if !linkage_ok {
            return false;
        }
        // Hash recomputation in parallel
        events.par_iter().all(|ev| {
            let prev_hash = ev.get("prev_hash").and_then(|v| v.as_str()).unwrap_or("");
            let event_id = ev.get("event_id").and_then(|v| v.as_str()).unwrap_or("");
            let stored_hash = ev.get("event_hash").and_then(|v| v.as_str()).unwrap_or("");
            let canonical = if let Some(p) = ev.get("payload") {
                canonical_json(p)
            } else {
                "{}".to_string()
            };
            let expected = compute_hash_rs(prev_hash, event_id, &canonical);
            stored_hash == expected
        })
    });
    Ok(valid)
}

// ═══════════════════════════════════════════════════════════════════════════
// ESAA — QueryCache and EventBuffer Python bindings
// ═══════════════════════════════════════════════════════════════════════════

/// Python-exposed QueryCache\<String\> for caching query results.
///
/// Thread-safe LRU cache with optional TTL expiry and hit/miss statistics.
#[pyclass(name = "QueryCache")]
pub struct PyQueryCache {
    inner: touring_intelligence::rl::aco::esaa::QueryCache<String>,
}

#[pymethods]
impl PyQueryCache {
    /// Create a new QueryCache.
    ///
    /// Args:
    ///     max_size: Maximum number of entries.
    ///     ttl_ms: Optional time-to-live in milliseconds.
    #[new]
    #[pyo3(signature = (max_size, ttl_ms=None))]
    fn new(max_size: usize, ttl_ms: Option<u64>) -> Self {
        Self {
            inner: touring_intelligence::rl::aco::esaa::QueryCache::new(max_size, ttl_ms),
        }
    }

    /// Get a cached value by key. Returns None if not found or expired.
    fn get(&self, key: &str) -> Option<String> {
        self.inner.get(key)
    }

    /// Insert or update a key-value pair.
    fn put(&self, key: String, value: String) {
        self.inner.put(key, value);
    }

    /// Remove a specific key. Returns True if the key existed.
    fn invalidate(&self, key: &str) -> bool {
        self.inner.invalidate(key)
    }

    /// Remove all keys containing the pattern substring.
    /// Returns the number of entries removed.
    fn invalidate_pattern(&self, pattern: &str) -> usize {
        self.inner.invalidate_pattern(pattern)
    }

    /// Clear all entries. Returns the number of entries removed.
    fn clear(&self) -> usize {
        self.inner.clear()
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn __contains__(&self, key: &str) -> bool {
        self.inner.contains(key)
    }

    fn __repr__(&self) -> String {
        let s = self.inner.stats();
        format!(
            "QueryCache(len={}, hit_rate={:.2})",
            self.inner.len(),
            s.hit_rate()
        )
    }

    /// Get cache statistics as JSON string.
    fn stats(&self) -> PyResult<String> {
        let s = self.inner.stats();
        serde_json::to_string(&s)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Get current hit rate as float in [0.0, 1.0].
    fn hit_rate(&self) -> f64 {
        self.inner.stats().hit_rate()
    }
}

/// Python-exposed EventBuffer for batched event processing.
///
/// Write-ahead batch buffer that accumulates events and flushes them
/// when the batch reaches max_batch_size or max_age_ms elapses.
#[pyclass(name = "EventBuffer")]
pub struct PyEventBuffer {
    inner: touring_intelligence::rl::aco::esaa::EventBuffer,
}

#[pymethods]
impl PyEventBuffer {
    /// Create a new EventBuffer.
    ///
    /// Args:
    ///     max_batch_size: Maximum events before auto-ready (default: 100).
    ///     max_age_ms: Maximum milliseconds before auto-ready (default: 50).
    #[new]
    #[pyo3(signature = (max_batch_size=100, max_age_ms=50))]
    fn new(max_batch_size: usize, max_age_ms: u64) -> Self {
        Self {
            inner: touring_intelligence::rl::aco::esaa::EventBuffer::new(
                touring_intelligence::rl::aco::esaa::EventBufferConfig {
                    max_batch_size,
                    max_age_ms,
                },
            ),
        }
    }

    /// Push an event (as JSON string) into the buffer.
    ///
    /// Raises RuntimeError if the buffer has been shut down.
    fn push(&self, event_json: String) -> PyResult<()> {
        self.inner
            .push(event_json)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Flush all buffered events, returning them as a list.
    fn flush(&self) -> Vec<String> {
        self.inner.flush()
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    /// Check if the buffer is ready to flush (batch full or age exceeded).
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }

    /// Shut down the buffer. No more events can be pushed after this.
    ///
    /// Args:
    ///     flush_remaining: If True, returns remaining events.
    fn shutdown(&self, flush_remaining: bool) -> Option<Vec<String>> {
        self.inner.shutdown(flush_remaining)
    }

    fn __repr__(&self) -> String {
        format!(
            "EventBuffer(len={}, ready={})",
            self.inner.len(),
            self.inner.is_ready()
        )
    }

    /// Get buffer statistics as JSON string.
    fn stats(&self) -> PyResult<String> {
        let s = self.inner.stats();
        serde_json::to_string(&s)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Module registration
// ═══════════════════════════════════════════════════════════════════════════

/// Register all ACO PyO3 classes and functions in the parent module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Tracker classes
    m.add_class::<TrackerStatus>()?;
    m.add_class::<DimResult>()?;
    m.add_class::<TrackerReport>()?;

    // Tracker functions
    m.add_function(wrap_pyfunction!(py_compute_composite, m)?)?;
    m.add_function(wrap_pyfunction!(py_determine_status, m)?)?;
    m.add_function(wrap_pyfunction!(py_build_report, m)?)?;

    // Constants (MUST match touring_intelligence::rl::aco::tracker)
    m.add("VETO_THRESHOLD", VETO_THRESHOLD)?;
    m.add("HALT_THRESHOLD", HALT_THRESHOLD)?;
    m.add("HALT_ITERATIONS", HALT_ITERATIONS)?;
    m.add("CRITICAL_WEIGHT", CRITICAL_WEIGHT)?;
    m.add("NORMAL_WEIGHT", NORMAL_WEIGHT)?;

    // Graph
    m.add_class::<PyAcoGraph>()?;
    m.add_class::<GeneratorNode>()?;

    // ESAA — EventProjector + verify_chain_parallel (backward compat)
    m.add_class::<PyEventProjector>()?;
    m.add_function(wrap_pyfunction!(verify_chain_parallel, m)?)?;

    // ESAA — QueryCache and EventBuffer (new)
    m.add_class::<PyQueryCache>()?;
    m.add_class::<PyEventBuffer>()?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dim_result_weight_critical() {
        let d = DimResult {
            dim_id: "D1".into(),
            name: "Precision".into(),
            score: 0.9,
            checks_passed: 8,
            checks_total: 9,
            details: vec![],
        };
        assert_eq!(d.weight(), CRITICAL_WEIGHT);
    }

    #[test]
    fn test_dim_result_weight_normal() {
        let d = DimResult {
            dim_id: "D3".into(),
            name: "Performance".into(),
            score: 0.9,
            checks_passed: 8,
            checks_total: 9,
            details: vec![],
        };
        assert_eq!(d.weight(), NORMAL_WEIGHT);
    }

    #[test]
    fn test_tracker_status_conversion() {
        let ts: TrackerStatus = tl_aco::TrackerStatus::Pass.into();
        assert_eq!(ts, TrackerStatus::Pass);
        let ts: TrackerStatus = tl_aco::TrackerStatus::Veto.into();
        assert_eq!(ts, TrackerStatus::Veto);
        let ts: TrackerStatus = tl_aco::TrackerStatus::Halt.into();
        assert_eq!(ts, TrackerStatus::Halt);
    }

    #[test]
    fn test_canonical_json_sorted_keys() {
        let val = serde_json::json!({"z": 1, "a": 2});
        let canonical = canonical_json(&val);
        // serde_json with BTreeMap produces sorted keys
        assert!(canonical.contains("\"a\":2"));
        assert!(canonical.contains("\"z\":1"));
    }

    #[test]
    fn test_compute_hash_rs() {
        let hash = compute_hash_rs("prev", "event1", "{\"a\":1}");
        assert_eq!(hash.len(), 64); // SHA-256 hex = 64 chars
    }

    #[test]
    fn test_constants_match() {
        assert!((VETO_THRESHOLD - 0.80).abs() < 1e-10);
        assert!((HALT_THRESHOLD - 0.50).abs() < 1e-10);
        assert_eq!(HALT_ITERATIONS, 3);
        assert!((CRITICAL_WEIGHT - 1.5).abs() < 1e-10);
        assert!((NORMAL_WEIGHT - 1.0).abs() < 1e-10);
    }

    // ── QueryCache tests ───────────────────────────────────────────────

    #[test]
    fn test_query_cache_basic() {
        let cache = PyQueryCache::new(10, None);
        cache.put("key1".into(), "value1".into());
        assert_eq!(cache.get("key1"), Some("value1".into()));
        assert_eq!(cache.__len__(), 1);
        assert!(cache.__contains__("key1"));
        assert!(!cache.__contains__("nonexistent"));
    }

    #[test]
    fn test_query_cache_invalidate() {
        let cache = PyQueryCache::new(10, None);
        cache.put("key1".into(), "value1".into());
        cache.put("key2".into(), "value2".into());
        assert!(cache.invalidate("key1"));
        assert!(!cache.invalidate("key1")); // already removed
        assert_eq!(cache.__len__(), 1);
    }

    #[test]
    fn test_query_cache_invalidate_pattern() {
        let cache = PyQueryCache::new(10, None);
        cache.put("query:user:1".into(), "v1".into());
        cache.put("query:user:2".into(), "v2".into());
        cache.put("query:order:1".into(), "v3".into());
        let removed = cache.invalidate_pattern("user");
        assert_eq!(removed, 2);
        assert_eq!(cache.__len__(), 1);
    }

    #[test]
    fn test_query_cache_clear() {
        let cache = PyQueryCache::new(10, None);
        cache.put("a".into(), "1".into());
        cache.put("b".into(), "2".into());
        let cleared = cache.clear();
        assert_eq!(cleared, 2);
        assert_eq!(cache.__len__(), 0);
    }

    #[test]
    fn test_query_cache_hit_rate() {
        let cache = PyQueryCache::new(10, None);
        cache.put("k".into(), "v".into());
        let _ = cache.get("k"); // hit
        let _ = cache.get("missing"); // miss
        let rate = cache.hit_rate();
        assert!((rate - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_query_cache_stats_json() {
        let cache = PyQueryCache::new(10, None);
        cache.put("k".into(), "v".into());
        let json = cache.stats().unwrap();
        assert!(json.contains("\"hits\""));
        assert!(json.contains("\"misses\""));
        assert!(json.contains("\"max_size\""));
    }

    // ── EventBuffer tests ──────────────────────────────────────────────

    #[test]
    fn test_event_buffer_basic() {
        let buf = PyEventBuffer::new(100, 5000);
        buf.push("{\"event\": \"test\"}".into()).unwrap();
        assert_eq!(buf.__len__(), 1);
        let batch = buf.flush();
        assert_eq!(batch.len(), 1);
        assert_eq!(buf.__len__(), 0);
    }

    #[test]
    fn test_event_buffer_is_ready_by_size() {
        let buf = PyEventBuffer::new(2, 60_000);
        assert!(!buf.is_ready());
        buf.push("e1".into()).unwrap();
        assert!(!buf.is_ready());
        buf.push("e2".into()).unwrap();
        assert!(buf.is_ready());
    }

    #[test]
    fn test_event_buffer_shutdown() {
        let buf = PyEventBuffer::new(100, 50);
        buf.push("e1".into()).unwrap();
        let remaining = buf.shutdown(true);
        assert_eq!(remaining.unwrap().len(), 1);
        // After shutdown, push should fail
        assert!(buf.push("e2".into()).is_err());
    }

    #[test]
    fn test_event_buffer_stats_json() {
        let buf = PyEventBuffer::new(100, 50);
        buf.push("e1".into()).unwrap();
        let json = buf.stats().unwrap();
        assert!(json.contains("\"buffer_size\""));
        assert!(json.contains("\"flush_count\""));
    }
}
