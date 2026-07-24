//! Bug Bounty Tracking Module
//!
//! Tracks security vulnerabilities with CVSS scoring, CVE references,
//! and affected module inventory.
//!
//! # CVSS Scoring
//!
//! - 9.0-10.0: Critical
//! - 7.0-8.9: High
//! - 4.0-6.9: Medium
//! - 0.1-3.9: Low
//! - 0.0: None
//!
//! # Integration
//!
//! This module provides wiring points for the Touring ecosystem:
//! - `from_cve()` for fetching CVE details from NVD (requires `cve-fetch` feature)
//! - `export_to_json()` for integration with touring-learning
//! - `Serialize` is implemented for JSON export

use serde::{Deserialize, Serialize};
// Note: thiserror::Error is derived via macro, import not needed

/// Bug bounty vulnerability status lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BugStatus {
    /// Reported, not yet triaged
    #[default]
    Open,
    /// Triaged and confirmed
    Triaged,
    /// Bounty awarded
    Rewarded,
    /// Closed without reward or fixed
    Closed,
}

/// Core CVSS severity classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    /// CVSS 9.0-10.0 — exploitable with severe impact.
    Critical,
    /// CVSS 7.0-8.9 — high-risk vulnerability.
    High,
    /// CVSS 4.0-6.9 — moderate-risk vulnerability.
    Medium,
    /// CVSS 0.1-3.9 — low-risk vulnerability.
    Low,
    /// CVSS 0.0 — no measurable security impact.
    None,
}

impl From<f32> for Severity {
    fn from(cvss: f32) -> Self {
        if cvss >= 9.0 {
            Severity::Critical
        } else if cvss >= 7.0 {
            Severity::High
        } else if cvss >= 4.0 {
            Severity::Medium
        } else if cvss > 0.0 {
            Severity::Low
        } else {
            Severity::None
        }
    }
}

/// Error type for bug bounty operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum BugBountyError {
    /// A CVSS score outside the valid 0.0-10.0 range was supplied.
    #[error("CVSS must be between 0.0 and 10.0, got {cvss}")]
    InvalidCvss {
        /// The out-of-range CVSS score that was rejected.
        cvss: f32,
    },

    /// An illegal bug status lifecycle transition was attempted.
    #[error("invalid transition from {from:?} to {to:?}")]
    InvalidTransition {
        /// The status the bug was transitioning from.
        from: BugStatus,
        /// The status the bug was attempting to transition to.
        to: BugStatus,
    },

    /// Fetching CVE details from the upstream feed failed.
    #[error("CVE fetch failed: {0}")]
    CveFetchError(String),

    /// The requested CVE identifier was not present in the feed.
    #[error("CVE not found: {0}")]
    CveNotFound(String),

    /// Serializing the tracker to JSON failed.
    #[error("JSON serialization error: {0}")]
    SerializationError(String),
}

/// Bug bounty tracker for tracking vulnerability lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugBountyTracker {
    /// Unique vulnerability identifier (e.g., CVE-2024-1234)
    pub id: String,
    /// CVSS score (0.0-10.0)
    pub cvss: f32,
    /// Current lifecycle status
    pub status: BugStatus,
    /// CVE references linked to this vulnerability
    pub cve_references: Vec<String>,
    /// Affected modules in the Touring ecosystem
    pub affected_modules: Vec<String>,
    /// Internal tracking flag
    is_internal: bool,
    /// Optional tracker ID for cross-system correlation
    tracker_id: Option<u64>,
}

