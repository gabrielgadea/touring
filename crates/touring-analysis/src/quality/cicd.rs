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
//! | `no-cache` | a workflow with `cargo build` / `cargo test` *without* a cache step (slow CI) -- except in a job that holds an OIDC token | `.github/workflows/*.yml` |
//! | `no-quality-gate` | a workflow *without* an `elite_aggregate.py --check` or `touring-quality` invocation | `.github/workflows/*.yml` |
//!
//! **Supply-chain rules (structural, severity-capped).** The smells above are
//! *densities*: `weight x count / lines`. That is the wrong shape for a finding whose
//! blast radius does not depend on how long the file is -- one `id-token: write` is as
//! exploitable in 561 lines as in 27, yet it cost `ci.yml` ~0.03 of its score and let
//! it read Diamond. These rules therefore also set a **ceiling** on the score
//! ([`CiSeverity::score_ceiling`]) that no amount of surrounding clean text lifts.
//! They read the job/step shape (`structure`), not substrings:
//!
//! | Rule | Signal | Severity |
//! |------|--------|----------|
//! | [`CiRule::PwnRequestCheckout`] | `pull_request_target` + a checkout of the pull request head | critical |
//! | [`CiRule::OidcWorkflowScope`] | `id-token: write` in the top-level `permissions` (every job inherits it) | high |
//! | [`CiRule::OidcWithSharedCache`] | a job holding `id-token: write` (own or inherited) that restores a shared cache | high |
//! | [`CiRule::CheckoutPersistsCredentials`] | `actions/checkout` without `persist-credentials: false` in a job that can write | medium |
//! | [`CiRule::MutableThirdPartyWithWrite`] | a non-`actions/`/`github/` action pinned by a mutable tag in a job that can write | medium (ceiling only) |
//!
//! A job-level `permissions:` REPLACES the workflow-level one (it does not merge); a
//! job with no `permissions:` at any level is treated as write-capable, as the
//! `no-permissions-block` finding already states ("default token = write").
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
//! The structural rules follow the cache-poisoning + OIDC-token-theft class of the
//! TanStack incident and GitHub's "keeping your GitHub Actions secure" guidance
//! (pwn requests, `persist-credentials`).

use std::cmp::Reverse;
use std::ops::Deref;

use memchr::memmem;

use super::SmellReport;
use super::code_regions::offset_suppressed;

mod structure;
use structure::{Job, Perms, Step, Workflow, parse_workflows, yaml_comment_regions};

const SCALE: f32 = 6.0;

/// How bad a structural CI-security finding is; sets the score ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CiSeverity {
    /// Hygiene with a real attack path (a persisted token, a mutable third-party ref).
    Medium,
    /// A privileged token reachable from shared state.
    High,
    /// Untrusted code runs with a privileged token.
    Critical,
}

impl CiSeverity {
    /// The highest score a workflow carrying a finding of this severity can have.
    ///
    /// `Medium` stops just under Diamond (0.95), `High` under Gold (0.80, a WARN),
    /// `Critical` under the 0.50 FAIL line.
    #[must_use]
    pub const fn score_ceiling(self) -> f32 {
        match self {
            Self::Medium => 0.90,
            Self::High => 0.70,
            Self::Critical => 0.40,
        }
    }
}

/// A structural supply-chain rule of F4.7 (see the module table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CiRule {
    /// `pull_request_target` checks out the pull request head.
    PwnRequestCheckout,
    /// `id-token: write` at the top-level `permissions`.
    OidcWorkflowScope,
    /// A job with an OIDC token restores a shared cache.
    OidcWithSharedCache,
    /// `actions/checkout` keeps the token in `.git/config` in a write-capable job.
    CheckoutPersistsCredentials,
    /// A third-party action by mutable tag in a write-capable job.
    MutableThirdPartyWithWrite,
}

/// Every rule, most severe first -- the order findings are reported in.
const RULES: [CiRule; 5] = [
    CiRule::PwnRequestCheckout,
    CiRule::OidcWorkflowScope,
    CiRule::OidcWithSharedCache,
    CiRule::CheckoutPersistsCredentials,
    CiRule::MutableThirdPartyWithWrite,
];

impl CiRule {
    /// The rule's severity.
    #[must_use]
    pub const fn severity(self) -> CiSeverity {
        match self {
            Self::PwnRequestCheckout => CiSeverity::Critical,
            Self::OidcWorkflowScope | Self::OidcWithSharedCache => CiSeverity::High,
            Self::CheckoutPersistsCredentials | Self::MutableThirdPartyWithWrite => {
                CiSeverity::Medium
            }
        }
    }

