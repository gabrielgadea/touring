use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Memory tier classification for the 5-tier RLM system.
///
/// Each tier has different persistence and retrieval characteristics:
/// - **Reflexive**: Immediate, sub-millisecond recall (L1 cache)
/// - **Working**: Current-task context (session-scoped)
/// - **Session**: Persists within a single Claude Code session
/// - **Project**: Persists across sessions for a given project
/// - **Core**: Permanent, cross-project knowledge
///
/// Tiers are ordered by persistence scope: `Reflexive < Working < Session < Project < Core`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryTier {
    /// L1 cache-like recall, sub-millisecond. Lost on process restart.
    Reflexive,
    /// Current-task working memory, session-scoped. Lost on session end.
    Working,
    /// Single Claude Code session, persisted to local SQLite. Lost on
    /// `claude --clear` or workspace archive.
    Session,
    /// Per-project, persisted across sessions for the same project path.
    Project,
    /// Permanent, cross-project knowledge. Stored in the global Touring
    /// memory DB; the source of truth for `touring memory recall`.
    Core,
}

impl fmt::Display for MemoryTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reflexive => write!(f, "reflexive"),
            Self::Working => write!(f, "working"),
            Self::Session => write!(f, "session"),
            Self::Project => write!(f, "project"),
            Self::Core => write!(f, "core"),
        }
    }
}

impl MemoryTier {
    /// All variants in persistence-scope order (`Reflexive` first,
    /// `Core` last). Useful for exhaustive iteration and
    /// roundtrip-test scaffolding.
    pub const ALL: [MemoryTier; 5] = [
        Self::Reflexive,
        Self::Working,
        Self::Session,
        Self::Project,
        Self::Core,
    ];

    /// Returns the numeric index (0-4) for state mapping.
    #[inline]
    #[must_use]
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    /// Whether this tier persists across sessions.
    #[inline]
    #[must_use]
    pub fn is_persistent(&self) -> bool {
        matches!(self, Self::Project | Self::Core)
    }
}

impl FromStr for MemoryTier {
    type Err = crate::TouringError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "reflexive" => Ok(Self::Reflexive),
            "working" => Ok(Self::Working),
            "session" => Ok(Self::Session),
            "project" => Ok(Self::Project),
            "core" => Ok(Self::Core),
            other => Err(crate::TouringError::Parse(format!(
                "unknown memory tier: '{other}'"
            ))),
        }
    }
}

/// CILA complexity level for intent routing (L0-L6).
///
/// Determines the routing strategy and tool augmentation level:
/// - **L0**: Direct response, no tools needed
/// - **L1**: PAL (Program-Aided Language), simple computation
/// - **L2**: Tool-augmented, single tool call
/// - **L3**: Pipeline execution (e.g., ANTT F1-F16)
/// - **L4**: Agent loops, iterative refinement
/// - **L5**: Self-modifying, meta-generation
/// - **L6**: Multi-agent teams (CILA L6)
///
/// Levels are ordered by complexity: `L0 < L1 < ... < L6`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CILALevel {
    /// Direct response, no tool augmentation. E.g. simple Q&A.
    L0,
    /// Program-Aided Language — solve by writing a small script the
    /// model can mentally execute. Single tool, deterministic.
    L1,
    /// Tool-augmented, single tool call. The model picks one
    /// capability and consumes its output directly.
    L2,
    /// Pipeline execution — multi-step deterministic flows (e.g.
    /// ANTT F1-F16 master plan waves). State must be persisted
    /// between steps.
    L3,
    /// Agent loop with iterative refinement. The model decides
    /// when to stop and may re-enter earlier pipeline steps.
    L4,
    /// Self-modifying or meta-generative — the agent rewrites its
    /// own scaffolding, plans, or tools at runtime.
    L5,
    /// Multi-agent teams. Independent agents collaborate via shared
    /// state and message passing. CILA L6+ ceiling.
    L6,
}

