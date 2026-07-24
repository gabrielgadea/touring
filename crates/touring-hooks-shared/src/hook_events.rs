//! Hook event types for RL reward signal injection.
//!
//! Centralizes all hook lifecycle events used by the touring-learning crate.

use chrono::{DateTime, Utc};

/// Priority tier for hook events — controls what survives context compaction.
///
/// CRITICAL/HIGH events are preserved through PreCompact;
/// MEDIUM/LOW events are dropped to save tokens.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventPriority {
    /// Must survive — session errors, security violations, RL reward signals.
    /// Preserved 100% through PreCompact.
    Critical,
    /// High value — tool outcomes, quality signals, plan hints.
    /// Preserved through PreCompact.
    High,
    /// Medium value — symbol enrichment, context signals.
    /// Dropped in PreCompact.
    Medium,
    /// Low value — informational, low-signal events.
    /// Dropped in PreCompact.
    Low,
}

impl EventPriority {
    /// Returns true if this priority tier survives PreCompact filtering.
    pub fn survives_compaction(self) -> bool {
        matches!(self, EventPriority::Critical | EventPriority::High)
    }

    /// SCREAMING_SNAKE label used as the SQL `hook_events.priority_tier` value.
    pub fn as_label(self) -> &'static str {
        match self {
            EventPriority::Critical => "CRITICAL",
            EventPriority::High => "HIGH",
            EventPriority::Medium => "MEDIUM",
            EventPriority::Low => "LOW",
        }
    }

    /// Inverse of [`Self::as_label`]: parse a stored label back. Unknown
    /// values fall back to [`EventPriority::Medium`] for forward-compat.
    pub fn from_label(label: &str) -> Self {
        match label {
            "CRITICAL" => EventPriority::Critical,
            "HIGH" => EventPriority::High,
            "LOW" => EventPriority::Low,
            _ => EventPriority::Medium,
        }
    }
}

/// Classify a priority tier directly from the hook name (snake_case string)
/// stored in `hook_events.hook_name`. Mirrors the [`HookEvent`] enum mapping
/// in [`classify_event_priority`] but works on the SQL-row representation.
///
/// Used by `SqliteHookMemoryBridge::upsert_event` (D3.4) to populate the
/// `priority_tier` column without requiring callers to construct a HookEvent
/// enum value.
///
/// I-13 — Extended taxonomy (26 events, 5-tier prioritisation matching
/// context-mode):
///
/// | Tier | Categories | Rationale |
/// |---|---|---|
/// | CRITICAL (P1) | session_*, error, decision, rejected_approach | survives 100% PreCompact |
/// | HIGH (P2) | post_edit, blocker, constraint, error_resolution, plan_*, rule_load | tool outcomes + invariants |
/// | MEDIUM (P3a) | pre_*, latency_spike, iteration_loop, mcp_call, agent_finding | context-shaping signals |
/// | NORMAL (P3b) — implicit via Medium today | environment_change, subagent_*, skill_invocation, external_ref | survive when budget allows |
/// | LOW (P4) | hook_memory_*, intent, role, large_user_data | metadata; first to drop |
pub fn classify_priority_by_hook_name(hook_name: &str) -> EventPriority {
    match hook_name {
        // P1 CRITICAL — must survive context compaction 100%
        "session_start" | "session_stop" | "stop" | "subagent_stop" | "error" | "user_decision"
        | "rejected_approach" | "user_prompt_submit" | "rule_load" => EventPriority::Critical,

        // P2 HIGH — tool outcomes + invariants (carried over PreCompact)
        "post_edit" | "post_read" | "post_write" | "post_bash" | "pre_edit" | "blocker"
        | "constraint" | "error_resolution" | "plan_enter" | "plan_exit" | "plan_approved"
        | "plan_rejected" => EventPriority::High,

        // P3 MEDIUM — context-shaping (drop first when budget tight)
        "pre_read" | "pre_write" | "pre_bash" | "latency_spike" | "iteration_loop" | "mcp_call"
        | "agent_finding" | "environment_change" | "subagent_launch" | "subagent_complete"
        | "skill_invocation" | "external_ref" => EventPriority::Medium,

        // P4 LOW — metadata / housekeeping
        "hook_memory_store"
        | "hook_memory_recall"
        | "intent_classification"
        | "role_directive"
        | "large_user_data" => EventPriority::Low,

        // Unknown hook names default to Low (safest — won't pollute compaction)
        _ => EventPriority::Low,
    }
}

/// Classify a hook event type into its priority tier.
///
/// # Priority Classification Rules
///
/// | Tier | Hook Types | Rationale |
/// |------|------------|-----------|
/// | CRITICAL | SessionStart, SessionStop | Session lifecycle + error recovery |
/// | HIGH | PostEdit, PostRead, PostWrite, PostBash, PreEdit | Tool outcomes + quality signals |
/// | MEDIUM | PreRead, PreWrite, PreBash | Context enrichment signals |
/// | LOW | Metadata / housekeeping (future: HookMemoryStore, HookMemoryRecall) |
pub fn classify_event_priority(event: &HookEvent) -> EventPriority {
    match event {
        // CRITICAL: Session lifecycle and error recovery
        HookEvent::SessionStart { .. } | HookEvent::SessionStop { .. } => EventPriority::Critical,

        // HIGH: Tool outcomes + quality signals (RL feedback loop)
        HookEvent::PostEdit { .. }
        | HookEvent::PostRead { .. }
        | HookEvent::PostWrite { .. }
        | HookEvent::PostBash { .. }
        | HookEvent::PreEdit { .. } => EventPriority::High,

        // MEDIUM: Context enrichment signals
        HookEvent::PreRead { .. } | HookEvent::PreWrite { .. } | HookEvent::PreBash { .. } => {
            EventPriority::Medium
        } // LOW: Metadata / housekeeping
          // (HookMemoryStore, HookMemoryRecall — future variants, reserved for D3.4 extension)
    }
}

