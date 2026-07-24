//! EvidenceAdapter — bridging touring-simd Evidence to touring-cortex Evidence
//!
//! These are HOMONYMOUS types with different structures — they cannot be merged
//! but can be bridged via this adapter.
//!
//! - touring-simd: `Evidence { source_id, value, confidence, successes, total }` (simple struct)
//! - touring-cortex: `Evidence { TypedID, Confidence, Priority, timestamp, source, payload }` (rich typed)
//!
//! ## Conversion Strategy
//! - `source_id` → preserved in payload as `"source_id"` for traceability
//! - `value` → stored in payload as `"value"`
//! - `confidence` → converted to `Confidence::new()`
//! - `successes/total` → stored in payload as `"successes"` / `"total"` and `"success_rate"`
//! - `source` → formatted as `"simd-{source_id}"` for handler identification
//! - `priority` → derived from confidence (higher confidence = higher priority)

use crate::fascicles::evidence::{Confidence, Evidence, HandlerName, Priority};
use std::sync::Arc;

/// Error type for evidence adaptation failures.
#[derive(Debug, Clone, PartialEq)]
pub enum EvidenceAdaptError {
    /// Confidence value out of valid range after clamping.
    InvalidConfidence {
        /// The original out-of-range confidence value.
        original: f64,
        /// The value it was clamped into the valid range.
        clamped: f64,
    },
}

impl std::fmt::Display for EvidenceAdaptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfidence { original, clamped } => {
                write!(
                    f,
                    "confidence {} out of range, clamped to {}",
                    original, clamped
                )
            }
        }
    }
}

impl std::error::Error for EvidenceAdaptError {}

/// Adapter for converting touring-simd Evidence to touring-cortex Evidence.
///
/// This adapter is OPTIONAL for the fasciculus arqueado express route.
/// CortexDispatcher → CortexRuntime subscription (F1) is the primary route.
/// This adapter is only needed if FascicleDispatcher is also activated.
#[derive(Debug, Clone, Default)]
pub struct EvidenceAdapter;

impl EvidenceAdapter {
    /// Creates a new EvidenceAdapter.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Converts a touring-simd Evidence into a touring-cortex Evidence.
    ///
    /// # Mapping Details
    ///
    /// | touring-simd Field | touring-cortex Field | Transformation |
    /// |-------------------|----------------------|---------------|
    /// | `source_id` | `id` | `TypedID::new()` (new unique ID) |
    /// | `source_id` | `payload["source_id"]` | Preserved for traceability |
    /// | `value` | `payload["value"]` | Stored as JSON f64 |
    /// | `confidence` | `confidence` | `Confidence::new()` with clamping |
    /// | `successes` | `payload["successes"]` | Stored as JSON u32 |
    /// | `total` | `payload["total"]` | Stored as JSON u32 |
    /// | `successes/total` | `payload["success_rate"]` | Computed ratio |
    /// | — | `timestamp` | `Utc::now()` at conversion time |
    /// | `source_id` | `source` | `Arc::from(format!("simd-{}", source_id))` |
    /// | `confidence` | `priority` | Derived: `Priority::new((confidence * 255.0) as u8)` |
    ///
    /// # Example
    ///
    /// ```
    /// use touring_cortex::fascicles::evidence_adapter::EvidenceAdapter;
    /// use touring_simd::cortex::Evidence;
    ///
    /// let simd_evidence = Evidence {
    ///     source_id: 42,
    ///     value: 0.95,
    ///     confidence: 0.87,
    ///     successes: 17,
    ///     total: 20,
    /// };
    ///
    /// let adapter = EvidenceAdapter::new();
    /// let cortex_evidence = adapter.adapt(simd_evidence);
    /// ```
    #[inline]
    #[must_use]
    pub fn adapt(&self, simd_evidence: touring_simd::cortex::Evidence) -> Evidence {
        let source_id = simd_evidence.source_id;
        let confidence = simd_evidence.confidence;
        let total = simd_evidence.total;

        // Build handler name from source_id
        let source: HandlerName = Arc::from(format!("simd-{}", source_id));

        // Derive priority from confidence (scale 0.0-1.0 to 0-255)
        let priority = Priority::new((confidence * 255.0_f64).round() as u8);

        // Build payload with preserved SIMD evidence data
        let mut payload = rustc_hash::FxHashMap::default();
        payload.insert("source_id".to_string(), serde_json::json!(source_id));
        payload.insert("value".to_string(), serde_json::json!(simd_evidence.value));
        payload.insert(
            "successes".to_string(),
            serde_json::json!(simd_evidence.successes),
        );
        payload.insert("total".to_string(), serde_json::json!(total));
        // Store success rate for quick access
        let success_rate = if total > 0 {
            simd_evidence.successes as f64 / total as f64
        } else {
            0.0
        };
        payload.insert("success_rate".to_string(), serde_json::json!(success_rate));

        Evidence::new(Confidence::new(confidence), priority, source, payload)
    }

