//! Project Tools - touring_project
//!
//! Implements project management operations:
//! - status: Get project status
//! - activate: Set active project
//! - config: Get/set configuration values

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use touring_foundation::config::TouringConfig;
use tracing::{debug, info};

/// Errors specific to project operations
#[derive(Error, Debug)]
pub enum ProjectToolsError {
    /// A filesystem I/O operation failed.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Loading or applying project configuration failed.
    #[error("Config error: {0}")]
    Config(String),

    /// The named project is not registered.
    #[error("Project not found: {0}")]
    NotFound(String),

    /// The requested project operation is not valid.
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}

/// Result type for project tool operations
pub(crate) type Result<T> = std::result::Result<T, ProjectToolsError>;

/// Project operation types
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectAction {
    /// Get project status
    Status,
    /// Activate a project
    Activate,
    /// Get/set configuration
    Config,
}

impl std::fmt::Display for ProjectAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectAction::Status => write!(f, "status"),
            ProjectAction::Activate => write!(f, "activate"),
            ProjectAction::Config => write!(f, "config"),
        }
    }
}

/// Input for touring_project tool
#[derive(Debug, Clone, Deserialize)]
pub struct ProjectInput {
    /// Action to perform
    pub action: ProjectAction,
    /// Project name or path (for activate)
    pub project: Option<String>,
    /// Configuration key (for config)
    pub key: Option<String>,
    /// Configuration value (for config set)
    pub value: Option<String>,
}

/// Project information
#[derive(Debug, Clone, Serialize)]
pub struct ProjectInfo {
    /// Project name
    pub name: String,
    /// Project root path
    pub root: String,
    /// Is currently active
    pub is_active: bool,
    /// Number of files indexed
    pub files_indexed: Option<usize>,
    /// Last indexed timestamp
    pub last_indexed: Option<String>,
}

/// Configuration entry
#[derive(Debug, Clone, Serialize)]
pub struct ConfigEntry {
    /// Configuration key
    pub key: String,
    /// Configuration value
    pub value: serde_json::Value,
    /// Is default value
    pub is_default: bool,
}

/// Output from touring_project tool
#[derive(Debug, Clone, Serialize)]
pub struct ProjectOutput {
    /// Success status
    pub success: bool,
    /// Action performed
    pub action: String,
    /// Active project info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_project: Option<ProjectInfo>,
    /// List of projects (for status)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projects: Option<Vec<ProjectInfo>>,
    /// Configuration values (for config)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<Vec<ConfigEntry>>,
    /// Error message if operation failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Status message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Project tools handler
#[derive(Debug)]
pub struct ProjectTools {
    config: TouringConfig,
    active_project: Option<String>,
}

impl ProjectTools {
    /// Create new project tools handler
    pub fn new(config: &TouringConfig) -> Self {
        Self {
            config: config.clone(),
            active_project: None,
        }
    }

    /// Get project status
    fn get_status(&self) -> Result<ProjectOutput> {
        let root_str = self.config.project_root.to_string_lossy().to_string();
        let project_name = self
            .config
            .project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let active_project = ProjectInfo {
            name: project_name.clone(),
            root: root_str.clone(),
            is_active: true,
            files_indexed: None,
            last_indexed: None,
        };

        let projects = vec![active_project.clone()];

        Ok(ProjectOutput {
            success: true,
            action: "status".to_string(),
            active_project: Some(active_project),
            projects: Some(projects),
            config: None,
            error: None,
            message: Some(format!("Active project: {} at {}", project_name, root_str)),
        })
    }

    /// Activate a project
    fn activate_project(&mut self, project_path: &str) -> Result<ProjectOutput> {
        let path = PathBuf::from(project_path);

        if !path.exists() {
            return Err(ProjectToolsError::NotFound(project_path.to_string()));
        }

        if !path.is_dir() {
            return Err(ProjectToolsError::InvalidOperation(
                "Project path must be a directory".to_string(),
            ));
        }

        let project_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        self.active_project = Some(project_path.to_string());

        info!("Activated project: {} at {}", project_name, project_path);

        let project_info = ProjectInfo {
            name: project_name.clone(),
            root: path.to_string_lossy().to_string(),
            is_active: true,
            files_indexed: None,
            last_indexed: None,
        };

        Ok(ProjectOutput {
            success: true,
            action: "activate".to_string(),
            active_project: Some(project_info),
            projects: None,
            config: None,
            error: None,
            message: Some(format!("Activated project: {}", project_name)),
        })
    }