/// Unified hook event enum for RL reward computation.
/// Each variant carries metadata sufficient for reward signal generation.
#[derive(Debug, Clone)]
pub enum HookEvent {
    /// Fired before a file is read.
    PreRead {
        /// Path of the file about to be read.
        file_path: String,
        /// When the event was emitted.
        timestamp: DateTime<Utc>,
    },
    /// Fired after a file was read.
    PostRead {
        /// Path of the file that was read.
        file_path: String,
        /// Number of bytes read.
        bytes_read: u64,
        /// When the event was emitted.
        timestamp: DateTime<Utc>,
    },
    /// Fired before a file is edited.
    PreEdit {
        /// Path of the file about to be edited.
        file_path: String,
        /// When the event was emitted.
        timestamp: DateTime<Utc>,
    },
    /// Fired after a file was edited.
    PostEdit {
        /// Path of the file that was edited.
        file_path: String,
        /// Whether the edit succeeded.
        success: bool,
        /// When the event was emitted.
        timestamp: DateTime<Utc>,
    },
    /// Fired before a file is written.
    PreWrite {
        /// Path of the file about to be written.
        file_path: String,
        /// When the event was emitted.
        timestamp: DateTime<Utc>,
    },
    /// Fired after a file was written.
    PostWrite {
        /// Path of the file that was written.
        file_path: String,
        /// Whether the write succeeded.
        success: bool,
        /// When the event was emitted.
        timestamp: DateTime<Utc>,
    },
    /// Fired before a Bash command runs.
    PreBash {
        /// The command line about to run.
        command: String,
        /// When the event was emitted.
        timestamp: DateTime<Utc>,
    },
    /// Fired after a Bash command finishes.
    PostBash {
        /// The command line that ran.
        command: String,
        /// Process exit code returned by the command.
        exit_code: i32,
        /// When the event was emitted.
        timestamp: DateTime<Utc>,
    },
    /// Fired when a session starts.
    SessionStart {
        /// Identifier of the session that started.
        session_id: String,
        /// When the event was emitted.
        timestamp: DateTime<Utc>,
    },
    /// Fired when a session stops.
    SessionStop {
        /// Identifier of the session that stopped.
        session_id: String,
        /// When the event was emitted.
        timestamp: DateTime<Utc>,
    },
}

impl HookEvent {
    /// Construct a PreRead event.
    pub fn pre_read(file_path: impl Into<String>) -> Self {
        Self::PreRead {
            file_path: file_path.into(),
            timestamp: Utc::now(),
        }
    }

    /// Construct a PostRead event.
    pub fn post_read(file_path: impl Into<String>, bytes_read: u64) -> Self {
        Self::PostRead {
            file_path: file_path.into(),
            bytes_read,
            timestamp: Utc::now(),
        }
    }

    /// Construct a PreEdit event.
    pub fn pre_edit(file_path: impl Into<String>) -> Self {
        Self::PreEdit {
            file_path: file_path.into(),
            timestamp: Utc::now(),
        }
    }

    /// Construct a PostEdit event.
    pub fn post_edit(file_path: impl Into<String>, success: bool) -> Self {
        Self::PostEdit {
            file_path: file_path.into(),
            success,
            timestamp: Utc::now(),
        }
    }

    /// Construct a PreWrite event.
    pub fn pre_write(file_path: impl Into<String>) -> Self {
        Self::PreWrite {
            file_path: file_path.into(),
            timestamp: Utc::now(),
        }
    }

    /// Construct a PostWrite event.
    pub fn post_write(file_path: impl Into<String>, success: bool) -> Self {
        Self::PostWrite {
            file_path: file_path.into(),
            success,
            timestamp: Utc::now(),
        }
    }

    /// Construct a PreBash event.
    pub fn pre_bash(command: impl Into<String>) -> Self {
        Self::PreBash {
            command: command.into(),
            timestamp: Utc::now(),
        }
    }

    /// Construct a PostBash event.
    pub fn post_bash(command: impl Into<String>, exit_code: i32) -> Self {
        Self::PostBash {
            command: command.into(),
            exit_code,
            timestamp: Utc::now(),
        }
    }

    /// Construct a SessionStart event.
    pub fn session_start(session_id: impl Into<String>) -> Self {
        Self::SessionStart {
            session_id: session_id.into(),
            timestamp: Utc::now(),
        }
    }

    /// Construct a SessionStop event.
    pub fn session_stop(session_id: impl Into<String>) -> Self {
        Self::SessionStop {
            session_id: session_id.into(),
            timestamp: Utc::now(),
        }
    }
}