impl fmt::Display for CILALevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::L0 => write!(f, "L0"),
            Self::L1 => write!(f, "L1"),
            Self::L2 => write!(f, "L2"),
            Self::L3 => write!(f, "L3"),
            Self::L4 => write!(f, "L4"),
            Self::L5 => write!(f, "L5"),
            Self::L6 => write!(f, "L6"),
        }
    }
}

impl FromStr for CILALevel {
    type Err = crate::TouringError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "L0" => Ok(Self::L0),
            "L1" => Ok(Self::L1),
            "L2" => Ok(Self::L2),
            "L3" => Ok(Self::L3),
            "L4" => Ok(Self::L4),
            "L5" => Ok(Self::L5),
            "L6" => Ok(Self::L6),
            other => Err(crate::TouringError::Parse(format!(
                "unknown CILA level: '{other}'"
            ))),
        }
    }
}

impl CILALevel {
    /// All variants in complexity order (`L0` first, `L6` last).
    /// Length MUST equal the variant count; updated in lockstep
    /// with the enum definition.
    pub const ALL: [CILALevel; 7] = [
        Self::L0,
        Self::L1,
        Self::L2,
        Self::L3,
        Self::L4,
        Self::L5,
        Self::L6,
    ];

    /// Numeric value (0-6) for comparison and Q-table state mapping.
    #[inline]
    #[must_use]
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    /// Whether this level requires pipeline state verification.
    #[inline]
    #[must_use]
    pub fn requires_pipeline_state(&self) -> bool {
        matches!(self, Self::L3 | Self::L4 | Self::L5 | Self::L6)
    }

    /// Whether this level supports agent teams.
    #[inline]
    #[must_use]
    pub fn supports_agent_teams(&self) -> bool {
        matches!(self, Self::L6)
    }

    /// Whether this level requires tool augmentation.
    #[inline]
    #[must_use]
    pub fn requires_tools(&self) -> bool {
        !matches!(self, Self::L0)
    }
}

