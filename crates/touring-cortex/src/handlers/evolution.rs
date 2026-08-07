//! ACO/Generator Evolution Handlers — Learning from session patterns.
//!
//! - `AcoEvolutionHandler`: Stop (async) — ACO evolution checkpoint
//! - `GeneratorLearningHandler`: Stop (async) — generator outcome recording
//! - `SessionOptimizerHandler`: Stop (async) — session pattern analysis
//! - `RetrospectiveHandler`: PreCompact (sync) — lessons learned injection

use std::collections::HashMap;

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};

// ── H47: AcoEvolution ─────────────────────────────────────────────────

/// Save ACO evolution checkpoint on session end.
pub struct AcoEvolutionHandler;

impl Handler for AcoEvolutionHandler {
    fn name(&self) -> &str {
        "aco_evolution"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::Stop]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let persistence = match &ctx.persistence {
            Some(p) => p,
            None => return HandlerResult::skip(self.name()),
        };

        // Check for ACO-related activity in session
        let events = persistence.recent_hook_events(7200).unwrap_or_default();
        let aco_events: Vec<_> = events
            .iter()
            .filter(|(_, et, _, _)| {
                et.contains("Task") || et.contains("Subagent") || et.contains("ACO")
            })
            .collect();

        if aco_events.is_empty() {
            return HandlerResult::skip(self.name());
        }

        let checkpoint = serde_json::json!({
            "session_id": ctx.session_id,
            "aco_events": aco_events.len(),
            "total_events": events.len(),
            "timestamp": epoch_secs(),
        });

        // Write checkpoint
        let ckpt_dir = ctx.project_root.join(".claude/checkpoints");
        let _ = std::fs::create_dir_all(&ckpt_dir);
        let path = ckpt_dir.join(format!("aco_evolution_{}.toon", epoch_secs()));
        if let Ok(json) = serde_json::to_string_pretty(&checkpoint) {
            let _ = std::fs::write(&path, json);
        }

        // Store in RlmMemory
        if let Some(ref rlm) = ctx.rlm {
            let val = serde_json::to_string(&checkpoint).unwrap_or_default();
            let _ = rlm.store(
                &format!("aco:evolution:{}", epoch_secs()),
                touring_intelligence::rl::memory::rlm::MemoryTier::Reference,
                &val,
                Some("aco_evolution"),
                None,
            );
        }

        let _ = persistence.log_hook_event(
            "Stop",
            "AcoEvolution",
            &format!("aco_events={}", aco_events.len()),
            0.3,
        );

        HandlerResult::skip(self.name())
    }
}

// ── H48: GeneratorLearning ────────────────────────────────────────────

/// Record generator execution outcomes for learning.
pub struct GeneratorLearningHandler;

impl Handler for GeneratorLearningHandler {
    fn name(&self) -> &str {
        "generator_learning"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::Stop]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let persistence = match &ctx.persistence {
            Some(p) => p,
            None => return HandlerResult::skip(self.name()),
        };

        // Scan for generator-related events
        let events = persistence.recent_hook_events(7200).unwrap_or_default();
        let gen_events: Vec<_> = events
            .iter()
            .filter(|(_, _, tool, _)| tool.contains("generator") || tool.contains("Generator"))
            .collect();

        if gen_events.is_empty() {
            return HandlerResult::skip(self.name());
        }

        // Update Wilson for each generator type
        for (_, _, tool_name, reward) in &gen_events {
            let wilson_key = format!("generator:{tool_name}");
            let success = *reward > 0.0;
            let _ = persistence.wilson_update(&wilson_key, success);
        }

        // Store aggregate outcome
        if let Some(ref rlm) = ctx.rlm {
            let val = format!(
                "generators_used={} avg_reward={:.2}",
                gen_events.len(),
                gen_events.iter().map(|(_, _, _, r)| r).sum::<f64>() / gen_events.len() as f64,
            );
            let _ = rlm.store(
                &format!("generator:outcome:{}", epoch_secs()),
                touring_intelligence::rl::memory::rlm::MemoryTier::Reference,
                &val,
                Some("generator_outcome"),
                None,
            );
        }

