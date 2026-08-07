//! Utility Tools - touring_index_status and touring_checkpoint
//!
//! Implements:
//! - touring_index_status: Return index statistics, cache stats, memory stats
//! - touring_checkpoint: Read/write checkpoint data (JSON or TOON format)

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use touring_foundation::config::TouringConfig;
use tracing::{debug, info};

/// Errors specific to utility operations
#[derive(Error, Debug)]
pub(crate) enum UtilityToolsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Checkpoint not found: {0}")]
    CheckpointNotFound(PathBuf),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}

/// Result type for utility tool operations
pub(crate) type Result<T> = std::result::Result<T, UtilityToolsError>;

/// Checkpoint format types
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum CheckpointFormat {
    /// JSON format
    #[default]
    Json,
    /// TOON format (Touring Object Notation)
    Toon,
}

impl std::fmt::Display for CheckpointFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckpointFormat::Json => write!(f, "json"),
            CheckpointFormat::Toon => write!(f, "toon"),
        }
    }
}

/// Input for touring_index_status tool
#[derive(Debug, Clone, Deserialize)]
pub struct IndexStatusInput {
    /// Include detailed memory stats
    #[serde(default)]
    pub detailed: bool,
}

/// Input for touring_checkpoint tool
#[derive(Debug, Clone, Deserialize)]
pub struct CheckpointInput {
    /// Operation: read or write
    pub operation: String,
    /// Checkpoint name/identifier
    pub name: String,
    /// Data to write (for write operation)
    pub data: Option<serde_json::Value>,
    /// Format: json or toon
    #[serde(default)]
    pub format: CheckpointFormat,
}

/// Memory statistics
#[derive(Debug, Clone, Serialize)]
pub struct MemoryStats {
    /// Total memory usage estimate (bytes)
    pub estimated_bytes: usize,
    /// Number of cached entries
    pub cached_entries: usize,
    /// Cache hit rate
    pub cache_hit_rate: f64,
}

/// Index status information
#[derive(Debug, Clone, Serialize)]
pub struct IndexStatus {
    /// Files indexed
    pub files_indexed: usize,
    /// Total symbols indexed
    pub total_symbols: usize,
    /// Cache statistics
    pub cache_stats: CacheStats,
    /// Memory statistics
    pub memory_stats: MemoryStats,
    /// Last update timestamp
    pub last_update: Option<String>,
    /// Index is active
    pub is_active: bool,
}

/// Cache statistics
#[derive(Debug, Clone, Serialize)]
pub struct CacheStats {
    /// Entries in cache
    pub entries: usize,
    /// Current cache size (bytes)
    pub current_size: usize,
    /// Maximum cache size (bytes)
    pub max_size: usize,
    /// Hit rate (0.0 - 1.0)
    pub hit_rate: f64,
}

/// Output from touring_index_status tool
#[derive(Debug, Clone, Serialize)]
pub struct IndexStatusOutput {
    /// Success status
    pub success: bool,
    /// Index status
    pub status: IndexStatus,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Checkpoint data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointData {
    /// Checkpoint name
    pub name: String,
    /// Creation timestamp
    pub created_at: String,
    /// Checkpoint data
    pub data: serde_json::Value,
    /// Format
    pub format: String,
    /// Version
    pub version: String,
}

/// Output from touring_checkpoint tool
#[derive(Debug, Clone, Serialize)]
pub struct CheckpointOutput {
    /// Success status
    pub success: bool,
    /// Operation performed
    pub operation: String,
    /// Checkpoint name
    pub name: String,
    /// Checkpoint data (for read operations)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<CheckpointData>,
    /// List of checkpoints (for list operations)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoints: Option<Vec<String>>,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Status message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Query real stats from the symbols SQLite database.
///
/// Returns (files_indexed, total_symbols, last_update) with graceful fallback to zeros.
fn query_symbols_db_stats(db_path: &std::path::Path) -> (usize, usize, Option<String>) {
    let conn = match rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(_) => return (0, 0, None),
    };

    let files: usize = conn
        .query_row("SELECT COUNT(DISTINCT file_path) FROM symbols", [], |row| {
            row.get::<_, i64>(0).map(|v| v as usize)
        })
        .unwrap_or(0);

    let symbols: usize = conn
        .query_row("SELECT COUNT(*) FROM symbols", [], |row| {
            row.get::<_, i64>(0).map(|v| v as usize)
        })
        .unwrap_or(0);

    let last_update: Option<String> = conn
        .query_row("SELECT MAX(updated_at) FROM symbols", [], |row| row.get(0))
        .ok()
        .flatten();

