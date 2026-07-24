//! Tool Catalog — Complete inventory of all Touring CLI commands.
//!
//! This catalog provides the N1 generator with awareness of all available tools
//! for sequence generation. There are TWO distinct invocation paths:
//!
//! 1. **Native tools** (Read, Edit, Write, Bash) — called directly by the runtime
//! 2. **CLI tools** (touring_*) — executed via `Bash` as `touring <subapp> <command> [args]`
//!
//! MCP tools are NOT used directly by BasicGenerator — CLI commands are the
//! primary interface for touring intelligence tools.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// TYPE DEFINITIONS (must appear before use in impl blocks)
// ============================================================================

/// How a tool is invoked at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvocationType {
    /// Native tools called directly by the runtime (Read, Edit, Write, Bash).
    Native,
    /// CLI tools executed via Bash as `touring <subapp> <command> [args]`.
    Cli,
}

/// Tool categories for organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    /// File operations: Read, Edit, Write, Bash
    FileOperations,
    /// Index and AST tools
    IndexAST,
    /// Memory/knowledge graph
    Memory,
    /// Session management
    Session,
    /// DAG decomposition
    Decompose,
    /// Cognitive/reasoning (MCTS, suggestions)
    Cognitive,
    /// Wiring intelligence
    Wiring,
    /// Evolution/drift detection
    Evolution,
    /// Reinforcement learning
    Learning,
    /// Flywheel/component health
    Flywheel,
    /// Gotcha/pitfall database
    Gotcha,
    /// Incremental/parser cache
    Incremental,
    /// Shadow/speculative validation
    Shadow,
}

impl std::fmt::Display for ToolCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileOperations => write!(f, "file_operations"),
            Self::IndexAST => write!(f, "index_ast"),
            Self::Memory => write!(f, "memory"),
            Self::Session => write!(f, "session"),
            Self::Decompose => write!(f, "decompose"),
            Self::Cognitive => write!(f, "cognitive"),
            Self::Wiring => write!(f, "wiring"),
            Self::Evolution => write!(f, "evolution"),
            Self::Learning => write!(f, "learning"),
            Self::Flywheel => write!(f, "flywheel"),
            Self::Gotcha => write!(f, "gotcha"),
            Self::Incremental => write!(f, "incremental"),
            Self::Shadow => write!(f, "shadow"),
        }
    }
}

/// A tool argument specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolArg {
    /// Argument name.
    pub name: String,
    /// Human-readable description of the argument.
    pub description: String,
    /// Whether the argument is required.
    pub required: bool,
    /// Argument type name (e.g. `"string"`, `"bool"`).
    pub arg_type: String,
}

/// Description of a single tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    /// Tool name (e.g., "Read", "touring_index_find").
    pub name: String,
    /// How this tool is invoked at runtime.
    pub invocation: InvocationType,
    /// Category of the tool.
    pub category: ToolCategory,
    /// Human-readable description.
    pub description: String,
    /// Tool arguments.
    pub arguments: Vec<ToolArg>,
    /// Whether this tool can be parallelized with others.
    pub parallelizable: bool,
    /// Whether this tool supports rollback.
    pub has_rollback: bool,
}

// ============================================================================
// CLI TOOL DATA (static tables for lookup)
// ============================================================================

const CLI_CATEGORIES: &[(&str, ToolCategory)] = &[
    ("touring index", ToolCategory::IndexAST),
    ("touring ast", ToolCategory::IndexAST),
    ("touring memory", ToolCategory::Memory),
    ("touring session", ToolCategory::Session),
    ("touring decompose", ToolCategory::Decompose),
    ("touring cognitive", ToolCategory::Cognitive),
    ("touring suggest", ToolCategory::Cognitive),
    ("touring mcts", ToolCategory::Cognitive),
    ("touring classify", ToolCategory::Cognitive),
    ("touring scan-pii", ToolCategory::Cognitive),
    ("touring wiring", ToolCategory::Wiring),
    ("touring evolution", ToolCategory::Evolution),
    ("touring learning", ToolCategory::Learning),
    ("touring flywheel", ToolCategory::Flywheel),
    ("touring gotcha", ToolCategory::Gotcha),
    ("touring incremental", ToolCategory::Incremental),
    ("touring shadow", ToolCategory::Shadow),
];

