//! I-15 — SessionStart Guide builder (15 structured sections).
//!
//! Replicates context-mode's "Session Guide" injected at SessionStart so the
//! LLM can resume context after PreCompact with structured semantic state.
//! Every section is optional and rendered Markdown-style with a `## Header`.
//! Empty sections are suppressed (no header emitted).
//!
//! Per-section size cap: 500 chars (truncate + ellipsis). Total cap: 5000
//! chars to avoid bloating instructions-loaded.
//!
//! The 15 sections (matching context-mode exactly):
//!
//! 1. Last Request    — most recent UserPrompt
//! 2. Tasks           — open subtasks from `decompose status`
//! 3. Plans           — recently modified `*.md` plans
//! 4. Decisions       — `HookEvent::Decision` from last 7d
//! 5. Files Modified  — `post_edit`/`post_write` recent
//! 6. Errors          — recent error events
//! 7. Constraints     — discovered limitations
//! 8. Blockers        — open "blocked on" items
//! 9. Git             — checkout/commit/diff recent (when available)
//! 10. Rules          — loaded CLAUDE/AGENTS/GEMINI files
//! 11. MCP Tools      — most-used MCP calls
//! 12. Subagents      — recent launches/completions
//! 13. Skills         — slash commands invoked
//! 14. Rejected       — user-denied tool calls
//! 15. References     — URLs/issue refs deduped
//!
//! Used by `touring_hooks::instructions_loaded` to prepend structured context.

const MAX_SECTION_CHARS: usize = 500;
const MAX_TOTAL_CHARS: usize = 5000;
const TRUNCATION_SUFFIX: &str = " …(truncated)";

/// Builder pattern: each `with_*` populates one section. Order is canonical
/// to match context-mode rendering.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct SessionGuide {
    /// The user's most recent request, if captured.
    pub last_request: Option<String>,
    /// Summary of active or recent tasks.
    pub tasks: Option<String>,
    /// Summary of plans in progress.
    pub plans: Option<String>,
    /// Key decisions made during the session.
    pub decisions: Option<String>,
    /// Files modified during the session.
    pub files_modified: Option<String>,
    /// Notable errors encountered.
    pub errors: Option<String>,
    /// Active constraints the session must respect.
    pub constraints: Option<String>,
    /// Outstanding blockers.
    pub blockers: Option<String>,
    /// Version-control status summary.
    pub git: Option<String>,
    /// Relevant project rules.
    pub rules: Option<String>,
    /// MCP tools relevant to the session.
    pub mcp_tools: Option<String>,
    /// Subagents involved in the session.
    pub subagents: Option<String>,
    /// Skills relevant to the session.
    pub skills: Option<String>,
    /// Approaches that were considered and rejected.
    pub rejected: Option<String>,
    /// External references gathered during the session.
    pub references: Option<String>,
}

impl SessionGuide {
    /// Construct an empty guide.
    pub fn new() -> Self {
        Self::default()
    }

    /// Setter chain — each method returns `self` for builder ergonomics.
    pub fn with_last_request(mut self, s: impl Into<String>) -> Self {
        self.last_request = nonempty_truncated(s.into());
        self
    }

    /// Sets the tasks section.
    pub fn with_tasks(mut self, s: impl Into<String>) -> Self {
        self.tasks = nonempty_truncated(s.into());
        self
    }

    /// Sets the plans section.
    pub fn with_plans(mut self, s: impl Into<String>) -> Self {
        self.plans = nonempty_truncated(s.into());
        self
    }

    /// Sets the decisions section.
    pub fn with_decisions(mut self, s: impl Into<String>) -> Self {
        self.decisions = nonempty_truncated(s.into());
        self
    }

    /// Sets the files-modified section.
    pub fn with_files_modified(mut self, s: impl Into<String>) -> Self {
        self.files_modified = nonempty_truncated(s.into());
        self
    }

    /// Sets the errors section.
    pub fn with_errors(mut self, s: impl Into<String>) -> Self {
        self.errors = nonempty_truncated(s.into());
        self
    }

