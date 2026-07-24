//! C7 (coupling backlog) — RGAO task routing: map a 5-component task-scope
//! vector `c = (d, n_f, n_s, h, ρ)` to a CILA level + routing topology +
//! recommended TACO phases. Replaces the trivial `u8 → CilaLevel` mapping
//! ([`super::decomposer::CilaLevel::from_u8`]) with a computed heuristic over
//! real code metrics — blast depth, files, symbols, cognitive entropy, coupling
//! density — gathered from `ast blast` / `wiring` / `index`.
//!
//! Pure and unit-testable: the orchestrator (or `touring route`) computes the
//! vector from a task's scope and calls [`route`]; the routing/parallelism are
//! then reused from the existing [`CilaLevel`] methods, so this module owns only
//! the *vector → level* heuristic, not a parallel taxonomy.

use serde::Serialize;

use super::decomposer::{CilaLevel, RoutingMode};

/// A task-scope feature vector `c = (d, n_f, n_s, h, ρ)`. Raw and
/// un-normalized: `depth`/`files`/`symbols` are counts; `cognitive`/`coupling`
/// are already expected in `[0, 1]` (clamped on use).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RouteVector {
    /// `d` — blast/dependency depth (e.g. `BlastRadiusResult::max_depth`).
    pub depth: usize,
    /// `n_f` — number of files in scope.
    pub files: usize,
    /// `n_s` — number of symbols in scope.
    pub symbols: usize,
    /// `h` — cognitive entropy in `[0, 1]` (e.g. `cognitive_score`).
    pub cognitive: f64,
    /// `ρ` — coupling density in `[0, 1]` (e.g. normalized `fan_in + fan_out`).
    pub coupling: f64,
}

/// Saturation cap for blast depth — the value at which depth is maximally
/// complex (normalized to 1.0). From the file-metadata-first threshold
/// (`blast_radius > 10` is "high").
const DEPTH_CAP: f64 = 10.0;
/// Saturation cap for files in scope.
const FILES_CAP: f64 = 20.0;
/// Saturation cap for symbols in scope.
const SYMBOLS_CAP: f64 = 50.0;

/// Component weights (sum = 1.0). Depth and coupling dominate because blast
/// radius and fan density drive orchestration risk; symbol count is the weakest
/// signal.
const W_DEPTH: f64 = 0.30;
const W_COUPLING: f64 = 0.25;
const W_COGNITIVE: f64 = 0.20;
const W_FILES: f64 = 0.15;
const W_SYMBOLS: f64 = 0.10;

/// The result of routing a [`RouteVector`]. Serialize-only — `phases` holds
/// `&'static str` labels (not deserializable), and nothing reads a `RouteResult`
/// back from JSON.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RouteResult {
    /// Computed CILA level (L0–L6).
    pub level: CilaLevel,
    /// Routing topology derived from the level.
    pub routing_mode: RoutingMode,
    /// Recommended max parallel subtasks for the level.
    pub max_parallelism: usize,
    /// The composite complexity score in `[0, 1]` that produced the level.
    pub composite: f64,
    /// Recommended TACO phases for the level.
    pub phases: Vec<&'static str>,
}

/// Normalize a count against a cap to `[0, 1]` (saturating).
fn norm(value: usize, cap: f64) -> f64 {
    (value as f64 / cap).clamp(0.0, 1.0)
}

/// The composite complexity score in `[0, 1]` for a vector — the weighted sum of
/// its normalized components. Pure; split from the level banding so the scoring
/// is testable on its own. Out-of-range `cognitive`/`coupling` are clamped, so
/// the result is always in `[0, 1]`.
pub fn composite_score(v: &RouteVector) -> f64 {
    W_DEPTH * norm(v.depth, DEPTH_CAP)
        + W_FILES * norm(v.files, FILES_CAP)
        + W_SYMBOLS * norm(v.symbols, SYMBOLS_CAP)
        + W_COGNITIVE * v.cognitive.clamp(0.0, 1.0)
        + W_COUPLING * v.coupling.clamp(0.0, 1.0)
}

/// Recommended TACO phases for a CILA level — mirrors the phase protocol:
/// L0–L1 solo, L2 scout→engineer, L3 adds architect + audit, L4+ all phases.
fn phases_for(level: CilaLevel) -> Vec<&'static str> {
    match level {
        CilaLevel::L0 | CilaLevel::L1 => vec!["solo"],
        CilaLevel::L2 => vec!["P1:scout", "P5:engineer"],
        CilaLevel::L3 => vec!["P1:scout", "P2:architect", "P5:engineers", "P6:audit"],
        CilaLevel::L4 | CilaLevel::L5 | CilaLevel::L6 => vec![
            "P0:health",
            "P1:scout",
            "P2:architect",
            "P3:context7",
            "P4:decompose",
            "P4.5:pre-audit",
            "P5:engineers",
            "P6:post-audit",
            "P7:docs",
        ],
    }
}