const CLI_DESCRIPTIONS: &[(&str, &str)] = &[
    (
        "touring index find",
        "Find symbol definitions in the knowledge base",
    ),
    ("touring index search", "Search index with prefix matching"),
    ("touring index status", "Check health of symbol index"),
    ("touring ast overview", "Get overview of symbols in a file"),
    (
        "touring ast blast",
        "Analyze blast radius of changes to a file",
    ),
    (
        "touring memory recall",
        "Recall knowledge from persistent memory",
    ),
    (
        "touring memory store",
        "Store knowledge in persistent memory",
    ),
    ("touring memory stats", "Get memory statistics"),
    ("touring session start", "Start a new Touring session"),
    ("touring session assess", "Assess session quality"),
    ("touring session checkpoint", "Create session checkpoint"),
    ("touring decompose create", "Create a new task DAG"),
    ("touring decompose add", "Add subtask to DAG"),
    ("touring decompose validate", "Validate DAG for cycles"),
    ("touring cognitive metrics", "Get cognitive engine metrics"),
    (
        "touring suggest next",
        "Get RL-guided next action suggestion",
    ),
    (
        "touring mcts search",
        "Run Monte Carlo Tree Search for multi-path decisions",
    ),
    (
        "touring classify-intent",
        "Classify user intent (CILA routing)",
    ),
    (
        "touring scan-pii",
        "Scan for Personally Identifiable Information",
    ),
    ("touring wiring status", "Get wiring integration summary"),
    (
        "touring wiring orphans",
        "Find orphan pub symbols without consumers",
    ),
    ("touring wiring modules", "Get integration scores by module"),
    ("touring evolution insights", "Get evolution insights"),
    ("touring evolution drift", "Detect drift in tracked metrics"),
    ("touring evolution tools", "Get tool effectiveness metrics"),
    (
        "touring evolution blast",
        "Analyze blast radius for evolution",
    ),
    ("touring learning status", "Get RL learning engine status"),
    (
        "touring learning reward",
        "Inject reward signal for online learning",
    ),
    ("touring flywheel status", "Get component health status"),
    ("touring gotcha list", "List known pitfalls and gotchas"),
    ("touring incremental status", "Get parser cache status"),
    (
        "touring shadow validate",
        "Speculatively validate changes in shadow branch",
    ),
];

fn categorize_cli_tool(cli_name: &str) -> ToolCategory {
    for (prefix, cat) in CLI_CATEGORIES {
        if cli_name.starts_with(prefix) {
            return *cat;
        }
    }
    ToolCategory::Cognitive
}

fn description_for_cli(cli_name: &str) -> String {
    CLI_DESCRIPTIONS
        .iter()
        .find(|(name, _)| *name == cli_name)
        .map(|(_, desc)| (*desc).into())
        .unwrap_or_else(|| format!("Touring CLI tool: {}", cli_name))
}

// ============================================================================
// TOOL CATALOG
// ============================================================================

/// A catalog of all available tools in the Touring ecosystem.
#[derive(Debug, Clone)]
pub struct ToolCatalog {
    tools: HashMap<String, ToolDescriptor>,
}