    (files, symbols, last_update)
}

/// Utility tools handler
#[derive(Debug)]
pub struct UtilityTools {
    config: TouringConfig,
    checkpoint_dir: PathBuf,
}

impl UtilityTools {
    /// Create new utility tools handler
    pub fn new(config: &TouringConfig) -> Self {
        let checkpoint_dir = config.project_root.join(".claude").join("checkpoints");
        Self {
            config: config.clone(),
            checkpoint_dir,
        }
    }

    /// Get index status by querying the real symbols database.
    pub(crate) fn get_index_status(&self, _input: IndexStatusInput) -> Result<IndexStatusOutput> {
        let db_path = &self.config.symbols_db_path;
        let (files_indexed, total_symbols, last_update) = if db_path.exists() {
            query_symbols_db_stats(db_path)
        } else {
            (0, 0, None)
        };

        let cache_stats = CacheStats {
            entries: 0,
            current_size: 0,
            max_size: self.config.cache_size,
            hit_rate: 0.0,
        };

        let memory_stats = MemoryStats {
            estimated_bytes: 0,
            cached_entries: 0,
            cache_hit_rate: 0.0,
        };

        let status = IndexStatus {
            files_indexed,
            total_symbols,
            cache_stats,
            memory_stats,
            last_update,
            is_active: db_path.exists(),
        };

        Ok(IndexStatusOutput {
            success: true,
            status,
            error: None,
        })
    }

    /// Get checkpoint path
    fn checkpoint_path(&self, name: &str, format: CheckpointFormat) -> PathBuf {
        let ext = match format {
            CheckpointFormat::Json => "json",
            CheckpointFormat::Toon => "toon",
        };
        self.checkpoint_dir.join(format!("{}.{}", name, ext))
    }

    /// Read a checkpoint
    fn read_checkpoint(&self, name: &str, format: CheckpointFormat) -> Result<CheckpointOutput> {
        let path = self.checkpoint_path(name, format);

        if !path.exists() {
            return Err(UtilityToolsError::CheckpointNotFound(path));
        }

        let content = std::fs::read_to_string(&path)?;

        let data = match format {
            CheckpointFormat::Json => serde_json::from_str(&content)?,
            CheckpointFormat::Toon => {
                // Parse TOON format (simplified)
                Self::parse_toon(&content, name)?
            }
        };

        Ok(CheckpointOutput {
            success: true,
            operation: "read".to_string(),
            name: name.to_string(),
            data: Some(data),
            checkpoints: None,
            error: None,
            message: Some(format!("Read checkpoint: {}", name)),
        })
    }

    /// Write a checkpoint
    fn write_checkpoint(
        &self,
        name: &str,
        data: serde_json::Value,
        format: CheckpointFormat,
    ) -> Result<CheckpointOutput> {
        // Ensure checkpoint directory exists
        std::fs::create_dir_all(&self.checkpoint_dir)?;

        let path = self.checkpoint_path(name, format);

        let checkpoint = CheckpointData {
            name: name.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            data,
            format: format.to_string(),
            version: "1.0".to_string(),
        };

        let content = match format {
            CheckpointFormat::Json => serde_json::to_string_pretty(&checkpoint)?,
            CheckpointFormat::Toon => Self::to_toon(&checkpoint)?,
        };

        // Atomic write
        let temp_path = path.with_extension("tmp");
        std::fs::write(&temp_path, content)?;
        std::fs::rename(&temp_path, &path)?;

        info!("Wrote checkpoint: {:?}", path);

        Ok(CheckpointOutput {
            success: true,
            operation: "write".to_string(),
            name: name.to_string(),
            data: None,
            checkpoints: None,
            error: None,
            message: Some(format!("Wrote checkpoint: {}", name)),
        })
    }

    /// List all checkpoints
    fn list_checkpoints(&self) -> Result<CheckpointOutput> {
        let mut checkpoints = Vec::new();

        if self.checkpoint_dir.exists() {
            for entry in std::fs::read_dir(&self.checkpoint_dir)? {
                let entry = entry?;
                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                // Extract name without extension
                if let Some(stem) = name_str.split('.').next()
                    && !checkpoints.contains(&stem.to_string())
                {
                    checkpoints.push(stem.to_string());
                }
            }
        }

        checkpoints.sort();

        Ok(CheckpointOutput {
            success: true,
            operation: "list".to_string(),
            name: "".to_string(),
            data: None,
            checkpoints: Some(checkpoints.clone()),
            error: None,
            message: Some(format!("Found {} checkpoints", checkpoints.len())),
        })
    }