/// Route a task-scope vector to a CILA level + topology + phases. Maps the
/// composite score in `[0, 1]` onto the 7 CILA bands via
/// [`CilaLevel::from_u8`], then reuses the existing [`CilaLevel`] routing and
/// parallelism methods. Deterministic and pure.
pub fn route(v: &RouteVector) -> RouteResult {
    let composite = composite_score(v);
    // Map [0,1] → 0..=6: floor(composite * 7), capped at 6 (only composite == 1.0
    // reaches 7). Even bands — the component weighting already shapes the curve.
    let band = (composite * 7.0).floor().min(6.0) as u8;
    let level = CilaLevel::from_u8(band);
    RouteResult {
        level,
        routing_mode: level.routing_mode(),
        max_parallelism: level.max_parallelism(),
        composite,
        phases: phases_for(level),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trivial_vector_routes_to_solo() {
        let v = RouteVector {
            depth: 0,
            files: 1,
            symbols: 1,
            cognitive: 0.0,
            coupling: 0.0,
        };
        let r = route(&v);
        assert!(matches!(r.level, CilaLevel::L0 | CilaLevel::L1));
        assert_eq!(r.routing_mode, RoutingMode::Solo);
        assert_eq!(r.phases, vec!["solo"]);
        assert!(r.composite < 0.15, "composite was {}", r.composite);
    }

    #[test]
    fn maximal_vector_routes_to_full_taco() {
        let v = RouteVector {
            depth: 50,
            files: 100,
            symbols: 500,
            cognitive: 1.0,
            coupling: 1.0,
        };
        let r = route(&v);
        assert_eq!(r.level, CilaLevel::L6);
        assert_eq!(r.routing_mode, RoutingMode::FullTaco);
        assert!((r.composite - 1.0).abs() < 1e-9);
        assert!(r.phases.contains(&"P0:health"));
    }

    #[test]
    fn composite_is_monotonic_in_depth() {
        let base = |d: usize| RouteVector {
            depth: d,
            files: 1,
            symbols: 1,
            cognitive: 0.1,
            coupling: 0.1,
        };
        assert!(
            composite_score(&base(9)) > composite_score(&base(1)),
            "more blast depth must raise complexity"
        );
    }

    #[test]
    fn composite_stays_in_unit_interval_for_absurd_inputs() {
        // Saturating norm + clamp keep the score in [0, 1] even for overflow-ish
        // counts and out-of-range cognitive/coupling.
        let s = composite_score(&RouteVector {
            depth: usize::MAX,
            files: usize::MAX,
            symbols: usize::MAX,
            cognitive: 5.0,
            coupling: -3.0,
        });
        assert!((0.0..=1.0).contains(&s), "composite out of range: {s}");
    }

    #[test]
    fn mid_complexity_routes_to_orchestration_band() {
        let v = RouteVector {
            depth: 5,
            files: 8,
            symbols: 20,
            cognitive: 0.5,
            coupling: 0.5,
        };
        let r = route(&v);
        assert!(
            matches!(r.level, CilaLevel::L2 | CilaLevel::L3 | CilaLevel::L4),
            "mid task routed to {:?} (composite {:.3})",
            r.level,
            r.composite
        );
    }

    #[test]
    fn parallelism_grows_from_solo_to_full() {
        let solo = route(&RouteVector {
            depth: 0,
            files: 1,
            symbols: 0,
            cognitive: 0.0,
            coupling: 0.0,
        });
        let full = route(&RouteVector {
            depth: 50,
            files: 100,
            symbols: 500,
            cognitive: 1.0,
            coupling: 1.0,
        });
        assert!(full.max_parallelism > solo.max_parallelism);
    }

    #[test]
    fn result_serializes_to_json() {
        let r = route(&RouteVector {
            depth: 3,
            files: 4,
            symbols: 10,
            cognitive: 0.3,
            coupling: 0.4,
        });
        let json = serde_json::to_string(&r).expect("RouteResult serializes");
        assert!(json.contains("\"level\""));
        assert!(json.contains("\"composite\""));
        assert!(json.contains("\"phases\""));
    }
}
