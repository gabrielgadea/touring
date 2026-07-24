//! Infrastructure as Code (D49 / F4.9) — Terraform/Kubernetes/Dockerfile
//! anti-patterns. The IaC is where cloud misconfigurations live: S3 public
//! ACLs, open security groups, missing encryption, latest tags.
//!
//! | Smell | Signal | File |
//! |-------|--------|------|
//! | `s3-public-read` | `acl = "public-read"` / `"public-read-write"` in Terraform | `*.tf` |
//! | `s3-no-encryption` | `aws_s3_bucket` resource without `server_side_encryption_configuration` | `*.tf` |
//! | `s3-no-versioning` | `aws_s3_bucket` resource without `versioning { enabled = true }` | `*.tf` |
//! | `s3-no-logging` | `aws_s3_bucket` resource without `logging { ... }` | `*.tf` |
//! | `sg-open-world` | `cidr_blocks = ["0.0.0.0/0"]` on a security-group ingress | `*.tf` |
//! | `dockerfile-expose-22` | `EXPOSE 22` in a Dockerfile | `Dockerfile*` |
//! | `dockerfile-no-user` | `FROM` without a `USER` directive (CKV_DOCKER_3) | `Dockerfile*` |
//! | `dockerfile-latest-tag` | `FROM <image>:latest` (CKV_DOCKER_7) | `Dockerfile*` |
//! | `dockerfile-add-vs-copy` | `ADD` (which can fetch URLs) — should be `COPY` | `Dockerfile*` |
//! | `rds-no-encryption` | `aws_db_instance` / `aws_rds_cluster` without `storage_encrypted = true` | `*.tf` |
//!
//! **Disjoint** from F2.6 config (which keys on secrets/CORS/headers; F4.9
//! keys on cloud-resource shape) and D43 modernization (F4.9 is about IaC
//! not language idioms).
//!
//! **Sources (context7, `/bridgecrewio/checkov`, High reputation, bench
//! 84.67):** `aws_s3_bucket` with `acl = "public-read"` is CKV_AWS_20
//! (Checkov); `EXPOSE 22` is CKV_DOCKER_1; `FROM <img>:latest` is
//! CKV_DOCKER_7; missing `USER` is CKV_DOCKER_3. Checkov scan examples use
//! these exact patterns.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

const SCALE: f32 = 6.0;

#[derive(Debug, Clone, Default)]
/// Infrastructure-as-Code findings for one IaC file.
pub struct IacReport {
    /// Total raw violation count across all detectors.
    pub violations: usize,
    /// Weighted violation total (per-smell weights applied).
    pub weighted_total: f32,
    /// Lines scanned (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired detector, sorted by count desc.
    pub findings: Vec<(String, usize)>,
}

impl IacReport {
    fn push(&mut self, message: &'static str, count: usize, weight: f32) {
        if count > 0 {
            self.violations += count;
            self.weighted_total += count as f32 * weight;
            self.findings.push((message.to_string(), count));
        }
    }
}

// Use the string value alone (Terraform HCL often has irregular whitespace
// between `acl` and `=`; matching the bare string value catches all).
const ACL_PUBLIC_READ: &[u8] = b"public-read";
const ACL_PUBLIC_READ_WRITE: &[u8] = b"public-read-write";
const ACL_PUBLIC_ACL: &[u8] = b"public-read-write";
const S3_RESOURCE: &[u8] = b"aws_s3_bucket";
const S3_ENCRYPTION: &[u8] = b"server_side_encryption_configuration";
const S3_VERSIONING: &[u8] = b"versioning";
const S3_LOGGING: &[u8] = b"logging {";
const SG_CIDR_OPEN: &[u8] = b"0.0.0.0/0";
const SG_INGRESS: &[u8] = b"ingress {";
const DOCKER_FROM: &[u8] = b"FROM ";
const DOCKER_USER: &[u8] = b"\nUSER ";
const DOCKER_ADD: &[u8] = b"\nADD ";
const DOCKER_COPY: &[u8] = b"\nCOPY ";
const RDS_INSTANCE: &[u8] = b"aws_db_instance";
const RDS_CLUSTER: &[u8] = b"aws_rds_cluster";
const RDS_ENCRYPTED: &[u8] = b"storage_encrypted";
// True = "true" / "True" / "TRUE" / etc. — bare needle catches the key
// regardless of spacing (`storage_encrypted = true` vs `storage_encrypted  = true`).

fn count_in_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> usize {
    memmem::find_iter(bytes, needle)
        .filter(|&off| !offset_suppressed(off, regions))
        .count()
}