    /// Parse TOON format (simplified)
    fn parse_toon(content: &str, name: &str) -> Result<CheckpointData> {
        // Simple TOON parser - looks for key: value pairs
        let mut data = serde_json::Map::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(pos) = line.find(':') {
                let key = line[..pos].trim();
                let value = line[pos + 1..].trim();

                // Try to parse as different types
                let json_value = if value == "true" {
                    serde_json::Value::Bool(true)
                } else if value == "false" {
                    serde_json::Value::Bool(false)
                } else if let Ok(n) = value.parse::<i64>() {
                    serde_json::Value::Number(n.into())
                } else if let Ok(n) = value.parse::<f64>() {
                    serde_json::Number::from_f64(n)
                        .map(serde_json::Value::Number)
                        .unwrap_or_else(|| serde_json::Value::String(value.to_string()))
                } else {
                    serde_json::Value::String(value.to_string())
                };

                data.insert(key.to_string(), json_value);
            }
        }

        Ok(CheckpointData {
            name: name.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            data: serde_json::Value::Object(data),
            format: "toon".to_string(),
            version: "1.0".to_string(),
        })
    }

    /// Convert to TOON format (simplified)
    fn to_toon(checkpoint: &CheckpointData) -> Result<String> {
        let mut lines = Vec::new();

        lines.push(format!("# Checkpoint: {}", checkpoint.name));
        lines.push(format!("# Created: {}", checkpoint.created_at));
        lines.push(format!("# Version: {}", checkpoint.version));
        lines.push("".to_string());

        if let serde_json::Value::Object(map) = &checkpoint.data {
            for (key, value) in map {
                let value_str = match value {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    serde_json::Value::Null => "null".to_string(),
                    _ => value.to_string(),
                };
                lines.push(format!("{}: {}", key, value_str));
            }
        } else {
            lines.push(format!("data: {}", checkpoint.data));
        }

        Ok(lines.join("\n"))
    }

    /// Handle touring_index_status tool
    pub(crate) fn handle_index_status(&self, input: IndexStatusInput) -> IndexStatusOutput {
        match self.get_index_status(input) {
            Ok(output) => output,
            Err(e) => IndexStatusOutput {
                success: false,
                status: IndexStatus {
                    files_indexed: 0,
                    total_symbols: 0,
                    cache_stats: CacheStats {
                        entries: 0,
                        current_size: 0,
                        max_size: 0,
                        hit_rate: 0.0,
                    },
                    memory_stats: MemoryStats {
                        estimated_bytes: 0,
                        cached_entries: 0,
                        cache_hit_rate: 0.0,
                    },
                    last_update: None,
                    is_active: false,
                },
                error: Some(e.to_string()),
            },
        }
    }

    /// Handle touring_checkpoint tool
    pub(crate) fn handle_checkpoint(&self, input: CheckpointInput) -> CheckpointOutput {
        debug!("Checkpoint: {} operation: {}", input.name, input.operation);

        let result = match input.operation.as_str() {
            "read" => self.read_checkpoint(&input.name, input.format),
            "write" => {
                if let Some(data) = input.data {
                    self.write_checkpoint(&input.name, data, input.format)
                } else {
                    Err(UtilityToolsError::InvalidOperation(
                        "Write requires data".to_string(),
                    ))
                }
            }
            "list" => self.list_checkpoints(),
            _ => Err(UtilityToolsError::InvalidOperation(format!(
                "Unknown operation: {}",
                input.operation
            ))),
        };

        match result {
            Ok(output) => output,
            Err(e) => CheckpointOutput {
                success: false,
                operation: input.operation,
                name: input.name,
                data: None,
                checkpoints: None,
                error: Some(e.to_string()),
                message: None,
            },
        }
    }

    /// Handle async version of index_status
    pub async fn handle_index_status_async(&self, input: IndexStatusInput) -> IndexStatusOutput {
        self.handle_index_status(input)
    }

    /// Handle async version of checkpoint
    pub async fn handle_checkpoint_async(&self, input: CheckpointInput) -> CheckpointOutput {
        self.handle_checkpoint(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_config(temp_dir: &TempDir) -> TouringConfig {
        TouringConfig {
            project_root: temp_dir.path().to_path_buf(),
            symbols_db_path: temp_dir.path().join("symbols.db"),
            rlm_db_path: temp_dir.path().join("rlm.db"),
            semantic_db_path: temp_dir.path().join("semantic.db"),
            cache_size: 1000,
            watcher_debounce_ms: 100,
            max_file_size: 1024 * 1024,
            debug: false,
            gpu_service_url: "http://localhost:8200".to_string(),
            embedding_dim: 384,
            auto_embed: false,
            ..Default::default()
        }
    }

    #[test]
    fn test_get_index_status_no_db() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let tools = UtilityTools::new(&config);

        let input = IndexStatusInput { detailed: false };
        let output = tools.get_index_status(input).unwrap();

        assert!(output.success);
        // No symbols.db in temp dir → not active, zero counts
        assert!(!output.status.is_active);
        assert_eq!(output.status.files_indexed, 0);
        assert_eq!(output.status.total_symbols, 0);
    }

    #[test]
    fn test_get_index_status_with_db() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = create_test_config(&temp_dir);
        let db_path = temp_dir.path().join("symbols.db");
        // Create a minimal symbols table
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS symbols (
                file_path TEXT NOT NULL,
                name TEXT NOT NULL,
                updated_at TEXT DEFAULT (datetime('now'))
            );
            INSERT INTO symbols (file_path, name) VALUES ('a.rs', 'foo');
            INSERT INTO symbols (file_path, name) VALUES ('a.rs', 'bar');
            INSERT INTO symbols (file_path, name) VALUES ('b.rs', 'baz');",
        )
        .unwrap();
        config.symbols_db_path = db_path;
        let tools = UtilityTools::new(&config);

