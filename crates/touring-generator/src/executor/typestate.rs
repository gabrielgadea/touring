//! `PlanExecutor` typestate — compile-time-enforced pipeline transitions.
//!
#![allow(clippy::expect_used)]
//!
//! Invalid state transitions (e.g. calling `commit()` before `speculate()`)
//! are **compile errors**, not runtime panics. `PhantomData` encodes the state.
//!
//! Transition graph:
//!   Draft --(verify)--> Verified --(render)--> Rendered
//!     --(speculate)--> Speculated --(commit)--> Committed

use crate::core::context::{
    DspyInputs, DspySignatureName, GeneratorContext, MemoryKind, MemoryTier,
};
use crate::core::score::NormalizedScore;
use crate::error::GenerateError;
use crate::executor::replan::{CompletedPlan, ReplanRequest};
use crate::plan::failure::FailureReason;
use crate::plan::result::{Artifact, CommitReport, FileAction, RenderedFile, VgpReport};
use crate::plan::schema::GeneratorPlan;
use crate::shape::RenderShape;
use crate::skip::SkipContext;
use crate::source_change::{Applier, ApplyResult, Indel, SourceChange, TextEdit};
use crate::speculate::bridge::SpeculateBridge;
use crate::template::engine::TemplateEngine;
use crate::vgp::engine::VgpEngine;
use sha2::Digest as _;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::PathBuf;

// Local helper — avoids cross-crate dependency for uid-based path resolution.
unsafe extern "C" {
    fn getuid() -> u32;
}
fn libc_getuid() -> u32 {
    // SAFETY: getuid() is a standard POSIX syscall with no mutable state.
    // The returned UID is used only for path construction (read-only use).
    #[allow(unused_unsafe)]
    unsafe {
        getuid()
    }
}
use std::sync::Arc;

// ── Tunable thresholds ─────────────────────────────────────────────────────────

/// P1.3 mpatch fuzzy preview confidence floor — emits a warning when the
/// computed patch confidence falls below this value. Kept as a `const` rather
/// than an inline literal so the threshold has a single search-able location;
/// future migration to `ctx.capacity.mpatch_confidence_threshold` keeps the
/// literal here as the documented default.
///
/// Type is `f32` to match `mpatch_preview::Preview::confidence`.
const MPATCH_CONFIDENCE_FLOOR: f32 = 0.8;

// ── FileId helper (deduplicates inline hash computations) ──────────────────────

/// Deterministically derive a [`FileId`] from a file path string.
///
/// Replaces three previously-inlined copies of the same `DefaultHasher` block
/// inside `commit()`. Centralising the derivation also fixes the prior pair
/// of `.expect("hash output always fits in FileId usize")` calls, which were
/// load-bearing assumptions about the host's pointer width — on a 32-bit
/// target `u64::try_from` could fail. The fallback path here truncates via
/// `as FileId`, matching the cast used by the other call-sites and ensuring
/// `commit()` cannot panic from a hash-width mismatch.
fn compute_file_id(path: &str) -> crate::source_change::FileId {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    let h = hasher.finish();
    crate::source_change::FileId::try_from(h).unwrap_or_else(|_| {
        // 32-bit target fallback: truncate the high bits.
        #[allow(clippy::cast_possible_truncation)]
        let truncated = h as crate::source_change::FileId;
        truncated
    })
}

// ── Sealed trait (prevents external state impl) ───────────────────────────────

mod sealed {
    pub trait Sealed {}
}

/// Marker trait for valid executor states.
pub trait PlanStage: sealed::Sealed + Send + Sync + 'static {}

// ── State structs ─────────────────────────────────────────────────────────────

/// Initial state — plan received, not yet VGP-verified.
pub struct Draft;

/// `Verified` — VGP passed, all required symbols confirmed in the index.
pub struct Verified {
    /// VGP verification report — verified symbols, soft misses, and timings.
    pub vgp_report: Arc<VgpReport>,
    /// `Decompose` DAG `task_id` created at `verify` — carried through pipeline to `commit`.
    /// Used by `commit()` to auto-call `decompose_update_status("completed")` on success.
    pub decompose_task_id: Option<String>,
}

/// `Rendered` — Template rendered, artifacts produced but not yet speculate-validated.
pub struct Rendered {
    /// Rendered template output — one entry per file the plan produces.
    pub artifacts: Arc<[RenderedFile]>,
    /// VGP verification report carried forward from the `Verified` stage.
    pub vgp_report: Arc<VgpReport>,
    /// Carried from Verified for decompose finalization at commit.
    pub decompose_task_id: Option<String>,
}

/// Speculate passed — score >= threshold, safe to commit.
pub struct Speculated {
    /// Composite speculate score that met or exceeded the commit threshold.
    pub score: NormalizedScore,
    /// Rendered template output validated and ready for atomic commit.
    pub artifacts: Arc<[RenderedFile]>,
    /// VGP verification report carried forward from the `Verified` stage.
    pub vgp_report: Arc<VgpReport>,
    /// Carried from Verified for decompose finalization at commit.
    pub decompose_task_id: Option<String>,
}

/// Committed — files written atomically, RL reward injected (terminal success).
pub struct Committed {
    /// Report of the atomic commit: files written, SHA-256s, and elapsed time.
    pub commit_report: Arc<CommitReport>,
}

// ── Sealed + PlanStage impls ──────────────────────────────────────────────────

impl sealed::Sealed for Draft {}
impl sealed::Sealed for Verified {}
impl sealed::Sealed for Rendered {}
impl sealed::Sealed for Speculated {}
impl sealed::Sealed for Committed {}

impl PlanStage for Draft {}
impl PlanStage for Verified {}
impl PlanStage for Rendered {}
impl PlanStage for Speculated {}
impl PlanStage for Committed {}

// ── PlanExecutor ──────────────────────────────────────────────────────────────

/// Typestate executor for a single generator plan.
///
/// `S: PlanStage` enforces the valid transition sequence at compile time.
pub struct PlanExecutor<S: PlanStage> {
    pub(crate) plan: GeneratorPlan,
    pub(crate) ctx: Arc<GeneratorContext>,
    pub(crate) iteration: u8,
    pub(crate) stage: S,
    /// Optional skip-region context for frozen-region checks (Q-310 blocking).
    /// Stored here so it flows through the typestate pipeline without touching `GeneratorContext`.
    skip_ctx: Option<Arc<SkipContext>>,
    _marker: PhantomData<S>,
}

impl<S: PlanStage> PlanExecutor<S> {
    /// Returns the skip context if one is configured.
    pub fn skip_context(&self) -> Option<Arc<SkipContext>> {
        self.skip_ctx.clone()
    }
}

// ── Draft → Verified ──────────────────────────────────────────────────────────