impl TryFrom<u8> for CILALevel {
    type Error = crate::TouringError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::L0),
            1 => Ok(Self::L1),
            2 => Ok(Self::L2),
            3 => Ok(Self::L3),
            4 => Ok(Self::L4),
            5 => Ok(Self::L5),
            6 => Ok(Self::L6),
            _ => Err(crate::TouringError::InvalidParameter {
                param: "CILALevel".to_string(),
                value: value.to_string(),
                reason: "must be 0-6".to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── MemoryTier tests ──────────────────────────────────────────────

    #[test]
    fn test_memory_tier_display_roundtrip() {
        for tier in MemoryTier::ALL {
            let s = tier.to_string();
            let parsed: MemoryTier = s.parse().unwrap();
            assert_eq!(tier, parsed);
        }
    }

    #[test]
    fn test_memory_tier_serde_roundtrip() {
        let tier = MemoryTier::Project;
        let json = serde_json::to_string(&tier).unwrap();
        assert_eq!(json, "\"project\"");
        let back: MemoryTier = serde_json::from_str(&json).unwrap();
        assert_eq!(back, tier);
    }

    #[test]
    fn test_memory_tier_fromstr_case_insensitive() {
        assert_eq!("CORE".parse::<MemoryTier>().unwrap(), MemoryTier::Core);
        assert_eq!(
            "Session".parse::<MemoryTier>().unwrap(),
            MemoryTier::Session
        );
    }

    #[test]
    fn test_memory_tier_fromstr_invalid() {
        assert!("unknown".parse::<MemoryTier>().is_err());
    }

    #[test]
    fn test_memory_tier_ordering() {
        assert!(MemoryTier::Reflexive < MemoryTier::Working);
        assert!(MemoryTier::Working < MemoryTier::Session);
        assert!(MemoryTier::Session < MemoryTier::Project);
        assert!(MemoryTier::Project < MemoryTier::Core);
    }

    #[test]
    fn test_memory_tier_persistence() {
        assert!(!MemoryTier::Reflexive.is_persistent());
        assert!(!MemoryTier::Working.is_persistent());
        assert!(!MemoryTier::Session.is_persistent());
        assert!(MemoryTier::Project.is_persistent());
        assert!(MemoryTier::Core.is_persistent());
    }

    #[test]
    fn test_memory_tier_as_u8() {
        for (i, tier) in MemoryTier::ALL.iter().enumerate() {
            assert_eq!(tier.as_u8(), i as u8);
        }
    }

    #[test]
    fn test_memory_tier_all_exhaustive() {
        assert_eq!(MemoryTier::ALL.len(), 5);
    }

    // ── CILALevel tests ──────────────────────────────────────────────

    #[test]
    fn test_cila_display_roundtrip() {
        for level in CILALevel::ALL {
            let s = level.to_string();
            let parsed: CILALevel = s.parse().unwrap();
            assert_eq!(level, parsed);
        }
    }

    #[test]
    fn test_cila_fromstr_case_insensitive() {
        assert_eq!("l0".parse::<CILALevel>().unwrap(), CILALevel::L0);
        assert_eq!("L6".parse::<CILALevel>().unwrap(), CILALevel::L6);
    }

    #[test]
    fn test_cila_as_u8() {
        assert_eq!(CILALevel::L0.as_u8(), 0);
        assert_eq!(CILALevel::L6.as_u8(), 6);
    }

    #[test]
    fn test_cila_as_u8_all_sequential() {
        for (i, level) in CILALevel::ALL.iter().enumerate() {
            assert_eq!(level.as_u8(), i as u8);
        }
    }

    #[test]
    fn test_cila_try_from_u8_roundtrip() {
        for level in CILALevel::ALL {
            let n = level.as_u8();
            let back = CILALevel::try_from(n).unwrap();
            assert_eq!(level, back);
        }
    }

    #[test]
    fn test_cila_try_from_u8_invalid() {
        assert!(CILALevel::try_from(7).is_err());
        assert!(CILALevel::try_from(255).is_err());
    }

    #[test]
    fn test_cila_requires_pipeline_state() {
        assert!(!CILALevel::L0.requires_pipeline_state());
        assert!(!CILALevel::L2.requires_pipeline_state());
        assert!(CILALevel::L3.requires_pipeline_state());
        assert!(CILALevel::L6.requires_pipeline_state());
    }

    #[test]
    fn test_cila_supports_agent_teams() {
        assert!(!CILALevel::L5.supports_agent_teams());
        assert!(CILALevel::L6.supports_agent_teams());
    }

    #[test]
    fn test_cila_requires_tools() {
        assert!(!CILALevel::L0.requires_tools());
        assert!(CILALevel::L1.requires_tools());
        assert!(CILALevel::L6.requires_tools());
    }

    #[test]
    fn test_cila_ordering() {
        assert!(CILALevel::L0 < CILALevel::L1);
        assert!(CILALevel::L3 < CILALevel::L6);
        assert!(CILALevel::L5 > CILALevel::L2);
    }

    #[test]
    fn test_cila_serde_roundtrip() {
        let level = CILALevel::L3;
        let json = serde_json::to_string(&level).unwrap();
        let back: CILALevel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, level);
    }

    #[test]
    fn test_cila_fromstr_invalid() {
        assert!("L7".parse::<CILALevel>().is_err());
        assert!("foo".parse::<CILALevel>().is_err());
    }

    #[test]
    fn test_cila_all_exhaustive() {
        assert_eq!(CILALevel::ALL.len(), 7);
    }
}

// ── TodoKind enum ──────────────────────────────────────────────────────────────

/// Kind of TODO comment detected in source code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TodoKind {
    /// Generic `TODO` marker — work to be done, no urgency.
    Todo,
    /// `FIXME` — known broken or incorrect; should be fixed before
    /// the next release.
    Fixme,
    /// `XXX` — fragile code that needs a second look.
    Xxx,
    /// `HACK` — workaround that should eventually be replaced with
    /// a proper solution.
    Hack,
    /// `NOTE` — informational comment, not actionable.
    Note,
    /// `DEPRECATED` — symbol or block scheduled for removal.
    Deprecated,
}

