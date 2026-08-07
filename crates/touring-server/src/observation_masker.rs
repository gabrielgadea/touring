//! Observation Masker — reduces context tokens by summarizing tool outputs.
//!
//! Tool results (Read, Grep, Bash, etc.) are the largest consumers of context
//! tokens. This module classifies context entries by role and replaces verbose
//! tool outputs with 1-line summaries while preserving all action traces
//! (System, User, Assistant, ToolCall).
//!
//! ## Integration
//!
//! Called in the context compilation pipeline BEFORE `compile_context()`:
//!
//! ```text
//! raw context → ObservationMasker::mask_observations() → compile_context()
//! ```
//!
//! Only activates when estimated total tokens exceed `activation_threshold`.

use std::collections::HashMap;

use once_cell::sync::Lazy;
use regex::Regex;

/// Rough chars-per-token estimate (matches `context_compiler::CHARS_PER_TOKEN`).
const CHARS_PER_TOKEN: usize = 4;

/// Default token threshold below which masking is skipped entirely.
const DEFAULT_ACTIVATION_THRESHOLD: usize = 4000;

// ── Regex patterns for role classification ──────────────────────────────

static RE_SYSTEM: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^(system|<system|##\s*system)").expect("valid regex"));

static RE_USER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^(user|Human:|<user)").expect("valid regex"));

static RE_ASSISTANT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^(assistant|Assistant:|<assistant)").expect("valid regex"));

static RE_TOOL_CALL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(tool_call|ToolCall|<tool_call|→\s*(Read|Grep|Bash|Edit|Write|Glob)\b)")
        .expect("valid regex")
});

static RE_TOOL_RESULT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(tool_result|ToolResult|<tool_result|Result\s*\(|Output:\s)")
        .expect("valid regex")
});

/// Patterns that identify a Read tool result block.
static RE_READ_RESULT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^\s*\d+[→│|]\s*").expect("valid regex"));

/// Patterns that identify a Grep result block (file matches).
static RE_GREP_RESULT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[/\w].*:\d+:").expect("valid regex"));

/// Patterns that identify a Bash result (exit code or shell output).
static RE_BASH_EXIT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(exit\s*code|exited\s*with|return\s*code)\s*[:=]?\s*\d+").expect("valid regex")
});

// ── Public types ────────────────────────────────────────────────────────

/// Role classification for context entries.
///
/// Determines how each block of context is treated during masking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextRole {
    /// System instructions — always preserved.
    System,
    /// User messages — always preserved.
    UserMessage,
    /// Assistant reasoning — always preserved.
    AssistantReason,
    /// Tool invocations — preserved (action trace).
    ToolCall,
    /// Tool outputs — masked to 1-line summary.
    ToolResult,
}

/// Strategy for masking different tool outputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaskingStrategy {
    /// Replace with an N-line summary.
    Summarize {
        /// Maximum number of summary lines to keep.
        max_lines: usize,
    },
    /// Keep first and last N lines, elide the middle.
    HeadTail {
        /// Number of leading lines to preserve.
        head: usize,
        /// Number of trailing lines to preserve.
        tail: usize,
    },
    /// Full preservation (no masking).
    Preserve,
}

/// Statistics returned after a masking pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskingStats {
    /// Estimated tokens before masking.
    pub original_tokens: usize,
    /// Estimated tokens after masking.
    pub masked_tokens: usize,
    /// Number of tool result blocks that were masked.
    pub blocks_masked: usize,
}

/// Observation masker that reduces context tokens by summarizing tool outputs.
///
/// Preserves System, User, Assistant, and ToolCall lines verbatim.
/// Replaces ToolResult blocks with concise 1-line summaries.
#[derive(Debug)]
pub struct ObservationMasker {
    /// Per-tool masking strategies (key = lowercase tool name).
    strategies: HashMap<String, MaskingStrategy>,
    /// Token count below which masking is skipped entirely.
    activation_threshold_tokens: usize,
}

