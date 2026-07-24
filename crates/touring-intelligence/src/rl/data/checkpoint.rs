//! CheckpointLoader — loads `.toon` checkpoint files into [`MutableGeneratorGraph`].
//!
//! Scans `~/.claude/checkpoints/*.toon` and builds one graph per checkpoint.
//! Each checkpoint file is parsed and a single `MutableGeneratorGraph` is returned.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::rl::aco::graph::{GraphError, MutableGeneratorGraph};
use crate::rl::aco::models::{GeneratorContract, GeneratorNode, GeneratorOutputs, GeneratorType};

// ── Error types ─────────────────────────────────────────────────────────────

/// Errors raised while loading or parsing `.toon` checkpoint files.
#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    /// Filesystem read failed.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// Checkpoint JSON could not be deserialized into `CheckpointData`.
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    /// Building the `MutableGeneratorGraph` from checkpoint data failed.
    #[error("Graph error: {0}")]
    Graph(#[from] GraphError),
    /// Named checkpoint file does not exist in the loader directory.
    #[error("Checkpoint not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, CheckpointError>;

// ── Checkpoint data parsed from a .toon file ─────────────────────────────────

/// Raw checkpoint data — supports multiple .toon file formats.
/// Variants are constructed by serde deserialization (untagged union) and matched in
/// `build_graph` to produce the appropriate `MutableGeneratorGraph` node type.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum CheckpointData {
    /// `aco_evolution_<ts>.toon` — ACO event metadata.
    AcoEvolution {
        aco_events: u64,
        session_id: String,
        /// EC65: deserialized for schema completeness; not used in build_graph.
        #[allow(dead_code)]
        timestamp: u64,
        total_events: u64,
    },
    /// `session_summary_<ts>.toon` — session summary with file list.
    SessionSummary {
        ended_at: String,
        file_list: Vec<String>,
    },
    /// `pln2_phase_<N>.toon` — plan phase.
    Pln2Phase {
        id: String,
        /// EC65: deserialized for schema completeness; not used in build_graph.
        #[allow(dead_code)]
        title: String,
        status: String,
        /// EC65: deserialized for schema completeness; not used in build_graph.
        #[allow(dead_code)]
        tasks: Vec<String>,
    },
    /// `dspy_*.toon` — DSPy integration result.
    DspyComplete {
        status: String,
        version: String,
        checks_passed: u32,
        checks_total: u32,
        composite_score: f64,
    },
    /// Generic JSON object fallback.
    Generic(HashMap<String, serde_json::Value>),
}

// ── CheckpointLoader ─────────────────────────────────────────────────────────

/// Loads `.toon` checkpoint files from a directory.
#[derive(Debug)]
pub struct CheckpointLoader {
    dir: PathBuf,
}

