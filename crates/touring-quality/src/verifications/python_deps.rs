//! S510 (2026-09-26) — the Python backend of the two P0 dependency dimensions.
//!
//! Before it, a Python root fell to F2.5's Rust path ("no Cargo.lock" — a
//! verdict about the wrong ecosystem) or to `[N/A]`, and F4.5 answered `[N/A]`
//! for every Python project: two of the six P0 BLOCK dimensions silent on the
//! analise, whose 412 locked dependencies carried 146 known vulnerabilities
//! (PT2, 26/09/2026).
//!
//! * **F2.5** — `uv export --frozen` → `uvx pip-audit==PIN -s osv -f json`
//!   (PT2: 412 dependencies in 110 s, network-bound). The audit is cached per
//!   `uv.lock` digest for [`CACHE_TTL_SECS`]; a live run needs the opt-in
//!   [`LIVE_ENV`]. No fresh audit ⇒ UNMEASURED (0.5), never a pass.
//! * **F4.5** — `uv lock --check --offline` (lock vs manifest) plus the
//!   `[project] dependencies` that carry no version bound.
//!
//! The verdict reads each tool's OUTPUT, never its exit code alone: pip-audit
//! exits 1 both when it finds vulnerabilities and when it cannot reach OSV (PT2:
//! rc 1 with a JSON report vs rc 1 with a `ConnectionError`), and `uv lock
//! --check` exits 1 for a stale lock and for an offline resolution failure alike.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// pip-audit version pinned by PT2 (26/09/2026) — never an implicit "latest".
pub(crate) const PIP_AUDIT_PIN: &str = "2.10.1";
/// Opt-in for the live (network) audit — the variable `touring_analysis::osv`
/// already names in its evidence, now honoured for Python.
pub(crate) const LIVE_ENV: &str = "TOURING_OSV_SCAN";
/// How long one audit answers for the same `uv.lock` (OSV advisories move daily).
pub(crate) const CACHE_TTL_SECS: u64 = 24 * 3600;
const EXPORT_TIMEOUT: Duration = Duration::from_secs(120);
const AUDIT_TIMEOUT: Duration = Duration::from_secs(600);
const LOCK_CHECK_TIMEOUT: Duration = Duration::from_secs(120);
/// `uv lock --check` names a stale lock with this phrase (measured, uv 0.12.10).
const STALE_LOCK: &str = "needs to be updated";
/// …and a missing one with this.
const MISSING_LOCK: &str = "Unable to find lockfile";

/// Where, and whether live, the F2.5 audit may be answered.
#[derive(Debug, Clone)]
pub(crate) struct AuditConfig {
    /// Directory of the per-lock audit cache.
    pub cache_dir: PathBuf,
    /// Whether a missing/stale audit may be refreshed over the network.
    pub live: bool,
    /// "Now", in seconds since the epoch (injected so freshness is testable).
    pub now_secs: u64,
}

impl AuditConfig {
    /// Production configuration: `TOURING_QUALITY_CACHE_DIR`, else the XDG cache,
    /// else `~/.cache/touring/quality`; live only when [`LIVE_ENV`]`=1`.
    pub(crate) fn from_env() -> Self {
        let cache_dir = std::env::var_os("TOURING_QUALITY_CACHE_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("XDG_CACHE_HOME").map(|d| PathBuf::from(d).join("touring/quality"))
            })
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache/touring/quality"))
            })
            .unwrap_or_else(|| std::env::temp_dir().join("touring-quality-cache"));
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Self {
            cache_dir,
            live: std::env::var(LIVE_ENV).is_ok_and(|v| v == "1"),
            now_secs,
        }
    }
}

/// The Python dependency root `target` belongs to, when `target` is a Python
/// manifest target: a `pyproject.toml`/`uv.lock` file, or a directory holding one
/// of them and no `Cargo.toml` (a Rust root keeps the Rust engine). The root is
/// the nearest ancestor holding a `uv.lock` — a uv workspace member resolves the
/// workspace lock — else the manifest's own directory.
pub(crate) fn python_root(target: &Path) -> Option<PathBuf> {
    let start = if target.is_dir() {
        let has = |name: &str| target.join(name).is_file();
        if has("Cargo.toml") || !(has("uv.lock") || has("pyproject.toml")) {
            return None;
        }
        target.to_path_buf()
    } else {
        match target.file_name().and_then(|n| n.to_str()) {
            Some("pyproject.toml" | "uv.lock") => target.parent()?.to_path_buf(),
            _ => return None,
        }
    };
    let start = start.canonicalize().unwrap_or(start);
    let mut dir = start.clone();
    loop {
        if dir.join("uv.lock").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return Some(start);
        }
    }
}