impl ObservationMasker {
    /// Create a new masker with default strategies and threshold.
    pub fn new() -> Self {
        let mut strategies = HashMap::new();
        strategies.insert(
            "read".to_string(),
            MaskingStrategy::Summarize { max_lines: 1 },
        );
        strategies.insert(
            "grep".to_string(),
            MaskingStrategy::Summarize { max_lines: 1 },
        );
        strategies.insert(
            "bash".to_string(),
            MaskingStrategy::Summarize { max_lines: 1 },
        );
        strategies.insert(
            "write".to_string(),
            MaskingStrategy::Summarize { max_lines: 1 },
        );
        strategies.insert(
            "edit".to_string(),
            MaskingStrategy::Summarize { max_lines: 1 },
        );
        strategies.insert(
            "glob".to_string(),
            MaskingStrategy::Summarize { max_lines: 1 },
        );
        Self {
            strategies,
            activation_threshold_tokens: DEFAULT_ACTIVATION_THRESHOLD,
        }
    }

    /// Create a masker with a custom activation threshold.
    pub fn with_threshold(threshold: usize) -> Self {
        let mut masker = Self::new();
        masker.activation_threshold_tokens = threshold;
        masker
    }

    /// Override the masking strategy for a specific tool.
    pub fn set_strategy(&mut self, tool: &str, strategy: MaskingStrategy) {
        self.strategies.insert(tool.to_lowercase(), strategy);
    }