    /// Get configuration
    fn get_config(&self) -> Result<ProjectOutput> {
        let config_entries = vec![
            ConfigEntry {
                key: "project_root".to_string(),
                value: serde_json::Value::String(
                    self.config.project_root.to_string_lossy().to_string(),
                ),
                is_default: false,
            },
            ConfigEntry {
                key: "symbols_db_path".to_string(),
                value: serde_json::Value::String(
                    self.config.symbols_db_path.to_string_lossy().to_string(),
                ),
                is_default: false,
            },
            ConfigEntry {
                key: "rlm_db_path".to_string(),
                value: serde_json::Value::String(
                    self.config.rlm_db_path.to_string_lossy().to_string(),
                ),
                is_default: false,
            },
            ConfigEntry {
                key: "semantic_db_path".to_string(),
                value: serde_json::Value::String(
                    self.config.semantic_db_path.to_string_lossy().to_string(),
                ),
                is_default: false,
            },
            ConfigEntry {
                key: "cache_size".to_string(),
                value: serde_json::Value::Number(self.config.cache_size.into()),
                is_default: self.config.cache_size == 10000,
            },
            ConfigEntry {
                key: "watcher_debounce_ms".to_string(),
                value: serde_json::Value::Number(self.config.watcher_debounce_ms.into()),
                is_default: self.config.watcher_debounce_ms == 100,
            },
            ConfigEntry {
                key: "max_file_size".to_string(),
                value: serde_json::Value::Number(self.config.max_file_size.into()),
                is_default: self.config.max_file_size == 5 * 1024 * 1024,
            },
            ConfigEntry {
                key: "debug".to_string(),
                value: serde_json::Value::Bool(self.config.debug),
                is_default: !self.config.debug,
            },
        ];

        Ok(ProjectOutput {
            success: true,
            action: "config".to_string(),
            active_project: None,
            projects: None,
            config: Some(config_entries),
            error: None,
            message: None,
        })
    }

    /// Handle touring_project tool
    pub fn handle(&mut self, input: ProjectInput) -> ProjectOutput {
        debug!("ProjectTools: {:?}", input.action);

        let result = match input.action {
            ProjectAction::Status => self.get_status(),
            ProjectAction::Activate => {
                if let Some(project) = input.project {
                    self.activate_project(&project)
                } else {
                    Err(ProjectToolsError::InvalidOperation(
                        "Activate requires project path".to_string(),
                    ))
                }
            }
            ProjectAction::Config => self.get_config(),
        };

        match result {
            Ok(output) => output,
            Err(e) => ProjectOutput {
                success: false,
                action: input.action.to_string(),
                active_project: None,
                projects: None,
                config: None,
                error: Some(e.to_string()),
                message: None,
            },
        }
    }

    /// Handle async version
    pub async fn handle_async(&mut self, input: ProjectInput) -> ProjectOutput {
        self.handle(input)
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
    fn test_get_status() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let tools = ProjectTools::new(&config);

        let output = tools.get_status().unwrap();
        assert!(output.success);
        assert!(output.active_project.is_some());
        assert!(output.projects.is_some());
        assert_eq!(output.projects.unwrap().len(), 1);
    }

    #[test]
    fn test_activate_project() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let mut tools = ProjectTools::new(&config);

        // Create a subdirectory to activate
        let subdir = temp_dir.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();

        let output = tools
            .activate_project(subdir.to_string_lossy().as_ref())
            .unwrap();
        assert!(output.success);
        assert!(output.active_project.is_some());
        assert_eq!(output.active_project.unwrap().name, "subdir".to_string());
    }

    #[test]
    fn test_activate_nonexistent_project() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let mut tools = ProjectTools::new(&config);

        let result = tools.activate_project("/nonexistent/path");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_config() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let tools = ProjectTools::new(&config);

        let output = tools.get_config().unwrap();
        assert!(output.success);
        assert!(output.config.is_some());

        let config = output.config.unwrap();
        assert!(!config.is_empty());

        // Check for expected keys
        let keys: Vec<_> = config.iter().map(|e| e.key.as_str()).collect();
        assert!(keys.contains(&"project_root"));
        assert!(keys.contains(&"cache_size"));
        assert!(keys.contains(&"debug"));
    }

    #[test]
    fn test_handle_status() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let mut tools = ProjectTools::new(&config);

        let input = ProjectInput {
            action: ProjectAction::Status,
            project: None,
            key: None,
            value: None,
        };

        let output = tools.handle(input);
        assert!(output.success);
        assert!(output.message.is_some());
    }

    #[test]
    fn test_handle_config() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let mut tools = ProjectTools::new(&config);

        let input = ProjectInput {
            action: ProjectAction::Config,
            project: None,
            key: None,
            value: None,
        };

        let output = tools.handle(input);
        assert!(output.success);
        assert!(output.config.is_some());
    }
}
