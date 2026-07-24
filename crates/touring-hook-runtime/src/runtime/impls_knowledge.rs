//! KnowledgeDB and ResultCache implementations for HookRuntime.

use super::traits::{KnowledgeDB, ResultCache};
use crate::aco_bridge::{HookOutcome, HookQualityAssessment, HookResultCache};
use crate::async_knowledge::AsyncFileKnowledgeDB;
use crate::knowledge::{FileKnowledgeDB, PreReadSignals};
use crate::runtime::HookRuntime;

impl KnowledgeDB for HookRuntime {
    fn record_hook_outcome(&mut self, outcome: HookOutcome) {
        if let Some(ref mut assessment) = self.ctx.quality_assessment {
            assessment.record(outcome);
        }
    }

    fn quality_report(
        &self,
        iteration: u32,
    ) -> Option<touring_intelligence::rl::aco::tracker::TrackerReport> {
        self.ctx
            .quality_assessment
            .as_ref()
            .map(|a| a.to_tracker_report(iteration))
    }

    fn reset_quality_tracking(&mut self, session_id: &str) {
        self.ctx.quality_assessment = Some(HookQualityAssessment::new(session_id));
        self.session_turn
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }

    fn knowledge_db(&self) -> &FileKnowledgeDB {
        &self.ctx.knowledge
    }

    fn async_knowledge_db(&self) -> Option<&AsyncFileKnowledgeDB> {
        self.ctx.async_knowledge.as_ref()
    }

    fn quality_assessment(&self) -> Option<&HookQualityAssessment> {
        self.ctx.quality_assessment.as_ref()
    }

    fn init_async_knowledge(&mut self) {
        let db_path = touring_foundation::TouringConfig::knowledge_db_canonical(&self.project_root);
        match AsyncFileKnowledgeDB::new(&db_path) {
            Ok(async_db) => {
                self.ctx.async_knowledge = Some(async_db);
                tracing::info!("async knowledge pool initialized");
            }
            Err(e) => {
                tracing::warn!("async knowledge pool init failed: {e} — falling back to sync");
            }
        }
    }

    fn batch_pre_read_signals(&self, file_path: &str) -> PreReadSignals {
        self.ctx
            .knowledge
            .batch_pre_read_signals(file_path)
            .unwrap_or_default()
    }
}

impl ResultCache for HookRuntime {
    fn check_cache(&self, hook_name: &str, file_path: &str) -> Option<String> {
        self.ctx.result_cache.get_result(hook_name, file_path)
    }

    fn store_cache(&self, hook_name: &str, file_path: &str, result_json: String) {
        self.ctx
            .result_cache
            .cache_result(hook_name, file_path, result_json);
    }

    fn invalidate_cache_for_file(&self, file_path: &str) -> usize {
        self.ctx.result_cache.invalidate_file(file_path)
    }

    fn cache_hit_rate(&self) -> f64 {
        self.ctx.result_cache.hit_rate()
    }

    fn cache_ref(&self) -> &HookResultCache {
        &self.ctx.result_cache
    }
}
