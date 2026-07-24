//! Deployment Strategy (D48 / F4.8) -- Kubernetes / Argo Rollouts
//! anti-patterns. The deployment strategy determines the blast radius of a
//! bad release: big-bang (no rollback) vs canary/blue-green (gradual traffic
//! shift with rollback).
//!
//! | Smell | Signal | File |
//! |-------|--------|------|
//! | `kind-deployment-no-rollout` | a `kind: Deployment` manifest (should be `kind: Rollout` for canary/blue-green) | `.yaml` (k8s) |
//! | `rollout-no-strategy` | a `kind: Rollout` *without* `strategy:` | `.yaml` (k8s) |
//! | `rollout-no-maxsurge` | a `kind: Rollout` *without* `maxSurge` (zero-downtime impossible without surge) | `.yaml` (k8s) |
//! | `rollout-no-pause-step` | a canary strategy *without* any `pause:` step (full auto-promote; no human-in-the-loop on a bad metric) | `.yaml` (k8s) |
//! | `rollout-no-setweight-step` | a `strategy.canary` *without* any `setWeight:` step (not actually a canary -- full traffic cut) | `.yaml` (k8s) |
//! | `rollout-no-rollback` | a `kind: Rollout` *without* `rollbackWindow` (no automatic rollback on metric regression) | `.yaml` (k8s) |
//! | `rollout-no-traffic-routing` | a canary with `setWeight` *but no* `trafficRouting:` (weight is applied to ReplicaSet, not real traffic) | `.yaml` (k8s) |
//! | `deployment-no-readiness-probe` | a `kind: Deployment` *without* `readinessProbe:` (zero-downtime impossible -- traffic hits un-ready pods) | `.yaml` (k8s) |
//!
//! **Disjoint** from F4.7 CI/CD (which keys on GitHub Actions workflow
//! shape; F4.8 keys on K8s manifest shape) and F2.6 config (which keys on
//! secrets/permissions; F4.8 keys on deploy strategy).
//!
//! **Sources (context7, `/argoproj/argo-rollouts`, High reputation, bench
//! 83.08):** canary strategy requires `setWeight` + `pause` steps; the
//! final step must be `setWeight: 100` for full promotion. `maxSurge` is
//! how new pods come up alongside old (zero-downtime requires `maxSurge ≥
//! 1`). `trafficRouting` (e.g. `alb`, `nginx`, `traefik`) is what makes
//! `setWeight` actually shift traffic (vs just replica count). `blueGreen`
//! needs `activeService` (production) + `previewService` (canary).

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

const SCALE: f32 = 6.0;

#[derive(Debug, Clone, Default)]
/// Deployment-strategy findings for one K8s/Argo-Rollouts manifest.
pub struct DeployReport {
    /// Total raw violation count across all detectors.
    /// Total raw violation count across all detectors.
    pub violations: usize,
    /// Weighted violation total (per-smell weights applied).
    pub weighted_total: f32,
    /// Lines scanned (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired detector, sorted by count desc.
    pub findings: Vec<(String, usize)>,
}

impl DeployReport {
    fn push(&mut self, message: &'static str, count: usize, weight: f32) {
        if count > 0 {
            self.violations += count;
            self.weighted_total += count as f32 * weight;
            self.findings.push((message.to_string(), count));
        }
    }
}

const KIND_DEPLOYMENT: &[u8] = b"kind: Deployment";
const KIND_ROLLOUT: &[u8] = b"kind: Rollout";
const STRATEGY: &[u8] = b"strategy:";
const MAX_SURGE: &[u8] = b"maxSurge";
const STRATEGY_CANARY: &[u8] = b"canary:";
const PAUSE: &[u8] = b"pause:";
const SET_WEIGHT: &[u8] = b"setWeight:";
const ROLLBACK_WINDOW: &[u8] = b"rollbackWindow";
const TRAFFIC_ROUTING: &[u8] = b"trafficRouting:";
const READINESS_PROBE: &[u8] = b"readinessProbe:";

fn count_in_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> usize {
    memmem::find_iter(bytes, needle)
        .filter(|&off| !offset_suppressed(off, regions))
        .count()
}

fn has_in_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> bool {
    count_in_executable(bytes, regions, needle) > 0
}

fn detect_kind_deployment_no_rollout(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_in_executable(bytes, regions, KIND_DEPLOYMENT)
}

