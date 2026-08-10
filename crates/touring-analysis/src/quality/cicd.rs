//! CI/CD Pipeline (D47 / F4.7) -- GitHub Actions / CircleCI / similar
//! workflow anti-patterns. The CI is the enforcement point for the other 49
//! dims; gaps in CI allow regressions to ship.
//!
//! | Smell | Signal | File |
//! |-------|--------|------|
//! | `no-clippy-gate` | a workflow file *without* `cargo clippy` (or any `clippy` invocation) | `.github/workflows/*.yml` |
//! | `no-test-gate` | a workflow file *without* `cargo test` (or any test invocation) | `.github/workflows/*.yml` |
//! | `no-permissions-block` | a workflow file *without* a `permissions:` block (default token = write) | `.github/workflows/*.yml` |
//! | `unpinned-action` | a `uses: org/action@` with a mutable tag (e.g. `@v4`, `@main`, `@latest`) instead of a SHA | `.github/workflows/*.yml` |
//! | `script-injection-risk` | a `run:` block interpolating `${{ github.event.* }}` (untrusted input) | `.github/workflows/*.yml` |
//! | `no-cache` | a workflow with `cargo build` / `cargo test` *without* `actions/cache` (slow CI) | `.github/workflows/*.yml` |
//! | `no-quality-gate` | a workflow *without* an `elite_aggregate.py --check` or `touring-quality` invocation | `.github/workflows/*.yml` |
//!
//! **Disjoint** from F4.6 build-config (which keys on `Cargo.toml` profile
//! shape; F4.7 keys on workflow-file shape) and F4.5 pkg-mgmt (which keys on
//! `Cargo.toml` dep count; F4.7 keys on workflow gates).
//!
//! **Sources (context7, `/rhysd/actionlint`, High reputation, bench 86.83):**
//! actionlint checks syntax + types + `${{ }}` expressions + runner labels +
//! script injection (`github.event.*` interpolated into `run:` is a script-
//! injection vector). GitHub security guidance: pin third-party actions
//! by SHA (immutable) -- `@v4` is mutable. `permissions:` block with the
//! least-privilege value (e.g. `permissions: read-all`) limits the GITHUB_TOKEN.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

const SCALE: f32 = 6.0;

/// CI/CD-pipeline findings for one workflow file.
pub type CicdReport = crate::quality::SmellReport;

const CLIPPY: &[u8] = b"clippy";
const CARGO_TEST: &[u8] = b"cargo test";
const PERMISSIONS: &[u8] = b"permissions:";
const USES_LINE: &[u8] = b"uses: ";
const RUN_LINE: &[u8] = b"run:";
const ACTIONS_CACHE: &[u8] = b"actions/cache";
const ELITE_AGGREGATE: &[u8] = b"elite_aggregate";
const TOURING_QUALITY: &[u8] = b"touring-quality";
const GITHUB_EVENT_INJECT: &[u8] = b"github.event";

fn has_in_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> bool {
    memmem::find_iter(bytes, needle).any(|off| !offset_suppressed(off, regions))
}

fn count_in_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> usize {
    memmem::find_iter(bytes, needle)
        .filter(|&off| !offset_suppressed(off, regions))
        .count()
}

fn detect_no_clippy_gate(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    if has_in_executable(bytes, regions, CLIPPY) {
        0
    } else {
        1
    }
}

fn detect_no_test_gate(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    if has_in_executable(bytes, regions, CARGO_TEST) {
        0
    } else {
        1
    }
}

fn detect_no_permissions_block(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    if has_in_executable(bytes, regions, PERMISSIONS) {
        0
    } else {
        1
    }
}