// ── F2.5 ──────────────────────────────────────────────────────────────────────

/// One audited dependency of a pip-audit report.
#[derive(Debug)]
struct AuditedDep {
    name: String,
    version: String,
    /// DISTINCT advisory ids — pip-audit repeats an id within one package (PT2:
    /// pypdf 69 rows, 35 ids), so the raw rows are never the count.
    ids: Vec<String>,
}

/// A parsed pip-audit `-f json` report.
#[derive(Debug)]
struct Audit {
    audited: Vec<AuditedDep>,
    /// `(name, skip_reason)` — URL/editable requirements pip-audit cannot pin.
    skipped: Vec<(String, String)>,
}

fn parse_audit(stdout: &str) -> Result<Audit, String> {
    let doc: serde_json::Value =
        serde_json::from_str(stdout.trim()).map_err(|e| format!("not JSON ({e})"))?;
    let deps = doc
        .get("dependencies")
        .and_then(serde_json::Value::as_array)
        .ok_or("no `dependencies` array")?;
    let text = |v: &serde_json::Value, key: &str| {
        v.get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let mut audit = Audit {
        audited: Vec::new(),
        skipped: Vec::new(),
    };
    for dep in deps {
        if let Some(reason) = dep.get("skip_reason").and_then(serde_json::Value::as_str) {
            audit.skipped.push((text(dep, "name"), reason.to_string()));
            continue;
        }
        let mut ids: Vec<String> = dep
            .get("vulns")
            .and_then(serde_json::Value::as_array)
            .map(|vulns| vulns.iter().map(|v| text(v, "id")).collect())
            .unwrap_or_default();
        ids.sort();
        ids.dedup();
        audit.audited.push(AuditedDep {
            name: text(dep, "name"),
            version: text(dep, "version"),
            ids,
        });
    }
    Ok(audit)
}

/// F2.5 verdict of a pip-audit `-f json` report — pure, so a recorded OSV
/// response drives the tests. Unparseable or empty ⇒ UNVERIFIED, never a pass.
pub(crate) fn score_pip_audit(stdout: &str) -> (f32, String) {
    let audit = match parse_audit(stdout) {
        Ok(audit) => audit,
        Err(why) => {
            return (
                0.5,
                format!(
                    "F2.5: UNVERIFIED — the Python dependency audit produced no report ({why}). \
                     Not a clean pass."
                ),
            );
        }
    };
    if audit.audited.is_empty() {
        return (
            0.5,
            "F2.5: UNVERIFIED — pip-audit audited no Python dependency (empty export). \
             Not a clean pass."
                .to_string(),
        );
    }
    let skipped_note = if audit.skipped.is_empty() {
        String::new()
    } else {
        let names: Vec<&str> = audit.skipped.iter().map(|(n, _)| n.as_str()).collect();
        format!(
            " | {} not auditable (URL/editable requirement): {}",
            names.len(),
            names.join(", ")
        )
    };
    let mut vulnerable: Vec<&AuditedDep> =
        audit.audited.iter().filter(|d| !d.ids.is_empty()).collect();
    if vulnerable.is_empty() {
        return (
            1.0,
            format!(
                "F2.5: 0 known vulnerabilities in {} Python dependencies (OSV via pip-audit \
                 {PIP_AUDIT_PIN}){skipped_note}",
                audit.audited.len()
            ),
        );
    }
    vulnerable.sort_by(|a, b| b.ids.len().cmp(&a.ids.len()).then(a.name.cmp(&b.name)));
    let total: usize = vulnerable.iter().map(|d| d.ids.len()).sum();
    let shown: Vec<String> = vulnerable
        .iter()
        .take(5)
        .map(|d| format!("{}@{} ({})", d.name, d.version, d.ids.len()))
        .collect();
    let more = if vulnerable.len() > 5 {
        format!(" (+{} more)", vulnerable.len() - 5)
    } else {
        String::new()
    };
    (
        0.0,
        format!(
            "F2.5: {total} known vulnerabilit{} in {} of {} Python dependencies (OSV via \
             pip-audit {PIP_AUDIT_PIN}): {}{more}{skipped_note}",
            if total == 1 { "y" } else { "ies" },
            vulnerable.len(),
            audit.audited.len(),
            shown.join(", ")
        ),
    )
}

fn cache_key(lock: &[u8]) -> String {
    touring_foundation::hash::content_hash(lock)[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A cached audit younger than [`CACHE_TTL_SECS`]: `(raw report, age in seconds)`.
fn read_fresh_cache(file: &Path, now_secs: u64) -> Option<(String, u64)> {
    let doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
    let measured_at = doc.get("measured_at")?.as_u64()?;
    let age = now_secs.checked_sub(measured_at)?;
    if age >= CACHE_TTL_SECS {
        return None;
    }
    Some((doc.get("audit")?.to_string(), age))
}

fn write_cache(file: &Path, report: &str, now_secs: u64) {
    let Ok(audit) = serde_json::from_str::<serde_json::Value>(report.trim()) else {
        return;
    };
    let doc = serde_json::json!({
        "measured_at": now_secs,
        "pip_audit": PIP_AUDIT_PIN,
        "audit": audit,
    });
    if let Err(e) = std::fs::write(file, doc.to_string()) {
        tracing::warn!(error = %e, file = %file.display(), "F2.5 audit cache not written");
    }
}

/// F2.5 over the resolved Python dependency tree of `root`.
pub(crate) fn f2_5(root: &Path, cfg: &AuditConfig) -> (f32, String) {
    let Ok(lock) = std::fs::read(root.join("uv.lock")) else {
        return (
            0.5,
            format!(
                "F2.5: UNVERIFIED — the Python project at {} has no uv.lock, so its dependency \
                 tree was never resolved and no advisory was checked. Fix: `uv lock`. \
                 Not a clean pass.",
                root.display()
            ),
        );
    };
    let key = cache_key(&lock);
    let cache_file = cfg
        .cache_dir
        .join(format!("{key}-pip-audit-{PIP_AUDIT_PIN}.json"));
    if let Some((report, age)) = read_fresh_cache(&cache_file, cfg.now_secs) {
        let (score, evidence) = score_pip_audit(&report);
        return (
            score,
            format!("{evidence} [audit of this uv.lock, {}h old]", age / 3600),
        );
    }
    if !cfg.live {
        return (
            0.5,
            format!(
                "F2.5: UNMEASURED — the Python dependency audit needs the network (OSV) and no \
                 audit of this uv.lock is fresh. Run `{LIVE_ENV}=1 touring-quality check --gate \
                 F2.5 --target {}` (~2 min; answers for 24 h). Not a clean pass.",
                root.display()
            ),
        );
    }
    match live_audit(root, &cfg.cache_dir, &key) {
        Ok(report) => {
            write_cache(&cache_file, &report, cfg.now_secs);
            score_pip_audit(&report)
        }
        Err(why) => (
            0.5,
            format!(
                "F2.5: UNVERIFIED — the Python dependency audit failed: {why}. Not a clean pass."
            ),
        ),
    }
}

/// `uv export --frozen` → `uvx pip-audit` — the network half of F2.5. Returns the
/// raw JSON report, or why there is none: pip-audit's exit code cannot tell a
/// finding from a network failure, so the JSON decides.
fn live_audit(root: &Path, cache_dir: &Path, key: &str) -> Result<String, String> {
    std::fs::create_dir_all(cache_dir)
        .map_err(|e| format!("cache dir {}: {e}", cache_dir.display()))?;
    let requirements = cache_dir.join(format!("{key}-requirements.txt"));
    let export = run_with_timeout(
        Command::new("uv")
            .args([
                "export",
                "--frozen",
                "--no-hashes",
                "--all-packages",
                "--no-emit-workspace",
                "--format",
                "requirements-txt",
                "-o",
            ])
            .arg(&requirements)
            .current_dir(root),
        EXPORT_TIMEOUT,
    )?;
    if !export.status.success() {
        return Err(format!(
            "`uv export --frozen` exited {}: {}",
            exit_code(&export),
            tail(&export.stderr)
        ));
    }
    let audit = run_with_timeout(
        Command::new("uvx")
            .arg(format!("pip-audit=={PIP_AUDIT_PIN}"))
            .arg("-r")
            .arg(&requirements)
            .args(["--no-deps", "--disable-pip", "-s", "osv", "-f", "json"])
            .current_dir(root),
        AUDIT_TIMEOUT,
    )?;
    let report = String::from_utf8_lossy(&audit.stdout).into_owned();
    match parse_audit(&report) {
        Ok(_) => Ok(report),
        Err(why) => Err(format!(
            "pip-audit exited {} without a report ({why}): {}",
            exit_code(&audit),
            tail(&audit.stderr)
        )),
    }
}

// ── F4.5 ──────────────────────────────────────────────────────────────────────

/// `[project] dependencies` entries that carry no version bound — neither a
/// comparison operator, nor a direct reference (`name @ url`), nor a
/// `[tool.uv.sources]` entry that pins them (workspace, path, git, URL).
pub(crate) fn unbounded_requirements(pyproject: &str) -> Vec<String> {
    let pinned = pinned_sources(pyproject);
    project_dependencies(pyproject)
        .into_iter()
        .filter_map(|req| {
            let spec = req.split(';').next().unwrap_or_default().trim();
            let name = spec.split('[').next().unwrap_or(spec).trim();
            let bounded = spec.contains('@')
                || spec.contains(['<', '>', '=', '!', '~'])
                || pinned.contains(&normalize(name));
            (!bounded && !name.is_empty()).then(|| name.to_string())
        })
        .collect()
}

/// PEP 503 name normalization: lowercase, runs of `-_.` become one `-`.
fn normalize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if matches!(c, '-' | '_' | '.') {
            if !out.ends_with('-') {
                out.push('-');
            }
        } else {
            out.push(c.to_ascii_lowercase());
        }
    }
    out
}

/// The body of the TOML table `header` (`[project]`): from the line AFTER a
/// line that IS the header — never a mention of it, which the analise's
/// comments hold — to the next unindented table header.
fn toml_table<'a>(text: &'a str, header: &str) -> Option<&'a str> {
    let mut offset = 0;
    let mut body_start = None;
    for line in text.split_inclusive('\n') {
        let at = offset;
        offset += line.len();
        match body_start {
            None if line.trim() == header => body_start = Some(offset),
            Some(start) if line.starts_with('[') => return Some(&text[start..at]),
            _ => {}
        }
    }
    body_start.map(|start| &text[start..])
}