impl BugBountyTracker {
    /// Creates a new tracker for a given vulnerability ID and CVSS score.
    ///
    /// # Arguments
    ///
    /// * `id` - Vulnerability identifier (CVE format recommended)
    /// * `cvss` - CVSS v3 score from 0.0 to 10.0
    ///
    /// # Example
    ///
    /// ```rust
    /// use touring_offensive::bug_bounty::{BugBountyTracker, BugStatus};
    ///
    /// let tracker = BugBountyTracker::new("CVE-2024-9999", 9.5);
    /// assert_eq!(tracker.status, BugStatus::Open);
    /// assert_eq!(tracker.severity(), touring_offensive::bug_bounty::Severity::Critical);
    /// ```
    pub fn new(id: impl Into<String>, cvss: f32) -> Self {
        if !(0.0..=10.0).contains(&cvss) {
            panic!("CVSS must be between 0.0 and 10.0, got {cvss}");
        }
        Self {
            id: id.into(),
            cvss,
            status: BugStatus::Open,
            cve_references: Vec::new(),
            affected_modules: Vec::new(),
            is_internal: false,
            tracker_id: None,
        }
    }

    /// Creates a new tracker with validated CVSS, returning Result instead of panicking.
    ///
    /// # Arguments
    ///
    /// * `id` - Vulnerability identifier (CVE format recommended)
    /// * `cvss` - CVSS v3 score from 0.0 to 10.0
    pub fn new_validated(id: impl Into<String>, cvss: f32) -> Result<Self, BugBountyError> {
        if !(0.0..=10.0).contains(&cvss) {
            return Err(BugBountyError::InvalidCvss { cvss });
        }
        Ok(Self {
            id: id.into(),
            cvss,
            status: BugStatus::Open,
            cve_references: Vec::new(),
            affected_modules: Vec::new(),
            is_internal: false,
            tracker_id: None,
        })
    }

    /// Fetches CVE details from NVD and creates a BugBountyTracker.
    ///
    /// Requires the `cve-fetch` feature to be enabled.
    ///
    /// # Arguments
    ///
    /// * `cve_id` - CVE identifier (e.g., "CVE-2024-1234")
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use touring_offensive::bug_bounty::BugBountyTracker;
    ///
    /// let tracker = BugBountyTracker::from_cve("CVE-2024-9999").unwrap();
    /// ```
    #[cfg(feature = "cve-fetch")]
    pub async fn from_cve(cve_id: &str) -> Result<Self, BugBountyError> {
        let cve_id = cve_id.trim();
        if !cve_id.starts_with("CVE-") {
            return Err(BugBountyError::CveNotFound(cve_id.to_string()));
        }

        let url = format!(
            "https://services.nvd.nist.gov/rest/json/cves/2.0?cveId={}",
            cve_id
        );

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| BugBountyError::CveFetchError(e.to_string()))?;

        let response = client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| BugBountyError::CveFetchError(e.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(BugBountyError::CveNotFound(cve_id.to_string()));
        }

        if !response.status().is_success() {
            return Err(BugBountyError::CveFetchError(format!(
                "NVD API returned status {}",
                response.status()
            )));
        }

        #[derive(Deserialize)]
        struct NvdResponse {
            vulnerabilities: Vec<NvdVulnerability>,
        }

        #[derive(Deserialize)]
        struct NvdVulnerability {
            cve: NvdCve,
        }

        #[derive(Deserialize)]
        struct NvdCve {
            id: String,
            metrics: Option<NvdMetrics>,
            descriptions: Vec<NvdDescription>,
        }

        #[derive(Deserialize)]
        struct NvdMetrics {
            cvss_metric_v31: Option<Vec<NvdCvssV31>>,
            cvss_metric_v30: Option<Vec<NvdCvssV30>>,
            cvss_metric_v2: Option<Vec<NvdCvssV2>>,
        }

        #[derive(Deserialize)]
        struct NvdCvssV31 {
            cvss_data: NvdCvssData,
        }

        #[derive(Deserialize)]
        struct NvdCvssV30 {
            cvss_data: NvdCvssData,
        }

        #[derive(Deserialize)]
        struct NvdCvssV2 {
            cvss_data: NvdCvssDataV2,
        }

        #[derive(Deserialize)]
        struct NvdCvssData {
            base_score: f32,
        }

        #[derive(Deserialize)]
        struct NvdCvssDataV2 {
            base_score: f32,
        }

