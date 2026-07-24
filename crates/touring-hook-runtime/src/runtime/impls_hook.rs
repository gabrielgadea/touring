//! HookExecutor implementation for HookRuntime.

use std::path::PathBuf;

use super::traits::HookExecutor;
use crate::HookResponse;
use crate::runtime::HookRuntime;

impl HookExecutor for HookRuntime {
    fn build_allow(&self) -> HookResponse {
        HookResponse::Allow
    }

    fn build_context(&self, context: &str) -> HookResponse {
        HookResponse::Context {
            context: context.to_string(),
            event_name: None,
        }
    }

    fn build_context_for_event(&self, context: &str, event_name: &str) -> HookResponse {
        HookResponse::Context {
            context: context.to_string(),
            event_name: Some(event_name.to_string()),
        }
    }

    fn detect_project_root(&self) -> PathBuf {
        std::env::var("CLAUDE_PROJECT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
}