fn has_in_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> bool {
    count_in_executable(bytes, regions, needle) > 0
}

/// `acl = "public-read"` or `"public-read-write"` literal in Terraform
/// (Checkov CKV_AWS_20).
fn detect_s3_public_acl(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_in_executable(bytes, regions, ACL_PUBLIC_READ)
        + count_in_executable(bytes, regions, ACL_PUBLIC_READ_WRITE)
        + count_in_executable(bytes, regions, ACL_PUBLIC_ACL)
}

fn detect_s3_no_encryption(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let s3_count = count_in_executable(bytes, regions, S3_RESOURCE);
    if s3_count == 0 {
        return 0;
    }
    if has_in_executable(bytes, regions, S3_ENCRYPTION) {
        0
    } else {
        s3_count
    }
}

fn detect_s3_no_versioning(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let s3_count = count_in_executable(bytes, regions, S3_RESOURCE);
    if s3_count == 0 {
        return 0;
    }
    if has_in_executable(bytes, regions, S3_VERSIONING) {
        0
    } else {
        s3_count
    }
}

fn detect_s3_no_logging(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let s3_count = count_in_executable(bytes, regions, S3_RESOURCE);
    if s3_count == 0 {
        return 0;
    }
    if has_in_executable(bytes, regions, S3_LOGGING) {
        0
    } else {
        s3_count
    }
}

/// `cidr_blocks = ["0.0.0.0/0"]` on an ingress block (Checkov CKV_AWS_24/260).
fn detect_sg_open_world(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let ingress_count = count_in_executable(bytes, regions, SG_INGRESS);
    if ingress_count == 0 {
        return 0;
    }
    count_in_executable(bytes, regions, SG_CIDR_OPEN)
}

fn detect_dockerfile_expose_22(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    // Line-walk: for each `EXPOSE` line, check if `22` appears in the port
    // list (`EXPOSE 22`, `EXPOSE 22 80`, `EXPOSE 3000 22` -- all bad).
    // Walk each `EXPOSE` line directly.
    let mut off = 0;
    let mut count = 0;
    while let Some(rel) = memmem::find(&bytes[off..], b"EXPOSE ") {
        let start = off + rel + 7; // skip "EXPOSE "
        let rest = &bytes[start..];
        let line_end = rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len());
        let line = &rest[..line_end];
        // Tokenize by whitespace; check if any token is `22`.
        let mut token_start = 0usize;
        let mut found = false;
        for (i, &b) in line.iter().enumerate() {
            if b == b' ' || b == b'\t' {
                if i > token_start {
                    let token = &line[token_start..i];
                    if token == b"22" || token == b"22/tcp" || token == b"22/udp" {
                        found = true;
                        break;
                    }
                }
                token_start = i + 1;
            }
        }
        // check last token
        if !found && token_start < line.len() {
            let token = &line[token_start..];
            if token == b"22" || token == b"22/tcp" || token == b"22/udp" {
                found = true;
            }
        }
        // Suppress if in a non-executable region
        let abs_pos = start - 7;
        if found && !offset_suppressed(abs_pos, regions) {
            count += 1;
        }
        off = start + line_end;
    }
    count
}

/// `FROM <image>` without a `USER` directive (Checkov CKV_DOCKER_3).
fn detect_dockerfile_no_user(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let from_count = count_in_executable(bytes, regions, DOCKER_FROM);
    if from_count == 0 {
        return 0;
    }
    if has_in_executable(bytes, regions, DOCKER_USER) {
        0
    } else {
        1
    }
}

fn detect_dockerfile_latest_tag(bytes: &[u8], _regions: &[(usize, usize)]) -> usize {
    // Re-walk: find each `FROM <img>...` line and check if it ends with `:latest`.
    let mut off = 0;
    let mut count = 0;
    while let Some(rel) = memmem::find(&bytes[off..], DOCKER_FROM) {
        let start = off + rel + DOCKER_FROM.len();
        let rest = &bytes[start..];
        let line_end = rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len());
        let line = &rest[..line_end];
        // trim trailing whitespace manually (no `trim_end` on &[u8])
        let trimmed_end = line
            .iter()
            .rposition(|&b| b != b' ' && b != b'\t' && b != b'\r')
            .map(|i| i + 1)
            .unwrap_or(0);
        if trimmed_end >= 7 && &line[trimmed_end - 7..trimmed_end] == b":latest" {
            count += 1;
        }
        off = start + line_end;
    }
    count
}