    /// Classify a context line's role.
    pub fn classify_role(&self, line: &str) -> ContextRole {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            // Blank lines inherit context; treat as assistant by default.
            return ContextRole::AssistantReason;
        }
        if RE_SYSTEM.is_match(trimmed) {
            return ContextRole::System;
        }
        if RE_USER.is_match(trimmed) {
            return ContextRole::UserMessage;
        }
        if RE_ASSISTANT.is_match(trimmed) {
            return ContextRole::AssistantReason;
        }
        if RE_TOOL_CALL.is_match(trimmed) {
            return ContextRole::ToolCall;
        }
        if RE_TOOL_RESULT.is_match(trimmed) {
            return ContextRole::ToolResult;
        }
        // Default: assistant reasoning (preserves unknown lines).
        ContextRole::AssistantReason
    }

    /// Mask observations in a context string.
    ///
    /// Returns the masked context and statistics about the transformation.
    /// If estimated tokens are below `activation_threshold_tokens`, returns
    /// the original context unchanged.
    pub fn mask_observations(&self, context: &str) -> (String, MaskingStats) {
        let original_tokens = context.len() / CHARS_PER_TOKEN;

        // Short-circuit: skip masking for small contexts.
        if original_tokens < self.activation_threshold_tokens {
            return (
                context.to_string(),
                MaskingStats {
                    original_tokens,
                    masked_tokens: original_tokens,
                    blocks_masked: 0,
                },
            );
        }

        let blocks = self.parse_blocks(context);
        let mut output = String::with_capacity(context.len());
        let mut blocks_masked: usize = 0;

        for block in &blocks {
            match block.role {
                ContextRole::ToolResult => {
                    let summary = self.summarize_tool_result(&block.tool_name, &block.content);
                    output.push_str(&summary);
                    output.push('\n');
                    blocks_masked += 1;
                }
                // All other roles: preserve verbatim.
                _ => {
                    output.push_str(&block.content);
                    output.push('\n');
                }
            }
        }

        let masked_tokens = output.len() / CHARS_PER_TOKEN;
        (
            output,
            MaskingStats {
                original_tokens,
                masked_tokens,
                blocks_masked,
            },
        )
    }

    /// Generate a summary for a tool result block, consulting the strategy map.
    ///
    /// - `Summarize { max_lines }` → 1-line summary (default for all tools).
    /// - `HeadTail { head, tail }` → first `head` + last `tail` lines, middle elided.
    /// - `Preserve` → full content, no masking.
    fn summarize_tool_result(&self, tool_name: &str, result: &str) -> String {
        let lines: Vec<&str> = result.lines().collect();
        let line_count = lines.len();
        let tool_lower = tool_name.to_lowercase();

        // Consult strategy map — fall back to Summarize { max_lines: 1 }.
        let default_strategy = MaskingStrategy::Summarize { max_lines: 1 };
        let strategy = self
            .strategies
            .get(&tool_lower)
            .unwrap_or(&default_strategy);

        match strategy {
            MaskingStrategy::Preserve => result.to_string(),
            MaskingStrategy::HeadTail { head, tail } => {
                let head_n = (*head).min(line_count);
                let tail_n = (*tail).min(line_count.saturating_sub(head_n));
                if head_n + tail_n >= line_count {
                    // Nothing to elide — return full content.
                    return result.to_string();
                }
                // SAFETY: head_n <= line_count and tail_n <= line_count - head_n,
                // and we early-return when head_n + tail_n >= line_count above.
                #[allow(clippy::indexing_slicing)]
                let head_part: Vec<&str> = lines[..head_n].to_vec();
                #[allow(clippy::indexing_slicing)]
                let tail_part: Vec<&str> = lines[line_count - tail_n..].to_vec();
                let elided = line_count - head_n - tail_n;
                format!(
                    "{}\n[... {elided} lines elided ...]\n{}",
                    head_part.join("\n"),
                    tail_part.join("\n"),
                )
            }
            MaskingStrategy::Summarize { .. } => match tool_lower.as_str() {
                "read" => self.summarize_read(result, line_count),
                "grep" => self.summarize_grep(result, line_count),
                "bash" => self.summarize_bash(result, line_count),
                _ => self.summarize_generic(tool_name, result, line_count),
            },
        }
    }

    /// Summarize a Read tool result.
    ///
    /// Format: `[Read] {file}:{start}-{end} ({N} lines)`
    fn summarize_read(&self, result: &str, line_count: usize) -> String {
        // Try to extract file path and line range from the content.
        // Read results typically have numbered lines like "  1→content"
        let mut file_path = String::new();
        let mut first_num: Option<usize> = None;
        let mut last_num: Option<usize> = None;

        for line in result.lines() {
            let trimmed = line.trim();
            // Look for file path indicator.
            if (trimmed.starts_with("File:") || trimmed.starts_with("Reading"))
                && let Some(path_part) = trimmed.split_whitespace().last()
            {
                file_path = path_part.to_string();
            }
            // Parse line numbers from "  N→" or "  N│" format.
            if RE_READ_RESULT.is_match(trimmed)
                && let Some(num_str) = trimmed.split(|c: char| !c.is_ascii_digit()).next()
                && let Ok(n) = num_str.parse::<usize>()
            {
                if first_num.is_none() {
                    first_num = Some(n);
                }
                last_num = Some(n);
            }
        }

        let range = match (first_num, last_num) {
            (Some(f), Some(l)) => format!(":{f}-{l}"),
            _ => String::new(),
        };

        if file_path.is_empty() {
            format!("[Read]{range} ({line_count} lines)")
        } else {
            format!("[Read] {file_path}{range} ({line_count} lines)")
        }
    }

    /// Summarize a Grep tool result.
    ///
    /// Format: `[Grep] {N} matches in {M} files`
    fn summarize_grep(&self, result: &str, _line_count: usize) -> String {
        let mut match_count: usize = 0;
        let mut files_seen: std::collections::HashSet<&str> = std::collections::HashSet::new();

        for line in result.lines() {
            if RE_GREP_RESULT.is_match(line.trim()) {
                match_count += 1;
                // Extract file path (everything before the first `:N:`)
                if let Some(colon_pos) = line.find(':') {
                    let file = &line[..colon_pos];
                    files_seen.insert(file);
                }
            }
        }

        let file_count = files_seen.len();
        if match_count > 0 {
            format!("[Grep] {match_count} matches in {file_count} files")
        } else {
            "[Grep] 0 matches".to_string()
        }
    }

    /// Summarize a Bash tool result.
    ///
    /// Format: `[Bash] exit {code}, {N} lines output`
    fn summarize_bash(&self, result: &str, line_count: usize) -> String {
        // Try to find exit code in the result.
        let mut exit_code: Option<i32> = None;
        for line in result.lines() {
            if let Some(caps) = RE_BASH_EXIT.find(line.trim()) {
                let matched = caps.as_str();
                // Extract the trailing number.
                if let Some(num_str) = matched.split_whitespace().last()
                    && let Ok(code) = num_str.parse::<i32>()
                {
                    exit_code = Some(code);
                    break;
                }
            }
        }

        match exit_code {
            Some(code) => format!("[Bash] exit {code}, {line_count} lines output"),
            None => format!("[Bash] {line_count} lines output"),
        }
    }

    /// Summarize a generic tool result.
    ///
    /// Format: `[{ToolName}] {N} lines, {first_line_preview}...`
    fn summarize_generic(&self, tool_name: &str, result: &str, line_count: usize) -> String {
        let preview = result
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim();

        // Cap preview at 80 chars (MSRV-safe char boundary detection).
        let truncated = if preview.len() > 80 {
            let mut end = 77;
            while end > 0 && !preview.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}...", &preview[..end])
        } else {
            preview.to_string()
        };

        if truncated.is_empty() {
            format!("[{tool_name}] {line_count} lines")
        } else {
            format!("[{tool_name}] {line_count} lines, {truncated}")
        }
    }

    /// Parse raw context into classified blocks.
    ///
    /// A block is a contiguous run of lines with the same role. ToolResult
    /// blocks capture everything until the next non-ToolResult role marker.
    fn parse_blocks(&self, context: &str) -> Vec<ContextBlock> {
        let mut blocks: Vec<ContextBlock> = Vec::new();
        let mut current_role = ContextRole::AssistantReason;
        let mut current_tool = String::new();
        let mut current_lines: Vec<&str> = Vec::new();
        let mut in_tool_result = false;

        for line in context.lines() {
            let role = self.classify_role(line);

            match role {
                ContextRole::ToolResult => {
                    if !in_tool_result {
                        // Flush previous block.
                        if !current_lines.is_empty() {
                            blocks.push(ContextBlock {
                                role: current_role,
                                tool_name: current_tool.clone(),
                                content: current_lines.join("\n"),
                            });
                            current_lines.clear();
                        }
                        current_role = ContextRole::ToolResult;
                        current_tool = detect_tool_name(line);
                        in_tool_result = true;
                    }
                    current_lines.push(line);
                }
                ContextRole::System
                | ContextRole::UserMessage
                | ContextRole::AssistantReason
                | ContextRole::ToolCall => {
                    if in_tool_result {
                        // End the tool result block.
                        blocks.push(ContextBlock {
                            role: ContextRole::ToolResult,
                            tool_name: current_tool.clone(),
                            content: current_lines.join("\n"),
                        });
                        current_lines.clear();
                        in_tool_result = false;
                    } else if role != current_role && !current_lines.is_empty() {
                        // Role changed — flush.
                        blocks.push(ContextBlock {
                            role: current_role,
                            tool_name: current_tool.clone(),
                            content: current_lines.join("\n"),
                        });
                        current_lines.clear();
                    }
                    current_role = role;
                    if role == ContextRole::ToolCall {
                        current_tool = detect_tool_name(line);
                    }
                    current_lines.push(line);
                }
            }
        }

        // Flush remaining.
        if !current_lines.is_empty() {
            blocks.push(ContextBlock {
                role: current_role,
                tool_name: current_tool,
                content: current_lines.join("\n"),
            });
        }

        blocks
    }
}

