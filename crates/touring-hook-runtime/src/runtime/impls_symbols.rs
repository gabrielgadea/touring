//! SymbolIndex, Pipeline, and DependencyCacheOps implementations for HookRuntime.

use std::path::{Path, PathBuf};

use touring_code::ast::graph::SymbolLocation;
use touring_code::ast::{
    BlastRadiusOutput, IncrementalEditResult, SharedPipeline, SymbolIndex, SymbolStore,
};

use super::traits::{DependencyCacheOps, Pipeline, SymbolIndexOps};
use crate::dependency_cache::DependencyCache;
use crate::runtime::HookRuntime;

impl SymbolIndexOps for HookRuntime {
    fn symbol_store(&self) -> Option<&SymbolStore> {
        self.infra.symbol_store.as_ref()
    }

    fn get_symbol_index(&mut self) -> &SymbolIndex {
        // One implementation, and the one loader: the inherent method.
        HookRuntime::get_symbol_index(self)
    }

    fn find_symbol(&mut self, name: &str) -> Vec<SymbolLocation> {
        HookRuntime::find_symbol(self, name)
    }

    fn blast_radius(&mut self, file_path: &str, max_depth: usize) -> BlastRadiusOutput {
        HookRuntime::blast_radius(self, file_path, max_depth)
    }

    fn persist_symbols(&self, file_path: &str, symbols: &[SymbolLocation]) -> Result<(), String> {
        if let Some(ref store) = self.infra.symbol_store {
            store
                .replace_file_symbols(file_path, symbols)
                .map_err(|e| format!("Failed to persist symbols: {e}"))?;
        }
        Ok(())
    }
}

impl Pipeline for HookRuntime {
    fn process_file(
        &self,
        file_path: &str,
        content: &str,
    ) -> Result<IncrementalEditResult, String> {
        // One implementation: the inherent method also applies the memory budget.
        HookRuntime::process_file(self, file_path, content)
    }

    fn get_cached_symbols(&self, file_path: &str) -> Vec<SymbolLocation> {
        self.infra
            .pipeline
            .as_ref()
            .map(|p| p.get_symbols(file_path))
            .unwrap_or_default()
    }

    fn pipeline_cache_stats(&self) -> Option<(usize, usize)> {
        self.infra.pipeline.as_ref().map(|p| p.cache_stats())
    }

    fn pipeline_ref(&self) -> Option<&SharedPipeline> {
        self.infra.pipeline.as_ref()
    }
}

impl DependencyCacheOps for HookRuntime {
    fn init_dependency_cache(&mut self) {
        let relations = self.ctx.knowledge.all_file_relations();
        let cache = DependencyCache::build_from_relations(
            relations.into_iter().map(|r| (r.source, r.target)),
        );
        tracing::info!(
            nodes = cache.node_count(),
            edges = cache.edge_count(),
            "dependency_cache initialized from SQLite"
        );
        self.infra.dependency_cache = Some(cache);
    }

    fn add_dependency(&mut self, from: &Path, to: &Path) {
        if let Some(ref mut cache) = self.infra.dependency_cache {
            cache.add_relation(&from.to_path_buf(), &to.to_path_buf());
        }
    }

    fn invalidate_dependency_cache_for_file(&mut self, path: &Path) {
        if let Some(ref mut cache) = self.infra.dependency_cache {
            cache.invalidate_file(&path.to_path_buf());
        }
    }

    fn petgraph_blast_radius(&self, path: &Path) -> Option<Vec<PathBuf>> {
        self.infra
            .dependency_cache
            .as_ref()
            .map(|c| c.blast_radius(&path.to_path_buf()).files())
    }

    fn dependency_cache_ref(&self) -> Option<&DependencyCache> {
        self.infra.dependency_cache.as_ref()
    }
}