    /// Adapts a batch of touring-simd Evidence items.
    ///
    /// This is more efficient than calling `adapt` individually when
    /// processing multiple evidence items from the SIMD pipeline.
    #[inline]
    #[must_use]
    pub fn adapt_batch(
        &self,
        batch: impl IntoIterator<Item = touring_simd::cortex::Evidence>,
    ) -> Vec<Evidence> {
        batch.into_iter().map(|e| self.adapt(e)).collect()
    }
}

impl From<touring_simd::cortex::Evidence> for Evidence {
    /// Converts a touring-simd Evidence directly into a touring-cortex Evidence.
    ///
    /// Uses the default EvidenceAdapter for conversion.
    fn from(simd_evidence: touring_simd::cortex::Evidence) -> Self {
        EvidenceAdapter::new().adapt(simd_evidence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use touring_simd::cortex::Evidence as SimdEvidence;

    #[test]
    fn test_adapt_basic() {
        let simd_evidence = SimdEvidence {
            source_id: 42,
            value: 0.95,
            confidence: 0.87,
            successes: 17,
            total: 20,
        };

        let adapter = EvidenceAdapter::new();
        let cortex_evidence = adapter.adapt(simd_evidence);

        // Verify confidence is properly wrapped and clamped
        let conf: f64 = cortex_evidence.confidence.into();
        assert!((conf - 0.87).abs() < f64::EPSILON);

        // Verify priority derived from confidence
        let expected_priority: u8 = (0.87 * 255.0_f64).round() as u8;
        assert_eq!(cortex_evidence.priority.value(), expected_priority);

        // Verify source name
        assert_eq!(&*cortex_evidence.source, "simd-42");

        // Verify payload contents
        assert_eq!(
            cortex_evidence
                .payload
                .get("source_id")
                .and_then(|v| v.as_i64()),
            Some(42)
        );
        assert!(
            cortex_evidence
                .payload
                .get("value")
                .and_then(|v| v.as_f64())
                .is_some()
        );
        assert_eq!(
            cortex_evidence
                .payload
                .get("successes")
                .and_then(|v| v.as_u64()),
            Some(17)
        );
        assert_eq!(
            cortex_evidence
                .payload
                .get("total")
                .and_then(|v| v.as_u64()),
            Some(20)
        );
    }

    #[test]
    fn test_adapt_confidence_clamping() {
        // Test upper bound clamping
        let high_conf = SimdEvidence {
            source_id: 1,
            value: 1.0,
            confidence: 1.5, // Out of range
            successes: 10,
            total: 10,
        };
        let adapter = EvidenceAdapter::new();
        let result = adapter.adapt(high_conf);
        let conf: f64 = result.confidence.into();
        assert_eq!(conf, 1.0);

        // Test lower bound clamping
        let low_conf = SimdEvidence {
            source_id: 2,
            value: 0.0,
            confidence: -0.5, // Out of range
            successes: 0,
            total: 10,
        };
        let result = adapter.adapt(low_conf);
        let conf: f64 = result.confidence.into();
        assert_eq!(conf, 0.0);
    }

    #[test]
    fn test_adapt_zero_total() {
        let simd_evidence = SimdEvidence {
            source_id: 99,
            value: 0.5,
            confidence: 0.5,
            successes: 0,
            total: 0, // Edge case: no observations
        };

        let adapter = EvidenceAdapter::new();
        let cortex_evidence = adapter.adapt(simd_evidence);

        // success_rate should be 0.0 when total is 0
        assert_eq!(
            cortex_evidence
                .payload
                .get("success_rate")
                .and_then(|v| v.as_f64()),
            Some(0.0)
        );
    }

    #[test]
    fn test_from_trait() {
        let simd_evidence = SimdEvidence {
            source_id: 7,
            value: 0.123,
            confidence: 0.5,
            successes: 5,
            total: 10,
        };

        // Use From trait directly
        let cortex_evidence: Evidence = simd_evidence.into();

        assert_eq!(&*cortex_evidence.source, "simd-7");
        let conf: f64 = cortex_evidence.confidence.into();
        assert!((conf - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_adapt_batch() {
        let batch = vec![
            SimdEvidence {
                source_id: 1,
                value: 0.9,
                confidence: 0.8,
                successes: 8,
                total: 10,
            },
            SimdEvidence {
                source_id: 2,
                value: 0.7,
                confidence: 0.6,
                successes: 6,
                total: 10,
            },
        ];

        let adapter = EvidenceAdapter::new();
        let results = adapter.adapt_batch(batch);

        assert_eq!(results.len(), 2);
        assert_eq!(&*results[0].source, "simd-1");
        assert_eq!(&*results[1].source, "simd-2");
    }

    #[test]
    fn test_typed_id_unique() {
        let simd_evidence1 = SimdEvidence {
            source_id: 1,
            value: 0.5,
            confidence: 0.5,
            successes: 5,
            total: 10,
        };
        let simd_evidence2 = SimdEvidence {
            source_id: 2,
            value: 0.6,
            confidence: 0.6,
            successes: 6,
            total: 10,
        };

        let adapter = EvidenceAdapter::new();
        let cortex1 = adapter.adapt(simd_evidence1);
        let cortex2 = adapter.adapt(simd_evidence2);

        // Each adapted evidence gets a unique TypedID
        assert_ne!(cortex1.id.value(), cortex2.id.value());
    }
}
