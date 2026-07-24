//! TOML theme loader for viz styling.
//!
//! Loads visual theme configuration from `~/.config/touring/viz-theme.toml`.
//! Falls back to sensible defaults if the file is absent or malformed.

use serde::Deserialize;
use std::path::PathBuf;
/// Top-level visual theme aggregating node, edge, and cluster styling rules.
#[derive(Debug, Clone, Deserialize)]
pub struct Theme {
    /// Node styling rules.
    pub node: NodeTheme,
    /// Edge styling rules.
    pub edge: EdgeTheme,
    /// Cluster/subgraph styling rules.
    pub cluster: ClusterTheme,
}

/// Node-specific styling rules.
#[derive(Debug, Clone, Deserialize)]
pub struct NodeTheme {
    /// Shape mapping (node_type -> shape).
    pub shape: std::collections::HashMap<String, String>,
    /// Fill color mapping (quality_bucket -> color).
    pub fill: std::collections::HashMap<String, String>,
    /// Border style mapping (property -> style).
    pub border: std::collections::HashMap<String, String>,
    /// Font sizing rules.
    pub size: NodeSizeTheme,
}

/// Font sizing rules for node labels.
#[derive(Debug, Clone, Deserialize)]
pub struct NodeSizeTheme {
    /// Base font size in points.
    pub base_font_size: u8,
    /// Logarithmic scale factor for fan-in/fan-out sizing.
    pub log_factor: f32,
    /// Minimum font size in points.
    pub min_size: u8,
    /// Maximum font size in points.
    pub max_size: u8,
}

/// Edge-specific styling rules.
#[derive(Debug, Clone, Deserialize)]
pub struct EdgeTheme {
    /// Color mapping (edge_kind -> color hex).
    pub color: std::collections::HashMap<String, String>,
    /// Style mapping (edge_kind -> DOT style).
    pub style: std::collections::HashMap<String, String>,
}

/// Cluster/subgraph styling rules.
#[derive(Debug, Clone, Deserialize)]
pub struct ClusterTheme {
    /// Fill color for workspace root clusters.
    pub workspace_root: String,
    /// Fill color for test directory clusters.
    pub test_dir: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            node: NodeTheme {
                shape: std::collections::HashMap::new(),
                fill: std::collections::HashMap::new(),
                border: std::collections::HashMap::new(),
                size: NodeSizeTheme {
                    base_font_size: 8,
                    log_factor: 1.5,
                    min_size: 8,
                    max_size: 18,
                },
            },
            edge: EdgeTheme {
                color: std::collections::HashMap::new(),
                style: std::collections::HashMap::new(),
            },
            cluster: ClusterTheme {
                workspace_root: "lightblue".to_string(),
                test_dir: "lightgrey".to_string(),
            },
        }
    }
}

impl Theme {
    /// Load theme from `~/.config/touring/viz-theme.toml`.
    ///
    /// Falls back to [`Theme::default`] if the file does not exist or
    /// cannot be parsed as valid TOML.
    pub fn load() -> Self {
        let config_path = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("touring")
            .join("viz-theme.toml");

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path).ok();
            if let Some(c) = content {
                if let Ok(theme) = toml::from_str::<Theme>(&c) {
                    return theme;
                }
            }
        }
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_default() {
        let theme = Theme::default();
        assert_eq!(theme.node.size.base_font_size, 8);
        assert_eq!(theme.node.size.min_size, 8);
        assert_eq!(theme.node.size.max_size, 18);
        assert_eq!(theme.cluster.workspace_root, "lightblue");
        assert_eq!(theme.cluster.test_dir, "lightgrey");
    }

    #[test]
    fn test_theme_load_nonexistent() {
        // Should return default when file doesn't exist
        let theme = Theme::load();
        assert_eq!(theme.node.size.base_font_size, 8);
    }
}