    /// The finding text (what `touring-quality check --gate F4.7` prints).
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::PwnRequestCheckout => {
                "`pull_request_target` checks out the pull request head (untrusted code runs with a write token and secrets -- check out the base, or use `pull_request`)"
            }
            Self::OidcWorkflowScope => {
                "`id-token: write` in the workflow-level `permissions` (every job inherits an OIDC token -- grant it only to the job that mints it)"
            }
            Self::OidcWithSharedCache => {
                "job holding an OIDC token (`id-token: write`) restores a shared cache (cache poisoning -> token theft -- drop the cache from this job)"
            }
            Self::CheckoutPersistsCredentials => {
                "`actions/checkout` without `persist-credentials: false` in a job that can write (the token stays in .git/config for every later step)"
            }
            Self::MutableThirdPartyWithWrite => {
                "third-party action pinned by a mutable tag in a job that can write (supply-chain: pin it by 40-hex SHA)"
            }
        }
    }

    /// Density weight. The mutable-ref rule weighs nothing: the generic
    /// `unpinned-action` smell already counted the same `uses:` line; this rule only
    /// adds the ceiling.
    const fn weight(self) -> f32 {
        match self {
            Self::PwnRequestCheckout | Self::OidcWorkflowScope | Self::OidcWithSharedCache => 1.0,
            Self::CheckoutPersistsCredentials => 0.6,
            Self::MutableThirdPartyWithWrite => 0.0,
        }
    }
}

/// One structural finding with its location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CiFinding {
    /// The rule that fired.
    pub rule: CiRule,
    /// The job it fired in (`None` for a workflow-level finding).
    pub job: Option<String>,
    /// 1-based line (of the step, or of the `permissions:` key).
    pub line: usize,
}

/// CI/CD-pipeline findings for one workflow file: the density smells
/// (`Deref` to [`SmellReport`]) plus the severity-capped structural findings.
#[derive(Debug, Clone)]
pub struct CicdReport {
    smells: SmellReport,
    /// The structural supply-chain findings, with locations.
    pub structural: Vec<CiFinding>,
}

impl Deref for CicdReport {
    type Target = SmellReport;
    fn deref(&self) -> &SmellReport {
        &self.smells
    }
}

impl CicdReport {
    /// The finding the evidence points at: the first occurrence of the most severe rule
    /// that fired -- the same rule that leads `findings`, and the one the score ceiling
    /// comes from. `None` when no structural rule fired.
    #[must_use]
    pub fn primary(&self) -> Option<&CiFinding> {
        RULES
            .iter()
            .find_map(|rule| self.structural.iter().find(|f| f.rule == *rule))
    }

    /// The highest score the structural findings allow (1.0 when there are none).
    #[must_use]
    pub fn score_ceiling(&self) -> f32 {
        self.structural
            .iter()
            .map(|f| f.rule.severity().score_ceiling())
            .fold(1.0, f32::min)
    }
}

const CLIPPY: &[u8] = b"clippy";
const CARGO_TEST: &[u8] = b"cargo test";
const PERMISSIONS: &[u8] = b"permissions:";
const USES_LINE: &[u8] = b"uses: ";
const RUN_LINE: &[u8] = b"run:";
const ELITE_AGGREGATE: &[u8] = b"elite_aggregate";
const TOURING_QUALITY: &[u8] = b"touring-quality";
const GITHUB_EVENT_INJECT: &[u8] = b"github.event";

/// Actions that restore a shared cache into the job (text needles; the `uses:` owner
/// part is case-insensitive on GitHub, the needles are the spellings in use).
const CACHE_NEEDLES: [&[u8]; 4] = [
    b"actions/cache",
    b"Swatinem/rust-cache",
    b"swatinem/rust-cache",
    b"sccache-action",
];

/// Owners whose actions are first-party: GitHub itself.
const FIRST_PARTY_OWNERS: [&str; 2] = ["actions", "github"];

/// `with:` values that name the untrusted pull request head.
const PR_HEAD_MARKERS: [&str; 3] = [
    "github.event.pull_request.head",
    "github.head_ref",
    "refs/pull/",
];

fn has_in_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> bool {
    memmem::find_iter(bytes, needle).any(|off| !offset_suppressed(off, regions))
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
///
/// Each `uses:` is judged on ITS OWN line: the `@` and the ref are looked up in the
/// remainder of that line only. (The search used to run over the rest of the FILE, so
/// a local `uses: ./action` -- no `@` on its line -- borrowed the next action's ref
/// and was counted as a second mutable tag.)
fn detect_unpinned_action(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    memmem::find_iter(bytes, USES_LINE)
        .filter(|&off| !offset_suppressed(off, regions))
        .filter(|&off| {
            let rest = &bytes[off + USES_LINE.len()..];
            let line = &rest[..rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len())];
            let value = line
                .split(|&b| b == b' ' || b == b'#' || b == b'"' || b == b'\'' || b == b'\r')
                .find(|part| !part.is_empty())
                .unwrap_or_default();
            is_mutable_ref(value)
        })
        .count()
}

/// `value` is a `uses:` target pinned by a mutable ref: it has an `@`, a non-empty
/// ref, and the ref is not a 40-hex commit SHA. Local (`./x`) and `docker://` targets
/// have no ref to pin.
fn is_mutable_ref(value: &[u8]) -> bool {
    if value.starts_with(b"./") || value.starts_with(b"docker://") {
        return false;
    }
    let Some(at) = value.iter().rposition(|&b| b == b'@') else {
        return false;
    };
    let reference = &value[at + 1..];
    !reference.is_empty() && !is_commit_sha(reference)
}