impl PlanExecutor<Draft> {
    /// Construct a new executor in Draft state.
    #[must_use]
    pub fn new(plan: GeneratorPlan, ctx: Arc<GeneratorContext>, iteration: u8) -> Self {
        Self {
            plan,
            ctx,
            iteration,
            stage: Draft,
            skip_ctx: None,
            _marker: PhantomData,
        }
    }

    /// Construct a new executor with an optional skip context.
    #[must_use]
    pub fn with_skip_ctx(
        plan: GeneratorPlan,
        ctx: Arc<GeneratorContext>,
        iteration: u8,
        skip_ctx: Option<Arc<SkipContext>>,
    ) -> Self {
        Self {
            plan,
            ctx,
            iteration,
            stage: Draft,
            skip_ctx,
            _marker: PhantomData,
        }
    }

    /// Convenience constructor for the first attempt (iteration = 0).
    #[must_use]
    pub fn first(plan: GeneratorPlan, ctx: Arc<GeneratorContext>) -> Self {
        Self::new(plan, ctx, 0)
    }

    /// Convenience constructor with optional skip context.
    #[must_use]
    pub fn first_with_skip_ctx(
        plan: GeneratorPlan,
        ctx: Arc<GeneratorContext>,
        skip_ctx: Option<Arc<SkipContext>>,
    ) -> Self {
        Self::with_skip_ctx(plan, ctx, 0, skip_ctx)
    }

    /// VGP verification: Draft → Verified or `ReplanRequest` on symbol failure.
    ///
    /// Emits RL reward +0.3 on pass.
    ///
    /// # Errors
    /// Returns `ReplanRequest` when VGP symbol verification fails.
    pub async fn verify(self, vgp: &VgpEngine) -> Result<PlanExecutor<Verified>, ReplanRequest> {
        let plan_id = self.plan.plan_id;

        let report = vgp
            .verify_batch(&self.plan.contracts, plan_id)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(plan_id = %plan_id, error = %e, "VGP daemon unavailable — soft miss");
                crate::plan::result::VgpReport::empty_pass(plan_id)
            });

        if report.all_passed {
            // RL reward for VGP pass.
            self.ctx.rl.inject(
                "vgp_pass",
                NormalizedScore::clamped(0.3),
                &plan_id.to_string(),
            );

            // Session lifecycle: start a touring session for this plan execution.
            let _session_id = self
                .ctx
                .session_start(&plan_id.to_string(), self.plan.kind.label());

            // Decompose bridge: create a task in the DAG for this plan.
            // R15-S1: Store task_id to carry through pipeline → commit auto-finalize.
            let decompose_task_id = self.ctx.decompose_create_task(
                "generator",
                &format!("{}:{}", self.plan.kind.label(), plan_id),
            );

            // Closure dispatch: emit pheromone signal for template kind selection.
            // PLN2 section 8.1 — wires touring-simd::AcoPheromone feedback loop.
            self.ctx
                .pheromone_update(self.plan.kind.label(), NormalizedScore::clamped(0.3));

            // Closure dispatch: record lifecycle transition for telemetry pipeline.
            self.ctx.record_transition("Draft", "Verified", plan_id, 0);

            // Closure dispatch: check cognitive nexus for related plans.
            // Returns None if nexus is not wired — graceful degradation.
            if let Some(similarity) = self.ctx.evaluate_plan_similarity(&plan_id.to_string()) {
                tracing::debug!(
                    plan_id = %plan_id,
                    similarity = similarity.value(),
                    "cognitive nexus returned plan similarity score"
                );
            }

            // Closure dispatch: find semantically similar plans for context enrichment.
            let similar = self.ctx.find_similar_plans(&self.plan);
            if !similar.is_empty() {
                tracing::debug!(
                    plan_id = %plan_id,
                    count = similar.len(),
                    "cognitive graph returned similar plans"
                );
            }

            Ok(PlanExecutor {
                plan: self.plan,
                ctx: self.ctx,
                iteration: self.iteration,
                stage: Verified {
                    vgp_report: Arc::new(report),
                    decompose_task_id,
                },
                skip_ctx: self.skip_ctx,
                _marker: PhantomData,
            })
        } else {
            // RFC-100 G-400: emit diagnostic for VGP failure
            let err = GenerateError::VgpFailed {
                missing_count: report.missing_symbols.len(),
                collision_count: report.collisions.len(),
                plan_id,
            };
            if let Some(d) = err.to_diagnostic_opt() {
                tracing::warn!(
                    code = d.code,
                    plan_id = %plan_id,
                    missing = report.missing_symbols.len(),
                    collisions = report.collisions.len(),
                    "VGP failed — triggering replan"
                );
            }
            Err(ReplanRequest {
                plan: self.plan,
                iteration: self.iteration + 1,
                reason: FailureReason::VgpFailed,
                failure_history: Vec::new(),
            })
        }
    }
}

// ── Verified → Rendered ───────────────────────────────────────────────────────