impl ToolCatalog {
    /// Create a new catalog with all Touring tools pre-registered.
    pub fn new() -> Self {
        let mut tools = HashMap::new();

        // =================================================================
        // NATIVE TOOLS (called directly by the runtime)
        // =================================================================
        tools.insert(
            "Read".into(),
            ToolDescriptor {
                name: "Read".into(),
                invocation: InvocationType::Native,
                category: ToolCategory::FileOperations,
                description: "Read file contents".into(),
                arguments: vec![ToolArg {
                    name: "file_path".into(),
                    description: "Path to file".into(),
                    required: true,
                    arg_type: String::from("string"),
                }],
                parallelizable: true,
                has_rollback: false,
            },
        );

        tools.insert(
            "Edit".into(),
            ToolDescriptor {
                name: "Edit".into(),
                invocation: InvocationType::Native,
                category: ToolCategory::FileOperations,
                description: "Edit file contents".into(),
                arguments: vec![
                    ToolArg {
                        name: "file_path".into(),
                        description: "Path to file".into(),
                        required: true,
                        arg_type: String::from("string"),
                    },
                    ToolArg {
                        name: "old_string".into(),
                        description: "String to replace".into(),
                        required: true,
                        arg_type: String::from("string"),
                    },
                    ToolArg {
                        name: "new_string".into(),
                        description: "Replacement string".into(),
                        required: true,
                        arg_type: String::from("string"),
                    },
                ],
                parallelizable: false,
                has_rollback: true,
            },
        );

        tools.insert(
            "Write".into(),
            ToolDescriptor {
                name: "Write".into(),
                invocation: InvocationType::Native,
                category: ToolCategory::FileOperations,
                description: "Write new file or overwrite existing".into(),
                arguments: vec![
                    ToolArg {
                        name: "file_path".into(),
                        description: "Path to file".into(),
                        required: true,
                        arg_type: String::from("string"),
                    },
                    ToolArg {
                        name: "content".into(),
                        description: "File content".into(),
                        required: true,
                        arg_type: String::from("string"),
                    },
                ],
                parallelizable: false,
                has_rollback: true,
            },
        );

        tools.insert(
            "Bash".into(),
            ToolDescriptor {
                name: "Bash".into(),
                invocation: InvocationType::Native,
                category: ToolCategory::FileOperations,
                description: "Execute shell command".into(),
                arguments: vec![ToolArg {
                    name: "command".into(),
                    description: "Shell command".into(),
                    required: true,
                    arg_type: String::from("string"),
                }],
                parallelizable: true,
                has_rollback: false,
            },
        );

        // =================================================================
        // CLI TOOLS (executed via Bash as `touring <subapp> <command> [args]`)
        // =================================================================

        // -- INDEX / AST --
        tools.insert(
            "touring_index_find".into(),
            make_cli_tool(
                "touring index find",
                vec![ToolArg {
                    name: "symbol".into(),
                    description: "Symbol name to find".into(),
                    required: true,
                    arg_type: String::from("string"),
                }],
            ),
        );

        tools.insert(
            "touring_index_search".into(),
            make_cli_tool(
                "touring index search",
                vec![ToolArg {
                    name: "query".into(),
                    description: "Search query".into(),
                    required: true,
                    arg_type: String::from("string"),
                }],
            ),
        );

        tools.insert(
            "touring_index_status".into(),
            make_cli_tool("touring index status", vec![]),
        );

        tools.insert(
            "touring_ast_overview".into(),
            make_cli_tool(
                "touring ast overview",
                vec![ToolArg {
                    name: "file_path".into(),
                    description: "Path to file".into(),
                    required: true,
                    arg_type: String::from("string"),
                }],
            ),
        );

        tools.insert(
            "touring_ast_blast".into(),
            make_cli_tool(
                "touring ast blast",
                vec![ToolArg {
                    name: "file_path".into(),
                    description: "Path to file".into(),
                    required: true,
                    arg_type: String::from("string"),
                }],
            ),
        );

        // -- MEMORY --
        tools.insert(
            "touring_memory_recall".into(),
            make_cli_tool(
                "touring memory recall",
                vec![
                    ToolArg {
                        name: "query".into(),
                        description: "Query string".into(),
                        required: true,
                        arg_type: String::from("string"),
                    },
                    ToolArg {
                        name: "top_k".into(),
                        description: "Number of results".into(),
                        required: false,
                        arg_type: String::from("number"),
                    },
                ],
            ),
        );

        tools.insert(
            "touring_memory_store".into(),
            make_cli_tool(
                "touring memory store",
                vec![
                    ToolArg {
                        name: "key".into(),
                        description: "Memory key".into(),
                        required: true,
                        arg_type: String::from("string"),
                    },
                    ToolArg {
                        name: "entry_type".into(),
                        description: "Type: lesson|pattern|insight|gotcha".into(),
                        required: false,
                        arg_type: String::from("string"),
                    },
                ],
            ),
        );

        tools.insert(
            "touring_memory_stats".into(),
            make_cli_tool("touring memory stats", vec![]),
        );

        // -- SESSION --
        tools.insert(
            "touring_session_start".into(),
            make_cli_tool(
                "touring session start",
                vec![
                    ToolArg {
                        name: "id".into(),
                        description: "Session ID".into(),
                        required: false,
                        arg_type: String::from("string"),
                    },
                    ToolArg {
                        name: "type".into(),
                        description: "Session type".into(),
                        required: false,
                        arg_type: String::from("string"),
                    },
                    ToolArg {
                        name: "objective".into(),
                        description: "Session objective".into(),
                        required: false,
                        arg_type: String::from("string"),
                    },
                ],
            ),
        );

        tools.insert(
            "touring_session_assess".into(),
            make_cli_tool(
                "touring session assess",
                vec![ToolArg {
                    name: "session_id".into(),
                    description: "Session ID".into(),
                    required: true,
                    arg_type: String::from("string"),
                }],
            ),
        );

        tools.insert(
            "touring_session_checkpoint".into(),
            make_cli_tool(
                "touring session checkpoint",
                vec![
                    ToolArg {
                        name: "checkpoint_id".into(),
                        description: "Checkpoint ID".into(),
                        required: true,
                        arg_type: String::from("string"),
                    },
                    ToolArg {
                        name: "data".into(),
                        description: "Checkpoint data".into(),
                        required: false,
                        arg_type: String::from("string"),
                    },
                ],
            ),
        );

        // -- DECOMPOSE --
        tools.insert(
            "touring_decompose_create".into(),
            make_cli_tool(
                "touring decompose create",
                vec![
                    ToolArg {
                        name: "task_type".into(),
                        description: "Task type".into(),
                        required: true,
                        arg_type: String::from("string"),
                    },
                    ToolArg {
                        name: "description".into(),
                        description: "Task description".into(),
                        required: false,
                        arg_type: String::from("string"),
                    },
                ],
            ),
        );

        tools.insert(
            "touring_decompose_add".into(),
            make_cli_tool(
                "touring decompose add",
                vec![
                    ToolArg {
                        name: "task_id".into(),
                        description: "Parent task ID".into(),
                        required: true,
                        arg_type: String::from("string"),
                    },
                    ToolArg {
                        name: "subtask_id".into(),
                        description: "Subtask ID".into(),
                        required: true,
                        arg_type: String::from("string"),
                    },
                    ToolArg {
                        name: "description".into(),
                        description: "Subtask description".into(),
                        required: false,
                        arg_type: String::from("string"),
                    },
                    ToolArg {
                        name: "depends_on".into(),
                        description: "Dependencies".into(),
                        required: false,
                        arg_type: String::from("array"),
                    },
                ],
            ),
        );

        tools.insert(
            "touring_decompose_validate".into(),
            make_cli_tool(
                "touring decompose validate",
                vec![ToolArg {
                    name: "task_id".into(),
                    description: "Task ID to validate".into(),
                    required: true,
                    arg_type: String::from("string"),
                }],
            ),
        );

        // -- COGNITIVE --
        tools.insert(
            "touring_cognitive_metrics".into(),
            make_cli_tool("touring cognitive metrics", vec![]),
        );

        tools.insert(
            "touring_suggest_next".into(),
            make_cli_tool(
                "touring suggest next",
                vec![ToolArg {
                    name: "query".into(),
                    description: "Query for suggestion".into(),
                    required: false,
                    arg_type: String::from("string"),
                }],
            ),
        );

        tools.insert(
            "touring_mcts_search".into(),
            make_cli_tool(
                "touring mcts search",
                vec![
                    ToolArg {
                        name: "root_state".into(),
                        description: "Root state for MCTS".into(),
                        required: false,
                        arg_type: String::from("string"),
                    },
                    ToolArg {
                        name: "num_rollouts".into(),
                        description: "Number of rollouts".into(),
                        required: false,
                        arg_type: String::from("number"),
                    },
                    ToolArg {
                        name: "max_depth".into(),
                        description: "Maximum depth".into(),
                        required: false,
                        arg_type: String::from("number"),
                    },
                ],
            ),
        );

        tools.insert(
            "touring_classify_intent".into(),
            make_cli_tool(
                "touring classify-intent",
                vec![ToolArg {
                    name: "text".into(),
                    description: "Text to classify".into(),
                    required: true,
                    arg_type: String::from("string"),
                }],
            ),
        );

        tools.insert(
            "touring_scan_pii".into(),
            make_cli_tool(
                "touring scan-pii",
                vec![ToolArg {
                    name: "content".into(),
                    description: "Content to scan".into(),
                    required: true,
                    arg_type: String::from("string"),
                }],
            ),
        );

        // -- WIRING --
        tools.insert(
            "touring_wiring_status".into(),
            make_cli_tool("touring wiring status", vec![]),
        );

        tools.insert(
            "touring_wiring_orphans".into(),
            make_cli_tool("touring wiring orphans", vec![]),
        );

        tools.insert(
            "touring_wiring_modules".into(),
            make_cli_tool("touring wiring modules", vec![]),
        );

        // -- EVOLUTION --
        tools.insert(
            "touring_evolution_insights".into(),
            make_cli_tool("touring evolution insights", vec![]),
        );

        tools.insert(
            "touring_evolution_drift".into(),
            make_cli_tool("touring evolution drift", vec![]),
        );

        tools.insert(
            "touring_evolution_tools".into(),
            make_cli_tool("touring evolution tools", vec![]),
        );

        tools.insert(
            "touring_evolution_blast".into(),
            make_cli_tool(
                "touring evolution blast",
                vec![ToolArg {
                    name: "file".into(),
                    description: "File to analyze".into(),
                    required: true,
                    arg_type: String::from("string"),
                }],
            ),
        );

        // -- LEARNING --
        tools.insert(
            "touring_learning_status".into(),
            make_cli_tool("touring learning status", vec![]),
        );

        tools.insert(
            "touring_online_learn".into(),
            make_cli_tool(
                "touring learning reward",
                vec![
                    ToolArg {
                        name: "action".into(),
                        description: "Learning action".into(),
                        required: true,
                        arg_type: String::from("string"),
                    },
                    ToolArg {
                        name: "reward_type".into(),
                        description: "Reward type".into(),
                        required: false,
                        arg_type: String::from("string"),
                    },
                ],
            ),
        );

        // -- OTHER --
        tools.insert(
            "touring_flywheel_status".into(),
            make_cli_tool("touring flywheel status", vec![]),
        );

        tools.insert(
            "touring_gotcha_list".into(),
            make_cli_tool("touring gotcha list", vec![]),
        );

        tools.insert(
            "touring_incremental_status".into(),
            make_cli_tool("touring incremental status", vec![]),
        );

        tools.insert(
            "touring_shadow_validate".into(),
            make_cli_tool("touring shadow validate", vec![]),
        );

        Self { tools }
    }