/// `ADD` (which can fetch URLs from remote hosts and extract archives) when
/// the file does not have `COPY` (suggesting the build could use the safer
/// `COPY` instead). Heuristic: if `ADD` is present, the file should be
/// reviewed; if `COPY` is also present, the dev knows the difference.
fn detect_dockerfile_add_vs_copy(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    if !has_in_executable(bytes, regions, DOCKER_ADD) {
        return 0;
    }
    // If COPY is also present, the dev knows when to use ADD. Only flag
    // when ADD is the *only* file-fetcher.
    if has_in_executable(bytes, regions, DOCKER_COPY) {
        return 0;
    }
    1
}

fn detect_rds_no_encryption(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let rds_count = count_in_executable(bytes, regions, RDS_INSTANCE)
        + count_in_executable(bytes, regions, RDS_CLUSTER);
    if rds_count == 0 {
        return 0;
    }
    // Look for `storage_encrypted ... true` (any spacing) -- use a line-walk
    // to handle irregular whitespace (`storage_encrypted  = true`).
    let mut off = 0;
    let mut encrypted_lines = 0;
    while let Some(rel) = memmem::find(&bytes[off..], RDS_ENCRYPTED) {
        let start = off + rel;
        let after = start + RDS_ENCRYPTED.len();
        let rest = &bytes[after..];
        let line_end = rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len());
        let line = &rest[..line_end];
        // Trim left and check for `= true` (any spacing)
        let trimmed = line
            .iter()
            .position(|&b| b != b' ' && b != b'\t')
            .map(|i| &line[i..])
            .unwrap_or(line);
        if trimmed.starts_with(b"= true")
            || trimmed.starts_with(b"=True")
            || trimmed.starts_with(b"=TRUE")
        {
            encrypted_lines += 1;
        }
        off = start + RDS_ENCRYPTED.len() + line_end;
    }
    if encrypted_lines >= rds_count {
        0
    } else {
        rds_count.saturating_sub(encrypted_lines)
    }
}