        #[derive(Deserialize)]
        struct NvdDescription {
            lang: String,
            value: String,
        }

        let nvd: NvdResponse = response
            .json()
            .await
            .map_err(|e| BugBountyError::CveFetchError(e.to_string()))?;

        let vuln = nvd
            .vulnerabilities
            .into_iter()
            .next()
            .ok_or_else(|| BugBountyError::CveNotFound(cve_id.to_string()))?;

        // Extract CVSS score - try v3.1 first, then v3.0, then v2.0
        let cvss_score = vuln
            .cve
            .metrics
            .as_ref()
            .and_then(|m| m.cvss_metric_v31.as_ref())
            .and_then(|v| v.first())
            .map(|v| v.cvss_data.base_score)
            .or_else(|| {
                vuln.cve
                    .metrics
                    .as_ref()
                    .and_then(|m| m.cvss_metric_v30.as_ref())
                    .and_then(|v| v.first())
                    .map(|v| v.cvss_data.base_score)
            })
            .or_else(|| {
                vuln.cve
                    .metrics
                    .as_ref()
                    .and_then(|m| m.cvss_metric_v2.as_ref())
                    .and_then(|v| v.first())
                    .map(|v| v.cvss_data.base_score)
            })
            .unwrap_or(0.0);

        let mut tracker = Self::new(vuln.cve.id, cvss_score);

        // Add English description
        if let Some(desc) = vuln.cve.descriptions.iter().find(|d| d.lang == "en") {
            tracker.add_cve(desc.value.clone());
        }

