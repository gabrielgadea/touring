//! Tier-based language support definitions.
//!
//! ## Tier Hierarchy
//!
//! | Tier | Languages | Capabilities |
//! |------|------------|---------------|
//! | 1 | Rust, TypeScript | Full AST, symbols, quality, wiring, cognitive |
//! | 2 | Python, Go, C | AST, symbols, quality; partial wiring |
//! | 3 | Kotlin, Swift, Java | AST, partial symbols |
//! | 4 | Ruby, PHP | Basic tokens only |

use serde::{Deserialize, Serialize};

/// Tier level — reflects capability maturity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// Tier 1 — Complete support: full AST, symbols, quality, wiring, cognitive.
    Tier1,
    /// Tier 2 — Most features: AST, symbols, quality; partial wiring.
    Tier2,
    /// Tier 3 — Experimental: AST, partial symbols.
    Tier3,
    /// Tier 4 — Limited: basic tokens only.
    Tier4,
}

impl Tier {
    /// Numeric value used for serialization.
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Tier1 => 1,
            Self::Tier2 => 2,
            Self::Tier3 => 3,
            Self::Tier4 => 4,
        }
    }

    /// Human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Tier1 => "Complete",
            Self::Tier2 => "Most features",
            Self::Tier3 => "Experimental",
            Self::Tier4 => "Limited",
        }
    }

    /// Description of what this tier guarantees.
    pub fn description(self) -> &'static str {
        match self {
            Self::Tier1 => "Full AST, symbols, quality, wiring, cognitive",
            Self::Tier2 => "AST, symbols, quality; partial wiring",
            Self::Tier3 => "AST, partial symbols",
            Self::Tier4 => "Basic tokens only",
        }
    }
}

impl Serialize for Tier {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(self.as_u8())
    }
}

impl<'de> Deserialize<'de> for Tier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let n = u8::deserialize(deserializer)?;
        match n {
            1 => Ok(Self::Tier1),
            2 => Ok(Self::Tier2),
            3 => Ok(Self::Tier3),
            4 => Ok(Self::Tier4),
            _ => Err(serde::de::Error::custom(format!("invalid tier: {n}"))),
        }
    }
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Tier{}", self.as_u8())
    }
}

/// Language identifier — intentionally limited to languages Touring can reason about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Rust — Tier 1.
    Rust,
    /// TypeScript — Tier 1.
    TypeScript,
    /// Python — Tier 2.
    Python,
    /// Go — Tier 2.
    Go,
    /// C — Tier 2.
    C,
    /// Kotlin — Tier 3.
    Kotlin,
    /// Swift — Tier 3.
    Swift,
    /// Java — Tier 3.
    Java,
    /// Ruby — Tier 4.
    Ruby,
    /// PHP — Tier 4.
    Php,
}

impl Language {
    /// Return the canonical tier for this language.
    pub fn tier(self) -> Tier {
        match self {
            Self::Rust | Self::TypeScript => Tier::Tier1,
            Self::Python | Self::Go | Self::C => Tier::Tier2,
            Self::Kotlin | Self::Swift | Self::Java => Tier::Tier3,
            Self::Ruby | Self::Php => Tier::Tier4,
        }
    }

    /// All languages in Tier 1.
    pub const fn tier1() -> &'static [Self] {
        &[Self::Rust, Self::TypeScript]
    }

    /// All languages in Tier 2.
    pub const fn tier2() -> &'static [Self] {
        &[Self::Python, Self::Go, Self::C]
    }

    /// All languages in Tier 3.
    pub const fn tier3() -> &'static [Self] {
        &[Self::Kotlin, Self::Swift, Self::Java]
    }

    /// All languages in Tier 4.
    pub const fn tier4() -> &'static [Self] {
        &[Self::Ruby, Self::Php]
    }

    /// All supported languages in display order.
    pub const fn all() -> &'static [Self] {
        &[
            Self::Rust,
            Self::TypeScript,
            Self::Python,
            Self::Go,
            Self::C,
            Self::Kotlin,
            Self::Swift,
            Self::Java,
            Self::Ruby,
            Self::Php,
        ]
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::Python => "python",
            Self::Go => "go",
            Self::C => "c",
            Self::Kotlin => "kotlin",
            Self::Swift => "swift",
            Self::Java => "java",
            Self::Ruby => "ruby",
            Self::Php => "php",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for Language {
    type Err = super::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rust" => Ok(Self::Rust),
            "typescript" | "ts" => Ok(Self::TypeScript),
            "python" | "py" => Ok(Self::Python),
            "go" | "golang" => Ok(Self::Go),
            "c" => Ok(Self::C),
            "kotlin" | "kt" => Ok(Self::Kotlin),
            "swift" => Ok(Self::Swift),
            "java" => Ok(Self::Java),
            "ruby" | "rb" => Ok(Self::Ruby),
            "php" => Ok(Self::Php),
            _ => Err(super::Error::invariant(format!("unknown language: {s}"))),
        }
    }
}