/// Mutable-tag uses: `actions/checkout@v4`, `@v1`, `@main`, `@latest`.
/// Mutable tags are `@<version>`, `@<word>`. SHA pins are 40 hex chars.
fn detect_unpinned_action(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let uses_count = count_in_executable(bytes, regions, USES_LINE);
    if uses_count == 0 {
        return 0;
    }
    let mut count = 0;
    for off in memmem::find_iter(bytes, USES_LINE) {
        if offset_suppressed(off, regions) {
            continue;
        }
        let after = off + USES_LINE.len();
        let rest = &bytes[after..];
        // Find `@` after the action name.
        if let Some(rel_at) = memmem::find(rest, b"@") {
            let after_at = rel_at + 1;
            let ref_str = &rest[after_at..];
            // SHA pin: 40 hex chars (no terminator needed since `rest` is bounded).
            if ref_str.len() >= 40 {
                let sha = &ref_str[..40];
                if sha.iter().all(|b| b.is_ascii_hexdigit()) {
                    continue;
                }
            }
            // Mutable: any non-SHA ref (`@v4`, `@v4.1`, `@main`, `@latest`,
            // `@release-v1`).
            let end = ref_str
                .iter()
                .position(|&b| b == b'\n' || b == b' ' || b == b'#' || b == b'"')
                .unwrap_or(ref_str.len());
            let r = &ref_str[..end];
            if r.is_empty() {
                continue;
            }
            // Local path uses: `uses: ./path` -- no `@` separator.
            if r.starts_with(b"./") || r.starts_with(b"docker://") {
                continue;
            }
            // Count as mutable.
            count += 1;
        }
    }
    count
}

/// `run:` line that interpolates `${{ github.event.* }}` (untrusted input
/// inlined into a shell command -- script injection).
fn detect_script_injection_risk(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let mut count = 0;
    for off in memmem::find_iter(bytes, RUN_LINE) {
        if offset_suppressed(off, regions) {
            continue;
        }
        let after = off + RUN_LINE.len();
        let rest = &bytes[after..];
        // Look for both `github.event` and `${{` in the same line.
        let line_end = rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len());
        let line = &rest[..line_end];
        if memmem::find(line, GITHUB_EVENT_INJECT).is_some() && memmem::find(line, b"${{").is_some()
        {
            count += 1;
        }
    }
    count
}

fn detect_no_cache(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    // `actions/cache` only matters if there's a Rust build/test step.
    let has_rust = has_in_executable(bytes, regions, b"cargo build")
        || has_in_executable(bytes, regions, CARGO_TEST)
        || has_in_executable(bytes, regions, b"cargo check");
    if !has_rust {
        return 0;
    }
    if has_in_executable(bytes, regions, ACTIONS_CACHE) {
        0
    } else {
        1
    }
}

fn detect_no_quality_gate(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    if has_in_executable(bytes, regions, ELITE_AGGREGATE)
        || has_in_executable(bytes, regions, TOURING_QUALITY)
    {
        0
    } else {
        1
    }
}