    /// Sets the constraints section.
    pub fn with_constraints(mut self, s: impl Into<String>) -> Self {
        self.constraints = nonempty_truncated(s.into());
        self
    }

    /// Sets the blockers section.
    pub fn with_blockers(mut self, s: impl Into<String>) -> Self {
        self.blockers = nonempty_truncated(s.into());
        self
    }

    /// Sets the git status section.
    pub fn with_git(mut self, s: impl Into<String>) -> Self {
        self.git = nonempty_truncated(s.into());
        self
    }

    /// Sets the rules section.
    pub fn with_rules(mut self, s: impl Into<String>) -> Self {
        self.rules = nonempty_truncated(s.into());
        self
    }

    /// Sets the MCP-tools section.
    pub fn with_mcp_tools(mut self, s: impl Into<String>) -> Self {
        self.mcp_tools = nonempty_truncated(s.into());
        self
    }

    /// Sets the subagents section.
    pub fn with_subagents(mut self, s: impl Into<String>) -> Self {
        self.subagents = nonempty_truncated(s.into());
        self
    }

    /// Sets the skills section.
    pub fn with_skills(mut self, s: impl Into<String>) -> Self {
        self.skills = nonempty_truncated(s.into());
        self
    }

    /// Sets the rejected-approaches section.
    pub fn with_rejected(mut self, s: impl Into<String>) -> Self {
        self.rejected = nonempty_truncated(s.into());
        self
    }

    /// Sets the references section.
    pub fn with_references(mut self, s: impl Into<String>) -> Self {
        self.references = nonempty_truncated(s.into());
        self
    }

    /// Render the populated sections as a Markdown document.
    /// Empty sections are suppressed. Total output capped at
    /// `MAX_TOTAL_CHARS`; sections after the cap are dropped (not
    /// truncated mid-section, preserving readability).
    pub fn render(&self) -> String {
        let sections: [(&str, &Option<String>); 15] = [
            ("Last Request", &self.last_request),
            ("Tasks", &self.tasks),
            ("Plans", &self.plans),
            ("Decisions", &self.decisions),
            ("Files Modified", &self.files_modified),
            ("Errors", &self.errors),
            ("Constraints", &self.constraints),
            ("Blockers", &self.blockers),
            ("Git", &self.git),
            ("Rules", &self.rules),
            ("MCP Tools", &self.mcp_tools),
            ("Subagents", &self.subagents),
            ("Skills", &self.skills),
            ("Rejected", &self.rejected),
            ("References", &self.references),
        ];
        let mut out = String::with_capacity(MAX_TOTAL_CHARS);
        out.push_str("# Session Guide\n\n");
        let mut budget = MAX_TOTAL_CHARS.saturating_sub(out.len());
        for (header, content) in sections.iter() {
            let Some(body) = content else { continue };
            let body = body.trim();
            if body.is_empty() {
                continue;
            }
            // header + body + 2 newlines
            let cost = header.len() + body.len() + 8;
            if cost > budget {
                break;
            }
            out.push_str("## ");
            out.push_str(header);
            out.push_str("\n\n");
            out.push_str(body);
            out.push_str("\n\n");
            budget = budget.saturating_sub(cost);
        }
        out
    }

    /// Number of populated (non-empty) sections.
    pub fn populated_count(&self) -> usize {
        let opts: [&Option<String>; 15] = [
            &self.last_request,
            &self.tasks,
            &self.plans,
            &self.decisions,
            &self.files_modified,
            &self.errors,
            &self.constraints,
            &self.blockers,
            &self.git,
            &self.rules,
            &self.mcp_tools,
            &self.subagents,
            &self.skills,
            &self.rejected,
            &self.references,
        ];
        opts.iter()
            .filter(|o| o.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false))
            .count()
    }
}