/// Analyze Infrastructure-as-Code smells in `source` (Terraform / Dockerfile /
/// K8s manifest). The lang parameter is ignored -- this engine is
/// language-agnostic (shape-based).
pub fn analyze_iac(source: &str, _lang: &str) -> IacReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, "rust");
    let mut report = IacReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    report.push(
        "S3 bucket with public ACL (acl = public-read / public-read-write / public) -- Checkov CKV_AWS_20",
        detect_s3_public_acl(bytes, &regions),
        1.0,
    );
    report.push(
        "aws_s3_bucket without server_side_encryption_configuration -- Checkov CKV_AWS_19",
        detect_s3_no_encryption(bytes, &regions),
        0.9,
    );
    report.push(
        "aws_s3_bucket without versioning { enabled = true } -- Checkov CKV_AWS_21",
        detect_s3_no_versioning(bytes, &regions),
        0.7,
    );
    report.push(
        "aws_s3_bucket without access logging -- Checkov CKV_AWS_18",
        detect_s3_no_logging(bytes, &regions),
        0.6,
    );
    report.push(
        "security-group ingress with cidr_blocks = [0.0.0.0/0] -- open to the world",
        detect_sg_open_world(bytes, &regions),
        1.0,
    );
    report.push(
        "Dockerfile EXPOSE 22 (SSH port -- should not be in container) -- Checkov CKV_DOCKER_1",
        detect_dockerfile_expose_22(bytes, &regions),
        0.9,
    );
    report.push(
        "Dockerfile FROM without USER directive (container runs as root) -- Checkov CKV_DOCKER_3",
        detect_dockerfile_no_user(bytes, &regions),
        0.8,
    );
    report.push(
        "Dockerfile FROM <image>:latest tag (non-reproducible builds) -- Checkov CKV_DOCKER_7",
        detect_dockerfile_latest_tag(bytes, &regions),
        0.7,
    );
    report.push(
        "Dockerfile uses ADD (can fetch URLs / extract archives) without COPY (use COPY for local files)",
        detect_dockerfile_add_vs_copy(bytes, &regions),
        0.5,
    );
    report.push(
        "aws_db_instance / aws_rds_cluster without storage_encrypted = true -- Checkov CKV_AWS_16/17",
        detect_rds_no_encryption(bytes, &regions),
        0.9,
    );
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`IacReport`] as `1 - density * SCALE`, clamped to `[0, 1]`.
pub fn score_iac(report: &IacReport) -> f32 {
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rep(src: &str) -> IacReport {
        analyze_iac(src, "rust")
    }

    #[test]
    fn empty_file_clean() {
        let r = rep("");
        // Empty input has no S3, Dockerfile, SG, or RDS -- the detectors
        // are gated on the presence of those resources, so no findings.
        assert_eq!(
            r.violations, 0,
            "empty IaC reports no findings: {:?}",
            r.findings
        );
    }

    #[test]
    fn s3_public_read_flagged() {
        let src = r#"resource "aws_s3_bucket" "foo" {
  bucket = "foo"
  acl    = "public-read"
}
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("public")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn s3_public_read_write_flagged() {
        let src = r#"resource "aws_s3_bucket" "foo" {
  acl = "public-read-write"
}
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("public")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn s3_with_encryption_clean() {
        let src = r#"resource "aws_s3_bucket" "foo" {
  bucket = "foo"
  acl    = "private"
  server_side_encryption_configuration {
    rule {
      apply_server_side_encryption_by_default {
        sse_algorithm = "AES256"
      }
    }
  }
  versioning {
    enabled = true
  }
  logging {
    target_bucket = "log-bucket"
    target_prefix = "log/"
  }
}
"#;
        let r = rep(src);
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("encryption")
                || m.contains("versioning")
                || m.contains("logging")),
            "fully-tuned S3: {:?}",
            r.findings
        );
    }

    #[test]
    fn s3_no_encryption_flagged() {
        let src = r#"resource "aws_s3_bucket" "foo" {
  bucket = "foo"
}
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("encryption")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn sg_open_world_flagged() {
        let src = r#"resource "aws_security_group" "web" {
  ingress {
    from_port   = 80
    to_port     = 80
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
}
"#;
        let r = rep(src);
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("open to the world")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn dockerfile_expose_22_flagged() {
        let src = r#"FROM node:20-alpine
EXPOSE 3000 22
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("EXPOSE 22")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn dockerfile_no_user_flagged() {
        let src = r#"FROM node:20-alpine
EXPOSE 3000
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("USER")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn dockerfile_with_user_clean() {
        let src = r#"FROM node:20-alpine
USER node
EXPOSE 3000
"#;
        let r = rep(src);
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("USER")),
            "USER declared: {:?}",
            r.findings
        );
    }

    #[test]
    fn dockerfile_latest_tag_flagged() {
        let src = r#"FROM node:latest
EXPOSE 3000
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("latest")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn dockerfile_pinned_clean() {
        let src = r#"FROM node:20.10.0-alpine
EXPOSE 3000
USER node
"#;
        let r = rep(src);
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("latest")),
            "pinned: {:?}",
            r.findings
        );
    }

    #[test]
    fn dockerfile_add_only_flagged() {
        let src = r#"FROM node:20
ADD https://example.com/file.tar.gz /tmp/
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("ADD")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn rds_no_encryption_flagged() {
        let src = r#"resource "aws_db_instance" "db" {
  engine = "postgres"
  storage_encrypted = false
}
"#;
        let r = rep(src);
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("storage_encrypted")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn rds_with_encryption_clean() {
        let src = r#"resource "aws_db_instance" "db" {
  engine             = "postgres"
  storage_encrypted  = true
}
"#;
        let r = rep(src);
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("storage_encrypted")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn fully_tuned_terraform_clean() {
        let src = r#"resource "aws_s3_bucket" "good" {
  bucket = "good"
  acl    = "private"
  server_side_encryption_configuration {
    rule {
      apply_server_side_encryption_by_default {
        sse_algorithm = "AES256"
      }
    }
  }
  versioning {
    enabled = true
  }
  logging {
    target_bucket = "logs"
    target_prefix = "log/"
  }
}

resource "aws_db_instance" "db" {
  storage_encrypted = true
}
"#;
        let r = rep(src);
        assert_eq!(r.violations, 0, "fully-tuned IaC: {:?}", r.findings);
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = rep(r#"resource "aws_s3_bucket" "x" {
  acl = "public-read"
}
"#);
        let good = rep(r#"resource "aws_s3_bucket" "x" {
  acl = "private"
  server_side_encryption_configuration {
    rule {
      apply_server_side_encryption_by_default {
        sse_algorithm = "AES256"
      }
    }
  }
  versioning {
    enabled = true
  }
  logging {
    target_bucket = "logs"
    target_prefix = "log/"
  }
}
"#);
        assert!(
            score_iac(&bad) < score_iac(&good),
            "untuned ({:.3}) must score below tuned ({:.3})",
            score_iac(&bad),
            score_iac(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = rep(r#"acl = "public-read"
"#);
        let s = score_iac(&r);
        assert!(s > 0.0, "short untuned must not score 0.0: {s}");
    }
}