impl fmt::Display for TodoKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Todo => write!(f, "TODO"),
            Self::Fixme => write!(f, "FIXME"),
            Self::Xxx => write!(f, "XXX"),
            Self::Hack => write!(f, "HACK"),
            Self::Note => write!(f, "NOTE"),
            Self::Deprecated => write!(f, "DEPRECATED"),
        }
    }
}

impl TodoKind {
    /// All variants in declared order. Used by
    /// `touring ast scan-debt` to walk every marker type in one pass.
    pub const ALL: [TodoKind; 6] = [
        Self::Todo,
        Self::Fixme,
        Self::Xxx,
        Self::Hack,
        Self::Note,
        Self::Deprecated,
    ];
    /// Parses a keyword string into a [`TodoKind`].
    ///
    /// Returns `None` if the string does not match any known keyword.
    /// (Named `parse` rather than `from_str` to avoid confusion with
    /// `std::str::FromStr`, which requires a `Result` return type.)
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "todo" => Some(Self::Todo),
            "fixme" => Some(Self::Fixme),
            "xxx" => Some(Self::Xxx),
            "hack" => Some(Self::Hack),
            "note" => Some(Self::Note),
            "deprecated" => Some(Self::Deprecated),
            _ => None,
        }
    }
}

// ── EdgeConfidence enum ────────────────────────────────────────────────────────

/// Confidence level for inferred file relationships.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeConfidence {
    /// Strong evidence for the relationship (e.g. explicit import
    /// observed in compiled crate).
    High,
    /// Inferred from co-occurrence statistics in the index.
    Medium,
    /// Single weak signal (e.g. shared substring in symbol names).
    Low,
    /// Confidence could not be determined; placeholder for lazy
    /// evaluation of the relationship.
    Unknown,
}

