//! JSONL line parsers for different source types
//!
//! Each source type (compliance, cost, skill, hook_trace) has its own
//! parser that extracts a key, tier, value text (for embedding), and entry_type.

use serde_json::Value;
use touring_foundation::truncate_str;

/// Supported JSONL source types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    /// Compliance ledger entries.
    Compliance,
    /// Cost/credit accounting entries.
    Cost,
    /// Skill invocation records.
    Skill,
    /// Hook activity trace entries.
    HookTrace,
    /// Learn-Nothing-Again (LNA) lesson entries.
    Lna,
    /// Claude Code conversation transcript (Phase 2 transcript mining)
    ConversationTranscript,
}

impl SourceType {
    /// Detect source type from filename
    pub(crate) fn from_filename(name: &str) -> Option<Self> {
        let lower = name.to_lowercase();
        if lower.contains("compliance") {
            Some(Self::Compliance)
        } else if lower.contains("cost") {
            Some(Self::Cost)
        } else if lower.contains("skill") {
            Some(Self::Skill)
        } else if lower.contains("hook_activity") || lower.contains("hook_trace") {
            Some(Self::HookTrace)
        } else if lower.contains("lna") {
            Some(Self::Lna)
        } else {
            None
        }
    }

    /// Default memory tier for this source type
    pub fn default_tier(&self) -> &'static str {
        match self {
            Self::Compliance => "reference",
            Self::Cost => "reference",
            Self::Skill => "reference",
            Self::HookTrace => "ephemeral",
            Self::Lna => "reference",
            Self::ConversationTranscript => "semantic",
        }
    }

    /// Entry type tag
    pub fn entry_type(&self) -> &'static str {
        match self {
            Self::Compliance => "compliance",
            Self::Cost => "cost",
            Self::Skill => "skill",
            Self::HookTrace => "hook_trace",
            Self::Lna => "lna",
            Self::ConversationTranscript => "conversation_transcript",
        }
    }
}

/// Parsed entry ready for storage
#[derive(Debug, Clone)]
pub struct ParsedEntry {
    /// Storage key derived from the source record.
    pub key: String,
    /// Target memory tier for the entry.
    pub tier: String,
    /// Text payload used for embedding and recall.
    pub value: String,
    /// Source-derived classification tag for the entry.
    pub entry_type: String,
}

/// JSONL parser for multiple source types
#[derive(Debug)]
pub struct JsonlParser;

impl JsonlParser {
    /// Parse a single JSONL line into a memory entry
    pub(crate) fn parse_line(line: &str, source_type: SourceType) -> Option<ParsedEntry> {
        let v: Value = serde_json::from_str(line).ok()?;

        match source_type {
            SourceType::Compliance => Self::parse_compliance(&v),
            SourceType::Cost => Self::parse_cost(&v),
            SourceType::Skill => Self::parse_skill(&v),
            SourceType::HookTrace => Self::parse_hook_trace(&v),
            SourceType::Lna => Self::parse_lna(&v),
            // Transcript lines are parsed by transcript_miner::parse_transcript_line,
            // not by this legacy ingestion path.
            SourceType::ConversationTranscript => None,
        }
    }

    fn parse_compliance(v: &Value) -> Option<ParsedEntry> {
        let ts = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown");
        let tool = v
            .get("tool_name")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown");
        let cila = v
            .get("cila_level")
            .and_then(|c| c.as_str())
            .or_else(|| v.get("cila_level").and_then(|c| c.as_i64()).map(|_| ""))
            .unwrap_or("L0");
        let score = v
            .get("enforcement_score")
            .and_then(|s| s.as_f64())
            .unwrap_or(0.0);
        let decision = v
            .get("hook_decision")
            .and_then(|d| d.as_str())
            .unwrap_or("allow");

        let key = format!("compliance:{}:{}", ts, tool);
        let value = format!(
            "{} cila={} score={:.2} decision={}",
            tool, cila, score, decision,
        );

        Some(ParsedEntry {
            key,
            tier: "reference".to_string(),
            value,
            entry_type: "compliance".to_string(),
        })
    }

    fn parse_cost(v: &Value) -> Option<ParsedEntry> {
        let ts = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown");
        let session = v
            .get("session_id")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");
        let input = v.get("input_tokens").and_then(|t| t.as_u64()).unwrap_or(0);
        let output = v.get("output_tokens").and_then(|t| t.as_u64()).unwrap_or(0);
        let cache = v
            .get("cache_read_tokens")
            .and_then(|t| t.as_u64())
            .unwrap_or(0);
        let cost = v.get("total_cost").and_then(|c| c.as_f64()).unwrap_or(0.0);
        let model = v.get("model").and_then(|m| m.as_str()).unwrap_or("unknown");

        let key = format!("cost:{}:{}", ts, &session[..session.len().min(8)]);
        let value = format!(
            "session tokens={} cache_read={} cost=${:.4} model={}",
            input + output,
            cache,
            cost,
            model,
        );

        Some(ParsedEntry {
            key,
            tier: "reference".to_string(),
            value,
            entry_type: "cost".to_string(),
        })
    }