/// Analyze CI/CD-pipeline smells in `source` (a YAML workflow file). The lang parameter is ignored -- this engine is YAML-only.
pub fn analyze_cicd(source: &str, _lang: &str) -> CicdReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, "rust");
    let mut report = CicdReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    report.push(
        "no cargo clippy in workflow (no lint gate -- warnings can ship)",
        detect_no_clippy_gate(bytes, &regions),
        0.9,
    );
    report.push(
        "no cargo test in workflow (no test gate -- broken tests can ship)",
        detect_no_test_gate(bytes, &regions),
        1.0,
    );
    report.push(
        "no `permissions:` block (default GITHUB_TOKEN is write -- least-privilege required)",
        detect_no_permissions_block(bytes, &regions),
        0.7,
    );
    report.push(
        "action pinned by mutable tag (use SHA for supply-chain safety -- e.g. `actions/checkout@<40-hex-sha>`)",
        detect_unpinned_action(bytes, &regions),
        0.8,
    );
    report.push(
        "run: block interpolates `${{ github.event.* }}` (script injection vector -- pass via env var)",
        detect_script_injection_risk(bytes, &regions),
        0.9,
    );
    report.push(
        "no `actions/cache` for cargo build/test (slow CI -- cache ~/.cargo and target/)",
        detect_no_cache(bytes, &regions),
        0.5,
    );
    report.push(
        "no quality gate (no `elite_aggregate.py --check` or `touring-quality` invocation)",
        detect_no_quality_gate(bytes, &regions),
        0.7,
    );
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`CicdReport`] as `1 - density·SCALE`, clamped to `[0, 1]`.
pub fn score_cicd(report: &CicdReport) -> f32 {
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rep(src: &str) -> CicdReport {
        analyze_cicd(src, "rust")
    }

    #[test]
    fn empty_file_clean() {
        // Empty input = no clippy/test gates, no permissions, etc -- the
        // engine legitimately flags an empty workflow as missing all gates.
        let r = rep("");
        assert!(
            r.violations >= 1,
            "empty workflow: every gate missing is a finding: {:?}",
            r.findings
        );
    }

    #[test]
    fn no_clippy_flagged() {
        let src = r#"name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo build
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("clippy")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn no_test_flagged() {
        let src = r#"name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo clippy -- -D warnings
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("cargo test")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn no_permissions_flagged() {
        let src = r#"name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo test
      - run: cargo clippy
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("permissions")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn permissions_set_clean() {
        let src = r#"name: CI
permissions:
  contents: read
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo test
      - run: cargo clippy
"#;
        let r = rep(src);
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("permissions")),
            "permissions set is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn mutable_action_tag_flagged() {
        let src = r#"- uses: actions/checkout@v4
- uses: actions/setup-node@v4
- uses: actions/cache@v4
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("mutable tag")),
            "mutable tag flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn sha_pinned_action_clean() {
        let src = r#"- uses: actions/checkout@692973e3d937129bcbf40652eb9f2f61becf3332
"#;
        let r = rep(src);
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("mutable tag")),
            "SHA-pinned action is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn script_injection_flagged() {
        let src = r#"- name: Show
  run: echo "${{ github.event.head_commit.message }}"
"#;
        let r = rep(src);
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("script injection")),
            "script injection flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn no_cache_for_rust_flagged() {
        let src = r#"- run: cargo build
- run: cargo test
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("actions/cache")),
            "no actions/cache for cargo flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn cache_present_clean() {
        let src = r#"- uses: actions/cache@v4
- run: cargo build
- run: cargo test
"#;
        let r = rep(src);
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("actions/cache")),
            "actions/cache present is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn no_quality_gate_flagged() {
        let src = r#"- run: cargo test
- run: cargo clippy
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("quality gate")),
            "no quality gate flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn quality_gate_present_clean() {
        let src = r#"- run: cargo test
- run: python3 docs/elite_aggregate.py --check
"#;
        let r = rep(src);
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("quality gate")),
            "quality gate present is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn fully_tuned_workflow_clean() {
        let src = r#"name: CI
permissions:
  contents: read
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@692973e3d937129bcbf40652eb9f2f61becf3332
      - uses: actions/cache@1bd1e32a3bdc45362d1e726936510720a7c30a57
      - run: cargo test
      - run: cargo clippy -- -D warnings
      - run: python3 docs/elite_aggregate.py --check
"#;
        let r = rep(src);
        assert_eq!(
            r.violations, 0,
            "fully-tuned workflow is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = rep("- run: cargo build
");
        let good = rep("permissions:
  contents: read
- uses: actions/checkout@692973e3d937129bcbf40652eb9f2f61becf3332
- uses: actions/cache@1bd1e32a3bdc45362d1e726936510720a7c30a57
- run: cargo test
- run: cargo clippy -- -D warnings
- run: python3 docs/elite_aggregate.py --check
");
        assert!(
            score_cicd(&bad) < score_cicd(&good),
            "untuned ({:.3}) must score below tuned ({:.3})",
            score_cicd(&bad),
            score_cicd(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = rep("- run: cargo build
");
        let s = score_cicd(&r);
        assert!(s > 0.0, "short untuned workflow must not score 0.0: {s}");
    }
}
