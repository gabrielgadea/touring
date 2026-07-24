//! Extended file knowledge query for salsa.

use crate::salsa::ExtendedKnowledge;
use crate::salsa::FileKey;
use crate::salsa::db::DatabaseImpl;

/// Compute extended knowledge metadata for a file.
///
/// This is NOT a #[salsa::tracked] function. Real implementation would
/// aggregate complexity_score, cognitive_score, fan_out/fan_in from
/// touring-analysis and touring-wiring.
pub fn file_knowledge_extended(db: &DatabaseImpl, file_key: FileKey) -> ExtendedKnowledge {
    let _ = db;
    ExtendedKnowledge {
        file_id: file_key,
        complexity_score: 0.0,
        cognitive_score: 0.0,
        fan_out: 0,
        fan_in: 0,
        modularity_score: 0.0,
        community_id: 0,
        is_orphan: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_knowledge_default() {
        let ek = ExtendedKnowledge::default();
        assert_eq!(ek.file_id, FileKey::default());
        assert_eq!(ek.complexity_score, 0.0);
    }

    #[test]
    fn file_knowledge_extended_unknown_file() {
        let db = DatabaseImpl::new();
        let ek = file_knowledge_extended(&db, FileKey::new(123));
        assert_eq!(ek.file_id, FileKey::new(123));
        assert!(!ek.is_orphan);
    }
}