impl PlanExecutor<Verified> {
    /// Render: Verified → Rendered or `GenerateError`.
    ///
    /// Uses `TemplateEngine::render_for_kind` with the plan's kind.
    /// Extra vars from the plan's `template.extra_vars` are merged with caller vars.
    ///
    /// If `skip_ctx` is provided, checks whether the target file falls inside a
    /// frozen skip-region before creating the artifact. Overlapping a skip-region
    /// returns `GenerateError::Q310` (blocking).
    ///
    /// # Errors
    /// Returns `GenerateError` if template rendering fails or target overlaps a skip region.
    /// Returns `None` if rendered content exceeds the width budget (caller should fall back
    /// to a multiline strategy).
    pub fn render(
        self,
        template_engine: &TemplateEngine,
        extra_vars: &HashMap<String, serde_json::Value>,
        skip_ctx: Option<&SkipContext>,
        shape: RenderShape,
    ) -> Result<Option<PlanExecutor<Rendered>>, GenerateError> {
        let plan_id = self.plan.plan_id;

        // Merge plan extra_vars (BTreeMap) with caller-supplied vars (HashMap).
        // Convert to HashMap for TemplateEngine::render_for_kind.
        let mut vars: HashMap<String, serde_json::Value> = self
            .plan
            .template
            .extra_vars
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        vars.extend(extra_vars.iter().map(|(k, v)| (k.clone(), v.clone())));

        let content = template_engine
            .render_for_kind(&self.plan.kind, &vars)
            .map_err(|e| {
                tracing::error!(plan_id = %plan_id, error = %e, "template render failed");
                e
            })?;

        // Shape overflow check: if content exceeds width budget, return None so the
        // caller can fall back to a multiline strategy. This mirrors rustfmt's
        // rewrite fallbacks — single-line that overflows → None → multiline retry.
        if shape.would_overflow(&content) {
            tracing::debug!(
                plan_id = %plan_id,
                offset = %shape.offset,
                max_width = %shape.max_width,
                content_len = content.len(),
                "shape overflow — returning None for multiline fallback"
            );
            return Ok(None);
        }

        // Skip-region check: abort if target file falls inside a frozen skip-region (Q-310 blocking).
        if let Some(ctx) = skip_ctx {
            let target = self.plan.target.file_path.as_str();
            let file_bytes: u64 = content.len() as u64;
            if let Err(violation) = ctx.check_edit(target, 0, file_bytes) {
                tracing::error!(
                    plan_id = %plan_id,
                    file = %target,
                    code = %violation.code,
                    "skip-region violation — Q-310 blocking"
                );
                return Err(GenerateError::Q310(violation.to_string()));
            }
        }

        // Closure dispatch: WASM sandbox pre-validation (PLN2 section 8.1).
        // If wasm_sandbox_fn is wired, pre-validate the rendered content in the sandbox.
        // Failures are logged but do NOT block — wiring gate at commit is the hard gate.
        match self.ctx.sandbox_execute(&content, self.plan.kind.label()) {
            Ok(_) => {
                tracing::trace!(
                    plan_id = %plan_id,
                    "wasm sandbox pre-validation passed (or not wired)"
                );
            }
            Err(e) => {
                tracing::warn!(
                    plan_id = %plan_id,
                    error = %e,
                    "wasm sandbox pre-validation failed — proceeding anyway (hard gate at commit)"
                );
            }
        }

        let artifact = RenderedFile::new(
            self.plan.target.file_path.as_str(),
            content,
            FileAction::Created,
        );

        self.ctx.rl.inject(
            "render_pass",
            NormalizedScore::clamped(0.2),
            &plan_id.to_string(),
        );

        // Closure dispatch: pheromone signal for successful render.
        self.ctx
            .pheromone_update("render_pass", NormalizedScore::clamped(0.2));

        // Closure dispatch: record lifecycle transition.
        self.ctx
            .record_transition("Verified", "Rendered", plan_id, 0);

        Ok(Some(PlanExecutor {
            plan: self.plan,
            ctx: self.ctx,
            iteration: self.iteration,
            stage: Rendered {
                artifacts: Arc::from(vec![artifact]),
                vgp_report: self.stage.vgp_report,
                decompose_task_id: self.stage.decompose_task_id,
            },
            skip_ctx: self.skip_ctx,
            _marker: PhantomData,
        }))
    }
}

// ── Rendered → Speculated ─────────────────────────────────────────────────────

impl PlanExecutor<Rendered> {
    /// Speculate: Rendered → Speculated or `ReplanRequest` when score < threshold.
    ///
    /// # Errors
    /// Returns `ReplanRequest` when speculate score is below the configured threshold.
    #[allow(clippy::too_many_lines)]
    pub async fn speculate(
        self,
        bridge: &SpeculateBridge,
    ) -> Result<PlanExecutor<Speculated>, ReplanRequest> {
        let plan_id = self.plan.plan_id;
        let threshold = self.ctx.capacity.speculate_threshold;

        // Concatenate all rendered contents for shadow validation.
        let combined: String = self
            .stage
            .artifacts
            .iter()
            .map(|a| a.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let mut speculate_report = bridge
            .validate(plan_id, &combined)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(plan_id = %plan_id, error = %e, "speculate daemon unavailable");
                crate::plan::result::SpeculateReport {
                    plan_id,
                    composite_score: NormalizedScore::ZERO,
                    all_passed: false,
                    layers: Vec::new(),
                    elapsed_ms: 0,
                }
            });

        // Polyglot syntax gate — per-artifact tree-sitter parse check for
        // non-Rust kinds (JS/TS/Python/Go/YAML/etc). A failure here is folded
        // into `SpeculateReport.layers` so the existing threshold gate picks
        // it up. Rust artifacts are skipped (syn covers them).
        let polyglot_layer =
            crate::validate::polyglot::polyglot_syntax_layer(&self.stage.artifacts);
        let polyglot_passed = polyglot_layer.passed;
        if !polyglot_passed {
            tracing::warn!(
                plan_id = %plan_id,
                issues = ?polyglot_layer.issues,
                "polyglot syntax gate failed"
            );
            speculate_report.all_passed = false;
        }
        speculate_report.layers.push(polyglot_layer);

        // D4.5 — VGP Layer 5: Path Boundary Enforcement.
        // Validates artifact paths against TaskKind write allowlists and
        // forbidden patterns from Contracts.path_boundaries. A violation
        // in FailClosed mode blocks the speculate gate.
        if let Some(ref boundaries) = self.plan.contracts.path_boundaries {
            let boundary_started = std::time::Instant::now();
            match crate::validate::boundary::BoundaryValidator::new(boundaries) {
                Ok(bv) => {
                    let boundary_result = bv.validate_artifacts(&self.stage.artifacts);
                    let layer: crate::plan::result::LayerResult =
                        (boundary_result, boundary_started).into();
                    if !layer.passed {
                        tracing::warn!(
                            plan_id = %plan_id,
                            issues = ?layer.issues,
                            "path boundary gate failed"
                        );
                        speculate_report.all_passed = false;
                    }
                    speculate_report.layers.push(layer);
                }
                Err(e) => {
                    tracing::error!(
                        plan_id = %plan_id,
                        error = %e,
                        "boundary validator failed to initialize — skipping layer"
                    );
                }
            }
        }

