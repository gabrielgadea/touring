//! TDG (Touring Diagnostic Grade) distribution analysis inferlet.
//!
//! Analyzes grade distribution across all Rust source files in the project.
//! Detects anomalies when a grade category exceeds a configurable threshold.

use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::thread_local;

// Thread-local error buffer for structured output propagation.
// (regular comment — rustdoc does not generate documentation for macro
// invocations like `thread_local!`)
thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Input structure for tdg_grade_distribution inferlet.
#[derive(Debug, Deserialize)]
pub struct Input {
    /// Anomaly threshold — a grade share above this fraction is flagged.
    #[serde(default = "default_threshold")]
    pub threshold: f64,
    /// Minimum number of graded files required before computing anomalies.
    #[serde(default = "default_min_files")]
    pub min_files: usize,
}

fn default_threshold() -> f64 {
    0.05
}
fn default_min_files() -> usize {
    5
}

/// Grade distribution — maps grade letter to count.
#[derive(Debug, Serialize, Default)]
pub struct Distribution {
    /// Number of files graded A+.
    pub ap: usize,
    /// Number of files graded A.
    pub a: usize,
    /// Number of files graded A-.
    pub am: usize,
    /// Number of files graded B+.
    pub bp: usize,
    /// Number of files graded B.
    pub b: usize,
    /// Number of files graded B-.
    pub bm: usize,
    /// Number of files graded C+.
    pub cp: usize,
    /// Number of files graded C.
    pub c: usize,
    /// Number of files graded C-.
    pub cm: usize,
    /// Number of files graded D+.
    pub dp: usize,
    /// Number of files graded D.
    pub d: usize,
    /// Number of files graded D-.
    pub dm: usize,
    /// Number of files graded F.
    pub f: usize,
}

impl Distribution {
    fn total(&self) -> usize {
        self.ap
            + self.a
            + self.am
            + self.bp
            + self.b
            + self.bm
            + self.cp
            + self.c
            + self.cm
            + self.dp
            + self.d
            + self.dm
            + self.f
    }
}

/// Output structure for tdg_grade_distribution inferlet.
#[derive(Debug, Serialize)]
pub struct Output {
    /// Total number of files included in the distribution.
    pub total_files: usize,
    /// Count of files per grade bucket.
    pub distribution: Distribution,
    /// Whether any grade bucket exceeded the anomaly threshold.
    pub anomaly_detected: bool,
    /// Grade buckets that triggered the anomaly.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub anomaly_grades: Vec<String>,
    /// Human-readable explanation of the detected anomaly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anomaly_reason: Option<String>,
}

/// Map grade string to distribution key.
fn grade_to_key(grade: &str) -> Option<&'static str> {
    match grade.trim().to_uppercase().as_str() {
        "A+" => Some("ap"),
        "A" => Some("a"),
        "A-" => Some("am"),
        "B+" => Some("bp"),
        "B" => Some("b"),
        "B-" => Some("bm"),
        "C+" => Some("cp"),
        "C" => Some("c"),
        "C-" => Some("cm"),
        "D+" => Some("dp"),
        "D" => Some("d"),
        "D-" => Some("dm"),
        "F" => Some("f"),
        _ => None,
    }
}

/// Quality score (0-1) to grade string.
fn score_to_grade(score: f64) -> &'static str {
    if score >= 0.97 {
        "A+"
    } else if score >= 0.93 {
        "A"
    } else if score >= 0.90 {
        "A-"
    } else if score >= 0.87 {
        "B+"
    } else if score >= 0.83 {
        "B"
    } else if score >= 0.80 {
        "B-"
    } else if score >= 0.77 {
        "C+"
    } else if score >= 0.73 {
        "C"
    } else if score >= 0.70 {
        "C-"
    } else if score >= 0.67 {
        "D+"
    } else if score >= 0.63 {
        "D"
    } else if score >= 0.60 {
        "D-"
    } else {
        "F"
    }
}