        Ok(tracker)
    }

    /// Synchronous CVE fetch for contexts where async is not available.
    /// Uses blocking reqwest.
    #[cfg(feature = "cve-fetch")]
    pub fn from_cve_sync(cve_id: &str) -> Result<Self, BugBountyError> {
        let cve_id = cve_id.trim();
        if !cve_id.starts_with("CVE-") {
            return Err(BugBountyError::CveNotFound(cve_id.to_string()));
        }

        let url = format!(
            "https://services.nvd.nist.gov/rest/json/cves/2.0?cveId={}",
            cve_id
        );

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| BugBountyError::CveFetchError(e.to_string()))?;

        let response = client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .map_err(|e| BugBountyError::CveFetchError(e.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(BugBountyError::CveNotFound(cve_id.to_string()));
        }

        if !response.status().is_success() {
            return Err(BugBountyError::CveFetchError(format!(
                "NVD API returned status {}",
                response.status()
            )));
        }

        #[derive(Deserialize)]
        struct NvdResponse {
            vulnerabilities: Vec<NvdVulnerability>,
        }

        #[derive(Deserialize)]
        struct NvdVulnerability {
            cve: NvdCve,
        }

        #[derive(Deserialize)]
        struct NvdCve {
            id: String,
            metrics: Option<NvdMetrics>,
            descriptions: Vec<NvdDescription>,
        }

        #[derive(Deserialize)]
        struct NvdMetrics {
            cvss_metric_v31: Option<Vec<NvdCvssV31>>,
            cvss_metric_v30: Option<Vec<NvdCvssV30>>,
        }

        #[derive(Deserialize)]
        struct NvdCvssV31 {
            cvss_data: NvdCvssData,
        }

        #[derive(Deserialize)]
        struct NvdCvssV30 {
            cvss_data: NvdCvssData,
        }

        #[derive(Deserialize)]
        struct NvdCvssData {
            base_score: f32,
        }

        #[derive(Deserialize)]
        struct NvdDescription {
            lang: String,
            value: String,
        }

        let nvd: NvdResponse = response
            .json()
            .map_err(|e| BugBountyError::CveFetchError(e.to_string()))?;

        let vuln = nvd
            .vulnerabilities
            .into_iter()
            .next()
            .ok_or_else(|| BugBountyError::CveNotFound(cve_id.to_string()))?;

        let cvss_score = vuln
            .cve
            .metrics
            .as_ref()
            .and_then(|m| m.cvss_metric_v31.as_ref())
            .and_then(|v| v.first())
            .map(|v| v.cvss_data.base_score)
            .or_else(|| {
                vuln.cve
                    .metrics
                    .as_ref()
                    .and_then(|m| m.cvss_metric_v30.as_ref())
                    .and_then(|v| v.first())
                    .map(|v| v.cvss_data.base_score)
            })
            .unwrap_or(0.0);

        let mut tracker = Self::new(vuln.cve.id, cvss_score);

        if let Some(desc) = vuln.cve.descriptions.iter().find(|d| d.lang == "en") {
            tracker.add_cve(desc.value.clone());
        }

        Ok(tracker)
    }

    /// Returns the computed severity classification.
    pub fn severity(&self) -> Severity {
        Severity::from(self.cvss)
    }

    /// Adds a CVE reference to this vulnerability.
    /// Deduplicates based on exact string match.
    pub fn add_cve(&mut self, cve: impl Into<String>) {
        let cve_str = cve.into();
        if !self.cve_references.contains(&cve_str) {
            self.cve_references.push(cve_str);
        }
    }

    /// Adds an affected module path.
    /// Deduplicates based on exact string match.
    pub fn add_affected_module(&mut self, module: impl Into<String>) {
        let module_str = module.into();
        if !self.affected_modules.contains(&module_str) {
            self.affected_modules.push(module_str);
        }
    }

    /// Transitions to a new status, enforcing valid transitions.
    ///
    /// Valid transitions:
    /// - Open -> Triaged
    /// - Triaged -> Rewarded | Closed
    /// - Rewarded -> Closed
    ///
    /// # Example
    ///
    /// ```rust
    /// use touring_offensive::bug_bounty::{BugBountyTracker, BugStatus};
    ///
    /// let mut tracker = BugBountyTracker::new("CVE-2024-0001", 5.0);
    /// tracker.transition_to(BugStatus::Triaged).unwrap();
    /// assert_eq!(tracker.status, BugStatus::Triaged);
    /// ```
    pub fn transition_to(&mut self, new_status: BugStatus) -> Result<(), BugStatusError> {
        let valid = matches!(
            (self.status, new_status),
            (BugStatus::Open, BugStatus::Triaged)
                | (BugStatus::Triaged, BugStatus::Rewarded)
                | (BugStatus::Triaged, BugStatus::Closed)
                | (BugStatus::Rewarded, BugStatus::Closed)
        );
        if valid {
            self.status = new_status;
            Ok(())
        } else {
            Err(BugStatusError::InvalidTransition {
                from: self.status,
                to: new_status,
            })
        }
    }

    /// Returns true if this is a critical or high severity vulnerability.
    pub fn is_critical(&self) -> bool {
        matches!(self.severity(), Severity::Critical | Severity::High)
    }

    /// Returns the number of affected modules.
    pub fn affected_module_count(&self) -> usize {
        self.affected_modules.len()
    }

    /// Marks the tracker as internal (not for public bounty program).
    pub fn set_internal(&mut self) {
        self.is_internal = true;
    }

    /// Checks if the tracker is marked internal.
    pub fn is_internal(&self) -> bool {
        self.is_internal
    }

    /// Sets a tracker ID for cross-system correlation.
    pub fn set_tracker_id(&mut self, id: u64) {
        self.tracker_id = Some(id);
    }

    /// Gets the tracker ID if set.
    pub fn tracker_id(&self) -> Option<u64> {
        self.tracker_id
    }

    /// Exports the tracker to a JSON string for integration with touring-learning.
    ///
    /// # Example
    ///
    /// ```rust
    /// use touring_offensive::bug_bounty::BugBountyTracker;
    ///
    /// let tracker = BugBountyTracker::new("CVE-2024-0001", 9.0);
    /// let json = tracker.export_to_json().unwrap();
    /// ```
    pub fn export_to_json(&self) -> Result<String, BugBountyError> {
        serde_json::to_string_pretty(self)
            .map_err(|e| BugBountyError::SerializationError(e.to_string()))
    }

    /// Returns the number of CVE references.
    pub fn cve_count(&self) -> usize {
        self.cve_references.len()
    }
}

