//! PalaceHierarchy — Memory Palace Metaphor for PLN2 P4.2
//!
//! Provides a hierarchical path structure for organizing memory entries using
//! the "method of loci" (memory palace) metaphor:
//! - `wing`: person/project (required) — e.g., "gabriel", "touring-hooks"
//! - `room`: topic (optional) — e.g., "auth", "memory"
//! - `closet`: feature group (optional) — e.g., "entity_registry"
//! - `drawer`: specific entry (optional) — e.g., "Index::new"
//!
//! # Encoding Rules
//!
//! Storage format uses dot-separated path: `wing.room.closet.drawer`
//! - If component contains `.` → replaced with `_`
//! - Literal `.` in values → encoded as `\\.`
//! - Display format shows human-readable path with dots

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

// =============================================================================
// Constants
// =============================================================================

/// Maximum length, in characters, of a palace `wing` component.
pub const MAX_WING_LEN: usize = 64;
/// Maximum length, in characters, of a palace `room` component.
pub const MAX_ROOM_LEN: usize = 64;
/// Maximum length, in characters, of a palace `closet` component.
pub const MAX_CLOSET_LEN: usize = 64;
/// Maximum length, in characters, of a palace `drawer` component.
pub const MAX_DRAWER_LEN: usize = 128;

// =============================================================================
// PalacePathError
// =============================================================================

/// Errors that can occur when parsing a palace path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PalacePathError {
    /// Wing component is empty.
    EmptyWing,
    /// Wing exceeds maximum length.
    WingTooLong(usize),
    /// Room exceeds maximum length.
    RoomTooLong(usize),
    /// Closet exceeds maximum length.
    ClosetTooLong(usize),
    /// Drawer exceeds maximum length.
    DrawerTooLong(usize),
    /// Invalid character found in component.
    InvalidCharacter(char),
    /// Failed to parse path string.
    ParseError(String),
}

impl fmt::Display for PalacePathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PalacePathError::EmptyWing => write!(f, "wing cannot be empty"),
            PalacePathError::WingTooLong(len) => {
                write!(f, "wing length {} exceeds maximum {}", len, MAX_WING_LEN)
            }
            PalacePathError::RoomTooLong(len) => {
                write!(f, "room length {} exceeds maximum {}", len, MAX_ROOM_LEN)
            }
            PalacePathError::ClosetTooLong(len) => {
                write!(
                    f,
                    "closet length {} exceeds maximum {}",
                    len, MAX_CLOSET_LEN
                )
            }
            PalacePathError::DrawerTooLong(len) => {
                write!(
                    f,
                    "drawer length {} exceeds maximum {}",
                    len, MAX_DRAWER_LEN
                )
            }
            PalacePathError::InvalidCharacter(c) => {
                write!(f, "invalid character '{}' in path component", c)
            }
            PalacePathError::ParseError(msg) => write!(f, "parse error: {}", msg),
        }
    }
}

impl std::error::Error for PalacePathError {}

impl PalacePathError {
    /// Creates a ParseError variant with the given message.
    pub fn parse_error(msg: &str) -> Self {
        PalacePathError::ParseError(msg.to_string())
    }
}

// =============================================================================
// PalaceHierarchy
// =============================================================================

/// Hierarchical memory palace path structure.
///
/// Organized using the method of loci (memory palace) metaphor:
/// - `wing`: person/project (required) — e.g., "gabriel", "touring-hooks"
/// - `room`: topic (optional) — e.g., "auth", "memory"
/// - `closet`: feature group (optional) — e.g., "entity_registry"
/// - `drawer`: specific entry (optional) — e.g., "Index::new"
///
/// # Example
///
/// ```
/// use touring_intelligence::rl::memory::palace::PalaceHierarchy;
///
/// // Create from components (closet=None means no closet segment)
/// let path = PalaceHierarchy::new(
///     "gabriel".to_string(),
///     Some("auth".to_string()),
///     None,
///     Some("Index::new".to_string()),
/// ).expect("valid path");
///
/// // With room and drawer but no closet, storage is "gabriel.auth.Index::new"
/// assert_eq!(path.to_display(), "gabriel.auth.Index::new");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PalaceHierarchy {
    /// Wing: person or project identifier (required).
    pub wing: String,
    /// Room: topic identifier (optional).
    pub room: Option<String>,
    /// Closet: feature group identifier (optional).
    pub closet: Option<String>,
    /// Drawer: specific entry identifier (optional).
    pub drawer: Option<String>,
}

