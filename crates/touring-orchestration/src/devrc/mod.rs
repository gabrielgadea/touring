//! Devrcfile adapter — parses Devrcfile YAML and converts to Touring Tasksfile.
//!
//! ## Conversion map
//!
//! | Devrcfile field | Touring Tasksfile field |
//! |-----------------|--------------------------|
//! | `devrc_config.shell` | `metadata.shell` |
//! | `devrc_config.log_level` | `metadata.log_level` |
//! | `devrc_config.cache_ttl` | `metadata.cache_ttl` |
//! | `variables` | `params` (global) |
//! | `env_file` | `includes[].file` (path_resolve: relative) |
//! | `environment` | task-level `env` |
//! | `before_script` | `hooks.before_all` |
//! | `after_script` | `hooks.after_all` |
//! | `before_task` | per-task `hooks.before` |
//! | `after_task` | per-task `hooks.after` |
//! | `include[].file` | `includes[].file` |
//! | `include[].url` | `includes[].url` |
//! | `tasks.*.desc` | `tasks.*.desc` |
//! | `tasks.*.params` | `tasks.*.params` |
//! | `tasks.*.deps` | `tasks.*.deps` |
//! | `tasks.*.environment` | `tasks.*.env` |
//! | `tasks.*.exec` | `tasks.*.command` |
//! | `tasks.*.timeout` | `tasks.*.timeout` |
//! | `tasks.*.tags` | `tasks.*.tags` |
//! | `tasks.*.retry_policy` | `tasks.*.retry_policy` |
//! | `tasks.*.review_required` | `tasks.*.review_required` |

pub mod converter;
pub mod parser;

pub use converter::{ConversionResult, devrcfile_to_tasksfile};
pub use parser::{DevrcfileRoot, parse_devrcfile};
