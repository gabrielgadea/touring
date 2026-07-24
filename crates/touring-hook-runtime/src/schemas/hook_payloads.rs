//! Hook payload schemas with validator derive.
//!
//! Feature D (2026-04-24) — Schema Validation Layer
//!
//! Provides type-safe payload structs for all Touring hook handlers.
//! Validated at dispatch time to catch malformed inputs early.

use serde::{Deserialize, Serialize};
use validator::Validate;

/// Selection range in editor.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Selection {
    /// Zero-based start offset of the selection.
    pub start: u32,
    /// Zero-based end offset of the selection (exclusive).
    pub end: u32,
}

/// Payload for the `pre_edit` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PreEditPayload {
    /// Path of the file about to be edited.
    #[validate(length(min = 1, message = "file_path cannot be empty"))]
    pub file_path: String,

    /// Text being replaced, if known.
    #[validate(length(max = 1_000_000, message = "old_string exceeds size limit (1MB)"))]
    pub old_string: Option<String>,

    /// Replacement text, if known.
    #[validate(length(max = 1_000_000, message = "new_string exceeds size limit (1MB)"))]
    pub new_string: Option<String>,

    /// Cursor offset at the time of the edit.
    pub cursor_position: Option<u32>,
    /// Active editor selection range, if any.
    pub selection: Option<Selection>,
}

/// Payload for the `pre_read` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PreReadPayload {
    /// Path of the file about to be read.
    #[validate(length(min = 1, message = "file_path cannot be empty"))]
    pub file_path: String,

    /// Byte offset at which to start reading.
    pub offset: Option<u64>,
    /// Maximum number of bytes (or lines) to read.
    pub limit: Option<u64>,
}

/// Payload for the `pre_write` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PreWritePayload {
    /// Path of the file about to be written.
    #[validate(length(min = 1, message = "file_path cannot be empty"))]
    pub file_path: String,

    /// Content that will be written, if available.
    pub content: Option<String>,
}

/// Payload for the `post_edit` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PostEditPayload {
    /// Path of the file that was edited.
    #[validate(length(min = 1, message = "file_path cannot be empty"))]
    pub file_path: String,

    /// Text that was replaced.
    #[validate(length(min = 1, message = "old_string cannot be empty"))]
    pub old_string: String,

    /// Text that replaced it.
    #[validate(length(min = 1, message = "new_string cannot be empty"))]
    pub new_string: String,

    /// Cursor offset after the edit.
    pub cursor_position: Option<u32>,
}

/// Payload for the `post_read` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PostReadPayload {
    /// Path of the file that was read.
    #[validate(length(min = 1, message = "file_path cannot be empty"))]
    pub file_path: String,

    /// Number of bytes read.
    pub bytes_read: Option<u64>,
    /// Wall-clock duration of the read in milliseconds.
    pub duration_ms: Option<u64>,
}

/// Payload for the `post_write` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PostWritePayload {
    /// Path of the file that was written.
    #[validate(length(min = 1, message = "file_path cannot be empty"))]
    pub file_path: String,

    /// Number of bytes written.
    pub bytes_written: Option<u64>,
    /// Wall-clock duration of the write in milliseconds.
    pub duration_ms: Option<u64>,
}

/// Payload for the `pre_bash` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PreBashPayload {
    /// Shell command about to be executed.
    #[validate(length(min = 1, message = "command cannot be empty"))]
    pub command: String,

    /// Working directory for the command, if specified.
    pub cwd: Option<String>,
    /// Execution deadline in milliseconds, if a timeout applies.
    pub deadline_ms: Option<u64>,
}

/// Payload for the `post_bash` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PostBashPayload {
    /// Shell command that was executed.
    #[validate(length(min = 1, message = "command cannot be empty"))]
    pub command: String,

    /// Process exit code.
    pub exit_code: Option<i32>,
    /// Captured standard output.
    pub stdout: Option<String>,
    /// Captured standard error.
    pub stderr: Option<String>,
    /// Wall-clock duration of the command in milliseconds.
    pub duration_ms: Option<u64>,
}

/// Payload for the `post_tool_failure` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PostToolFailurePayload {
    /// Name of the tool that failed.
    pub tool_name: String,

    /// Error message describing the failure.
    #[validate(length(min = 1, message = "error cannot be empty"))]
    pub error: String,

    /// Original tool payload as a JSON string, for diagnostics.
    pub payload_json: Option<String>,
}

/// Payload for the `session_start` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SessionStartPayload {
    /// Identifier of the starting session.
    pub session_id: Option<String>,

    /// Category of the session (e.g. feature, bugfix).
    #[validate(length(min = 1, max = 100, message = "session_type must be 1-100 chars"))]
    pub session_type: Option<String>,

    /// Free-form objective for the session.
    pub objective: Option<String>,
}