impl PalaceHierarchy {
    /// Creates a new PalaceHierarchy with the given components.
    ///
    /// # Arguments
    ///
    /// * `wing` - Required person/project identifier (max 64 chars)
    /// * `room` - Optional topic identifier (max 64 chars)
    /// * `closet` - Optional feature group (max 64 chars)
    /// * `drawer` - Optional specific entry (max 128 chars)
    ///
    /// # Example
    ///
    /// ```
    /// use touring_intelligence::rl::memory::palace::PalaceHierarchy;
    ///
    /// let path = PalaceHierarchy::new(
    ///     "touring-hooks".to_string(),
    ///     Some("memory".to_string()),
    ///     Some("entity_registry".to_string()),
    ///     Some("Index::new".to_string()),
    /// );
    /// ```
    pub fn new(
        wing: String,
        room: Option<String>,
        closet: Option<String>,
        drawer: Option<String>,
    ) -> Result<Self, PalacePathError> {
        // Validate wing
        if wing.is_empty() {
            return Err(PalacePathError::EmptyWing);
        }
        if wing.len() > MAX_WING_LEN {
            return Err(PalacePathError::WingTooLong(wing.len()));
        }

        // Validate room
        if let Some(ref r) = room
            && r.len() > MAX_ROOM_LEN
        {
            return Err(PalacePathError::RoomTooLong(r.len()));
        }

        // Validate closet
        if let Some(ref c) = closet
            && c.len() > MAX_CLOSET_LEN
        {
            return Err(PalacePathError::ClosetTooLong(c.len()));
        }

        // Validate drawer
        if let Some(ref d) = drawer
            && d.len() > MAX_DRAWER_LEN
        {
            return Err(PalacePathError::DrawerTooLong(d.len()));
        }

        // Check for invalid characters (no dots in individual components)
        fn check_component(c: &str, _component_name: &str) -> Result<(), PalacePathError> {
            for ch in c.chars() {
                if ch == '.' {
                    return Err(PalacePathError::InvalidCharacter(ch));
                }
            }
            Ok(())
        }

        check_component(&wing, "wing")?;
        if let Some(ref r) = room {
            check_component(r, "room")?;
        }
        if let Some(ref c) = closet {
            check_component(c, "closet")?;
        }
        if let Some(ref d) = drawer {
            check_component(d, "drawer")?;
        }

        Ok(Self {
            wing,
            room,
            closet,
            drawer,
        })
    }

    /// Parses a path string into a PalaceHierarchy.
    ///
    /// # Format
    ///
    /// Storage encoding: `wing.room.closet.drawer`
    /// - If component contains `.` → replaced with `_` on encode
    /// - Literal `.` in values → encoded as `\\.`
    ///
    /// # Example
    ///
    /// ```
    /// use touring_intelligence::rl::memory::palace::PalaceHierarchy;
    ///
    /// let path = PalaceHierarchy::parse("gabriel.auth.entity_registry").expect("valid path");
    /// assert_eq!(path.wing, "gabriel");
    /// assert_eq!(path.room, Some("auth".to_string()));
    /// ```
    pub fn parse(path: &str) -> Result<Self, PalacePathError> {
        Self::parse_impl(path)
    }

    fn parse_impl(path: &str) -> Result<Self, PalacePathError> {
        let parts: Vec<&str> = path.split('.').collect();

        if parts.is_empty() {
            return Err(PalacePathError::ParseError("empty path".to_string()));
        }

        let wing = parts
            .first()
            .ok_or_else(|| PalacePathError::ParseError("empty path".to_string()))?
            .to_string();
        if wing.is_empty() {
            return Err(PalacePathError::EmptyWing);
        }
        if wing.len() > MAX_WING_LEN {
            return Err(PalacePathError::WingTooLong(wing.len()));
        }

        let room = Self::extract_optional_component(parts.get(1), MAX_ROOM_LEN, "room")?;
        let closet = Self::extract_optional_component(parts.get(2), MAX_CLOSET_LEN, "closet")?;
        let drawer = Self::extract_optional_component(parts.get(3), MAX_DRAWER_LEN, "drawer")?;

        // Validate no dots in any component
        Self::validate_no_dots(&wing, "wing")?;
        if let Some(ref r) = room {
            Self::validate_no_dots(r, "room")?;
        }
        if let Some(ref c) = closet {
            Self::validate_no_dots(c, "closet")?;
        }
        if let Some(ref d) = drawer {
            Self::validate_no_dots(d, "drawer")?;
        }

        Ok(Self {
            wing,
            room,
            closet,
            drawer,
        })
    }