/// Names (normalized) whose `[tool.uv.sources]` entry pins them: the
/// workspace, a path, a git repository or a URL — not an `index`, which only
/// chooses where an unbounded version is resolved from.
fn pinned_sources(pyproject: &str) -> Vec<String> {
    let Some(table) = toml_table(pyproject, "[tool.uv.sources]") else {
        return Vec::new();
    };
    table
        .lines()
        .filter_map(|line| {
            let (name, source) = line.split_once('=')?;
            let pins = ["workspace", "path", "git", "url"]
                .iter()
                .any(|key| source.contains(key));
            (pins && !name.trim_start().starts_with('#'))
                .then(|| normalize(name.trim().trim_matches('"')))
        })
        .collect()
}

/// The quoted strings of the `dependencies = [ … ]` array of the `[project]`
/// table. `#` comments are skipped: a `]` or a quote inside one is prose.
fn project_dependencies(pyproject: &str) -> Vec<String> {
    let Some(table) = toml_table(pyproject, "[project]") else {
        return Vec::new();
    };
    let Some(key) = table.lines().position(|line| {
        let line = line.trim_start();
        line.starts_with("dependencies")
            && line["dependencies".len()..].trim_start().starts_with('=')
    }) else {
        return Vec::new();
    };
    let body: String = table.lines().skip(key).collect::<Vec<_>>().join("\n");
    let Some(open) = body.find('[') else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut quote: Option<char> = None;
    let mut in_comment = false;
    let mut current = String::new();
    for c in body[open + 1..].chars() {
        if in_comment {
            in_comment = c != '\n';
            continue;
        }
        match (quote, c) {
            (None, ']') => break,
            (None, '#') => in_comment = true,
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => {
                out.push(std::mem::take(&mut current));
                quote = None;
            }
            (Some(_), c) => current.push(c),
            (None, _) => {}
        }
    }
    out
}