        let input = IndexStatusInput { detailed: false };
        let output = tools.get_index_status(input).unwrap();

        assert!(output.success);
        assert!(output.status.is_active);
        assert_eq!(output.status.files_indexed, 2);
        assert_eq!(output.status.total_symbols, 3);
        assert!(output.status.last_update.is_some());
    }

    #[test]
    fn test_write_and_read_checkpoint_json() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let tools = UtilityTools::new(&config);

        // Write checkpoint
        let data = serde_json::json!({
            "key": "value",
            "number": 42,
            "bool": true
        });

        let write_input = CheckpointInput {
            operation: "write".to_string(),
            name: "test_checkpoint".to_string(),
            data: Some(data),
            format: CheckpointFormat::Json,
        };

        let write_output = tools.handle_checkpoint(write_input);
        assert!(write_output.success);

        // Read checkpoint
        let read_input = CheckpointInput {
            operation: "read".to_string(),
            name: "test_checkpoint".to_string(),
            data: None,
            format: CheckpointFormat::Json,
        };

        let read_output = tools.handle_checkpoint(read_input);
        assert!(read_output.success);
        assert!(read_output.data.is_some());

        let checkpoint = read_output.data.unwrap();
        assert_eq!(checkpoint.name, "test_checkpoint");
        assert_eq!(checkpoint.format, "json");
    }

    #[test]
    fn test_write_and_read_checkpoint_toon() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let tools = UtilityTools::new(&config);

        // Write checkpoint
        let data = serde_json::json!({
            "status": "active",
            "count": 10
        });

        let write_input = CheckpointInput {
            operation: "write".to_string(),
            name: "test_toon".to_string(),
            data: Some(data),
            format: CheckpointFormat::Toon,
        };

        let write_output = tools.handle_checkpoint(write_input);
        assert!(write_output.success);

        // Read checkpoint
        let read_input = CheckpointInput {
            operation: "read".to_string(),
            name: "test_toon".to_string(),
            data: None,
            format: CheckpointFormat::Toon,
        };

        let read_output = tools.handle_checkpoint(read_input);
        assert!(read_output.success);
        assert!(read_output.data.is_some());
    }

    #[test]
    fn test_list_checkpoints() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let tools = UtilityTools::new(&config);

        // Create a checkpoint first
        let data = serde_json::json!({"test": true});
        let write_input = CheckpointInput {
            operation: "write".to_string(),
            name: "checkpoint1".to_string(),
            data: Some(data),
            format: CheckpointFormat::Json,
        };
        tools.handle_checkpoint(write_input);

        // List checkpoints
        let list_input = CheckpointInput {
            operation: "list".to_string(),
            name: "".to_string(),
            data: None,
            format: CheckpointFormat::Json,
        };

        let list_output = tools.handle_checkpoint(list_input);
        assert!(list_output.success);
        assert!(list_output.checkpoints.is_some());
        assert!(
            list_output
                .checkpoints
                .unwrap()
                .contains(&"checkpoint1".to_string())
        );
    }

    #[test]
    fn test_read_nonexistent_checkpoint() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let tools = UtilityTools::new(&config);

        let read_input = CheckpointInput {
            operation: "read".to_string(),
            name: "nonexistent".to_string(),
            data: None,
            format: CheckpointFormat::Json,
        };

        let read_output = tools.handle_checkpoint(read_input);
        assert!(!read_output.success);
        assert!(read_output.error.is_some());
    }

    #[test]
    fn test_toon_format() {
        let checkpoint = CheckpointData {
            name: "test".to_string(),
            created_at: "2024-01-01".to_string(),
            data: serde_json::json!({
                "status": "active",
                "count": 42,
                "enabled": true
            }),
            format: "toon".to_string(),
            version: "1.0".to_string(),
        };

        let toon_str = UtilityTools::to_toon(&checkpoint).unwrap();
        assert!(toon_str.contains("status: active"));
        assert!(toon_str.contains("count: 42"));
        assert!(toon_str.contains("enabled: true"));

        // Parse it back
        let parsed = UtilityTools::parse_toon(&toon_str, "test").unwrap();
        assert_eq!(parsed.name, "test");
    }
}