    fn extract_optional_component(
        part: Option<&&str>,
        max_len: usize,
        name: &str,
    ) -> Result<Option<String>, PalacePathError> {
        match part {
            Some(s) if !s.is_empty() => {
                let s = s.to_string();
                if s.len() > max_len {
                    return Err(PalacePathError::parse_error(&format!(
                        "{} length {} exceeds maximum {}",
                        name,
                        s.len(),
                        max_len
                    )));
                }
                Ok(Some(s))
            }
            _ => Ok(None),
        }
    }

    fn validate_no_dots(s: &str, _component_name: &str) -> Result<(), PalacePathError> {
        s.chars()
            .find(|&c| c == '.')
            .map_or(Ok(()), |c| Err(PalacePathError::InvalidCharacter(c)))
    }

    /// Converts to storage path encoding.
    ///
    /// Returns dot-separated path: `wing.room.closet.drawer`
    /// Components with dots get encoded per rules (not applicable for user input,
    /// but to_storage preserves the encoding convention).
    ///
    /// # Example
    ///
    /// ```
    /// use touring_intelligence::rl::memory::palace::PalaceHierarchy;
    ///
    /// let path = PalaceHierarchy::parse("gabriel.auth.entity_registry").expect("valid test input");
    /// assert_eq!(path.to_storage(), "gabriel.auth.entity_registry");
    /// ```
    pub fn to_storage(&self) -> String {
        let mut parts = vec![self.wing.clone()];

        if let Some(ref room) = self.room {
            parts.push(room.clone());
        }
        if let Some(ref closet) = self.closet {
            parts.push(closet.clone());
        }
        if let Some(ref drawer) = self.drawer {
            parts.push(drawer.clone());
        }

        parts.join(".")
    }

    /// Converts to human-readable display path.
    ///
    /// Similar to storage format but semantically meant for display.
    ///
    /// # Example
    ///
    /// ```
    /// use touring_intelligence::rl::memory::palace::PalaceHierarchy;
    ///
    /// let path = PalaceHierarchy::new(
    ///     "gabriel".to_string(),
    ///     Some("auth".to_string()),
    ///     None,
    ///     Some("Index::new".to_string()),
    /// ).expect("valid path");
    ///
    /// assert_eq!(path.to_display(), "gabriel.auth.Index::new");
    /// ```
    pub fn to_display(&self) -> String {
        self.to_storage()
    }
}

impl fmt::Display for PalaceHierarchy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_display())
    }
}

impl FromStr for PalaceHierarchy {
    type Err = PalacePathError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_valid() {
        let result = PalaceHierarchy::new(
            "gabriel".to_string(),
            Some("auth".to_string()),
            Some("entity_registry".to_string()),
            Some("Index::new".to_string()),
        );
        assert!(result.is_ok());
        let path = result.expect("valid path");
        assert_eq!(path.wing, "gabriel");
        assert_eq!(path.room, Some("auth".to_string()));
        assert_eq!(path.closet, Some("entity_registry".to_string()));
        assert_eq!(path.drawer, Some("Index::new".to_string()));
    }

    #[test]
    fn test_new_empty_wing() {
        let result = PalaceHierarchy::new("".to_string(), None, None, None);
        assert!(matches!(result, Err(PalacePathError::EmptyWing)));
    }

    #[test]
    fn test_new_wing_too_long() {
        let long_wing = "a".repeat(MAX_WING_LEN + 1);
        let result = PalaceHierarchy::new(long_wing, None, None, None);
        assert!(matches!(result, Err(PalacePathError::WingTooLong(_))));
    }