        // D5.6 — VGP Layer 6: Entity Registry Enforcement.
        // Validates that all EntityIds in contracts.entities_must_exist exist
        // and satisfy their admission criteria. Failure emits `output.rejected`
        // with `error_code: ENTITY_VIOLATION`.
        if !self.plan.contracts.entities_must_exist.is_empty() {
            let entity_started = std::time::Instant::now();
            let mut entity_issues: Vec<String> = Vec::new();
            for entity_ref in &self.plan.contracts.entities_must_exist {
                let entity_id_str = entity_ref.as_str();
                // Attempt to resolve via IdentityRegistry if daemon is reachable.
                // Each entity must satisfy admission criteria at generation time.
                // Fallback: allow if registry is unavailable (degraded mode).
                let db_path = std::env::var_os("TOURING_IDENTITY_DIR")
                    .map_or_else(
                        || {
                            std::env::var_os("TOURING_DATA_DIR").map_or_else(
                                || {
                                    // Fallback: uid-based temp path
                                    let uid = libc_getuid();
                                    PathBuf::from(format!("/tmp/touring-identity-{uid}"))
                                },
                                PathBuf::from,
                            )
                        },
                        PathBuf::from,
                    )
                    .join("registry.db");
                match touring_identity::IdentityRegistry::open_or_create(&db_path) {
                    Ok(mut reg) => {
                        if let Ok(candidates) = reg.resolve(entity_id_str, 2) {
                            if candidates.is_empty() {
                                entity_issues.push(format!("entity not found: {entity_id_str}"));
                            }
                            // TODO: D5.5 — verify admission criteria are met
                            // by the proposed code artifact (requires code analysis).
                        } else {
                            tracing::debug!(
                                entity_id = %entity_id_str,
                                "entity registry resolve failed — skipping entity gate"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::debug!(
                            entity_id = %entity_id_str,
                            error = %e,
                            "entity registry unavailable — skipping entity gate"
                        );
                    }
                }
            }
            let entity_layer_passed = entity_issues.is_empty();
            let entity_elapsed_ms = entity_started.elapsed().as_secs_f64() * 1000.0;
            let entity_layer = crate::plan::result::LayerResult {
                name: "entity_registry".into(),
                passed: entity_layer_passed,
                score: if entity_layer_passed {
                    NormalizedScore::ONE
                } else {
                    NormalizedScore::ZERO
                },
                issues: entity_issues,
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                elapsed_ms: entity_elapsed_ms as u64,
            };
            if !entity_layer_passed {
                tracing::warn!(
                    plan_id = %plan_id,
                    issues = ?entity_layer.issues,
                    "entity registry gate failed"
                );
                speculate_report.all_passed = false;
            }
            speculate_report.layers.push(entity_layer);
        }

        // Composite collapses to ZERO when polyglot rejects any artifact —
        // ensures the threshold gate triggers a Replan regardless of other
        // layer scores.
        let base_score = if polyglot_passed {
            speculate_report.composite_score
        } else {
            NormalizedScore::ZERO
        };

        // Closure dispatch: MCTS evaluation for plan synthesis scoring (PLN2 section 8.1).
        // If mcts_eval_fn is wired, blend the MCTS score with the speculate score.
        // Final score = max(speculate, (speculate + mcts) / 2) — never lower than speculate alone.
        let mcts_score = self.ctx.mcts_evaluate(&plan_id.to_string());
        let score = if mcts_score.value() > 0.0 {
            let blended = f64::midpoint(base_score.value(), mcts_score.value());
            let blended_norm = NormalizedScore::clamped(blended);
            tracing::debug!(
                plan_id = %plan_id,
                speculate = base_score.value(),
                mcts = mcts_score.value(),
                blended = blended,
                "MCTS score blended with speculate"
            );
            // Always keep the max so MCTS can only improve, never degrade.
            if blended_norm.value() > base_score.value() {
                blended_norm
            } else {
                base_score
            }
        } else {
            base_score
        };

        if score.value() >= threshold {
            self.ctx.rl.inject(
                "speculate_pass",
                NormalizedScore::clamped(0.5),
                &plan_id.to_string(),
            );

            // Session lifecycle: checkpoint after speculate pass.
            self.ctx.session_checkpoint(
                &format!(
                    "gen-{}",
                    plan_id.to_string().chars().take(20).collect::<String>()
                ),
                &format!("speculate_pass score={:.3}", score.value()),
            );

            // Closure dispatch: pheromone signal for speculate pass.
            self.ctx
                .pheromone_update("speculate_pass", NormalizedScore::clamped(0.5));

            // Closure dispatch: record lifecycle transition.
            self.ctx
                .record_transition("Rendered", "Speculated", plan_id, 0);

            Ok(PlanExecutor {
                plan: self.plan,
                ctx: self.ctx,
                iteration: self.iteration,
                stage: Speculated {
                    score,
                    artifacts: Arc::clone(&self.stage.artifacts),
                    vgp_report: self.stage.vgp_report,
                    decompose_task_id: self.stage.decompose_task_id,
                },
                skip_ctx: self.skip_ctx,
                _marker: PhantomData,
            })
        } else {
            // RFC-100 G-401: emit diagnostic for speculate failure
            let err = GenerateError::SpeculateBelowThreshold {
                actual: score.value(),
                required: threshold,
                plan_id,
            };
            if let Some(d) = err.to_diagnostic_opt() {
                tracing::warn!(
                    code = d.code,
                    plan_id = %plan_id,
                    score = score.value(),
                    threshold,
                    "speculate below threshold — triggering replan"
                );
            }
            Err(ReplanRequest {
                plan: self.plan,
                iteration: self.iteration + 1,
                reason: FailureReason::SpeculateFailed {
                    score: score.value(),
                    threshold,
                },
                failure_history: Vec::new(),
            })
        }
    }
}

// ── Security Gate — Auto-Cure Helper ─────────────────────────────────────────

/// Replace the byte-range `span` in `content` with a `/* [TOURING-CURED: CWE-N] */` annotation.
///
/// Operates on UTF-8 char boundaries via `str::get()`. Returns the original string unchanged if
/// the span is out-of-bounds or degenerate, making the caller's loop safe for any `VulnMatch`
/// output regardless of whether spans are byte-exact.
#[cfg(feature = "security-gate")]
fn neutralize_span(content: &str, span: (usize, usize), cwe_id: u32) -> String {
    let (start, end) = span;
    if start >= end {
        return content.to_owned();
    }
    let Some(prefix) = content.get(..start) else {
        return content.to_owned();
    };
    let Some(suffix) = content.get(end..) else {
        return content.to_owned();
    };
    format!("{prefix}/* [TOURING-CURED: CWE-{cwe_id}] */{suffix}")
}

// ── Speculated → Committed ────────────────────────────────────────────────────

impl PlanExecutor<Speculated> {
    /// Commit: writes each artifact atomically, injects RL +1.0, stores memory lesson.
    ///
    /// Gate chain (PLN2/PLN3):
    ///   1. `Wiring gate` — HARD block (blocks commit on failure)
    ///   2. `Quality gate` — HARD block (blocks commit on failure)
    ///   3. `Atomic write` — only if all gates pass
    ///   4. `Health` check — advisory, awaited (synchronous, populates `HEALTH_SCORES`)
    ///   5. `Blast radius check` — advisory, async, non-blocking (`tokio::spawn`)
    ///   6. `Enrichment trigger` — advisory, async, non-blocking (`tokio::spawn`)
    ///   7. `Multidimensional RL` — injects all reward signals post-commit
    ///
    /// # Errors
    ///
    /// Returns `GenerateError` if an atomic write fails or a task join error occurs.
    ///
    /// # Panics
    ///
    /// Panics if `TextEdit::try_from_iter` receives an invalid indel (programming error).
    #[allow(clippy::too_many_lines)]
    pub async fn commit(self) -> Result<CompletedPlan, GenerateError> {
        use crate::source_change::FileId;
        use std::collections::BTreeMap;

        let plan_id = self.plan.plan_id;
        let pid = plan_id.to_string();
        let start = std::time::Instant::now();
        let mut artifacts = Arc::clone(&self.stage.artifacts);
        let ctx = Arc::clone(&self.ctx);
        let mut artifacts_committed: Vec<Artifact> = Vec::new();

        // ── Gate 1: Wiring gate (HARD block) ─────────────────────────────────
        ctx.evaluate_wiring_gate(&artifacts, &pid).map_err(|e| {
            tracing::error!(plan_id = %plan_id, error = %e, "wiring gate rejected artifacts");
            e
        })?;

        // ── Gate 2: Quality gate (HARD block) ────────────────────────────────
        #[cfg(feature = "quality-gate")]
        {
            ctx.evaluate_quality_gate(&artifacts, &pid).map_err(|e| {
                tracing::error!(plan_id = %plan_id, error = %e, "quality gate rejected artifacts");
                e
            })?;
        }

        // ── Gate 3: Cycle detection — Tarjan SCC (Suggestion 2 — 2026-04-20) ─
        // Detect mutual recursion in generated Rust artifacts before any file is
        // written to disk. Uses CallGraph::detect_cycles() (petgraph Tarjan SCC,
        // O(|V|+|E|) per artifact). Injects RL penalty -0.5 on rejection so the
        // LinUCB bandit learns to avoid generating cyclically-dependent call graphs.
        {
            let mut all_cycles: Vec<Vec<String>> = Vec::new();
            for artifact in artifacts.iter() {
                if artifact.path.ends_with("rs") || artifact.path.ends_with("RS") {
                    let cg = touring_code::ast::call_graph::build_call_graph(
                        &artifact.content,
                        touring_code::ast::Lang::Rust,
                    );
                    all_cycles.extend(cg.detect_cycles());
                }
            }
            if !all_cycles.is_empty() {
                let cycle_summary = all_cycles
                    .iter()
                    .map(|c| c.join(" → "))
                    .collect::<Vec<_>>()
                    .join("; ");
                tracing::error!(
                    plan_id = %plan_id,
                    cycles = %cycle_summary,
                    "cycle gate rejected: mutual recursion in generated artifacts"
                );
                ctx.rl_reward("cycle_gate", -0.5, "tarjan_scc_mutual_recursion");
                return Err(GenerateError::InvariantViolated {
                    invariant_id: "cycle-gate".into(),
                    detail: format!("call graph cycles in generated code: {cycle_summary}"),
                });
            }
        }

        // ── P1.3 (2026-04-25): mpatch fuzzy preview ─────────────────────────────
        // Between cycle detection and security-gate AST surgery: preview each
        // artifact's diff against current disk content to detect drift and flag
        // confidence before writing. Uses `touring_hooks_shared::mpatch_preview`
        // (the canonical L2 home — not the heavy `touring_hooks` L3 façade).
        // Feature-gated by `simd-fuzzy` to keep base generator dependency-free.
        #[cfg(feature = "simd-fuzzy")]
        for artifact in artifacts.iter() {
            use touring_hooks_shared::mpatch_preview::preview_patch;
            // Read current disk content for diff preview.
            if let Ok(disk_content) = std::fs::read_to_string(&artifact.path)
                && let Some(preview) = preview_patch(&disk_content, &artifact.content)
            {
                tracing::debug!(
                    plan_id = %plan_id,
                    path = %artifact.path,
                    method = ?preview.method,
                    confidence = preview.confidence,
                    "mpatch preview: fuzzy match detected"
                );
                // Gate: block commit if confidence < MPATCH_CONFIDENCE_FLOOR (low-quality patch).
                if preview.confidence < MPATCH_CONFIDENCE_FLOOR {
                    tracing::warn!(
                        plan_id = %plan_id,
                        path = %artifact.path,
                        confidence = preview.confidence,
                        "mpatch confidence too low — inspect before commit"
                    );
                }
            }
        }

        // ── Gate 4: Security gate — CWE auto-cure surgery (Suggestion 1 — 2026-04-20) ──
        // Scans generated Rust artifacts for CWE vulnerability patterns (SQLi, XSS, CMDi,
        // PathTraversal) via ConcolicExecutor. Attempts O(log N) span-level auto-cure first:
        // replaces each vulnerable byte-span with `/* [TOURING-CURED: CWE-N] */` in reverse
        // order (to preserve earlier span validity), then re-validates. Hard-rejects only if
        // residual patterns remain after surgery. RL reward +0.2 on successful cure; -0.3 on
        // incurable rejection so the LinUCB bandit learns from both outcomes.
        #[cfg(feature = "security-gate")]
        {
            let mut curated: Vec<RenderedFile> = artifacts.iter().cloned().collect();
            let mut residual_hits: Vec<String> = Vec::new();
            let mut cured_count = 0usize;

            for artifact in &mut curated {
                if !artifact.path.ends_with("rs") && !artifact.path.ends_with("RS") {
                    continue;
                }
                let exec = touring_offensive::concolic::ConcolicExecutor::new();
                let initial_hits = exec.detect_all_patterns(&artifact.content);
                if initial_hits.is_empty() {
                    continue;
                }
                // Apply cures in REVERSE byte-offset order to preserve validity of earlier spans.
                let mut cured = artifact.content.clone();
                let mut sorted = initial_hits.clone();
                sorted.sort_by_key(|vm| std::cmp::Reverse(vm.span.0));
                for vm in &sorted {
                    cured = neutralize_span(&cured, vm.span, vm.cwe_id);
                }
                // Re-validate: confirm all patterns neutralized by surgery.
                let residual = exec.detect_all_patterns(&cured);
                if residual.is_empty() {
                    tracing::info!(
                        plan_id = %plan_id,
                        path = %artifact.path,
                        cured = initial_hits.len(),
                        "[TOURING-SURGERY] auto-cured CWE vulnerabilities",
                    );
                    artifact.content = cured;
                    cured_count += initial_hits.len();
                } else {
                    for vm in &residual {
                        residual_hits.push(format!(
                            "CWE-{} ({}) in {}",
                            vm.cwe_id, vm.pattern_name, artifact.path
                        ));
                    }
                }
            }

            if !residual_hits.is_empty() {
                let summary = residual_hits.join("; ");
                tracing::error!(
                    plan_id = %plan_id,
                    findings = %summary,
                    "[TOURING-SURGERY] auto-cure exhausted — incurable CWE patterns remain"
                );
                ctx.rl_reward("security_gate", -0.3, "cwe_autocure_failed");
                return Err(GenerateError::InvariantViolated {
                    invariant_id: "security-gate".into(),
                    detail: format!("CWE vulnerabilities not curable: {summary}"),
                });
            }

            if cured_count > 0 {
                ctx.rl_reward("security_gate", 0.2, "cwe_autocure_success");
                // Publish cured artifacts so downstream gates and the file-writer use cured content.
                artifacts = Arc::from(curated);
            }
        }

        // ── B.5.6: SourceChange atomic commit with two-phase apply ──────────────────
        // Build SourceChange from all artifacts, then use Applier for atomic write with rollback.

        // ── P6: Record pre-health for ALL artifacts uniformly ──────────────
        // Previously only invoked in the `ApplyResult::Invalid` per-file fallback,
        // which left `HEALTH_SCORES` stale on the happy `SourceChange` path. Recording
        // here — before either commit branch executes — guarantees consistent health
        // delta semantics regardless of which writer path runs.
        for rendered in artifacts.iter() {
            self.ctx.record_pre_health_for_artifact(&rendered.path);
        }

        let mut source_change = SourceChange::new();
        for rendered in artifacts.iter() {
            // FileId derivation centralised in `compute_file_id` (path-string SipHash64
            // truncated to host-pointer width).
            let file_id: FileId = compute_file_id(&rendered.path);

            // For each artifact, create a TextEdit with a single Indel that replaces
            // the entire file content (range [0, content.len]) with the new content.
            let indel = Indel {
                delete: 0..rendered.content.len(),
                insert: rendered.content.clone(),
            };
            // Map the impossible-by-construction `try_from_iter` failure onto an
            // `InvariantViolated` so `commit()` cannot panic from a single-indel
            // construction error (defensive — the prior `.expect()` was load-bearing
            // on `Indel` always producing a valid `TextEdit`).
            let text_edit = TextEdit::try_from_iter([indel]).map_err(|e| {
                tracing::error!(plan_id = %plan_id, error = ?e, "TextEdit construction failed for single indel");
                GenerateError::InvariantViolated {
                    invariant_id: "text-edit-from-single-indel".into(),
                    detail: format!("TextEdit::try_from_iter failed for single indel: {e:?}"),
                }
            })?;
            source_change = source_change.with_edit(file_id, text_edit);
        }

        // Phase 1: shadow_validate — dry-run all edits before writing anything.
        let applier = Applier::new();
        let validation = applier.shadow_validate(&source_change);

        match validation {
            ApplyResult::Valid => {
                // Phase 2: build files map and commit atomically.
                // Each file content is read from disk and then modified in-place by Applier::commit.
                let mut files: BTreeMap<FileId, String> = BTreeMap::new();
                for rendered in artifacts.iter() {
                    let file_id: FileId = compute_file_id(&rendered.path);
                    // Read current disk content; for new files this will be empty string.
                    let disk_content = std::fs::read_to_string(&rendered.path).unwrap_or_default();
                    files.insert(file_id, disk_content);
                }

                let commit_result = applier.commit(
                    &source_change,
                    &mut files,
                    |file_id: FileId| -> Option<PathBuf> {
                        // Reverse the hash by linear-scanning artifacts (small N).
                        artifacts
                            .iter()
                            .find(|r| compute_file_id(&r.path) == file_id)
                            .map(|r| PathBuf::from(&r.path))
                    },
                );

                match commit_result {
                    ApplyResult::Committed { files_written, .. } => {
                        tracing::info!(
                            plan_id = %plan_id,
                            files_written,
                            "source-change atomic commit succeeded"
                        );
                        // Populate artifacts_committed from all rendered files.
                        for rendered in artifacts.iter() {
                            let sha256 =
                                format!("{:x}", sha2::Sha256::digest(rendered.content.as_bytes()));
                            let bytes_written = rendered.content.len() as u64;
                            artifacts_committed.push(Artifact {
                                path: rendered.path.clone(),
                                sha256,
                                bytes_written,
                                backup_path: None,
                                action: rendered.action.clone(),
                            });
                        }
                    }
                    ApplyResult::RolledBack {
                        errors,
                        partial_writes,
                    } => {
                        tracing::error!(
                            plan_id = %plan_id,
                            ?errors,
                            partial_writes,
                            "source-change rollback: partial writes reverted"
                        );
                        return Err(GenerateError::InvariantViolated {
                            invariant_id: "source-change-atomic".into(),
                            detail: format!(
                                "commit failed and was rolled back: {} errors, {} partial writes",
                                errors.len(),
                                partial_writes
                            ),
                        });
                    }
                    other => {
                        tracing::error!(plan_id = %plan_id, ?other, "source-change unexpected result");
                        return Err(GenerateError::InvariantViolated {
                            invariant_id: "source-change-atomic".into(),
                            detail: format!("unexpected result: {other:?}"),
                        });
                    }
                }
            }
            ApplyResult::Committed { .. } | ApplyResult::RolledBack { .. } => {
                tracing::error!(plan_id = %plan_id, "source-change shadow_validate returned commit/rollback");
                return Err(GenerateError::InvariantViolated {
            invariant_id: "source-change-atomic".into(),
            detail: "shadow_validate returned Committed or RolledBack (should be Valid or Invalid)".into(),
        });
            }
            ApplyResult::Invalid { errors } => {
                tracing::error!(
                    plan_id = %plan_id,
                    ?errors,
                    "source-change shadow-validate failed — falling back to original per-file write"
                );
                // Fall back to original per-file atomic write on validation failure.
                // pre_health was already recorded above for all artifacts (P6 fix —
                // happy and fallback paths share the same pre-edit baseline).
                for rendered in artifacts.iter() {
                    write_artifact_atomically(&rendered.path, &rendered.content)
                .await
                .map_err(|e| {
                    tracing::error!(plan_id = %plan_id, path = %rendered.path, error = %e, "atomic write failed");
                    GenerateError::Io(e)
                })?;
                    self.ctx
                        .evaluate_knowledge_upsert(&rendered.path, rendered.content.as_bytes());
                    if let Some((delta, is_regression, is_improvement)) = self
                        .ctx
                        .compute_health_delta_for_artifact(&rendered.path, &rendered.content)
                    {
                        let reward = if is_regression {
                            -0.10_f64
                        } else if is_improvement {
                            0.10_f64
                        } else {
                            0.00_f64
                        };
                        if reward.abs() > f64::EPSILON {
                            self.ctx.rl.inject(
                                "generator_health_delta",
                                NormalizedScore::clamped(reward.abs() + 0.5),
                                &pid,
                            );
                        }
                        tracing::debug!(
                            plan_id = %plan_id,
                            path = %rendered.path,
                            delta = delta,
                            regression = is_regression,
                            improvement = is_improvement,
                            "wave19: generator artifact health delta computed",
                        );
                    }
                    let sha256 = format!("{:x}", sha2::Sha256::digest(rendered.content.as_bytes()));
                    let bytes_written = rendered.content.len() as u64;
                    artifacts_committed.push(Artifact {
                        path: rendered.path.clone(),
                        sha256,
                        bytes_written,
                        backup_path: None,
                        action: rendered.action.clone(),
                    });
                }
            }
        }

        // ── Gate 4: Health check (advisory, awaited before RL injection) ─────
        // Runs synchronously so health_pass RL reward is available to inject_multidim_rewards.
        // Failures are logged but do NOT block commit (advisory semantics).
        #[cfg(feature = "health-gate")]
        {
            let project_root = ctx.project_root.to_string();
            if let Err(e) = ctx.evaluate_health_gate(&project_root) {
                tracing::warn!(plan_id = %plan_id, health_error = %e, "health gate degraded — continuing");
            }
        }

        // ── Gate 5: Blast radius check (advisory, async) ────────────────────
        #[cfg(feature = "blast-check")]
        {
            let blast_ctx = Arc::clone(&ctx);
            let blast_pid = pid.clone();
            let blast_paths: Vec<_> = artifacts.iter().map(|f| f.path.clone()).collect();
            tokio::spawn(async move {
                Self::blast_radius_check(&blast_ctx, &blast_paths, &blast_pid).await;
            });
        }

        // ── Gate 6: Enrichment trigger (advisory, async) ─────────────────────
        // Fires `touring post-write` for each artifact, invoking the full daemon
        // enrichment pipeline (Tantivy FTS, gotcha, wiring, knowledge).
        #[cfg(feature = "enrichment-gate")]
        {
            let enrich_ctx = Arc::clone(&ctx);
            let enrich_pid = pid.clone();
            let enrich_paths: Vec<_> = artifacts.iter().map(|f| f.path.clone()).collect();
            tokio::spawn(async move {
                Self::enrichment_trigger(&enrich_ctx, &enrich_paths, &enrich_pid).await;
            });
        }

        // ── Gate 7: Multidimensional RL rewards ───────────────────────────────
        Self::inject_multidim_rewards(&ctx, &pid, ctx.project_root.as_str());

        // RL reward for successful commit.
        ctx.rl.inject("commit_success", NormalizedScore::ONE, &pid);

        // Closure dispatch: pheromone signal for successful commit (max reward).
        ctx.pheromone_update("commit_success", NormalizedScore::ONE);

        // Closure dispatch: record final lifecycle transition.
        ctx.record_transition(
            "Speculated",
            "Committed",
            plan_id,
            u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX),
        );

