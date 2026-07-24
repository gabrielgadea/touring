//! Audit data loader — reads compliance/audit.jsonl into EvolutionAnalyzer.

pub use super::errors::{AuditError, Result};
pub use super::stats::{AuditEvent, AuditLoader, AuditSummary};
#[cfg(test)]
use super::stats::{SessionStats, ToolStats};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_event_is_denied() {
        let allow = AuditEvent {
            timestamp: 0.0,
            session_id: "s1".into(),
            tool_name: "Bash".into(),
            hook_event: "PostToolUse".into(),
            decision: "allow".into(),
            file_path: "".into(),
            details: "".into(),
        };
        assert!(!allow.is_denied());

        let deny = AuditEvent {
            decision: "deny".into(),
            ..allow.clone()
        };
        assert!(deny.is_denied());

        let block = AuditEvent {
            decision: "block".into(),
            ..allow.clone()
        };
        assert!(block.is_denied());
    }

    #[test]
    fn test_session_stats_duration() {
        let mut stats = SessionStats::default();
        stats.first_event = 1000.0;
        stats.last_event = 1060.0;
        assert!((stats.duration_secs() - 60.0).abs() < 0.001);
    }

    #[test]
    fn test_tool_stats_reliability() {
        let mut tool = ToolStats::default();
        tool.total_uses = 100;
        tool.deny_count = 5;
        assert!((tool.reliability() - 0.95).abs() < 0.001);
    }
}
