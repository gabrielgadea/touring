//! Public data types.

use serde::{Deserialize, Serialize};

/// Placeholder type — replace with the real domain payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Item {
    /// Stable identifier.
    pub id: String,
    /// Human-readable label.
    pub label: String,
}

impl Item {
    /// Construct a new [`Item`].
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_new_round_trip() {
        let item = Item::new("k1", "first");
        assert_eq!(item.id, "k1");
        assert_eq!(item.label, "first");
    }
}