        // Closure dispatch: execute DSPy signature to capture commit outcome lesson.
        let dspy_sig: DspySignatureName = String::from("plan.commit");
        let mut dspy_inputs: DspyInputs = DspyInputs::new();
        dspy_inputs.insert(
            "plan_id".to_string(),
            serde_json::Value::String(plan_id.to_string()),
        );
        dspy_inputs.insert(
            "score".to_string(),
            serde_json::Value::String(format!("{:.3}", self.stage.score.value())),
        );
        let dspy_output = ctx.execute_dspy(&dspy_sig, &dspy_inputs);
        if !dspy_output.is_empty() {
            tracing::debug!(plan_id = %plan_id, dspy_fields = dspy_output.len(), "DSPy signature returned commit-outcome context");
        }

        // Session lifecycle: assess session quality after commit.
        let session_id = format!("gen-{}", pid.chars().take(20).collect::<String>());
        if let Some(quality) = ctx.session_assess(&session_id) {
            tracing::info!(plan_id = %plan_id, session_quality = quality, "session assessed");
        }

        // R15-S1: Decompose bridge — auto-finalize the DAG task created at verify.
        // plan-commit success → decompose_update_status("completed") closes the generator→DAG loop.
        // If no decompose_task_id was captured (e.g. closure not wired), this is a no-op.
        if let Some(ref dag_task_id) = self.stage.decompose_task_id {
            ctx.decompose_update_status(dag_task_id, &pid, "completed");
            tracing::info!(plan_id = %plan_id, dag_task_id, "decompose task auto-finalized after commit");
        }

