//! Touring Tasksfile Parser — converts Tasksfile YAML into decompose DAGs.
//!
//! ## Tasksfile Format
//!
//! ```yaml
//! version: "1.0"
//! metadata:
//!   name: myproject
//!   description: My project tasks
//! templates:
//!   ci_job:
//!     timeout: 300s
//!     tags: [ci]
//!     retry_policy:
//!       max_attempts: 2
//!       backoff_ms: 1000
//! tasks:
//!   build:
//!     desc: "Build with {{ profile }} profile"
//!     command: cargo build --{{ profile }}
//!     params:
//!       profile:
//!         default: release
//!         options: [debug, release]
//!     deps: []
//!     tags: [ci, fast]
//! includes:
//!   - file: ./shared-tasks.yml
//! hooks:
//!   before_all:
//!     - echo "Starting..."
//! ```

pub mod compiler;
pub mod env_file;
pub mod error;
pub mod include;
pub mod parser;
pub mod schema;
#[cfg(feature = "templates")]
pub mod template_engine;

pub use compiler::{CompiledTask, CompiledTasksfile, TasksfileCompiler};
pub use error::{Result, TasksfileError};
pub use include::{IncludeSource, ResolvedInclude, resolve_includes};
pub use parser::{load_file, parse_yaml, validate_deps};
pub use schema::{
    GlobalHooks, IncludeSpec, Metadata, NetrcAuth, ParamDefinition, RetryPolicyDef, TaskDefinition,
    TaskHooks, TasksfileRoot, TemplateDefinition,
};