/// Per-section truncation: if `s` exceeds [`MAX_SECTION_CHARS`], cut at the
/// boundary and append `…(truncated)`. Returns `None` for empty/whitespace
/// strings so the section is suppressed in render.
fn nonempty_truncated(s: String) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().count() <= MAX_SECTION_CHARS {
        return Some(trimmed.to_string());
    }
    let cut: String = trimmed
        .chars()
        .take(MAX_SECTION_CHARS - TRUNCATION_SUFFIX.chars().count())
        .collect();
    Some(format!("{cut}{TRUNCATION_SUFFIX}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_guide_renders_only_title() {
        let g = SessionGuide::new();
        let out = g.render();
        assert!(out.starts_with("# Session Guide"));
        assert!(!out.contains("## "));
    }

    #[test]
    fn test_skips_empty_sections_in_render() {
        let g = SessionGuide::new()
            .with_tasks("Task A pending")
            .with_decisions(""); // empty: must be suppressed
        let out = g.render();
        assert!(out.contains("## Tasks"));
        assert!(!out.contains("## Decisions"));
    }

    #[test]
    fn test_truncates_section_above_max_chars() {
        let huge: String = "x".repeat(MAX_SECTION_CHARS + 100);
        let g = SessionGuide::new().with_tasks(huge);
        let body = g.tasks.as_ref().unwrap();
        assert!(body.contains(TRUNCATION_SUFFIX));
        assert!(body.chars().count() <= MAX_SECTION_CHARS);
    }

    #[test]
    fn test_populated_count_matches_inputs() {
        let g = SessionGuide::new()
            .with_last_request("foo")
            .with_tasks("bar")
            .with_decisions("baz");
        assert_eq!(g.populated_count(), 3);
    }

    #[test]
    fn test_full_guide_renders_15_sections() {
        let g = SessionGuide::new()
            .with_last_request("Implement I-15")
            .with_tasks("S-1: build SessionGuide")
            .with_plans("master-plan.md")
            .with_decisions("Use builder pattern")
            .with_files_modified("session_guide.rs")
            .with_errors("none")
            .with_constraints("CC < 15")
            .with_blockers("none")
            .with_git("commit pending")
            .with_rules("REGRA #14 applies")
            .with_mcp_tools("ctx_search")
            .with_subagents("touring-engineer")
            .with_skills("/Touring")
            .with_rejected("none")
            .with_references("github.com/mksglu/context-mode");
        let out = g.render();
        let header_count = out.matches("## ").count();
        assert_eq!(header_count, 15);
        assert_eq!(g.populated_count(), 15);
    }

    #[test]
    fn test_total_cap_drops_late_sections_when_budget_exhausted() {
        // Force scenario where each section is large enough to consume budget
        let big = "y".repeat(450);
        let g = SessionGuide::new()
            .with_last_request(big.clone())
            .with_tasks(big.clone())
            .with_plans(big.clone())
            .with_decisions(big.clone())
            .with_files_modified(big.clone())
            .with_errors(big.clone())
            .with_constraints(big.clone())
            .with_blockers(big.clone())
            .with_git(big.clone())
            .with_rules(big.clone())
            .with_mcp_tools(big.clone())
            .with_subagents(big.clone())
            .with_skills(big.clone())
            .with_rejected(big.clone())
            .with_references(big);
        let out = g.render();
        assert!(
            out.len() <= MAX_TOTAL_CHARS + 200,
            "output {} > cap",
            out.len()
        );
    }

    #[test]
    fn test_serializes_to_json() {
        let g = SessionGuide::new()
            .with_last_request("hello")
            .with_tasks("world");
        let json = serde_json::to_string(&g).expect("serialize");
        assert!(json.contains("\"last_request\":\"hello\""));
        assert!(json.contains("\"tasks\":\"world\""));
        assert!(json.contains("\"plans\":null"));
    }

    #[test]
    fn test_nonempty_truncated_returns_none_for_whitespace() {
        assert!(nonempty_truncated("   ".to_string()).is_none());
        assert!(nonempty_truncated("".to_string()).is_none());
        assert_eq!(
            nonempty_truncated("hello".to_string()),
            Some("hello".to_string())
        );
    }
}