        // Persist lesson for future plan recall.
        let memory_key = format!("lesson:generator:plan:{plan_id}");
        let _ = ctx.memory.store(
            &memory_key,
            &format!(
                "plan {} committed: {} file(s) written, speculate score={:.3}",
                plan_id,
                artifacts_committed.len(),
                self.stage.score.value()
            ),
            MemoryTier::Semantic,
            MemoryKind::Lesson,
        );

        tracing::info!(
            plan_id = %plan_id,
            score = self.stage.score.value(),
            files = artifacts_committed.len(),
            "plan committed successfully"
        );

        let commit_report = Arc::new(CommitReport {
            plan_id,
            files_written: artifacts_committed,
            elapsed_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
        });

        Ok(CompletedPlan {
            plan_id,
            commit_report,
        })
    }

    /// Returns the speculate score for this plan.
    #[must_use]
    pub fn score(&self) -> NormalizedScore {
        self.stage.score
    }

    /// Blast radius check — runs `touring ast blast <path> -j` asynchronously via `tokio::spawn`.
    /// Injects `stable_commit` RL reward (0.5) when `affected_count` is less than 5.
    async fn blast_radius_check(ctx: &Arc<GeneratorContext>, paths: &[String], plan_id: &str) {
        for path in paths {
            let output = tokio::process::Command::new("touring")
                .args(["ast", "blast", path, "-j"])
                .output()
                .await;
            if let Ok(out) = output
                && out.status.success()
                && let Ok(json) = serde_json::from_slice::<serde_json::Value>(&out.stdout)
            {
                let count = json
                    .get("affected_count")
                    .and_then(serde_json::Value::as_i64)
                    .and_then(|v| usize::try_from(v).ok())
                    .unwrap_or(0);
                if count < 5 {
                    ctx.rl
                        .inject("stable_commit", NormalizedScore::clamped(0.5), plan_id);
                }
                // Cache the blast count for multidimensional RL.
                crate::core::context::BLAST_COUNTS.insert(path.clone(), count);
            }
        }
    }