fn detect_rollout_no_strategy(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let rollouts = count_in_executable(bytes, regions, KIND_ROLLOUT);
    if rollouts == 0 {
        return 0;
    }
    if has_in_executable(bytes, regions, STRATEGY) {
        0
    } else {
        1
    }
}

fn detect_rollout_no_maxsurge(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let rollouts = count_in_executable(bytes, regions, KIND_ROLLOUT);
    if rollouts == 0 {
        return 0;
    }
    if has_in_executable(bytes, regions, MAX_SURGE) {
        0
    } else {
        1
    }
}

fn detect_rollout_no_pause_step(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let rollouts = count_in_executable(bytes, regions, KIND_ROLLOUT);
    if rollouts == 0 {
        return 0;
    }
    let canary = has_in_executable(bytes, regions, STRATEGY_CANARY);
    if !canary {
        return 0;
    }
    if has_in_executable(bytes, regions, PAUSE) {
        0
    } else {
        1
    }
}

fn detect_rollout_no_setweight_step(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let rollouts = count_in_executable(bytes, regions, KIND_ROLLOUT);
    if rollouts == 0 {
        return 0;
    }
    if !has_in_executable(bytes, regions, STRATEGY_CANARY) {
        return 0;
    }
    if has_in_executable(bytes, regions, SET_WEIGHT) {
        0
    } else {
        1
    }
}

fn detect_rollout_no_rollback(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let rollouts = count_in_executable(bytes, regions, KIND_ROLLOUT);
    if rollouts == 0 {
        return 0;
    }
    if has_in_executable(bytes, regions, ROLLBACK_WINDOW) {
        0
    } else {
        1
    }
}

fn detect_rollout_no_traffic_routing(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let rollouts = count_in_executable(bytes, regions, KIND_ROLLOUT);
    if rollouts == 0 {
        return 0;
    }
    if !has_in_executable(bytes, regions, STRATEGY_CANARY) {
        return 0;
    }
    // Has canary setWeight but no trafficRouting → just replica count, not real traffic split.
    if !has_in_executable(bytes, regions, SET_WEIGHT) {
        return 0;
    }
    if has_in_executable(bytes, regions, TRAFFIC_ROUTING) {
        0
    } else {
        1
    }
}

fn detect_deployment_no_readiness_probe(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let deployments = count_in_executable(bytes, regions, KIND_DEPLOYMENT);
    if deployments == 0 {
        return 0;
    }
    if has_in_executable(bytes, regions, READINESS_PROBE) {
        0
    } else {
        1
    }
}