/// Error type for invalid bug status transitions.
#[derive(Debug, Clone, thiserror::Error)]
pub enum BugStatusError {
    /// An illegal bug status lifecycle transition was attempted.
    #[error("invalid transition from {from:?} to {to:?}")]
    InvalidTransition {
        /// The status the bug was transitioning from.
        from: BugStatus,
        /// The status the bug was attempting to transition to.
        to: BugStatus,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    // B-2: CVSS Boundary Tests

    #[test]
    fn test_cvss_boundary_exactly_9_0() {
        let tracker = BugBountyTracker::new("CVE-2024-B9", 9.0);
        assert_eq!(tracker.severity(), Severity::Critical);
        assert!(tracker.is_critical());
    }

    #[test]
    fn test_cvss_boundary_exactly_7_0() {
        let tracker = BugBountyTracker::new("CVE-2024-B7", 7.0);
        assert_eq!(tracker.severity(), Severity::High);
        assert!(tracker.is_critical());
    }

    #[test]
    fn test_cvss_boundary_exactly_4_0() {
        let tracker = BugBountyTracker::new("CVE-2024-B4", 4.0);
        assert_eq!(tracker.severity(), Severity::Medium);
        assert!(!tracker.is_critical());
    }

    #[test]
    fn test_cvss_boundary_exactly_0_0() {
        let tracker = BugBountyTracker::new("CVE-2024-B0", 0.0);
        assert_eq!(tracker.severity(), Severity::None);
        assert!(!tracker.is_critical());
    }

    // B-2: State Transition Tests

    #[test]
    fn test_state_transition_open_to_triaged() {
        let mut tracker = BugBountyTracker::new("CVE-2024-T1", 5.0);
        assert_eq!(tracker.status, BugStatus::Open);
        tracker.transition_to(BugStatus::Triaged).unwrap();
        assert_eq!(tracker.status, BugStatus::Triaged);
    }

    #[test]
    fn test_state_transition_triaged_to_rewarded() {
        let mut tracker = BugBountyTracker::new("CVE-2024-T2", 5.0);
        tracker.transition_to(BugStatus::Triaged).unwrap();
        tracker.transition_to(BugStatus::Rewarded).unwrap();
        assert_eq!(tracker.status, BugStatus::Rewarded);
    }

    #[test]
    fn test_state_transition_triaged_to_closed() {
        let mut tracker = BugBountyTracker::new("CVE-2024-T3", 5.0);
        tracker.transition_to(BugStatus::Triaged).unwrap();
        tracker.transition_to(BugStatus::Closed).unwrap();
        assert_eq!(tracker.status, BugStatus::Closed);
    }

    #[test]
    fn test_state_transition_rewarded_to_closed() {
        let mut tracker = BugBountyTracker::new("CVE-2024-T4", 5.0);
        tracker.transition_to(BugStatus::Triaged).unwrap();
        tracker.transition_to(BugStatus::Rewarded).unwrap();
        tracker.transition_to(BugStatus::Closed).unwrap();
        assert_eq!(tracker.status, BugStatus::Closed);
    }

    // B-2: Invalid Transition Tests

    #[test]
    fn test_invalid_transition_open_to_rewarded() {
        let mut tracker = BugBountyTracker::new("CVE-2024-IT1", 5.0);
        let result = tracker.transition_to(BugStatus::Rewarded);
        assert!(result.is_err());
        assert_eq!(tracker.status, BugStatus::Open);
    }

    #[test]
    fn test_invalid_transition_closed_to_any() {
        let mut tracker = BugBountyTracker::new("CVE-2024-IT2", 5.0);
        tracker.transition_to(BugStatus::Triaged).unwrap();
        tracker.transition_to(BugStatus::Closed).unwrap();

        // Try all transitions from Closed - all should fail
        assert!(tracker.transition_to(BugStatus::Open).is_err());
        assert!(tracker.transition_to(BugStatus::Triaged).is_err());
        assert!(tracker.transition_to(BugStatus::Rewarded).is_err());
        assert_eq!(tracker.status, BugStatus::Closed);
    }

    // B-2: Deduplication Tests

    #[test]
    fn test_cve_reference_deduplication() {
        let mut tracker = BugBountyTracker::new("CVE-2024-D1", 5.0);
        tracker.add_cve("CVE-2024-1000");
        tracker.add_cve("CVE-2024-1000");
        tracker.add_cve("CVE-2024-1000");
        assert_eq!(tracker.cve_count(), 1);
    }

    #[test]
    fn test_affected_module_deduplication() {
        let mut tracker = BugBountyTracker::new("CVE-2024-D2", 5.0);
        tracker.add_affected_module("touring-core::config");
        tracker.add_affected_module("touring-core::config");
        tracker.add_affected_module("touring-hooks::registry");
        assert_eq!(tracker.affected_module_count(), 2);
    }

    // B-2: Internal Flag Lifecycle

    #[test]
    fn test_internal_flag_lifecycle() {
        let mut tracker = BugBountyTracker::new("INT-001", 5.0);
        assert!(!tracker.is_internal());

        tracker.set_internal();
        assert!(tracker.is_internal());

        // Setting internal again should be idempotent
        tracker.set_internal();
        assert!(tracker.is_internal());
    }

    // B-2: Severity / Criticality Tests

    #[test]
    fn test_is_critical_for_high_severity() {
        // Critical (9.0-10.0)
        let critical = BugBountyTracker::new("CVE-2024-C1", 9.5);
        assert!(critical.is_critical());

        // High (7.0-8.9)
        let high = BugBountyTracker::new("CVE-2024-H1", 7.5);
        assert!(high.is_critical());
    }

    #[test]
    fn test_is_critical_for_low_severity() {
        // Medium (4.0-6.9)
        let medium = BugBountyTracker::new("CVE-2024-M1", 5.0);
        assert!(!medium.is_critical());

        // Low (0.1-3.9)
        let low = BugBountyTracker::new("CVE-2024-L1", 2.0);
        assert!(!low.is_critical());

        // None (0.0)
        let none = BugBountyTracker::new("CVE-2024-N1", 0.0);
        assert!(!none.is_critical());
    }

    #[test]
    fn test_severity_from_cvss() {
        assert_eq!(Severity::from(10.0), Severity::Critical);
        assert_eq!(Severity::from(9.0), Severity::Critical);
        assert_eq!(Severity::from(8.9), Severity::High);
        assert_eq!(Severity::from(7.0), Severity::High);
        assert_eq!(Severity::from(6.9), Severity::Medium);
        assert_eq!(Severity::from(4.0), Severity::Medium);
        assert_eq!(Severity::from(3.9), Severity::Low);
        assert_eq!(Severity::from(0.1), Severity::Low);
        assert_eq!(Severity::from(0.0), Severity::None);
    }

    // B-2: Tracker ID Assignment

    #[test]
    fn test_tracker_id_assignment() {
        let mut tracker = BugBountyTracker::new("CVE-2024-ID1", 5.0);
        assert!(tracker.tracker_id().is_none());

        tracker.set_tracker_id(42);
        assert_eq!(tracker.tracker_id(), Some(42));

        tracker.set_tracker_id(100);
        assert_eq!(tracker.tracker_id(), Some(100));
    }

    // B-2: Multiple Addition Tests

    #[test]
    fn test_multiple_cve_additions() {
        let mut tracker = BugBountyTracker::new("CVE-2024-MC1", 5.0);
        tracker.add_cve("CVE-2024-2001");
        tracker.add_cve("CVE-2024-2002");
        tracker.add_cve("CVE-2024-2003");
        assert_eq!(tracker.cve_count(), 3);

        // Add duplicate
        tracker.add_cve("CVE-2024-2001");
        assert_eq!(tracker.cve_count(), 3);
    }

    #[test]
    fn test_multiple_module_additions() {
        let mut tracker = BugBountyTracker::new("CVE-2024-MM1", 5.0);
        tracker.add_affected_module("touring-core");
        tracker.add_affected_module("touring-hooks");
        tracker.add_affected_module("touring-learning");
        tracker.add_affected_module("touring-ast");
        assert_eq!(tracker.affected_module_count(), 4);

        // Add duplicate
        tracker.add_affected_module("touring-core");
        assert_eq!(tracker.affected_module_count(), 4);
    }

    // B-2: Serialization Tests

    #[test]
    fn test_serialization_roundtrip() {
        let mut tracker = BugBountyTracker::new("CVE-2024-SER1", 9.0);
        tracker.add_cve("A critical vulnerability description");
        tracker.add_affected_module("touring-core::parser");
        tracker.set_internal();

        let json = tracker.export_to_json().unwrap();

        let deserialized: BugBountyTracker =
            serde_json::from_str(&json).expect("Deserialization should succeed");

        assert_eq!(deserialized.id, tracker.id);
        assert_eq!(deserialized.cvss, tracker.cvss);
        assert_eq!(deserialized.status, tracker.status);
        assert_eq!(deserialized.cve_count(), tracker.cve_count());
        assert_eq!(
            deserialized.affected_module_count(),
            tracker.affected_module_count()
        );
        assert_eq!(deserialized.is_internal(), tracker.is_internal());
    }

    #[test]
    fn test_deserialization() {
        let json = r#"{
            "id": "CVE-2024-DES1",
            "cvss": 8.5,
            "status": "Triaged",
            "cve_references": ["CVE-2024-9999 description"],
            "affected_modules": ["touring-offensive::bug_bounty"],
            "is_internal": false,
            "tracker_id": 123
        }"#;

        let tracker: BugBountyTracker =
            serde_json::from_str(json).expect("Deserialization should succeed");

        assert_eq!(tracker.id, "CVE-2024-DES1");
        assert_eq!(tracker.cvss, 8.5);
        assert_eq!(tracker.status, BugStatus::Triaged);
        assert_eq!(tracker.cve_count(), 1);
        assert_eq!(tracker.affected_module_count(), 1);
        assert!(!tracker.is_internal());
        assert_eq!(tracker.tracker_id(), Some(123));
    }