impl fmt::Display for EdgeConfidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::High => write!(f, "high"),
            Self::Medium => write!(f, "medium"),
            Self::Low => write!(f, "low"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl EdgeConfidence {
    /// All variants in declared order (`High`, `Medium`, `Low`,
    /// `Unknown`).
    pub const ALL: [EdgeConfidence; 4] = [Self::High, Self::Medium, Self::Low, Self::Unknown];
    /// Numeric value (0-3) — the order is the declaration order.
    #[inline]
    #[must_use]
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
    /// Inverse of [`Self::as_u8`]. Returns `None` for values
    /// outside the variant range (i.e. anything but 0-3).
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::High),
            1 => Some(Self::Medium),
            2 => Some(Self::Low),
            3 => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// Truncate a UTF-8 string to at most `max_bytes` bytes without splitting a multi-byte character.
///
/// # Examples
/// ```
/// use touring_foundation::types::truncate_str;
/// assert_eq!(truncate_str("hello", 3), "hel");
/// // em-dash '—' is 3 bytes (0xE2 0x80 0x94)
/// assert_eq!(truncate_str("ab—cd", 4), "ab");   // can't fit partial '—'
/// assert_eq!(truncate_str("ab—cd", 5), "ab—");  // fits exactly
/// assert_eq!(truncate_str("short", 100), "short"); // no-op if under limit
/// ```
#[inline]
#[must_use]
pub fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests_truncate {
    use super::truncate_str;

    #[test]
    fn ascii_only() {
        assert_eq!(truncate_str("hello world", 5), "hello");
    }

    #[test]
    fn multibyte_boundary() {
        // '—' = 3 bytes (E2 80 94), starts at byte 2
        let s = "ab—cd";
        assert_eq!(truncate_str(s, 2), "ab");
        assert_eq!(truncate_str(s, 3), "ab"); // mid-char → back up
        assert_eq!(truncate_str(s, 4), "ab"); // still mid-char
        assert_eq!(truncate_str(s, 5), "ab—"); // exact end of '—'
    }

    #[test]
    fn under_limit() {
        assert_eq!(truncate_str("short", 100), "short");
    }

    #[test]
    fn empty() {
        assert_eq!(truncate_str("", 10), "");
    }

    #[test]
    fn emoji() {
        // '🦀' = 4 bytes
        let s = "a🦀b";
        assert_eq!(truncate_str(s, 1), "a");
        assert_eq!(truncate_str(s, 2), "a"); // mid emoji
        assert_eq!(truncate_str(s, 4), "a"); // still mid
        assert_eq!(truncate_str(s, 5), "a🦀"); // full emoji
    }

    // ── CILALevel tests (W5 wave 2026-06-04) ──────────────────────────────
}

#[cfg(test)]
mod tests_more {
    use super::*;

    #[test]
    fn cila_level_ordering() {
        // L0 < L1 < ... < L6
        assert!(CILALevel::L0 < CILALevel::L1);
        assert!(CILALevel::L1 < CILALevel::L2);
        assert!(CILALevel::L2 < CILALevel::L3);
        assert!(CILALevel::L3 < CILALevel::L4);
        assert!(CILALevel::L4 < CILALevel::L5);
        assert!(CILALevel::L5 < CILALevel::L6);
    }

    #[test]
    fn cila_level_serde_snake_case() {
        let json = serde_json::to_string(&CILALevel::L3).unwrap();
        // L3 is the variant name; serde with default uses it as-is.
        assert!(json.contains("L3"));
        let back: CILALevel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, CILALevel::L3);
    }

    // ── TodoKind tests (W5 wave 2026-06-04) ───────────────────────────────

    #[test]
    fn todo_kind_display_all_caps() {
        assert_eq!(TodoKind::Todo.to_string(), "TODO");
        assert_eq!(TodoKind::Fixme.to_string(), "FIXME");
        assert_eq!(TodoKind::Xxx.to_string(), "XXX");
        assert_eq!(TodoKind::Hack.to_string(), "HACK");
        assert_eq!(TodoKind::Note.to_string(), "NOTE");
        assert_eq!(TodoKind::Deprecated.to_string(), "DEPRECATED");
    }

    #[test]
    fn todo_kind_serde_lowercase() {
        // The enum is #[serde(rename_all = "lowercase")].
        let json = serde_json::to_string(&TodoKind::Fixme).unwrap();
        assert_eq!(json, "\"fixme\"");
        let back: TodoKind = serde_json::from_str("\"hack\"").unwrap();
        assert_eq!(back, TodoKind::Hack);
    }

    // ── EdgeConfidence tests (W5 wave 2026-06-04) ─────────────────────────

    #[test]
    fn edge_confidence_display_matches_serde() {
        // The Display impl and serde lowercase must agree.
        for ec in [
            EdgeConfidence::High,
            EdgeConfidence::Medium,
            EdgeConfidence::Low,
            EdgeConfidence::Unknown,
        ] {
            let s = ec.to_string();
            let json = serde_json::to_string(&ec).unwrap();
            // serde produces "high" / "medium" / "low" / "unknown".
            assert_eq!(json, format!("\"{s}\""), "Display != serde for {ec:?}");
        }
    }

    #[test]
    fn edge_confidence_serde_roundtrip() {
        for ec in [
            EdgeConfidence::High,
            EdgeConfidence::Medium,
            EdgeConfidence::Low,
            EdgeConfidence::Unknown,
        ] {
            let json = serde_json::to_string(&ec).unwrap();
            let back: EdgeConfidence = serde_json::from_str(&json).unwrap();
            assert_eq!(back, ec);
        }
    }

    #[test]
    fn edge_confidence_equality() {
        // Copy + PartialEq + Eq + Hash should make these trivially equal.
        assert_eq!(EdgeConfidence::High, EdgeConfidence::High);
        assert_ne!(EdgeConfidence::High, EdgeConfidence::Low);
        // Hash equality via HashMap insertion.
        use std::collections::HashSet;
        let mut set: HashSet<EdgeConfidence> = HashSet::new();
        set.insert(EdgeConfidence::Medium);
        assert!(set.contains(&EdgeConfidence::Medium));
        assert!(!set.contains(&EdgeConfidence::Low));
    }
}
