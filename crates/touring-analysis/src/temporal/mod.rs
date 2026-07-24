//! Temporal trend analysis (feature-gated: "temporal").
//!
//! Analyzes code quality and activity trends over time windows.

mod trends;

pub use trends::{
    TrendDirection, TrendReport, analyze_trends, detect_churn_patterns, quality_trend,
};