/// F4.5 over the Python project at `root`.
pub(crate) fn f4_5(root: &Path) -> (f32, String) {
    let check = run_with_timeout(
        Command::new("uv")
            .args(["lock", "--check", "--offline"])
            .current_dir(root),
        LOCK_CHECK_TIMEOUT,
    );
    let check = match check {
        Ok(output) => output,
        Err(why) => {
            return (
                0.5,
                format!(
                    "F4.5: UNMEASURED — `uv lock --check` could not run for the Python project: {why}"
                ),
            );
        }
    };
    let stderr = String::from_utf8_lossy(&check.stderr);
    if !check.status.success() {
        return if stderr.contains(STALE_LOCK) {
            (
                0.0,
                "F4.5: uv.lock out of sync with pyproject.toml [BLOCK] — `uv sync --frozen` \
                 installs what the manifest no longer declares. Fix: `uv lock` and commit it."
                    .to_string(),
            )
        } else if stderr.contains(MISSING_LOCK) {
            (
                0.5,
                "F4.5: the Python dependency tree is not locked (no uv.lock) — every install \
                 resolves anew. Fix: `uv lock` and commit it."
                    .to_string(),
            )
        } else {
            (
                0.5,
                format!(
                    "F4.5: UNMEASURED — `uv lock --check --offline` exited {} without a verdict: {}",
                    exit_code(&check),
                    tail(&check.stderr)
                ),
            )
        };
    }
    let manifest = std::fs::read_to_string(root.join("pyproject.toml")).unwrap_or_default();
    let unbounded = unbounded_requirements(&manifest);
    if unbounded.is_empty() {
        // The denominator travels with the verdict: "every requirement bounded"
        // over 9 of 56 read was the vacuous pass the analise exposed.
        return (
            1.0,
            format!(
                "F4.5: uv.lock in sync with pyproject.toml; all {} [project] requirements \
                 bounded (Python backend: uv lock --check)",
                project_dependencies(&manifest).len()
            ),
        );
    }
    let shown = unbounded[..unbounded.len().min(4)].join(", ");
    let more = if unbounded.len() > 4 {
        format!(" (+{} more)", unbounded.len() - 4)
    } else {
        String::new()
    };
    (
        0.7,
        format!(
            "F4.5: uv.lock in sync with pyproject.toml; {} requirement(s) with no version \
             bound: {shown}{more}",
            unbounded.len()
        ),
    )
}