    fn parse_skill(v: &Value) -> Option<ParsedEntry> {
        let phase = v.get("phase").and_then(|p| p.as_str()).unwrap_or("unknown");
        let script = v
            .get("script_path")
            .and_then(|s| s.as_str())
            .or_else(|| v.get("script").and_then(|s| s.as_str()))
            .unwrap_or("unknown");
        let process_type = v
            .get("process_type")
            .and_then(|p| p.as_str())
            .unwrap_or("unknown");
        let lna = v.get("lna_score").and_then(|l| l.as_f64()).unwrap_or(0.0);

        let key = format!("skill:{}:{}", phase, script);
        let value = format!(
            "{} {} lna={:.2} script={}",
            process_type, phase, lna, script,
        );

        Some(ParsedEntry {
            key,
            tier: "reference".to_string(),
            value,
            entry_type: "skill".to_string(),
        })
    }

    fn parse_hook_trace(v: &Value) -> Option<ParsedEntry> {
        let ts = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown");
        let hook = v
            .get("hook_name")
            .and_then(|h| h.as_str())
            .or_else(|| v.get("event_name").and_then(|e| e.as_str()))
            .unwrap_or("unknown");
        let tool = v.get("tool_name").and_then(|t| t.as_str()).unwrap_or("");
        let level = v.get("level").and_then(|l| l.as_str()).unwrap_or("info");
        let msg = v.get("message").and_then(|m| m.as_str()).unwrap_or("");

        let key = format!("hook:{}:{}", ts, hook);
        // Truncate message for embedding efficiency
        let msg_trunc = truncate_str(msg, 200);
        let value = format!("{} {} level={} {}", hook, tool, level, msg_trunc,);

        Some(ParsedEntry {
            key,
            tier: "ephemeral".to_string(),
            value,
            entry_type: "hook_trace".to_string(),
        })
    }

    fn parse_lna(v: &Value) -> Option<ParsedEntry> {
        let phase = v.get("phase").and_then(|p| p.as_str()).unwrap_or("unknown");
        let pid = v
            .get("process_id")
            .and_then(|p| p.as_str())
            .unwrap_or("unknown");
        let score = v.get("lna_score").and_then(|s| s.as_f64()).unwrap_or(0.0);
        let components = v.get("components").and_then(|c| c.as_str()).unwrap_or("");

        let key = format!("lna:{}:{}", phase, pid);
        let value = format!(
            "lna phase={} pid={} score={:.2} {}",
            phase, pid, score, components,
        );

        Some(ParsedEntry {
            key,
            tier: "reference".to_string(),
            value,
            entry_type: "lna".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_type_from_filename() {
        assert_eq!(
            SourceType::from_filename("compliance.jsonl"),
            Some(SourceType::Compliance)
        );
        assert_eq!(
            SourceType::from_filename("costs.jsonl"),
            Some(SourceType::Cost)
        );
        assert_eq!(
            SourceType::from_filename("rlm_skill_learning.jsonl"),
            Some(SourceType::Skill)
        );
        assert_eq!(
            SourceType::from_filename("hook_activity.jsonl"),
            Some(SourceType::HookTrace)
        );
        assert_eq!(
            SourceType::from_filename("lna_history.jsonl"),
            Some(SourceType::Lna)
        );
        assert_eq!(SourceType::from_filename("random.txt"), None);
    }

    #[test]
    fn test_parse_compliance_line() {
        let line = r#"{"timestamp":"2026-03-13T10:00:00","tool_name":"Write","cila_level":"L2","enforcement_score":9.5,"hook_decision":"allow"}"#;
        let entry = JsonlParser::parse_line(line, SourceType::Compliance).unwrap();
        assert_eq!(entry.entry_type, "compliance");
        assert!(entry.key.starts_with("compliance:"));
        assert!(entry.value.contains("Write"));
        assert!(entry.value.contains("cila=L2"));
        assert!(entry.value.contains("score=9.50"));
    }

    #[test]
    fn test_parse_cost_line() {
        let line = r#"{"timestamp":"2026-03-13T10:00:00","session_id":"abc12345-xyz","input_tokens":1000,"output_tokens":500,"cache_read_tokens":200,"total_cost":0.05,"model":"opus-4"}"#;
        let entry = JsonlParser::parse_line(line, SourceType::Cost).unwrap();
        assert_eq!(entry.entry_type, "cost");
        assert!(entry.value.contains("tokens=1500"));
        assert!(entry.value.contains("cache_read=200"));
    }

    #[test]
    fn test_parse_hook_trace_line() {
        let line = r#"{"timestamp":"2026-03-13T10:00:00","hook_name":"quality_gate","tool_name":"Edit","level":"warn","message":"ruff found 2 issues"}"#;
        let entry = JsonlParser::parse_line(line, SourceType::HookTrace).unwrap();
        assert_eq!(entry.entry_type, "hook_trace");
        assert_eq!(entry.tier, "ephemeral");
        assert!(entry.value.contains("quality_gate"));
    }

    #[test]
    fn test_parse_invalid_json() {
        let result = JsonlParser::parse_line("not json", SourceType::Compliance);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_skill_line() {
        let line = r#"{"phase":"F5","script_path":"compute_base.py","process_type":"REVISAO_ORDINARIA","lna_score":0.85}"#;
        let entry = JsonlParser::parse_line(line, SourceType::Skill).unwrap();
        assert_eq!(entry.entry_type, "skill");
        assert!(entry.value.contains("REVISAO_ORDINARIA"));
        assert!(entry.value.contains("lna=0.85"));
    }

    #[test]
    fn test_parse_lna_line() {
        let line = r#"{"phase":"F3","process_id":"50500.040225","lna_score":0.72,"components":"grounding=0.8 citation=0.6"}"#;
        let entry = JsonlParser::parse_line(line, SourceType::Lna).unwrap();
        assert_eq!(entry.entry_type, "lna");
        assert!(entry.value.contains("lna phase=F3"));
    }
}