        HandlerResult::skip(self.name())
    }
}

// ── H49: SessionOptimizer ─────────────────────────────────────────────

/// Analyze session patterns and store optimization hints.
/// Consolidated from dspy_session_optimizer + observer_learning_loop.
pub struct SessionOptimizerHandler;

impl Handler for SessionOptimizerHandler {
    fn name(&self) -> &str {
        "session_optimizer"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::Stop]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let persistence = match &ctx.persistence {
            Some(p) => p,
            None => return HandlerResult::skip(self.name()),
        };

        let events = persistence.recent_hook_events(7200).unwrap_or_default();
        if events.len() < 5 {
            return HandlerResult::skip(self.name());
        }

        // 1. Tool effectiveness: aggregate reward per tool
        let mut tool_rewards: HashMap<String, Vec<f64>> = HashMap::new();
        for (_, _, tool, reward) in &events {
            tool_rewards.entry(tool.clone()).or_default().push(*reward);
        }

        let mut hints: Vec<String> = Vec::new();

        // Best/worst tools
        let mut tool_avg: Vec<(String, f64)> = tool_rewards
            .iter()
            .filter(|(_, v)| v.len() >= 3)
            .map(|(k, v)| (k.clone(), v.iter().sum::<f64>() / v.len() as f64))
            .collect();
        tool_avg.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        if let Some((best, score)) = tool_avg.first() {
            hints.push(format!("best_tool={best} (avg_reward={score:.2})"));
        }
        if let Some((worst, score)) = tool_avg.last()
            && *score < 0.0
        {
            hints.push(format!("worst_tool={worst} (avg_reward={score:.2})"));
        }

        // 2. Rework detection
        let edits = ctx.knowledge.recent_edits_all(1000).unwrap_or_default();
        let mut file_edit_counts: HashMap<String, usize> = HashMap::new();
        for edit in &edits {
            *file_edit_counts.entry(edit.file_path.clone()).or_default() += 1;
        }
        let rework_files: Vec<_> = file_edit_counts.iter().filter(|(_, c)| **c >= 3).collect();
        if !rework_files.is_empty() {
            hints.push(format!("rework_files={}", rework_files.len()));
        }

        // 3. Failure clusters
        let failures: Vec<_> = events.iter().filter(|(_, _, _, r)| *r < 0.0).collect();
        if failures.len() > 3 {
            hints.push(format!("failure_count={}", failures.len()));
        }

        // Store optimization hints
        if !hints.is_empty()
            && let Some(ref rlm) = ctx.rlm
        {
            let val = hints.join(" | ");
            let _ = rlm.store(
                &format!("session:optimization:{}", epoch_secs()),
                touring_intelligence::rl::memory::rlm::MemoryTier::Working,
                &val,
                Some("optimization_hint"),
                None,
            );
        }

        HandlerResult::skip(self.name())
    }
}

// ── H50: Retrospective ───────────────────────────────────────────────

/// Session retrospective — lessons learned (complementary to Crystallizer).
pub struct RetrospectiveHandler;

impl Handler for RetrospectiveHandler {
    fn name(&self) -> &str {
        "retrospective"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreCompact]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let persistence = match &ctx.persistence {
            Some(p) => p,
            None => return HandlerResult::skip(self.name()),
        };

        if ctx.context_budget_remaining < 80 {
            return HandlerResult::skip(self.name());
        }

        let events = persistence.recent_hook_events(7200).unwrap_or_default();
        if events.len() < 5 {
            return HandlerResult::skip(self.name());
        }

        // Collect lessons
        let mut lessons: Vec<String> = Vec::new();