/// Analyze deployment-strategy smells in `source` (a YAML manifest). The lang parameter is ignored -- this engine is YAML-only.
pub fn analyze_deploy(source: &str, _lang: &str) -> DeployReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, "rust");
    let mut report = DeployReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    report.push(
        "`kind: Deployment` (use `kind: Rollout` for canary/blue-green progressive delivery)",
        detect_kind_deployment_no_rollout(bytes, &regions),
        0.5,
    );
    report.push(
        "`kind: Rollout` without `strategy:` (no canary/blue-green declared)",
        detect_rollout_no_strategy(bytes, &regions),
        1.0,
    );
    report.push(
        "`kind: Rollout` without `maxSurge` (zero-downtime impossible without surge capacity)",
        detect_rollout_no_maxsurge(bytes, &regions),
        0.9,
    );
    report.push(
        "canary strategy without `pause:` step (full auto-promote -- no human-in-the-loop on bad metric)",
        detect_rollout_no_pause_step(bytes, &regions),
        0.7,
    );
    report.push(
        "canary strategy without `setWeight:` step (not actually a canary -- full traffic cut)",
        detect_rollout_no_setweight_step(bytes, &regions),
        1.0,
    );
    report.push(
        "`kind: Rollout` without `rollbackWindow` (no automatic rollback on metric regression)",
        detect_rollout_no_rollback(bytes, &regions),
        0.6,
    );
    report.push(
        "canary with `setWeight` but no `trafficRouting` (weight is replica count, not real traffic split)",
        detect_rollout_no_traffic_routing(bytes, &regions),
        0.7,
    );
    report.push(
        "`kind: Deployment` without `readinessProbe:` (zero-downtime impossible -- traffic hits un-ready pods)",
        detect_deployment_no_readiness_probe(bytes, &regions),
        0.8,
    );
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`DeployReport`] as `1 - density·SCALE`, clamped to `[0, 1]`.
pub fn score_deploy(report: &DeployReport) -> f32 {
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rep(src: &str) -> DeployReport {
        analyze_deploy(src, "rust")
    }

    #[test]
    fn empty_file_clean() {
        let r = rep("");
        assert_eq!(r.violations, 0);
    }

    #[test]
    fn kind_deployment_flagged() {
        let src = r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: app
spec:
  replicas: 3
"#;
        let r = rep(src);
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("kind: Deployment")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn rollout_no_strategy_flagged() {
        let src = r#"apiVersion: argoproj.io/v1alpha1
kind: Rollout
metadata:
  name: app
spec:
  replicas: 3
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("strategy")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn rollout_no_maxsurge_flagged() {
        let src = r#"apiVersion: argoproj.io/v1alpha1
kind: Rollout
metadata:
  name: app
spec:
  strategy:
    canary:
      steps:
        - setWeight: 20
        - pause: {}
        - setWeight: 100
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("maxSurge")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn canary_no_pause_step_flagged() {
        let src = r#"apiVersion: argoproj.io/v1alpha1
kind: Rollout
metadata:
  name: app
spec:
  strategy:
    canary:
      maxSurge: '25%'
      steps:
        - setWeight: 20
        - setWeight: 100
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("pause")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn canary_no_setweight_step_flagged() {
        let src = r#"apiVersion: argoproj.io/v1alpha1
kind: Rollout
metadata:
  name: app
spec:
  strategy:
    canary:
      maxSurge: '25%'
      steps:
        - pause: {}
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("setWeight")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn rollout_no_rollback_flagged() {
        let src = r#"apiVersion: argoproj.io/v1alpha1
kind: Rollout
metadata:
  name: app
spec:
  strategy:
    canary:
      maxSurge: '25%'
      steps:
        - setWeight: 20
        - pause: {}
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("rollbackWindow")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn deployment_no_readiness_probe_flagged() {
        let src = r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: app
spec:
  replicas: 3
  template:
    spec:
      containers:
        - name: c
          image: i
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("readinessProbe")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn fully_tuned_rollout_clean() {
        let src = r#"apiVersion: argoproj.io/v1alpha1
kind: Rollout
metadata:
  name: app
spec:
  strategy:
    canary:
      canaryService: canary
      stableService: stable
      maxSurge: '25%'
      maxUnavailable: 0
      rollbackWindow:
        seconds: 60
      trafficRouting:
        alb:
          ingress: app-ing
          servicePort: 80
      steps:
        - setWeight: 10
        - pause: {duration: 1h}
        - setWeight: 50
        - pause: {}
        - setWeight: 100
        - pause: {}
"#;
        let r = rep(src);
        assert_eq!(
            r.violations, 0,
            "fully-tuned rollout is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn canary_setweight_no_traffic_routing_flagged() {
        let src = r#"apiVersion: argoproj.io/v1alpha1
kind: Rollout
metadata:
  name: app
spec:
  strategy:
    canary:
      maxSurge: '25%'
      maxUnavailable: 0
      steps:
        - setWeight: 20
        - pause: {}
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("trafficRouting")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = rep("apiVersion: apps/v1
kind: Deployment
metadata:
  name: app
spec:
  replicas: 3
");
        let good = rep("apiVersion: argoproj.io/v1alpha1
kind: Rollout
metadata:
  name: app
spec:
  strategy:
    canary:
      canaryService: canary
      stableService: stable
      maxSurge: '25%'
      maxUnavailable: 0
      rollbackWindow:
        seconds: 60
      trafficRouting:
        alb:
          ingress: app-ing
          servicePort: 80
      steps:
        - setWeight: 10
        - pause: {duration: 1h}
        - setWeight: 50
        - pause: {}
        - setWeight: 100
        - pause: {}
");
        assert!(
            score_deploy(&bad) < score_deploy(&good),
            "untuned ({:.3}) must score below tuned ({:.3})",
            score_deploy(&bad),
            score_deploy(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = rep("apiVersion: apps/v1
kind: Deployment
metadata:
  name: app
");
        let s = score_deploy(&r);
        assert!(s > 0.0, "short untuned manifest must not score 0.0: {s}");
    }
}