/// 40 hex digits (a git commit SHA-1) -- the only immutable action ref.
fn is_commit_sha(reference: &[u8]) -> bool {
    reference.len() == 40 && reference.iter().all(u8::is_ascii_hexdigit)
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

/// The job's effective permissions: its own block REPLACES the workflow's.
fn effective_perms<'a>(wf: &'a Workflow, job: &'a Job) -> Option<&'a Perms> {
    job.permissions.as_ref().or(wf.permissions.as_ref())
}

/// The job can mint an OIDC token.
fn job_has_oidc(wf: &Workflow, job: &Job) -> bool {
    effective_perms(wf, job).is_some_and(Perms::id_token_write)
}

/// The job's token can write. A job with no `permissions:` anywhere is write-capable
/// (the default token), as the `no-permissions-block` finding states.
fn job_can_write(wf: &Workflow, job: &Job) -> bool {
    match effective_perms(wf, job) {
        Some(perms) => perms.any_write(),
        None => true,
    }
}

fn step_runs_cargo(step: &Step) -> bool {
    [b"cargo build".as_slice(), CARGO_TEST, b"cargo check"]
        .iter()
        .any(|cmd| memmem::find(step.text.as_bytes(), cmd).is_some())
}

/// A Rust build/test with no cache step anywhere (slow CI).
///
/// A cache step is any of [`CACHE_NEEDLES`] -- `Swatinem/rust-cache` counts, it used
/// to be reported as "no `actions/cache`". A job that holds an OIDC token is NOT
/// asked for a cache: restoring one into it is the poisoning vector
/// ([`CiRule::OidcWithSharedCache`]), so the absence is the safe state.
fn detect_no_cache(bytes: &[u8], regions: &[(usize, usize)], workflows: &[Workflow]) -> usize {
    if CACHE_NEEDLES
        .iter()
        .any(|needle| has_in_executable(bytes, regions, needle))
    {
        return 0;
    }
    if workflows.iter().all(|wf| wf.jobs.is_empty()) {
        // A bare step list (a fragment): no jobs to tell OIDC from plain ones.
        let has_rust = has_in_executable(bytes, regions, b"cargo build")
            || has_in_executable(bytes, regions, CARGO_TEST)
            || has_in_executable(bytes, regions, b"cargo check");
        return usize::from(has_rust);
    }
    let needs_cache = workflows.iter().any(|wf| {
        wf.jobs
            .iter()
            .any(|job| !job_has_oidc(wf, job) && job.steps.iter().any(step_runs_cargo))
    });
    usize::from(needs_cache)
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

fn action_name(uses: &str) -> String {
    uses.split('@')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn is_checkout(uses: &str) -> bool {
    action_name(uses) == "actions/checkout"
}

fn is_cache_action(uses: &str) -> bool {
    let name = action_name(uses);
    name == "actions/cache"
        || name.starts_with("actions/cache/")
        || name == "swatinem/rust-cache"
        || name.ends_with("/sccache-action")
}

/// An action outside the `actions/` and `github/` owners (local and `docker://`
/// targets are not actions anyone else publishes).
fn is_third_party(uses: &str) -> bool {
    if uses.starts_with("./") || uses.starts_with("docker://") {
        return false;
    }
    let name = action_name(uses);
    let owner = name.split('/').next().unwrap_or_default();
    !FIRST_PARTY_OWNERS.contains(&owner)
}

fn pinned_by_sha(uses: &str) -> bool {
    uses.rsplit_once('@')
        .is_some_and(|(_, reference)| is_commit_sha(reference.as_bytes()))
}

fn persist_credentials_disabled(step: &Step) -> bool {
    step.with
        .iter()
        .any(|(k, v)| k == "persist-credentials" && v.eq_ignore_ascii_case("false"))
}

fn checks_out_pr_head(step: &Step) -> bool {
    step.with
        .iter()
        .any(|(_, v)| PR_HEAD_MARKERS.iter().any(|marker| v.contains(marker)))
}

/// The rules that fire on one step of a job.
fn step_rules(wf: &Workflow, job: &Job, step: &Step, uses: &str) -> Vec<CiRule> {
    let mut rules = Vec::new();
    if job_has_oidc(wf, job) && is_cache_action(uses) {
        rules.push(CiRule::OidcWithSharedCache);
    }
    if is_checkout(uses) {
        if wf.pull_request_target && checks_out_pr_head(step) {
            rules.push(CiRule::PwnRequestCheckout);
        }
        if job_can_write(wf, job) && !persist_credentials_disabled(step) {
            rules.push(CiRule::CheckoutPersistsCredentials);
        }
    }
    if job_can_write(wf, job) && is_third_party(uses) && !pinned_by_sha(uses) {
        rules.push(CiRule::MutableThirdPartyWithWrite);
    }
    rules
}

/// The structural supply-chain findings of every workflow in the text.
fn structural_findings(workflows: &[Workflow]) -> Vec<CiFinding> {
    let mut out = Vec::new();
    for wf in workflows {
        if let Some(perms) = wf.permissions.as_ref().filter(|p| p.id_token_write()) {
            out.push(CiFinding {
                rule: CiRule::OidcWorkflowScope,
                job: None,
                line: perms.line,
            });
        }
        for job in &wf.jobs {
            for step in &job.steps {
                let Some(uses) = step.uses.as_deref() else {
                    continue;
                };
                for rule in step_rules(wf, job, step, uses) {
                    out.push(CiFinding {
                        rule,
                        job: Some(job.id.clone()),
                        line: step.line,
                    });
                }
            }
        }
    }
    out
}

/// Analyze CI/CD-pipeline smells in `source` (a YAML workflow file). The lang parameter is ignored -- this engine is YAML-only.
pub fn analyze_cicd(source: &str, _lang: &str) -> CicdReport {
    let bytes = source.as_bytes();
    let regions = yaml_comment_regions(source);
    let workflows = parse_workflows(source);
    let structural = structural_findings(&workflows);
    let mut smells = SmellReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    // Structural findings first, most severe first: they are why the score is capped,
    // so they are what the evidence ("top: ...") must name.
    let mut ranked: Vec<&'static str> = Vec::new();
    for rule in RULES {
        let count = structural.iter().filter(|f| f.rule == rule).count();
        smells.push(rule.message(), count, rule.weight());
        if count > 0 {
            ranked.push(rule.message());
        }
    }
    smells.push(
        "no cargo clippy in workflow (no lint gate -- warnings can ship)",
        detect_no_clippy_gate(bytes, &regions),
        0.9,
    );
    smells.push(
        "no cargo test in workflow (no test gate -- broken tests can ship)",
        detect_no_test_gate(bytes, &regions),
        1.0,
    );
    smells.push(
        "no `permissions:` block (default GITHUB_TOKEN is write -- least-privilege required)",
        detect_no_permissions_block(bytes, &regions),
        0.7,
    );
    smells.push(
        "action pinned by mutable tag (use SHA for supply-chain safety -- e.g. `actions/checkout@<40-hex-sha>`)",
        detect_unpinned_action(bytes, &regions),
        0.8,
    );
    smells.push(
        "run: block interpolates `${{ github.event.* }}` (script injection vector -- pass via env var)",
        detect_script_injection_risk(bytes, &regions),
        0.9,
    );
    smells.push(
        "no cache step (`actions/cache` / `Swatinem/rust-cache`) for cargo build/test (slow CI -- cache ~/.cargo and target/)",
        detect_no_cache(bytes, &regions, &workflows),
        0.5,
    );
    smells.push(
        "no quality gate (no `elite_aggregate.py --check` or `touring-quality` invocation)",
        detect_no_quality_gate(bytes, &regions),
        0.7,
    );
    smells.findings.sort_by_key(|f| {
        (
            ranked.iter().position(|m| *m == f.0).unwrap_or(usize::MAX),
            Reverse(f.1),
        )
    });
    CicdReport { smells, structural }
}

/// Score a [`CicdReport`]: `1 - density·SCALE` clamped to `[0, 1]`, then capped by
/// the structural findings' ceiling ([`CicdReport::score_ceiling`]) -- a
/// supply-chain hole is not diluted by the length of the file around it.
pub fn score_cicd(report: &CicdReport) -> f32 {
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
        .min(report.score_ceiling())
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

    // ---- supply-chain rules (F4.7 calibration, 17/10/2026) -----------------------
    //
    // These tests read the report through `findings` + `score_cicd` only (the API the
    // detector had before the structural rules), so the same file can be run against
    // the original detector and shown red.

    const GATES: &str = "      - run: cargo test\n      - run: cargo clippy -- -D warnings\n      - run: python3 docs/elite_aggregate.py --check\n";
    const CHECKOUT: &str = "      - uses: actions/checkout@692973e3d937129bcbf40652eb9f2f61becf3332\n        with:\n          persist-credentials: false\n";
    const SHA: &str = "692973e3d937129bcbf40652eb9f2f61becf3332";

    /// A one-job workflow. `on` is the indented body of `on:`; `top`/`job` are
    /// `permissions:` blocks already indented for their level (or empty).
    fn workflow(on: &str, top: &str, job: &str, steps: &str) -> String {
        format!(
            "name: CI\non:\n{on}\n{top}jobs:\n  build:\n    runs-on: ubuntu-latest\n{job}    steps:\n{steps}{GATES}"
        )
    }

    fn has(r: &CicdReport, needle: &str) -> bool {
        r.findings.iter().any(|(m, _)| m.contains(needle))
    }

    fn count_of(r: &CicdReport, needle: &str) -> usize {
        r.findings
            .iter()
            .filter(|(m, _)| m.contains(needle))
            .map(|(_, c)| *c)
            .sum()
    }

    /// The same text with `n` comment lines of padding -- file length is irrelevant
    /// to a supply-chain hole, and must stay so to the score.
    fn padded(src: &str, n: usize) -> String {
        format!("{}{src}", "# padding\n".repeat(n))
    }

    const READ_ONLY: &str = "permissions:\n  contents: read\n";
    const OIDC_TOP: &str = "permissions:\n  contents: read\n  id-token: write\n";
    const OIDC_JOB: &str = "    permissions:\n      id-token: write\n";
    const RUST_CACHE: &str =
        "      - uses: Swatinem/rust-cache@e18b497796c12c097a38f9edb9d0641fb99eee32\n";

    #[test]
    fn rule_a_oidc_at_workflow_scope_is_high_and_capped() {
        let r = rep(&workflow("  push:", OIDC_TOP, "", CHECKOUT));
        assert!(has(&r, "workflow-level `permissions`"), "{:?}", r.findings);
        assert!(
            score_cicd(&r) <= 0.70,
            "high ceiling, got {}",
            score_cicd(&r)
        );
        // ...and the length of the file around it does not lift the ceiling.
        let long = rep(&padded(&workflow("  push:", OIDC_TOP, "", CHECKOUT), 800));
        assert!(
            score_cicd(&long) <= 0.70,
            "diluted to {}",
            score_cicd(&long)
        );
    }

    #[test]
    fn rule_a_negative_oidc_scoped_to_one_job_is_not_workflow_scope() {
        let r = rep(&workflow("  push:", READ_ONLY, OIDC_JOB, CHECKOUT));
        assert!(!has(&r, "workflow-level `permissions`"), "{:?}", r.findings);
        let flow = rep(&workflow(
            "  push:",
            "permissions: { contents: read }\n",
            "",
            CHECKOUT,
        ));
        assert!(
            !has(&flow, "workflow-level `permissions`"),
            "{:?}",
            flow.findings
        );
    }

    #[test]
    fn rule_a_write_all_and_flow_style_count_as_oidc() {
        let all = rep(&workflow(
            "  push:",
            "permissions: write-all\n",
            "",
            CHECKOUT,
        ));
        assert!(
            has(&all, "workflow-level `permissions`"),
            "{:?}",
            all.findings
        );
        let flow = rep(&workflow(
            "  push:",
            "permissions: { contents: read, id-token: write }\n",
            "",
            CHECKOUT,
        ));
        assert!(
            has(&flow, "workflow-level `permissions`"),
            "{:?}",
            flow.findings
        );
    }

    #[test]
    fn rule_a_b_negative_a_write_scope_other_than_id_token_is_not_oidc() {
        let top = "permissions:\n  contents: write\n  packages: write\n";
        let r = rep(&workflow("  push:", top, "", CHECKOUT));
        assert!(!has(&r, "workflow-level `permissions`"), "{:?}", r.findings);
        // ...so a cache in a job that merely writes contents is just a cache.
        let steps = format!("{CHECKOUT}{RUST_CACHE}");
        let write_job = "    permissions:\n      contents: write\n";
        let c = rep(&workflow("  push:", READ_ONLY, write_job, &steps));
        assert!(!has(&c, "restores a shared cache"), "{:?}", c.findings);
    }

    #[test]
    fn rule_b_job_with_oidc_and_cache_is_high_for_every_cache_action() {
        for cache in [
            "      - uses: Swatinem/rust-cache@e18b497796c12c097a38f9edb9d0641fb99eee32\n",
            "      - uses: actions/cache@1bd1e32a3bdc45362d1e726936510720a7c30a57\n",
            "      - uses: actions/cache/restore@1bd1e32a3bdc45362d1e726936510720a7c30a57\n",
            "      - uses: mozilla-actions/sccache-action@7d986dd989559c6ecdb630a3fd2557667be217ad\n",
        ] {
            let steps = format!("{CHECKOUT}{cache}");
            let r = rep(&workflow("  push:", READ_ONLY, OIDC_JOB, &steps));
            assert!(
                has(&r, "restores a shared cache"),
                "{cache}: {:?}",
                r.findings
            );
            assert!(score_cicd(&r) <= 0.70, "{cache}: {}", score_cicd(&r));
        }
    }

    #[test]
    fn rule_b_oidc_inherited_from_the_workflow_counts() {
        let steps = format!("{CHECKOUT}{RUST_CACHE}");
        let r = rep(&workflow("  push:", OIDC_TOP, "", &steps));
        assert!(has(&r, "restores a shared cache"), "{:?}", r.findings);
    }

    #[test]
    fn rule_b_negatives_override_no_oidc_and_no_cache() {
        let steps = format!("{CHECKOUT}{RUST_CACHE}");
        // The job's own `permissions:` REPLACES the workflow's: no id-token left.
        let overridden = rep(&workflow(
            "  push:",
            OIDC_TOP,
            "    permissions:\n      contents: read\n",
            &steps,
        ));
        assert!(
            !has(&overridden, "restores a shared cache"),
            "{:?}",
            overridden.findings
        );
        // A cache without a token is just a cache.
        let plain = rep(&workflow("  push:", READ_ONLY, "", &steps));
        assert!(
            !has(&plain, "restores a shared cache"),
            "{:?}",
            plain.findings
        );
        // A token without a cache is rule (a)/(none), not (b).
        let no_cache = rep(&workflow("  push:", READ_ONLY, OIDC_JOB, CHECKOUT));
        assert!(
            !has(&no_cache, "restores a shared cache"),
            "{:?}",
            no_cache.findings
        );
    }

    #[test]
    fn rule_c_pull_request_target_with_head_checkout_is_critical() {
        for head in [
            "${{ github.event.pull_request.head.sha }}",
            "${{ github.head_ref }}",
            "refs/pull/${{ github.event.number }}/merge",
        ] {
            let steps = format!(
                "      - uses: actions/checkout@{SHA}\n        with:\n          ref: {head}\n          persist-credentials: false\n"
            );
            let r = rep(&workflow("  pull_request_target:", READ_ONLY, "", &steps));
            assert!(
                has(&r, "`pull_request_target` checks out"),
                "{head}: {:?}",
                r.findings
            );
            assert!(
                score_cicd(&r) <= 0.40,
                "{head}: critical ceiling, got {}",
                score_cicd(&r)
            );
            assert!(
                r.findings[0].0.contains("pull_request_target"),
                "the cause of the cap leads the evidence: {:?}",
                r.findings
            );
        }
        let long = rep(&padded(
            &workflow(
                "  pull_request_target:",
                READ_ONLY,
                "",
                &format!(
                    "      - uses: actions/checkout@{SHA}\n        with:\n          ref: ${{{{ github.event.pull_request.head.sha }}}}\n          persist-credentials: false\n"
                ),
            ),
            800,
        ));
        assert!(
            score_cicd(&long) <= 0.40,
            "diluted to {}",
            score_cicd(&long)
        );
    }

    #[test]
    fn structural_findings_lead_the_evidence_even_when_a_smell_is_more_frequent() {
        // Three mutable tags outnumber the one pwn request; the evidence ("top: ...")
        // must still name the finding that capped the score.
        let steps = format!(
            "      - uses: actions/checkout@{SHA}\n        with:\n          ref: ${{{{ github.event.pull_request.head.sha }}}}\n          persist-credentials: false\n      - uses: a/b@v1\n      - uses: c/d@v1\n      - uses: e/f@v1\n"
        );
        let r = rep(&workflow("  pull_request_target:", READ_ONLY, "", &steps));
        assert_eq!(count_of(&r, "mutable tag"), 3, "{:?}", r.findings);
        assert!(
            r.findings[0].0.contains("pull_request_target"),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn primary_is_the_most_severe_finding_not_the_first_in_file_order() {
        // job `a` comes first and only persists credentials (medium); job `b` checks
        // out the pull request head (critical). The evidence must point at `b`.
        let src = format!(
            "name: CI\non:\n  pull_request_target:\npermissions:\n  contents: write\njobs:\n  a:\n    steps:\n      - uses: actions/checkout@{SHA}\n  b:\n    steps:\n      - uses: actions/checkout@{SHA}\n        with:\n          ref: ${{{{ github.event.pull_request.head.sha }}}}\n          persist-credentials: false\n{GATES}"
        );
        let r = rep(&src);
        let first = r.structural.first().expect("a finding");
        assert_eq!(
            first.rule,
            CiRule::CheckoutPersistsCredentials,
            "premise: file order"
        );
        let primary = r.primary().expect("a primary finding");
        assert_eq!(primary.rule, CiRule::PwnRequestCheckout);
        assert_eq!(primary.job.as_deref(), Some("b"));
        assert_eq!(primary.line, 12, "the checkout step of job b");
        assert!(rep("- run: cargo test\n").primary().is_none());
    }

    #[test]
    fn rule_c_negatives_base_checkout_and_plain_pull_request() {
        // pull_request_target checking out the BASE (no `ref`) is the safe form.
        let base = rep(&workflow("  pull_request_target:", READ_ONLY, "", CHECKOUT));
        assert!(
            !has(&base, "`pull_request_target` checks out"),
            "{:?}",
            base.findings
        );
        // `pull_request` runs the head WITHOUT secrets: not a pwn request.
        let steps = format!(
            "      - uses: actions/checkout@{SHA}\n        with:\n          ref: ${{{{ github.event.pull_request.head.sha }}}}\n          persist-credentials: false\n"
        );
        let plain = rep(&workflow("  pull_request:", READ_ONLY, "", &steps));
        assert!(
            !has(&plain, "`pull_request_target` checks out"),
            "{:?}",
            plain.findings
        );
        // The trigger is read from `on:`, not from any other mention.
        let named = rep(&format!(
            "name: pull_request_target docs\n{}",
            workflow("  push:", READ_ONLY, "", &steps)
        ));
        assert!(
            !has(&named, "`pull_request_target` checks out"),
            "{:?}",
            named.findings
        );
    }

    #[test]
    fn rule_d_checkout_without_persist_credentials_in_a_write_job_is_medium() {
        let steps = format!("      - uses: actions/checkout@{SHA}\n");
        let write_job = "    permissions:\n      contents: write\n";
        let r = rep(&workflow("  push:", READ_ONLY, write_job, &steps));
        assert!(has(&r, "persist-credentials"), "{:?}", r.findings);
        assert!(
            score_cicd(&r) <= 0.90,
            "medium ceiling, got {}",
            score_cicd(&r)
        );
        // No `permissions:` at all is the write-capable default token.
        let implicit = rep(&workflow("  push:", "", "", &steps));
        assert!(
            has(&implicit, "persist-credentials"),
            "{:?}",
            implicit.findings
        );
    }

    #[test]
    fn rule_d_negatives_disabled_read_only_and_other_actions() {
        let write_job = "    permissions:\n      contents: write\n";
        for flag in ["false", "'false'", "\"false\"", "False"] {
            let steps = format!(
                "      - uses: actions/checkout@{SHA}\n        with:\n          persist-credentials: {flag}\n"
            );
            let r = rep(&workflow("  push:", READ_ONLY, write_job, &steps));
            assert!(!has(&r, "persist-credentials"), "{flag}: {:?}", r.findings);
        }
        let steps = format!("      - uses: actions/checkout@{SHA}\n");
        let read_only = rep(&workflow("  push:", READ_ONLY, "", &steps));
        assert!(
            !has(&read_only, "persist-credentials"),
            "{:?}",
            read_only.findings
        );
        let true_flag = rep(&workflow(
            "  push:",
            READ_ONLY,
            write_job,
            &format!(
                "      - uses: actions/checkout@{SHA}\n        with:\n          persist-credentials: true\n"
            ),
        ));
        assert!(
            has(&true_flag, "persist-credentials"),
            "{:?}",
            true_flag.findings
        );
        let other = rep(&workflow(
            "  push:",
            READ_ONLY,
            write_job,
            &format!("      - uses: actions/setup-node@{SHA}\n"),
        ));
        assert!(!has(&other, "persist-credentials"), "{:?}", other.findings);
    }

    #[test]
    fn rule_e_no_cache_is_not_asked_of_a_job_holding_an_oidc_token() {
        let steps = format!("{CHECKOUT}      - run: cargo build\n");
        let oidc = rep(&workflow("  push:", READ_ONLY, OIDC_JOB, &steps));
        assert!(!has(&oidc, "for cargo build/test"), "{:?}", oidc.findings);
        let inherited = rep(&workflow("  push:", OIDC_TOP, "", &steps));
        assert!(
            !has(&inherited, "for cargo build/test"),
            "{:?}",
            inherited.findings
        );
        // Same job without the token: the cache is wanted.
        let plain = rep(&workflow("  push:", READ_ONLY, "", &steps));
        assert!(has(&plain, "for cargo build/test"), "{:?}", plain.findings);
        assert!(
            has(&plain, "actions/cache"),
            "the message still names actions/cache"
        );
    }

    #[test]
    fn rule_e_rust_cache_counts_as_a_cache() {
        let steps = format!("{CHECKOUT}{RUST_CACHE}      - run: cargo build\n");
        let r = rep(&workflow("  push:", READ_ONLY, "", &steps));
        assert!(!has(&r, "for cargo build/test"), "{:?}", r.findings);
    }

    #[test]
    fn rule_f_mutable_third_party_action_with_write_cannot_read_diamond() {
        let steps = format!("{CHECKOUT}      - uses: dtolnay/rust-toolchain@stable\n");
        let write_job = "    permissions:\n      contents: write\n";
        let r = rep(&workflow("  push:", READ_ONLY, write_job, &steps));
        assert!(has(&r, "third-party action pinned"), "{:?}", r.findings);
        assert!(
            score_cicd(&r) < 0.95,
            "Diamond is off the table, got {}",
            score_cicd(&r)
        );
        // 561 lines of clean text around it change nothing (the dilution that gave
        // `ci.yml` 0.97 for the same ref).
        let long = rep(&padded(
            &workflow("  push:", READ_ONLY, write_job, &steps),
            800,
        ));
        assert!(score_cicd(&long) < 0.95, "diluted to {}", score_cicd(&long));
    }

    #[test]
    fn rule_f_negatives_read_only_sha_pinned_and_first_party() {
        let write_job = "    permissions:\n      contents: write\n";
        let mutable = format!("{CHECKOUT}      - uses: dtolnay/rust-toolchain@stable\n");
        let read_only = rep(&workflow("  push:", READ_ONLY, "", &mutable));
        assert!(
            !has(&read_only, "third-party action pinned"),
            "{:?}",
            read_only.findings
        );
        let pinned = format!("{CHECKOUT}      - uses: dtolnay/rust-toolchain@{SHA}\n");
        let sha = rep(&workflow("  push:", READ_ONLY, write_job, &pinned));
        assert!(
            !has(&sha, "third-party action pinned"),
            "{:?}",
            sha.findings
        );
        let first = format!(
            "{CHECKOUT}      - uses: actions/setup-node@v4\n      - uses: github/codeql-action/init@v3\n"
        );
        let fp = rep(&workflow("  push:", READ_ONLY, write_job, &first));
        assert!(!has(&fp, "third-party action pinned"), "{:?}", fp.findings);
        // A local action has no publisher to trust or distrust.
        let local = format!("{CHECKOUT}      - uses: ./.github/actions/setup\n");
        let l = rep(&workflow("  push:", READ_ONLY, write_job, &local));
        assert!(!has(&l, "third-party action pinned"), "{:?}", l.findings);
    }

    #[test]
    fn a_clean_hardened_workflow_keeps_its_score() {
        let steps = format!("{CHECKOUT}      - uses: dtolnay/rust-toolchain@{SHA}\n{RUST_CACHE}");
        let r = rep(&workflow("  push:", READ_ONLY, "", &steps));
        assert!(r.structural.is_empty(), "{:?}", r.structural);
        assert!(
            score_cicd(&r) >= 0.95,
            "{} {:?}",
            score_cicd(&r),
            r.findings
        );
    }

    #[test]
    fn local_action_does_not_borrow_the_next_actions_ref() {
        // `uses: ./local` has no `@` on its line; it used to find the NEXT line's.
        let r = rep("- uses: ./.github/actions/setup\n- uses: x/y@v1\n");
        assert_eq!(count_of(&r, "mutable tag"), 1, "{:?}", r.findings);
    }

    #[test]
    fn a_shell_glob_in_an_earlier_step_does_not_hide_later_lines() {
        // The detectors masked the file with the RUST lexer, where `/*` opens a block
        // comment: `client/omarchy/hooks/*/*` in one `run:` swallowed the rest of
        // `ci.yml` -- 7 mutable `uses:` read as 3, and every gate below it unseen.
        let src = "- run: ls client/omarchy/hooks/*/*\n- uses: foo/bar@v1\n- uses: baz/qux@v2\n- run: cargo clippy -- -D warnings\n";
        let r = rep(src);
        assert_eq!(count_of(&r, "mutable tag"), 2, "{:?}", r.findings);
        assert!(
            !has(&r, "no cargo clippy"),
            "the clippy gate after the glob is seen: {:?}",
            r.findings
        );
    }

    #[test]
    fn a_commented_out_uses_is_not_a_finding() {
        let r = rep(
            "# - uses: foo/bar@v1\n- uses: ok/ok@692973e3d937129bcbf40652eb9f2f61becf3332 # was @v1\n",
        );
        assert_eq!(count_of(&r, "mutable tag"), 0, "{:?}", r.findings);
    }

    #[test]
    fn structural_findings_carry_the_job_and_the_line() {
        let r = rep(&workflow("  push:", OIDC_TOP, "", ""));
        let f = r
            .structural
            .iter()
            .find(|f| f.rule == CiRule::OidcWorkflowScope)
            .expect("rule a fires");
        assert_eq!(f.job, None);
        assert_eq!(f.line, 4, "the `permissions:` key");
        let steps = format!("{CHECKOUT}{RUST_CACHE}");
        let c = rep(&workflow("  push:", READ_ONLY, OIDC_JOB, &steps));
        let f = c
            .structural
            .iter()
            .find(|f| f.rule == CiRule::OidcWithSharedCache)
            .expect("rule b fires");
        assert_eq!(f.job.as_deref(), Some("build"));
        assert_eq!(c.score_ceiling(), CiSeverity::High.score_ceiling());
    }

    #[test]
    fn severity_ceilings_are_ordered() {
        assert!(CiSeverity::Critical.score_ceiling() < CiSeverity::High.score_ceiling());
        assert!(CiSeverity::High.score_ceiling() < CiSeverity::Medium.score_ceiling());
        assert!(
            CiSeverity::Medium.score_ceiling() < 0.95,
            "no finding reads Diamond"
        );
        assert!(
            CiSeverity::Critical.score_ceiling() < 0.5,
            "critical is a FAIL"
        );
        assert!(CiSeverity::High.score_ceiling() < 0.8, "high is a WARN");
    }

    #[test]
    fn concatenated_workflows_do_not_share_permissions() {
        // The directory reader joins every workflow with `\n`: the OIDC grant of the
        // first file must not leak into the cache job of the second.
        let a = workflow("  push:", OIDC_TOP, "", CHECKOUT);
        let b = workflow("  push:", READ_ONLY, "", &format!("{CHECKOUT}{RUST_CACHE}"));
        let r = rep(&format!("{a}\n{b}"));
        assert!(has(&r, "workflow-level `permissions`"), "{:?}", r.findings);
        assert!(!has(&r, "restores a shared cache"), "{:?}", r.findings);
    }
}