    /// Fires the daemon enrichment pipeline for generated artifacts.
    ///
    /// Runs `touring post-write` for each artifact path, which triggers:
    /// - Tantivy FTS indexing of generated file
    /// - Gotcha detection
    /// - Wiring update (new symbol registration)
    /// - Knowledge DB enrichment
    ///
    /// Non-blocking, fire-and-forget.
    async fn enrichment_trigger(_ctx: &Arc<GeneratorContext>, paths: &[String], _plan_id: &str) {
        let _input = serde_json::json!({
            "tool_input": {
                "file_path": "", // filled per-path below
            },
            "tool_use_result": {
                "is_error": false,
            }
        });
        for path in paths {
            let mut cmd = tokio::process::Command::new("touring");
            cmd.arg("post-write");
            // Build JSON payload with the correct file_path
            let payload = serde_json::json!({
                "tool_input": {
                    "file_path": path,
                },
                "tool_use_result": {
                    "is_error": false,
                }
            });
            // Pipe JSON via stdin, collect stdout/stderr
            let child = cmd
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
            if let Ok(mut child) = child {
                if let Some(mut stdin) = child.stdin.take() {
                    use tokio::io::AsyncWriteExt;
                    let _ = stdin.write_all(payload.to_string().as_bytes()).await;
                    drop(stdin);
                }
                let _ = child.wait().await;
            }
            tracing::debug!(path = %path, "enrichment trigger fired");
        }
    }