// ── process plumbing ──────────────────────────────────────────────────────────

/// Run `cmd` to completion or `timeout`, whichever comes first (killed on timeout).
fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<Output, String> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("`{program}` could not start: {e}"))?;
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("`{program}` exceeded {}s", timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("`{program}`: {e}")),
        }
    };
    Ok(Output {
        status,
        stdout: collect(stdout),
        stderr: collect(stderr),
    })
}

fn drain<R: Read + Send + 'static>(mut pipe: R) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf);
        buf
    })
}

fn collect(handle: Option<JoinHandle<Vec<u8>>>) -> Vec<u8> {
    handle.and_then(|h| h.join().ok()).unwrap_or_default()
}

fn exit_code(output: &Output) -> String {
    output
        .status
        .code()
        .map_or_else(|| "by signal".to_string(), |c| c.to_string())
}

fn tail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim();
    let start = text.char_indices().rev().nth(299).map_or(0, |(at, _)| at);
    text[start..].replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from the real PT2 report (26/09/2026): pypdf repeats an id, one
    /// clean dependency, one URL requirement pip-audit cannot pin.
    const RECORDED: &str = r#"{"dependencies": [
        {"name": "pypdf", "version": "6.6.0", "vulns": [
            {"id": "GHSA-4xc4-762w-m6qx", "fix_versions": ["6.7.1"], "aliases": ["CVE-2026-27025"]},
            {"id": "GHSA-4xc4-762w-m6qx", "fix_versions": ["6.7.1"], "aliases": ["CVE-2026-27025"]},
            {"id": "GHSA-7gw9-cf7v-778f", "fix_versions": ["6.7.2"], "aliases": []}]},
        {"name": "click", "version": "8.3.1", "vulns": [
            {"id": "GHSA-q9hh-v4xc-4m8x", "fix_versions": ["8.3.3"], "aliases": []}]},
        {"name": "requests", "version": "2.32.5", "vulns": []},
        {"name": "pt-core-news-lg", "skip_reason": "URL requirements cannot be pinned"}
    ], "fixes": []}"#;

    fn config(dir: &Path, live: bool, now_secs: u64) -> AuditConfig {
        AuditConfig {
            cache_dir: dir.to_path_buf(),
            live,
            now_secs,
        }
    }

    /// D8 guard: the OSV evidence PROMISES a live lookup behind an opt-in; this
    /// backend is the executor that honours it. The text named
    /// `TOURING_OSV_SCAN` for months while no code read it — now the promise and
    /// the predicate are one constant, and this test fails if they part.
    #[test]
    fn the_osv_evidence_names_the_opt_in_this_backend_honours() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"x\"\ndependencies = [\"requests==2.32.5\"]\n",
        )
        .expect("pyproject");
        let evidence = touring_analysis::osv::offline_summary(dir.path()).expect("a PyPI manifest");
        assert!(evidence.contains(LIVE_ENV), "{evidence}");
        assert!(evidence.contains("PyPI only"), "{evidence}");
    }

    #[test]
    fn a_recorded_osv_response_blocks_with_distinct_findings() {
        let (score, evidence) = score_pip_audit(RECORDED);
        assert!(score.abs() < f32::EPSILON, "{evidence}");
        assert!(
            evidence.contains("3 known vulnerabilities in 2 of 3 Python dependencies"),
            "the repeated pypdf row is one finding, and the skipped URL requirement is not \
             an audited dependency: {evidence}"
        );
        assert!(evidence.contains("pypdf@6.6.0 (2)"), "{evidence}");
        assert!(evidence.contains("pt-core-news-lg"), "{evidence}");
    }

    #[test]
    fn a_clean_report_passes_and_names_its_denominator() {
        let clean =
            r#"{"dependencies": [{"name": "a", "version": "1", "vulns": []}], "fixes": []}"#;
        let (score, evidence) = score_pip_audit(clean);
        assert!((score - 1.0).abs() < f32::EPSILON, "{evidence}");
        assert!(
            evidence.contains("0 known vulnerabilities in 1 Python"),
            "{evidence}"
        );
    }

    #[test]
    fn no_report_is_unverified_never_a_pass() {
        for raw in [
            "requests.exceptions.ConnectionError: HTTPSConnectionPool(host='api.osv.dev')",
            r#"{"dependencies": [], "fixes": []}"#,
            r#"{"fixes": []}"#,
        ] {
            let (score, evidence) = score_pip_audit(raw);
            assert!((score - 0.5).abs() < f32::EPSILON, "{raw}: {evidence}");
            assert!(evidence.contains("UNVERIFIED"), "{raw}: {evidence}");
        }
    }

    #[test]
    fn a_fresh_cached_audit_answers_without_the_network() {
        let root = tempfile::tempdir().expect("root");
        let cache = tempfile::tempdir().expect("cache");
        std::fs::write(root.path().join("uv.lock"), "version = 1\n# cached probe\n").expect("lock");
        let key = cache_key(b"version = 1\n# cached probe\n");
        let file = cache
            .path()
            .join(format!("{key}-pip-audit-{PIP_AUDIT_PIN}.json"));
        write_cache(&file, RECORDED, 1_000);

        let (score, evidence) = f2_5(root.path(), &config(cache.path(), false, 1_000 + 3_600));
        assert!(score.abs() < f32::EPSILON, "{evidence}");
        assert!(evidence.contains("1h old"), "{evidence}");

        let (stale, evidence) = f2_5(
            root.path(),
            &config(cache.path(), false, 1_000 + CACHE_TTL_SECS),
        );
        assert!((stale - 0.5).abs() < f32::EPSILON, "{evidence}");
        assert!(evidence.contains("UNMEASURED"), "{evidence}");
        assert!(
            evidence.contains(LIVE_ENV),
            "the remedy is named: {evidence}"
        );
    }

    #[test]
    fn python_root_claims_python_manifests_and_leaves_rust_roots() {
        let ws = tempfile::tempdir().expect("ws");
        let member = ws.path().join("packages/member");
        std::fs::create_dir_all(&member).expect("member");
        std::fs::write(ws.path().join("uv.lock"), "version = 1\n").expect("lock");
        std::fs::write(ws.path().join("pyproject.toml"), "[project]\n").expect("root");
        std::fs::write(member.join("pyproject.toml"), "[project]\n").expect("member");
        let canonical = ws.path().canonicalize().expect("canonical");

        assert_eq!(python_root(ws.path()), Some(canonical.clone()));
        assert_eq!(
            python_root(&member.join("pyproject.toml")),
            Some(canonical),
            "a workspace member resolves the workspace lock"
        );
        assert_eq!(
            python_root(&member.join("src.py")),
            None,
            "a source file is no manifest"
        );

        let rust = tempfile::tempdir().expect("rust");
        std::fs::write(rust.path().join("Cargo.toml"), "[package]\n").expect("cargo");
        std::fs::write(rust.path().join("pyproject.toml"), "[project]\n").expect("maturin");
        assert_eq!(
            python_root(rust.path()),
            None,
            "a Rust root keeps the Rust engine"
        );
    }

    #[test]
    fn unbounded_requirements_are_the_ones_without_a_bound() {
        let manifest = r#"
[build-system]
requires = ["hatchling"]

[project]
name = "x"
dependencies = [
    "requests",
    "numpy>=1.26",
    "uvicorn[standard]",
    "pkg @ git+https://example.org/pkg",
    "tomli; python_version < '3.11'",
    'pydantic~=2.0',
]
optional-dependencies = { dev = ["pytest"] }

[tool.other]
dependencies = ["ignored"]
"#;
        assert_eq!(
            unbounded_requirements(manifest),
            ["requests", "uvicorn", "tomli"],
            "a marker is not a bound; a direct reference is"
        );
        assert!(unbounded_requirements("[project]\nname = \"y\"\n").is_empty());
    }

    /// Measured on the analise (26/09/2026): a `# … the [css] extra …` comment
    /// inside the array closed it after 9 of 56 entries, and F4.5 answered
    /// "every requirement bounded" over the 9 — a vacuous pass. The `tomllib`
    /// control found the 2 the scanner never reached.
    #[test]
    fn a_comment_inside_the_array_is_not_syntax() {
        let manifest = "[project]\nname = \"x\"\ndependencies = [\n    \
            \"bleach[css]>=5.0\",  # the [css] extra pulls tinycss2; it's needed\n    \
            # a whole-line comment with a ] and a \" quote\n    \
            \"requests\",\n    \"numpy>=1.26\",\n]\n";
        assert_eq!(project_dependencies(manifest).len(), 3, "{manifest}");
        assert_eq!(unbounded_requirements(manifest), ["requests"]);
    }

    /// A requirement whose `[tool.uv.sources]` entry is the workspace, a path, a
    /// git repository or a URL is pinned by that source — not unbounded (the
    /// analise's `kazuba-converters-rs`, `antt-process-analysis[mcp]`). An
    /// `index` source only picks the index: the version is still unbounded. The
    /// header also appears inside a comment first, so a table is a whole line.
    #[test]
    fn a_workspace_or_path_source_is_a_bound_and_an_index_is_not() {
        let manifest = "# see [tool.uv.sources] below\n[project]\nname = \"x\"\n\
            dependencies = [\n    \"kazuba-converters-rs\",\n    \"antt-process-analysis[mcp]\",\n    \
            \"geo_engine\",\n    \"torch\",\n]\n\n[tool.uv.sources]\n\
            kazuba-converters-rs = { workspace = true }\n\
            antt-process-analysis = { workspace = true }\n\
            geo-engine = { path = \"packages/geo\", editable = true }\n\
            torch = { index = \"pytorch\" }\n";
        assert_eq!(unbounded_requirements(manifest), ["torch"]);
    }
}