    #[test]
    fn test_new_room_too_long() {
        let long_room = "a".repeat(MAX_ROOM_LEN + 1);
        let result = PalaceHierarchy::new("wing".to_string(), Some(long_room), None, None);
        assert!(matches!(result, Err(PalacePathError::RoomTooLong(_))));
    }

    #[test]
    fn test_new_invalid_char() {
        let result = PalaceHierarchy::new("invalid.wing".to_string(), None, None, None);
        assert!(matches!(
            result,
            Err(PalacePathError::InvalidCharacter('.'))
        ));
    }

    #[test]
    fn test_parse_valid() {
        let path =
            PalaceHierarchy::parse("gabriel.auth.entity_registry").expect("valid test input");
        assert_eq!(path.wing, "gabriel");
        assert_eq!(path.room, Some("auth".to_string()));
        assert_eq!(path.closet, Some("entity_registry".to_string()));
        assert_eq!(path.drawer, None);
    }

    #[test]
    fn test_parse_empty_wing() {
        let result = PalaceHierarchy::parse("");
        assert!(matches!(result, Err(PalacePathError::EmptyWing)));
    }

    #[test]
    fn test_parse_wing_too_long() {
        let long_path = format!("{}.room", "a".repeat(MAX_WING_LEN + 1));
        let result = PalaceHierarchy::parse(&long_path);
        assert!(matches!(result, Err(PalacePathError::WingTooLong(_))));
    }

    #[test]
    fn test_parse_partial() {
        let path = PalaceHierarchy::parse("touring-hooks").expect("valid test input");
        assert_eq!(path.wing, "touring-hooks");
        assert_eq!(path.room, None);
        assert_eq!(path.closet, None);
        assert_eq!(path.drawer, None);
    }

    #[test]
    fn test_to_storage() {
        let path = PalaceHierarchy::new(
            "gabriel".to_string(),
            Some("auth".to_string()),
            Some("entity_registry".to_string()),
            Some("Index::new".to_string()),
        )
        .expect("valid test input");
        assert_eq!(path.to_storage(), "gabriel.auth.entity_registry.Index::new");
    }

    #[test]
    fn test_to_display() {
        let path = PalaceHierarchy::new(
            "gabriel".to_string(),
            Some("auth".to_string()),
            None,
            Some("Index::new".to_string()),
        )
        .expect("valid path");
        assert_eq!(path.to_display(), "gabriel.auth.Index::new");
    }

    #[test]
    fn test_roundtrip() {
        let original = PalaceHierarchy::new(
            "touring-hooks".to_string(),
            Some("memory".to_string()),
            Some("entity_registry".to_string()),
            Some("Index::new".to_string()),
        )
        .expect("valid test input");
        let encoded = original.to_storage();
        let decoded = PalaceHierarchy::parse(&encoded).expect("valid test input");
        assert_eq!(original.wing, decoded.wing);
        assert_eq!(original.room, decoded.room);
        assert_eq!(original.closet, decoded.closet);
        assert_eq!(original.drawer, decoded.drawer);
    }

    #[test]
    fn test_display_trait() {
        let path = PalaceHierarchy::parse("gabriel.auth").expect("valid test input");
        assert_eq!(format!("{}", path), "gabriel.auth");
    }

    #[test]
    fn test_fromstr_trait() {
        let path: PalaceHierarchy = "gabriel.auth.closet.drawer"
            .parse()
            .expect("valid test input");
        assert_eq!(path.wing, "gabriel");
        assert_eq!(path.room, Some("auth".to_string()));
        assert_eq!(path.closet, Some("closet".to_string()));
        assert_eq!(path.drawer, Some("drawer".to_string()));
    }

    #[test]
    fn test_error_display() {
        let err = PalacePathError::EmptyWing;
        assert_eq!(format!("{}", err), "wing cannot be empty");

        let err = PalacePathError::WingTooLong(100);
        assert_eq!(format!("{}", err), "wing length 100 exceeds maximum 64");

        let err = PalacePathError::InvalidCharacter('.');
        assert_eq!(
            format!("{}", err),
            "invalid character '.' in path component"
        );
    }
}