/// Payload for the `session_stop` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SessionStopPayload {
    /// Identifier of the ending session.
    pub session_id: Option<String>,
    /// Summary outcome of the session.
    pub outcome: Option<String>,
    /// Final quality score for the session, in `[0.0, 1.0]`.
    pub quality_score: Option<f64>,
}

/// Payload for the `pre_task_scout` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PreTaskScoutPayload {
    /// Identifier of the task being scouted.
    pub task_id: Option<String>,
    /// Short subject line of the task.
    pub task_subject: Option<String>,
    /// Longer description of the task.
    pub task_description: Option<String>,
    /// Full prompt text for the task.
    pub task_prompt: Option<String>,
}

/// Payload for the `task_created` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TaskCreatedPayload {
    /// Identifier of the newly created task.
    #[validate(length(min = 1, message = "task_id cannot be empty"))]
    pub task_id: String,

    /// Optional — real `task_created` hook payloads send `task_subject` instead.
    pub description: Option<String>,

    /// Category of the task.
    pub task_type: Option<String>,
    /// CILA routing level assigned to the task.
    pub cila_level: Option<u8>,
}

/// Payload for the `task_completed` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TaskCompletedPayload {
    /// Identifier of the completed task.
    #[validate(length(min = 1, message = "task_id cannot be empty"))]
    pub task_id: String,

    /// Quality score for the completed task, in `[0.0, 1.0]`.
    pub quality_score: Option<f64>,
    /// Wall-clock duration of the task in milliseconds.
    pub duration_ms: Option<u64>,
}

/// Payload for the `post_tool_rl` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PostToolRlPayload {
    /// Name of the tool whose outcome feeds the RL reward.
    pub tool_name: Option<String>,
    /// Reward value for the tool invocation, in `[-1.0, 1.0]`.
    pub reward: Option<f64>,
    /// RL context as a JSON string.
    pub context_json: Option<String>,
    /// Optional quality score associated with the outcome.
    pub quality_score: Option<f64>,
}

/// Payload for the `decompose_event` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DecomposeEventPayload {
    /// Kind of decomposition event (e.g. create, add, finalize).
    #[validate(length(min = 1, message = "event_type cannot be empty"))]
    pub event_type: String,

    /// Identifier of the parent task in the DAG.
    #[validate(length(min = 1, message = "task_id cannot be empty"))]
    pub task_id: String,

    /// Identifier of the affected subtask, if any.
    pub subtask_id: Option<String>,
    /// New status of the task or subtask.
    pub status: Option<String>,
    /// Quality score reported with the event, in `[0.0, 1.0]`.
    pub quality_score: Option<f64>,
}

/// Payload for the `cortex` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CortexPayload {
    /// Name of the cortex event being dispatched.
    #[validate(length(min = 1, message = "event cannot be empty"))]
    pub event: String,

    /// Event-specific payload as a JSON string.
    pub payload_json: Option<String>,
}

/// Payload for the `instructions_loaded` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct InstructionsLoadedPayload {
    /// Root directory of the project whose instructions loaded.
    pub project_root: Option<String>,
    /// Content hash of the loaded instructions.
    pub instructions_hash: Option<String>,
    /// Number of hooks registered for the project.
    pub hook_count: Option<u32>,
}

/// Payload for the `post_compact` hook.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PostCompactPayload {
    /// Number of bytes reclaimed by the compaction.
    pub compacted_bytes: Option<u64>,
    /// Wall-clock duration of the compaction in milliseconds.
    pub duration_ms: Option<u64>,
    /// Number of files accessed during compaction.
    pub files_accessed: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_edit_payload_valid() {
        let p = PreEditPayload {
            file_path: "src/main.rs".to_string(),
            old_string: Some("fn foo()".to_string()),
            new_string: Some("fn bar()".to_string()),
            cursor_position: Some(3),
            selection: None,
        };
        assert!(p.validate().is_ok());
    }

    #[test]
    fn pre_edit_payload_empty_file_path_fails() {
        let p = PreEditPayload {
            file_path: "".to_string(),
            old_string: None,
            new_string: None,
            cursor_position: None,
            selection: None,
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn pre_read_payload_valid() {
        let p = PreReadPayload {
            file_path: "src/lib.rs".to_string(),
            offset: Some(10),
            limit: Some(100),
        };
        assert!(p.validate().is_ok());
    }

    #[test]
    fn post_tool_failure_valid() {
        let p = PostToolFailurePayload {
            tool_name: "Read".to_string(),
            error: "File not found".to_string(),
            payload_json: None,
        };
        assert!(p.validate().is_ok());
    }

    #[test]
    fn task_created_valid() {
        let p = TaskCreatedPayload {
            task_id: "task_123".to_string(),
            description: Some("Implement feature X".to_string()),
            task_type: Some("intent".to_string()),
            cila_level: Some(3),
        };
        assert!(p.validate().is_ok());
    }

    #[test]
    fn selection_valid() {
        let s = Selection { start: 0, end: 10 };
        assert!(s.validate().is_ok());
    }
}