/// Increment distribution by key.
fn incr(dist: &mut Distribution, key: &str) {
    match key {
        "ap" => dist.ap += 1,
        "a" => dist.a += 1,
        "am" => dist.am += 1,
        "bp" => dist.bp += 1,
        "b" => dist.b += 1,
        "bm" => dist.bm += 1,
        "cp" => dist.cp += 1,
        "c" => dist.c += 1,
        "cm" => dist.cm += 1,
        "dp" => dist.dp += 1,
        "d" => dist.d += 1,
        "dm" => dist.dm += 1,
        "f" => dist.f += 1,
        _ => {}
    }
}

/// Get repo-score overall grade.
fn get_repo_score_grade() -> Option<Distribution> {
    let json = crate::run_touring_json(&["repo-score", "-j"])?;
    let grade = json.get("grade")?.as_str()?;
    let mut dist = Distribution::default();
    if let Some(key) = grade_to_key(grade) {
        incr(&mut dist, key);
    }
    Some(dist)
}

/// Get grade from e2e quality phase.
fn get_e2e_grade() -> Option<Distribution> {
    let json = crate::run_touring_json(&["e2e", "-j"])?;
    let phases = json.get("phases")?.as_array()?;

    for phase in phases {
        if phase.get("phase")?.as_str()? != "quality" {
            continue;
        }
        let metrics = phase.get("metrics")?.as_object()?;
        let avg_q = metrics.get("avg_quality_score")?.as_str()?;
        let score: f64 = avg_q.parse().ok()?;
        let mut dist = Distribution::default();
        let grade = score_to_grade(score);
        if let Some(key) = grade_to_key(grade) {
            incr(&mut dist, key);
        }
        return Some(dist);
    }
    None
}

/// Detect anomaly grades exceeding threshold.
fn detect_anomalies(
    dist: &Distribution,
    total: usize,
    threshold: f64,
) -> (bool, Vec<String>, Option<String>) {
    if total == 0 {
        return (false, vec![], None);
    }

    let grades = [
        ("A+", dist.ap),
        ("A", dist.a),
        ("A-", dist.am),
        ("B+", dist.bp),
        ("B", dist.b),
        ("B-", dist.bm),
        ("C+", dist.cp),
        ("C", dist.c),
        ("C-", dist.cm),
        ("D+", dist.dp),
        ("D", dist.d),
        ("D-", dist.dm),
        ("F", dist.f),
    ];

    let mut anomaly_grades = Vec::new();
    let mut reasons = Vec::new();

    for (grade, count) in grades.iter() {
        let pct = *count as f64 / total as f64;
        if pct > threshold {
            anomaly_grades.push(grade.to_string());
            reasons.push(format!(
                "{} ({:.1}%) > {:.1}%",
                grade,
                pct * 100.0,
                threshold * 100.0
            ));
        }
    }

    let anomaly_detected = !anomaly_grades.is_empty();
    let anomaly_reason = if reasons.is_empty() {
        None
    } else {
        Some(reasons.join("; "))
    };
    (anomaly_detected, anomaly_grades, anomaly_reason)
}