    /// Get a tool by name.
    pub fn get(&self, name: &str) -> Option<&ToolDescriptor> {
        self.tools.get(name)
    }

    /// Get all tools in a category.
    pub fn by_category(&self, category: ToolCategory) -> Vec<&ToolDescriptor> {
        self.tools
            .values()
            .filter(|t| t.category == category)
            .collect()
    }

    /// Get all tool names.
    pub fn tool_names(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// Get tools matching a keyword in description.
    pub fn matching(&self, keyword: &str) -> Vec<&ToolDescriptor> {
        let kw_lower = keyword.to_lowercase();
        self.tools
            .values()
            .filter(|t| {
                t.name.to_lowercase().contains(&kw_lower)
                    || t.description.to_lowercase().contains(&kw_lower)
            })
            .collect()
    }

    /// Total tool count.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolCatalog {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to create a CLI tool descriptor.
fn make_cli_tool(cli_name: &str, arguments: Vec<ToolArg>) -> ToolDescriptor {
    ToolDescriptor {
        name: cli_name.into(),
        invocation: InvocationType::Cli,
        category: categorize_cli_tool(cli_name),
        description: description_for_cli(cli_name),
        arguments,
        parallelizable: true,
        has_rollback: false,
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_creation() {
        let catalog = ToolCatalog::new();
        assert!(catalog.len() > 0);
    }

    #[test]
    fn test_get_tool() {
        let catalog = ToolCatalog::new();
        let read = catalog.get("Read").expect("Read should exist");
        assert_eq!(read.category, ToolCategory::FileOperations);
        assert_eq!(read.invocation, InvocationType::Native);
    }

    #[test]
    fn test_native_tools() {
        let catalog = ToolCatalog::new();
        for name in ["Read", "Edit", "Write", "Bash"] {
            let tool = catalog.get(name).expect(&format!("{} should exist", name));
            assert_eq!(tool.invocation, InvocationType::Native);
        }
    }

    #[test]
    fn test_cli_tools_are_cli() {
        let catalog = ToolCatalog::new();
        for name in [
            "touring_index_find",
            "touring_memory_recall",
            "touring_wiring_status",
        ] {
            let tool = catalog.get(name).expect(&format!("{} should exist", name));
            assert_eq!(tool.invocation, InvocationType::Cli);
        }
    }

    #[test]
    fn test_by_category() {
        let catalog = ToolCatalog::new();
        let cognitive_tools = catalog.by_category(ToolCategory::Cognitive);
        assert!(!cognitive_tools.is_empty());
    }

    #[test]
    fn test_matching() {
        let catalog = ToolCatalog::new();
        let memory_tools = catalog.matching("memory");
        assert!(!memory_tools.is_empty());
    }

    #[test]
    fn test_all_tools_have_required_fields() {
        let catalog = ToolCatalog::new();
        for (name, tool) in catalog.tools.iter() {
            assert!(!name.is_empty(), "Tool name should not be empty");
            assert!(
                !tool.description.is_empty(),
                "Tool {} should have description",
                name
            );
        }
    }

    #[test]
    fn test_touring_tools_available() {
        let catalog = ToolCatalog::new();
        assert!(catalog.get("touring_index_find").is_some());
        assert!(catalog.get("touring_memory_recall").is_some());
        assert!(catalog.get("touring_wiring_status").is_some());
        assert!(catalog.get("touring_cognitive_metrics").is_some());
        assert!(catalog.get("touring_mcts_search").is_some());
    }
}