        // What went wrong: failures
        let failures: Vec<_> = events.iter().filter(|(_, _, _, r)| *r < -0.2).collect();
        if !failures.is_empty() {
            let fail_tools: Vec<&str> = failures.iter().map(|(_, _, t, _)| t.as_str()).collect();
            lessons.push(format!(
                "failures({}):{}",
                failures.len(),
                fail_tools
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }

        // What went right: high-reward events
        let successes = events.iter().filter(|(_, _, _, r)| *r > 0.5).count();
        if successes > 0 {
            lessons.push(format!("successes:{successes}"));
        }

        // Success rate
        let total = events.len();
        let positive = events.iter().filter(|(_, _, _, r)| *r > 0.0).count();
        let rate = if total > 0 {
            positive as f64 / total as f64
        } else {
            0.0
        };
        lessons.push(format!("rate:{:.0}%", rate * 100.0));

        // Store retrospective
        if let Some(ref rlm) = ctx.rlm {
            let val = lessons.join(" | ");
            let _ = rlm.store(
                &format!("session:retrospective:{}", epoch_secs()),
                touring_intelligence::rl::memory::rlm::MemoryTier::Working,
                &val,
                Some("retrospective"),
                None,
            );
        }

        if lessons.is_empty() {
            return HandlerResult::skip(self.name());
        }

        let context = format!("Retro: {}", lessons.join(" | "));
        HandlerResult::allow(self.name(), Some(context))
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ── Registration ──────────────────────────────────────────────────────

/// Registers all evolution-domain handlers on the given pipeline.
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(AcoEvolutionHandler));
    pipeline.register(Box::new(GeneratorLearningHandler));
    pipeline.register(Box::new(SessionOptimizerHandler));
    pipeline.register(Box::new(RetrospectiveHandler));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    #[allow(clippy::arc_with_non_send_sync)]
    fn make_ctx(
        event: HookEvent,
        input: serde_json::Value,
    ) -> (TempDir, crate::context::CortexContext) {
        let tmp = TempDir::new().unwrap();
        let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
        let knowledge = Arc::new(db);
        let ctx = crate::context::CortexContext::from_input(
            event,
            input,
            knowledge,
            tmp.path().to_path_buf(),
        );
        (tmp, ctx)
    }

    #[test]
    fn test_handler_properties() {
        assert_eq!(AcoEvolutionHandler.name(), "aco_evolution");
        assert_eq!(AcoEvolutionHandler.events(), &[HookEvent::Stop]);
        assert!(AcoEvolutionHandler.is_async());

        assert_eq!(GeneratorLearningHandler.name(), "generator_learning");
        assert!(GeneratorLearningHandler.is_async());

        assert_eq!(SessionOptimizerHandler.name(), "session_optimizer");
        assert!(SessionOptimizerHandler.is_async());

        assert_eq!(RetrospectiveHandler.name(), "retrospective");
        assert_eq!(RetrospectiveHandler.events(), &[HookEvent::PreCompact]);
        assert!(!RetrospectiveHandler.is_async()); // sync — injects context
    }

    // ── H47: AcoEvolutionHandler ──────────────────────────────────────

    #[test]
    fn test_aco_evolution_skips_no_persistence() {
        let (_tmp, mut ctx) = make_ctx(HookEvent::Stop, serde_json::json!({}));
        // No persistence → skip
        let result = AcoEvolutionHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_aco_evolution_handler_name_is_aco_evolution() {
        assert_eq!(AcoEvolutionHandler.name(), "aco_evolution");
    }

    #[test]
    fn test_aco_evolution_responds_to_stop_event_only() {
        assert_eq!(AcoEvolutionHandler.events(), &[HookEvent::Stop]);
        assert!(
            !AcoEvolutionHandler
                .events()
                .contains(&HookEvent::PreToolUse)
        );
        assert!(
            !AcoEvolutionHandler
                .events()
                .contains(&HookEvent::SessionStart)
        );
    }

    #[test]
    fn test_aco_evolution_is_async() {
        assert!(AcoEvolutionHandler.is_async());
    }

    #[test]
    fn test_aco_evolution_default_tool_matcher_none() {
        assert!(AcoEvolutionHandler.tool_matcher().is_none());
    }

    // ── H48: GeneratorLearningHandler ────────────────────────────────

    #[test]
    fn test_generator_learning_skips_no_persistence() {
        let (_tmp, mut ctx) = make_ctx(HookEvent::Stop, serde_json::json!({}));
        let result = GeneratorLearningHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_generator_learning_handler_name() {
        assert_eq!(GeneratorLearningHandler.name(), "generator_learning");
    }

    #[test]
    fn test_generator_learning_responds_to_stop() {
        assert_eq!(GeneratorLearningHandler.events(), &[HookEvent::Stop]);
    }

    #[test]
    fn test_generator_learning_is_async() {
        assert!(GeneratorLearningHandler.is_async());
    }

    #[test]
    fn test_generator_learning_tool_matcher_none() {
        assert!(GeneratorLearningHandler.tool_matcher().is_none());
    }

    // ── H49: SessionOptimizerHandler ─────────────────────────────────

    #[test]
    fn test_session_optimizer_skips_no_persistence() {
        let (_tmp, mut ctx) = make_ctx(HookEvent::Stop, serde_json::json!({}));
        let result = SessionOptimizerHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_session_optimizer_handler_name() {
        assert_eq!(SessionOptimizerHandler.name(), "session_optimizer");
    }

    #[test]
    fn test_session_optimizer_responds_to_stop() {
        assert_eq!(SessionOptimizerHandler.events(), &[HookEvent::Stop]);
    }

    #[test]
    fn test_session_optimizer_is_async() {
        assert!(SessionOptimizerHandler.is_async());
    }

    #[test]
    fn test_session_optimizer_tool_matcher_none() {
        assert!(SessionOptimizerHandler.tool_matcher().is_none());
    }

    // ── H50: RetrospectiveHandler ─────────────────────────────────────

    #[test]
    fn test_retrospective_skips_no_persistence() {
        let (_tmp, mut ctx) = make_ctx(HookEvent::PreCompact, serde_json::json!({}));
        let result = RetrospectiveHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_retrospective_handler_name() {
        assert_eq!(RetrospectiveHandler.name(), "retrospective");
    }

    #[test]
    fn test_retrospective_responds_to_precompact() {
        assert_eq!(RetrospectiveHandler.events(), &[HookEvent::PreCompact]);
        assert!(!RetrospectiveHandler.events().contains(&HookEvent::Stop));
    }

    #[test]
    fn test_retrospective_is_sync() {
        assert!(!RetrospectiveHandler.is_async());
    }

    #[test]
    fn test_retrospective_skips_low_budget() {
        let (_tmp, mut ctx) = make_ctx(HookEvent::PreCompact, serde_json::json!({}));
        ctx.context_budget_remaining = 50; // below 80 threshold
        let result = RetrospectiveHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_retrospective_tool_matcher_none() {
        assert!(RetrospectiveHandler.tool_matcher().is_none());
    }

    // ── Registration ─────────────────────────────────────────────────

    #[test]
    fn test_register_evolution_handlers() {
        let mut pipeline = Pipeline::new();
        register(&mut pipeline);
        // 4 handlers: H47, H48, H49, H50
        assert_eq!(pipeline.handler_count(), 4);
    }

    #[test]
    fn test_register_creates_pipeline_with_correct_events() {
        let mut pipeline = Pipeline::new();
        register(&mut pipeline);

        // Stop handlers: AcoEvolution, GeneratorLearning, SessionOptimizer
        let stop_handlers = pipeline.for_event(HookEvent::Stop, None);
        assert_eq!(stop_handlers.len(), 3);

        // PreCompact handlers: Retrospective
        let compact_handlers = pipeline.for_event(HookEvent::PreCompact, None);
        assert_eq!(compact_handlers.len(), 1);
        assert_eq!(compact_handlers[0].name(), "retrospective");

        // SessionStart: none in evolution
        let session_handlers = pipeline.for_event(HookEvent::SessionStart, None);
        assert_eq!(session_handlers.len(), 0);
    }

    #[test]
    fn test_evolution_handlers_stop_names() {
        let mut pipeline = Pipeline::new();
        register(&mut pipeline);
        let stop_handlers = pipeline.for_event(HookEvent::Stop, None);
        let names: Vec<&str> = stop_handlers.iter().map(|h| h.name()).collect();
        assert!(names.contains(&"aco_evolution"));
        assert!(names.contains(&"generator_learning"));
        assert!(names.contains(&"session_optimizer"));
    }
}