impl Default for ObservationMasker {
    fn default() -> Self {
        Self::new()
    }
}

// ── Internal types ──────────────────────────────────────────────────────

/// A classified block of context lines with owned content.
#[derive(Debug)]
struct ContextBlock {
    role: ContextRole,
    tool_name: String,
    content: String,
}

/// Detect the tool name from a line that starts a tool call or tool result.
fn detect_tool_name(line: &str) -> String {
    let trimmed = line.trim();
    // Pattern: "→ Read ..." or "→ Grep ..." (tool call markers)
    if let Some(rest) = trimmed.strip_prefix('→')
        && let Some(name) = rest.split_whitespace().next()
    {
        return name.to_string();
    }
    // Pattern: "ToolResult(Read)" or "tool_result: Read"
    for tool in &["Read", "Grep", "Bash", "Edit", "Write", "Glob"] {
        if trimmed.contains(tool) {
            return tool.to_string();
        }
    }
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_system_role() {
        let masker = ObservationMasker::new();
        assert_eq!(
            masker.classify_role("system: you are helpful"),
            ContextRole::System
        );
        assert_eq!(
            masker.classify_role("<system>instructions</system>"),
            ContextRole::System
        );
        assert_eq!(
            masker.classify_role("## System Prompt"),
            ContextRole::System
        );
    }

    #[test]
    fn test_classify_user_role() {
        let masker = ObservationMasker::new();
        assert_eq!(
            masker.classify_role("user: hello"),
            ContextRole::UserMessage
        );
        assert_eq!(
            masker.classify_role("Human: what is this?"),
            ContextRole::UserMessage
        );
        assert_eq!(
            masker.classify_role("<user>question</user>"),
            ContextRole::UserMessage
        );
    }

    #[test]
    fn test_classify_tool_result_role() {
        let masker = ObservationMasker::new();
        assert_eq!(
            masker.classify_role("tool_result: data"),
            ContextRole::ToolResult
        );
        assert_eq!(
            masker.classify_role("ToolResult: output"),
            ContextRole::ToolResult
        );
        assert_eq!(
            masker.classify_role("<tool_result>content</tool_result>"),
            ContextRole::ToolResult
        );
        assert_eq!(
            masker.classify_role("Result(Read): content"),
            ContextRole::ToolResult
        );
        assert_eq!(
            masker.classify_role("Output: some data here"),
            ContextRole::ToolResult
        );
    }

    #[test]
    fn test_classify_tool_call_role() {
        let masker = ObservationMasker::new();
        assert_eq!(
            masker.classify_role("tool_call: Read file.rs"),
            ContextRole::ToolCall
        );
        assert_eq!(
            masker.classify_role("→ Read src/lib.rs"),
            ContextRole::ToolCall
        );
        assert_eq!(
            masker.classify_role("→ Grep pattern"),
            ContextRole::ToolCall
        );
        assert_eq!(masker.classify_role("→ Bash ls -la"), ContextRole::ToolCall);
    }

    #[test]
    fn test_classify_assistant_role() {
        let masker = ObservationMasker::new();
        assert_eq!(
            masker.classify_role("assistant: I will help"),
            ContextRole::AssistantReason
        );
        assert_eq!(
            masker.classify_role("Assistant: Let me check"),
            ContextRole::AssistantReason
        );
        // Unknown lines default to assistant.
        assert_eq!(
            masker.classify_role("some random text"),
            ContextRole::AssistantReason
        );
    }

    #[test]
    fn test_mask_reduces_tokens() {
        let masker = ObservationMasker::with_threshold(10); // Low threshold to activate.
        let context = "user: show me the file\n\
                        → Read src/main.rs\n\
                        tool_result: Read output\n\
                        1→fn main() {}\n\
                        2→    println!(\"hello\");\n\
                        3→    let x = 42;\n\
                        4→    let y = x + 1;\n\
                        5→    println!(\"{}\", y);\n\
                        6→}\n\
                        7→\n\
                        8→fn helper() {}\n\
                        9→    // helper code\n\
                        10→    return 0;\n\
                        assistant: The file contains a main function.";

        let (masked, stats) = masker.mask_observations(context);
        assert!(
            stats.masked_tokens < stats.original_tokens,
            "masking should reduce tokens: original={}, masked={}",
            stats.original_tokens,
            stats.masked_tokens
        );
        assert!(stats.blocks_masked > 0, "should mask at least one block");
        assert!(
            masked.len() < context.len(),
            "masked output should be shorter"
        );
    }

    #[test]
    fn test_mask_preserves_system_and_user() {
        let masker = ObservationMasker::with_threshold(5);
        let context = "system: you are a coding assistant\n\
                        user: fix the bug in parser.rs\n\
                        tool_result: Read output\n\
                        1→fn parse() { buggy_code(); }\n\
                        2→fn parse2() { more_code(); }\n\
                        3→fn parse3() { even_more(); }";

        let (masked, _stats) = masker.mask_observations(context);
        assert!(
            masked.contains("system: you are a coding assistant"),
            "system messages must be preserved"
        );
        assert!(
            masked.contains("user: fix the bug in parser.rs"),
            "user messages must be preserved"
        );
    }

    #[test]
    fn test_mask_preserves_tool_calls() {
        let masker = ObservationMasker::with_threshold(5);
        let context = "→ Read src/lib.rs\n\
                        tool_result: Read output\n\
                        1→fn lib_func() {}\n\
                        2→fn another() {}\n\
                        3→fn third() {}";

        let (masked, _stats) = masker.mask_observations(context);
        assert!(
            masked.contains("→ Read src/lib.rs"),
            "tool calls must be preserved"
        );
    }

    #[test]
    fn test_activation_threshold_skips_small_contexts() {
        let masker = ObservationMasker::new(); // Default threshold = 4000 tokens.
        let small_context = "tool_result: some output\n1→line one\n2→line two";

        let (masked, stats) = masker.mask_observations(small_context);
        assert_eq!(
            masked, small_context,
            "small context should pass through unchanged"
        );
        assert_eq!(stats.blocks_masked, 0);
        assert_eq!(stats.original_tokens, stats.masked_tokens);
    }

    #[test]
    fn test_summarize_read_result() {
        let masker = ObservationMasker::new();
        let read_output = "  1→fn main() {\n  2→    println!(\"hello\");\n  3→}";
        let summary = masker.summarize_tool_result("Read", read_output);
        assert!(
            summary.starts_with("[Read]"),
            "Read summary must start with [Read]: {summary}"
        );
        assert!(
            summary.contains("lines"),
            "Read summary must mention line count: {summary}"
        );
    }

    #[test]
    fn test_summarize_grep_result() {
        let masker = ObservationMasker::new();
        let grep_output = "src/main.rs:10:fn main() {}\n\
                           src/main.rs:20:fn helper() {}\n\
                           src/lib.rs:5:pub fn api() {}";
        let summary = masker.summarize_tool_result("Grep", grep_output);
        assert!(
            summary.starts_with("[Grep]"),
            "Grep summary must start with [Grep]: {summary}"
        );
        assert!(
            summary.contains("3 matches"),
            "should count 3 matches: {summary}"
        );
        assert!(
            summary.contains("2 files"),
            "should count 2 unique files: {summary}"
        );
    }

    #[test]
    fn test_summarize_bash_result() {
        let masker = ObservationMasker::new();
        let bash_output =
            "total 42\ndrwxr-xr-x 3 user user 4096\n-rw-r--r-- 1 user user 128\nexit code: 0";
        let summary = masker.summarize_tool_result("Bash", bash_output);
        assert!(
            summary.starts_with("[Bash]"),
            "Bash summary must start with [Bash]: {summary}"
        );
        assert!(
            summary.contains("exit 0"),
            "should include exit code: {summary}"
        );
        assert!(summary.contains("4 lines"), "should count lines: {summary}");
    }

    #[test]
    fn test_summarize_generic_tool() {
        let masker = ObservationMasker::new();
        let output = "file1.rs\nfile2.rs\nfile3.rs";
        let summary = masker.summarize_tool_result("Glob", output);
        assert!(
            summary.starts_with("[Glob]"),
            "Generic summary must start with tool name: {summary}"
        );
        assert!(summary.contains("3 lines"), "should count lines: {summary}");
    }

    #[test]
    fn test_default_strategies_present() {
        let masker = ObservationMasker::new();
        assert!(masker.strategies.contains_key("read"));
        assert!(masker.strategies.contains_key("grep"));
        assert!(masker.strategies.contains_key("bash"));
        assert!(masker.strategies.contains_key("write"));
        assert!(masker.strategies.contains_key("edit"));
        assert!(masker.strategies.contains_key("glob"));
    }

    #[test]
    fn test_custom_strategy() {
        let mut masker = ObservationMasker::new();
        masker.set_strategy("read", MaskingStrategy::HeadTail { head: 3, tail: 2 });
        assert_eq!(
            masker.strategies.get("read"),
            Some(&MaskingStrategy::HeadTail { head: 3, tail: 2 })
        );
    }

    #[test]
    fn test_head_tail_strategy_applied() {
        let mut masker = ObservationMasker::new();
        masker.set_strategy("read", MaskingStrategy::HeadTail { head: 2, tail: 1 });

        let result = "line1\nline2\nline3\nline4\nline5\nline6";
        let summary = masker.summarize_tool_result("Read", result);
        assert!(
            summary.contains("line1"),
            "HeadTail should include first lines: {summary}"
        );
        assert!(
            summary.contains("line2"),
            "HeadTail should include head lines: {summary}"
        );
        assert!(
            summary.contains("line6"),
            "HeadTail should include last lines: {summary}"
        );
        assert!(
            summary.contains("elided"),
            "HeadTail should mention elided lines: {summary}"
        );
        assert!(
            !summary.contains("line4"),
            "HeadTail should not include middle lines: {summary}"
        );
    }

    #[test]
    fn test_preserve_strategy_returns_full_content() {
        let mut masker = ObservationMasker::new();
        masker.set_strategy("read", MaskingStrategy::Preserve);

        let result = "line1\nline2\nline3";
        let summary = masker.summarize_tool_result("Read", result);
        assert_eq!(summary, result, "Preserve should return full content");
    }

    #[test]
    fn test_head_tail_no_elision_when_short() {
        let mut masker = ObservationMasker::new();
        masker.set_strategy("bash", MaskingStrategy::HeadTail { head: 5, tail: 5 });

        let result = "line1\nline2\nline3";
        let summary = masker.summarize_tool_result("Bash", result);
        // 3 lines < head(5) + tail(5), so nothing to elide — return full.
        assert_eq!(summary, result, "Short content should not be elided");
    }

    #[test]
    fn test_empty_context() {
        let masker = ObservationMasker::with_threshold(0);
        let (masked, stats) = masker.mask_observations("");
        assert!(masked.is_empty() || masked.trim().is_empty());
        assert_eq!(stats.original_tokens, 0);
        assert_eq!(stats.blocks_masked, 0);
    }

    #[test]
    fn test_context_with_no_tool_results() {
        let masker = ObservationMasker::with_threshold(5);
        let context = "system: instructions\nuser: question\nassistant: answer";
        let (masked, stats) = masker.mask_observations(context);
        assert_eq!(stats.blocks_masked, 0, "no tool results to mask");
        // Content should be preserved (may have trailing newline differences).
        assert!(masked.contains("system: instructions"));
        assert!(masked.contains("user: question"));
        assert!(masked.contains("assistant: answer"));
    }

    #[test]
    fn test_detect_tool_name_from_arrow() {
        assert_eq!(detect_tool_name("→ Read src/main.rs"), "Read");
        assert_eq!(detect_tool_name("→ Grep pattern"), "Grep");
        assert_eq!(detect_tool_name("→ Bash ls -la"), "Bash");
    }

    #[test]
    fn test_detect_tool_name_from_result() {
        assert_eq!(detect_tool_name("tool_result: Read output"), "Read");
        assert_eq!(detect_tool_name("ToolResult: Grep matches"), "Grep");
    }

    #[test]
    fn test_masking_stats_accuracy() {
        let masker = ObservationMasker::with_threshold(5);
        let context = "user: check this\n\
                        tool_result: Bash output\n\
                        line 1 of output\n\
                        line 2 of output\n\
                        line 3 of output\n\
                        line 4 of output\n\
                        line 5 of output\n\
                        assistant: done";

        let (masked, stats) = masker.mask_observations(context);
        assert_eq!(stats.original_tokens, context.len() / CHARS_PER_TOKEN);
        assert_eq!(stats.masked_tokens, masked.len() / CHARS_PER_TOKEN);
        assert!(stats.masked_tokens <= stats.original_tokens);
    }

    #[test]
    fn test_grep_zero_matches() {
        let masker = ObservationMasker::new();
        let grep_output = "No matches found.";
        let summary = masker.summarize_tool_result("Grep", grep_output);
        assert!(
            summary.contains("0 matches"),
            "should report 0 matches: {summary}"
        );
    }

    #[test]
    fn test_bash_no_exit_code() {
        let masker = ObservationMasker::new();
        let bash_output = "Hello world\nSome output";
        let summary = masker.summarize_tool_result("Bash", bash_output);
        assert!(
            summary.starts_with("[Bash]"),
            "should still produce a Bash summary: {summary}"
        );
        assert!(summary.contains("2 lines"), "should count lines: {summary}");
        // Should NOT contain "exit" since no exit code was found.
        assert!(
            !summary.contains("exit"),
            "should not mention exit code when absent: {summary}"
        );
    }
}