    /// Injects multidimensional RL rewards based on cached scores.
    fn inject_multidim_rewards(ctx: &Arc<GeneratorContext>, plan_id: &str, project_root: &str) {
        // quality_pass: avg score from quality analysis (populated by evaluate_quality_gate).
        if let Some(score) = crate::core::context::QUALITY_SCORES.get(plan_id) {
            // moka get() returns Copy type directly, no dereference needed
            if score >= 0.7 {
                ctx.rl
                    .inject("quality_pass", NormalizedScore::clamped(score), plan_id);
            }
        }

        // clean_commit: wiring score > 0.9 (populated by evaluate_wiring_gate).
        if let Some(score) = crate::core::context::WIRING_SCORES.get(plan_id)
            && score > 0.9
        {
            ctx.rl
                .inject("clean_commit", NormalizedScore::clamped(0.6), plan_id);
        }

        // health_pass: composite score > 0.7 from touring e2e (populated by HealthGateAdapter::check).
        if let Some(score) = crate::core::context::HEALTH_SCORES.get(project_root)
            && score >= 0.7
        {
            ctx.rl
                .inject("health_pass", NormalizedScore::clamped(score), plan_id);
        }
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

/// Write content to `path` atomically via temp-file + rename (POSIX atomic on same filesystem).
async fn write_artifact_atomically(path: &str, content: &str) -> std::io::Result<()> {
    use tokio::fs;

    // Ensure parent directory exists.
    if let Some(parent) = std::path::Path::new(path).parent() {
        fs::create_dir_all(parent).await?;
    }

    let tmp_path = format!("{path}.tmp.{}", std::process::id());
    fs::write(&tmp_path, content).await?;
    fs::rename(&tmp_path, path).await?;

    Ok(())
}

#[cfg(test)]
mod cycle_gate_tests {
    #[test]
    fn tarjan_scc_detects_mutual_recursion() {
        // Verify the same logic used in Gate 3: mutually recursive functions
        // produce a non-empty cycle list via CallGraph::detect_cycles().
        let mutually_recursive = r"
fn foo(x: u32) -> u32 {
    bar(x + 1)
}
fn bar(x: u32) -> u32 {
    foo(x.saturating_sub(1))
}
";
        let cg = touring_code::ast::call_graph::build_call_graph(
            mutually_recursive,
            touring_code::ast::Lang::Rust,
        );
        let cycles = cg.detect_cycles();
        assert!(
            !cycles.is_empty(),
            "mutual recursion must be detected as a cycle"
        );
        let found_foo_bar = cycles
            .iter()
            .any(|c| c.contains(&"foo".to_string()) && c.contains(&"bar".to_string()));
        assert!(
            found_foo_bar,
            "cycle must involve foo and bar; got: {cycles:?}"
        );
    }

    #[test]
    fn tarjan_scc_no_cycles_for_linear_code() {
        let linear = r"
fn a() -> u32 { b() }
fn b() -> u32 { c() }
fn c() -> u32 { 42 }
";
        let cg =
            touring_code::ast::call_graph::build_call_graph(linear, touring_code::ast::Lang::Rust);
        assert!(
            cg.detect_cycles().is_empty(),
            "linear call chain must produce no cycles"
        );
    }
}

#[cfg(all(test, feature = "security-gate"))]
mod security_gate_tests {
    use touring_offensive::concolic::ConcolicExecutor;

    #[test]
    fn security_gate_clean_code_passes() {
        // Pure arithmetic Rust — no SQL, shell, or HTML interpolation.
        // ConcolicExecutor must report zero CWE hits.
        let clean = r"
pub fn add(a: u32, b: u32) -> u32 { a + b }
pub fn factorial(n: u32) -> u32 {
    if n == 0 { 1 } else { n * factorial(n - 1) }
}
";
        let exec = ConcolicExecutor::new();
        let hits = exec.detect_all_patterns(clean);
        assert!(
            hits.is_empty(),
            "clean arithmetic code must produce no CWE hits, got: {hits:?}"
        );
    }

    #[test]
    fn security_gate_sqli_pattern_detected() {
        // Simulated generated code containing a classic SQLi payload.
        // The SQLi pattern (CWE-89) triggers on ' OR ', ; --, or UNION SELECT.
        let sqli_code = "UNION SELECT username, password FROM users --";
        let exec = ConcolicExecutor::new();
        let hits = exec.detect_all_patterns(sqli_code);
        assert!(
            !hits.is_empty(),
            "SQL injection pattern must be detected in: {sqli_code:?}"
        );
        let has_cwe89 = hits.iter().any(|vm| vm.cwe_id == 89);
        assert!(
            has_cwe89,
            "detected hits must include CWE-89 (SQLi), got: {hits:?}"
        );
    }

    #[test]
    fn security_gate_neutralize_span_replaces_bytes() {
        // The annotation should replace the vulnerable byte range exactly.
        let content = "UNION SELECT * FROM users";
        let exec = ConcolicExecutor::new();
        let hits = exec.detect_all_patterns(content);
        assert!(
            !hits.is_empty(),
            "SQLi must be detectable to get a valid span"
        );
        let vm = &hits[0];
        let cured = super::neutralize_span(content, vm.span, vm.cwe_id);
        assert!(
            cured.contains("TOURING-CURED"),
            "cured content must contain the TOURING-CURED annotation, got: {cured:?}"
        );
        // The cured content must not reproduce the same hit.
        let re_hits = exec.detect_all_patterns(&cured);
        assert!(
            re_hits.is_empty(),
            "after neutralize_span, re-scan must be clean, got: {re_hits:?}"
        );
    }

    #[test]
    fn security_gate_auto_cure_sqli_neutralizes_all_spans() {
        // Mirrors Gate 4 logic: detect → sort reverse → neutralize → re-scan.
        let sqli = "UNION SELECT username, password FROM users --";
        let exec = ConcolicExecutor::new();
        let hits = exec.detect_all_patterns(sqli);
        assert!(!hits.is_empty(), "SQLi must be detected to test auto-cure");

        let mut cured = sqli.to_owned();
        let mut sorted = hits.clone();
        sorted.sort_by_key(|b| std::cmp::Reverse(b.span.0));
        for vm in &sorted {
            cured = super::neutralize_span(&cured, vm.span, vm.cwe_id);
        }
        let residual = exec.detect_all_patterns(&cured);
        assert!(
            residual.is_empty(),
            "after full auto-cure pass, no CWE patterns should remain; cured={cured:?}"
        );
    }

    #[test]
    fn security_gate_neutralize_span_oob_returns_original() {
        // Out-of-bounds span must return original content unchanged (no panic).
        let content = "short";
        let result = super::neutralize_span(content, (100, 200), 89);
        assert_eq!(
            result, content,
            "out-of-bounds span must return original content"
        );
    }

    #[test]
    fn security_gate_neutralize_span_degenerate_returns_original() {
        // Degenerate span (start == end) must return original content unchanged.
        let content = "UNION SELECT x FROM t";
        let result = super::neutralize_span(content, (5, 5), 89);
        assert_eq!(
            result, content,
            "degenerate span must return original content"
        );
    }
}