    // B-2: new_validated Tests

    #[test]
    fn test_new_validated_success() {
        let tracker = BugBountyTracker::new_validated("CVE-2024-V1", 7.5).unwrap();
        assert_eq!(tracker.cvss, 7.5);
    }

    #[test]
    fn test_new_validated_invalid_cvss() {
        let result = BugBountyTracker::new_validated("CVE-2024-V2", 11.0);
        assert!(result.is_err());

        let result = BugBountyTracker::new_validated("CVE-2024-V3", -1.0);
        assert!(result.is_err());
    }

    // B-2: Full Lifecycle Test

    #[test]
    fn test_full_lifecycle() {
        let mut tracker = BugBountyTracker::new("CVE-2024-FULL", 9.0);

        // Initial state
        assert_eq!(tracker.status, BugStatus::Open);
        assert!(tracker.is_critical());

        // Add data
        tracker.add_cve("Initial report");
        tracker.add_affected_module("touring-core");
        tracker.set_internal();

        // Transition through lifecycle
        tracker.transition_to(BugStatus::Triaged).unwrap();
        assert_eq!(tracker.status, BugStatus::Triaged);

        tracker.transition_to(BugStatus::Rewarded).unwrap();
        assert_eq!(tracker.status, BugStatus::Rewarded);

        tracker.transition_to(BugStatus::Closed).unwrap();
        assert_eq!(tracker.status, BugStatus::Closed);

        // Export final state
        let json = tracker.export_to_json().unwrap();
        assert!(json.contains("\"status\": \"Closed\""));
    }
}