/// Raw evaluate — returns 1 on success (anomaly or not), 0 on failure.
pub(crate) fn evaluate_raw(input: &str) -> i32 {
    let input = input.trim();
    if input.is_empty() {
        return 0;
    }

    let inp: Input = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(_) => {
            LAST_ERROR.with(|cell| *cell.borrow_mut() = Some("invalid JSON".to_string()));
            return 0;
        }
    };

    // Get grade distribution
    let mut dist = get_repo_score_grade()
        .or_else(get_e2e_grade)
        .unwrap_or_default();

    // Merge e2e if available
    if let Some(e2e_dist) = get_e2e_grade() {
        dist.ap += e2e_dist.ap;
        dist.a += e2e_dist.a;
        dist.am += e2e_dist.am;
        dist.bp += e2e_dist.bp;
        dist.b += e2e_dist.b;
        dist.bm += e2e_dist.bm;
        dist.cp += e2e_dist.cp;
        dist.c += e2e_dist.c;
        dist.cm += e2e_dist.cm;
        dist.dp += e2e_dist.dp;
        dist.d += e2e_dist.d;
        dist.dm += e2e_dist.dm;
        dist.f += e2e_dist.f;
    }

    let total_files = dist.total();
    if total_files < inp.min_files {
        LAST_ERROR.with(|cell| {
            *cell.borrow_mut() = Some(format!(
                "insufficient files: {} < {}",
                total_files, inp.min_files
            ))
        });
        return 0;
    }

    let (anomaly_detected, anomaly_grades, anomaly_reason) =
        detect_anomalies(&dist, total_files, inp.threshold);

    let output = Output {
        total_files,
        distribution: dist,
        anomaly_detected,
        anomaly_grades,
        anomaly_reason,
    };
    if let Ok(json) = serde_json::to_string(&output) {
        LAST_ERROR.with(|cell| *cell.borrow_mut() = Some(json));
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grade_to_key() {
        assert_eq!(grade_to_key("A+"), Some("ap"));
        assert_eq!(grade_to_key("A"), Some("a"));
        assert_eq!(grade_to_key("D-"), Some("dm"));
        assert_eq!(grade_to_key("F"), Some("f"));
        assert_eq!(grade_to_key("X"), None);
    }

    #[test]
    fn test_score_to_grade() {
        assert_eq!(score_to_grade(0.98), "A+");
        assert_eq!(score_to_grade(0.50), "F");
    }

    #[test]
    fn test_distribution_total() {
        let mut d = Distribution::default();
        assert_eq!(d.total(), 0);
        d.c = 5;
        d.b = 3;
        assert_eq!(d.total(), 8);
    }

    #[test]
    fn test_detect_anomalies_none() {
        let d = Distribution {
            ap: 5,
            ..Default::default()
        }; // exactly 5% threshold boundary
        let (det, grades, reason) = detect_anomalies(&d, 100, 0.05);
        assert!(!det, "5% should not exceed 5% threshold");
        assert!(grades.is_empty());
        assert!(reason.is_none());
    }

    #[test]
    fn test_detect_anomalies_found() {
        let d = Distribution {
            c: 60,
            b: 40,
            ..Default::default()
        }; // both exceed 5% threshold (60% and 40%)
        let (det, grades, reason) = detect_anomalies(&d, 100, 0.05);
        assert!(det);
        assert!(grades.contains(&"C".to_string()));
        assert!(grades.contains(&"B".to_string()));
        assert!(reason.is_some());
    }

    #[test]
    fn test_detect_anomalies_multiple() {
        let d = Distribution {
            c: 50,
            d: 30,
            f: 20,
            ..Default::default()
        };
        let (det, _, _) = detect_anomalies(&d, 100, 0.05);
        assert!(det);
    }

    #[test]
    fn test_incr() {
        let mut d = Distribution::default();
        incr(&mut d, "ap");
        incr(&mut d, "a");
        incr(&mut d, "a");
        incr(&mut d, "c");
        assert_eq!(d.ap, 1);
        assert_eq!(d.a, 2);
        assert_eq!(d.c, 1);
    }

    #[test]
    fn test_evaluate_empty_input() {
        assert_eq!(evaluate_raw(""), 0);
    }
    #[test]
    fn test_evaluate_invalid_json() {
        assert_eq!(evaluate_raw("not json"), 0);
    }

    #[test]
    fn test_output_serialization() {
        let out = Output {
            total_files: 100,
            distribution: Distribution {
                c: 30,
                b: 50,
                ap: 20,
                ..Default::default()
            },
            anomaly_detected: true,
            anomaly_grades: vec!["C".to_string()],
            anomaly_reason: Some("C (30.0%) > 5%".to_string()),
        };
        let json = serde_json::to_string(&out).unwrap();
        assert!(json.contains("\"total_files\":100"));
        assert!(json.contains("\"anomaly_detected\":true"));
    }

    #[test]
    fn test_input_defaults() {
        let inp: Input = serde_json::from_str("{}").unwrap();
        assert!((inp.threshold - 0.05).abs() < 1e-9);
        assert_eq!(inp.min_files, 5);
    }

    #[test]
    fn test_input_custom() {
        let inp: Input = serde_json::from_str(r#"{"threshold": 0.1, "min_files": 10}"#).unwrap();
        assert!((inp.threshold - 0.1).abs() < 1e-9);
        assert_eq!(inp.min_files, 10);
    }
}