impl CheckpointLoader {
    /// Create a new loader pointing at `dir`.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Default checkpoint directory: `~/.claude/checkpoints/`.
    pub fn default_dir() -> Self {
        let dir = if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".claude/checkpoints")
        } else {
            PathBuf::from(".claude/checkpoints")
        };
        Self::new(dir)
    }

    /// Load a specific checkpoint by name (without `.toon` extension).
    pub fn load_checkpoint(&self, name: &str) -> Result<MutableGeneratorGraph> {
        let file = self.dir.join(format!("{name}.toon"));
        if !file.exists() {
            return Err(CheckpointError::NotFound(name.to_string()));
        }
        self.parse_file(&file)
    }

    /// Load all `.toon` files from the directory.
    /// Returns a map of checkpoint name -> graph.
    pub fn load_all(&self) -> Result<HashMap<String, MutableGeneratorGraph>> {
        let mut out = HashMap::new();
        let entries = fs::read_dir(&self.dir)?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("toon") {
                continue;
            }
            let name = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            match self.parse_file(&path) {
                Ok(graph) => {
                    out.insert(name, graph);
                }
                Err(e) => {
                    tracing::warn!("[checkpoint] skipping '{}': {}", name, e);
                }
            }
        }
        Ok(out)
    }

    /// Return the most recent checkpoint by timestamp in filename.
    /// Returns `(name, graph)` for the checkpoint with the highest timestamp.
    pub fn latest(&self) -> Result<(String, MutableGeneratorGraph)> {
        let all = self.load_all()?;
        let mut sorted: Vec<_> = all.into_iter().collect();
        sorted.sort_by(|a, b| {
            timestamp_from_name(&a.0)
                .unwrap_or(0)
                .cmp(&timestamp_from_name(&b.0).unwrap_or(0))
        });
        sorted
            .pop()
            .ok_or_else(|| CheckpointError::NotFound("no checkpoints found".to_string()))
    }

    fn parse_file(&self, path: &Path) -> Result<MutableGeneratorGraph> {
        let data = fs::read_to_string(path)?;
        let data: CheckpointData = serde_json::from_str(&data)?;
        self.build_graph(
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref(),
            data,
        )
    }

    fn build_graph(&self, name: &str, data: CheckpointData) -> Result<MutableGeneratorGraph> {
        let mut graph = MutableGeneratorGraph::new();
        let node = match data {
            CheckpointData::AcoEvolution {
                aco_events,
                session_id,
                timestamp: _,
                total_events,
            } => {
                let desc = format!(
                    "aco_events={}, total_events={}, session_id={}",
                    aco_events, total_events, session_id
                );
                self.make_node(name, GeneratorType::Validator, &desc)
            }
            CheckpointData::SessionSummary {
                ended_at,
                file_list,
            } => {
                let desc = format!("ended_at={}, files={}", ended_at, file_list.len());
                self.make_node(name, GeneratorType::Composer, &desc)
            }
            CheckpointData::Pln2Phase { id, status, .. } => {
                let desc = format!("phase={}, status={}", id, status);
                self.make_node(name, GeneratorType::Template, &desc)
            }
            CheckpointData::DspyComplete {
                status,
                version,
                checks_passed,
                checks_total,
                composite_score,
            } => {
                let desc = format!(
                    "status={}, version={}, checks={}/{}, score={}",
                    status, version, checks_passed, checks_total, composite_score
                );
                self.make_node(name, GeneratorType::Transformer, &desc)
            }
            CheckpointData::Generic(map) => {
                let desc = format!("{:?}", map);
                self.make_node(name, GeneratorType::Validator, &desc)
            }
        };
        graph.add_node(node)?;
        Ok(graph)
    }

    fn make_node(&self, id: &str, gen_type: GeneratorType, desc: &str) -> GeneratorNode {
        GeneratorNode {
            id: id.to_string(),
            description: format!("[checkpoint] {}", desc),
            generator_type: gen_type,
            inputs_data: vec![],
            inputs_templates: vec![],
            inputs_constraints: vec![],
            outputs: GeneratorOutputs {
                execution_script: String::new(),
                validation_script: String::new(),
                rollback_script: String::new(),
            },
            contract: GeneratorContract {
                precondition: String::new(),
                postcondition: String::new(),
                invariant: String::new(),
            },
            acceptance_criteria: String::new(),
            depends_on: vec![],
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract timestamp from checkpoint name patterns like `aco_evolution_<ts>`.
fn timestamp_from_name(name: &str) -> Option<u64> {
    name.split('_').next_back().and_then(|s| {
        if s.len() == 10 && s.chars().all(|c| c.is_ascii_digit()) {
            s.parse::<u64>().ok()
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_from_name() {
        assert_eq!(
            timestamp_from_name("aco_evolution_1774137759"),
            Some(1774137759)
        );
        assert_eq!(
            timestamp_from_name("session_summary_1774137532"),
            Some(1774137532)
        );
        assert_eq!(timestamp_from_name("pln2_phase_01"), None);
        assert_eq!(timestamp_from_name("dspy_rust_integration-COMPLETE"), None);
    }

    #[test]
    fn test_checkpoint_loader_creation() {
        let loader = CheckpointLoader::new("/tmp/checkpoints");
        assert_eq!(loader.dir, PathBuf::from("/tmp/checkpoints"));
    }

    #[test]
    fn test_default_loader_dir() {
        let loader = CheckpointLoader::default_dir();
        assert!(loader.dir.to_string_lossy().contains(".claude/checkpoints"));
    }
}
