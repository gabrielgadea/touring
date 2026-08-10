//! Build Configuration (D46 / F4.6) — polyglot profile/manifest anti-patterns.
//! The build manifest shape determines reproducibility, debugability, and
//! binary/dependency footprint. Defaults are biased toward fast iteration
//! in dev and modest optimization in release -- the elite targets stricter
//! settings that reduce build time, dev target size, and release binary
//! size *and* enforce dep pinning, lock files, and language-version pinning.
//!
//! | Lang | Manifest | Smells detected |
//! |------|---------|-----------------|
//! | Rust | `Cargo.toml` | (existing 8) |
//! | Python | `pyproject.toml` | no `[build-system]`, `requires` unpinned (`>=` not `==`), no `requires-python`, no test runner declared |
//! | JS/TS | `package.json` | no `engines.node`, no `engines` at all, no `scripts.build`/`test`, `dependencies` with `*`/missing version |
//! | Go | `go.mod` | `go 1.X` not pinned (just `go 1`), module path missing, `require` block with `latest`/no version |
//!
//! **Disjoint** from F4.5 pkg-mgmt (which keys on dep *count* and
//! `unmaintained`/`multiple-versions`; F4.6 keys on manifest *shape*).
//!
//! **Sources (context7, `/rust-lang/cargo` 86.46 + `/python-poetry/poetry`):**
//! Cargo defaults: dev=opt-level=0+debug=true+incremental=true+codegen-units=256,
//! release=opt-level=3+lto=false+panic=unwind+strip=none. pyproject.toml
//! best-practice: `[build-system]` required (PEP 517), `requires` pinned
//! with `==` (reproducible builds), `requires-python` for compat. package.json
//! best-practice: `engines.node` to declare the supported Node major
//! version, `package-lock.json` for reproducible installs. go.mod: `go
//! 1.21.X` (not just `1.21` or `1`) for reproducible toolchain pin.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

const SCALE: f32 = 6.0;

/// Build-configuration findings for one manifest file.
pub type BuildConfigReport = crate::quality::SmellReport;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    Python,
    JsTs,
    Go,
    Other,
}

fn canonical_lang(lang: &str) -> Lang {
    match lang {
        "rust" | "rs" => Lang::Rust,
        "python" | "py" => Lang::Python,
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" | "mjs" | "cjs" => Lang::JsTs,
        "go" => Lang::Go,
        _ => Lang::Other,
    }
}

// ── Rust: Cargo.toml profile smells ────────────────────────────────────────

const PROFILE_RELEASE: &[u8] = b"[profile.release]";
const PROFILE_DEV: &[u8] = b"[profile.dev]";
const PROFILE_DEV_PKG: &[u8] = b"profile.dev.package";
const PROFILE_DEBUGGING: &[u8] = b"[profile.debugging]";

fn profile_release_range(bytes: &[u8], regions: &[(usize, usize)]) -> Option<(usize, usize)> {
    let start = memmem::find(bytes, PROFILE_RELEASE)?;
    if offset_suppressed(start, regions) {
        return None;
    }
    let after = start + PROFILE_RELEASE.len();
    let rest = &bytes[after..];
    let next = ["[profile.dev]", "[profile.bench]", "[profile.test]"]
        .iter()
        .filter_map(|p| memmem::find(rest, p.as_bytes()))
        .min()
        .unwrap_or(rest.len());
    Some((start, after + next))
}

fn profile_dev_range(bytes: &[u8], regions: &[(usize, usize)]) -> Option<(usize, usize)> {
    let start = find_profile_dev_exact(bytes, regions)?;
    let after = start + PROFILE_DEV.len();
    let rest = &bytes[after..];
    let next = [
        "[profile.release]",
        "[profile.bench]",
        "[profile.test]",
        "[profile.dev.package",
    ]
    .iter()
    .filter_map(|p| memmem::find(rest, p.as_bytes()))
    .min()
    .unwrap_or(rest.len());
    Some((start, after + next))
}

fn find_profile_dev_exact(bytes: &[u8], regions: &[(usize, usize)]) -> Option<usize> {
    for off in memmem::find_iter(bytes, PROFILE_DEV) {
        if offset_suppressed(off, regions) {
            continue;
        }
        let after = off + PROFILE_DEV.len();
        if bytes.get(after) == Some(&b'.') {
            continue;
        }
        return Some(off);
    }
    None
}

fn find_profile_debugging_exact(bytes: &[u8], regions: &[(usize, usize)]) -> Option<usize> {
    for off in memmem::find_iter(bytes, PROFILE_DEBUGGING) {
        if offset_suppressed(off, regions) {
            continue;
        }
        return Some(off);
    }
    None
}

fn has_key_in(profile_bytes: &[u8], key: &[u8]) -> bool {
    let mut search_from = 0;
    while let Some(rel) = memmem::find(&profile_bytes[search_from..], key) {
        let off = search_from + rel;
        search_from = off + key.len();
        if let Some(&prev) = profile_bytes.get(off.checked_sub(1).unwrap_or(usize::MAX))
            && (prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'-')
        {
            continue;
        }
        let after = off + key.len();
        let mut i = after;
        while i < profile_bytes.len() && (profile_bytes[i] == b' ' || profile_bytes[i] == b'\t') {
            i += 1;
        }
        if profile_bytes.get(i) == Some(&b'=') {
            return true;
        }
    }
    false
}

fn has_profile_release(bytes: &[u8], regions: &[(usize, usize)]) -> bool {
    profile_release_range(bytes, regions).is_some()
}

fn detect_release_no_lto(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let Some((s, e)) = profile_release_range(bytes, regions) else {
        return 0;
    };
    if has_key_in(&bytes[s..e], b"lto") {
        0
    } else {
        1
    }
}

fn detect_release_codegen_units_not_1(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let Some((s, e)) = profile_release_range(bytes, regions) else {
        return 0;
    };
    if has_key_in(&bytes[s..e], b"codegen-units") {
        0
    } else {
        1
    }
}

fn detect_release_panic_unwind(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let Some((s, e)) = profile_release_range(bytes, regions) else {
        return 0;
    };
    if has_key_in(&bytes[s..e], b"panic") {
        0
    } else {
        1
    }
}

fn detect_release_no_strip(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let Some((s, e)) = profile_release_range(bytes, regions) else {
        return 0;
    };
    if has_key_in(&bytes[s..e], b"strip") {
        0
    } else {
        1
    }
}

fn detect_release_incremental_true(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let Some((s, e)) = profile_release_range(bytes, regions) else {
        return 0;
    };
    let profile = &bytes[s..e];
    if !has_key_in(profile, b"incremental") {
        return 0;
    }
    memmem::find(profile, b"incremental = true")
        .map(|_| 1)
        .unwrap_or(0)
}

fn detect_dev_debug_full(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let Some((s, e)) = profile_dev_range(bytes, regions) else {
        return 0;
    };
    if has_key_in(&bytes[s..e], b"debug") {
        0
    } else {
        1
    }
}

fn detect_dev_no_package_debug_false(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    if profile_dev_range(bytes, regions).is_none() {
        return 0;
    }
    if !memmem::find_iter(bytes, PROFILE_DEV_PKG).any(|off| !offset_suppressed(off, regions)) {
        return 1;
    }
    let pkg_start = match memmem::find(bytes, PROFILE_DEV_PKG) {
        Some(s) if !offset_suppressed(s, regions) => s,
        _ => return 1,
    };
    let after = pkg_start + PROFILE_DEV_PKG.len();
    let rest = &bytes[after..];
    let next = ["[profile", "[dependencies]"]
        .iter()
        .filter_map(|p| memmem::find(rest, p.as_bytes()))
        .min()
        .unwrap_or(rest.len());
    let block = &bytes[after..after + next];
    if has_key_in(block, b"debug") { 0 } else { 1 }
}

fn detect_debugging_profile_no_debug_true(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let Some(start) = find_profile_debugging_exact(bytes, regions) else {
        return 0;
    };
    let after = start + PROFILE_DEBUGGING.len();
    let rest = &bytes[after..];
    let next = ["[profile", "[dependencies]"]
        .iter()
        .filter_map(|p| memmem::find(rest, p.as_bytes()))
        .min()
        .unwrap_or(rest.len());
    let block = &bytes[after..after + next];
    if has_key_in(block, b"debug") { 0 } else { 1 }
}

fn push_rust_findings(report: &mut BuildConfigReport, bytes: &[u8], regions: &[(usize, usize)]) {
    if !has_profile_release(bytes, regions) {
        report.push(
            "no [profile.release] override (uses Cargo defaults -- large binary, no LTO/strip/panic=abort)",
            1, 0.7,
        );
    } else {
        report.push(
            "release profile missing `lto` (no cross-crate inlining)",
            detect_release_no_lto(bytes, regions),
            0.9,
        );
        report.push(
            "release profile missing `codegen-units = 1` (parallel codegen = less optimization)",
            detect_release_codegen_units_not_1(bytes, regions),
            0.7,
        );
        report.push(
            "release profile missing `panic = abort` (larger binary, unwinding tables kept)",
            detect_release_panic_unwind(bytes, regions),
            0.6,
        );
        report.push(
            "release profile missing `strip` (default `none` -- symbols in binary)",
            detect_release_no_strip(bytes, regions),
            0.7,
        );
        report.push(
            "release profile with `incremental = true` (memory leak in long-running procs)",
            detect_release_incremental_true(bytes, regions),
            0.5,
        );
    }
    report.push("dev profile missing `debug = line-tables-only` (default `true` = full debug, bloats target)", detect_dev_debug_full(bytes, regions), 0.7);
    report.push(
        "missing `[profile.dev.package.*] debug = false` (deps carry full debug info into target)",
        detect_dev_no_package_debug_false(bytes, regions),
        0.6,
    );
    report.push(
        "`[profile.debugging]` declared but missing `debug = true` (opt-in profile is broken)",
        detect_debugging_profile_no_debug_true(bytes, regions),
        0.5,
    );
}

// ── Python: pyproject.toml profile smells ──────────────────────────────────

const PY_BUILD_SYSTEM: &[u8] = b"[build-system]";
const PY_REQUIRES_PYTHON: &[u8] = b"requires-python";

/// `[build-system]` block missing — project has no build configuration. PEP 517
/// requires this for any project that builds a wheel.
fn detect_py_no_build_system(bytes: &[u8], _regions: &[(usize, usize)]) -> usize {
    if memmem::find(bytes, PY_BUILD_SYSTEM).is_some() {
        0
    } else {
        1
    }
}

/// `[build-system] requires = [...]` with a `>=` (not `==`) — unpinned
/// build deps, non-reproducible builds.
fn detect_py_unpinned_build_requires(bytes: &[u8], _regions: &[(usize, usize)]) -> usize {
    let start = match memmem::find(bytes, PY_BUILD_SYSTEM) {
        Some(s) => s,
        None => return 0,
    };
    let after = start + PY_BUILD_SYSTEM.len();
    let rest = &bytes[after..];
    let next = ["[tool", "[project]"]
        .iter()
        .filter_map(|p| memmem::find(rest, p.as_bytes()))
        .min()
        .unwrap_or(rest.len());
    let block = &bytes[after..after + next];
    if memmem::find(block, b"\"setuptools>=").is_some()
        || memmem::find(block, b"\"wheel>=").is_some()
        || memmem::find(block, b"\"hatchling>=").is_some()
        || memmem::find(block, b"\"poetry>=").is_some()
    {
        1
    } else {
        0
    }
}

/// No `requires-python` in `[project]` (or `[tool.poetry.dependencies]`) — no
/// declared Python-version compatibility.
fn detect_py_no_requires_python(bytes: &[u8], _regions: &[(usize, usize)]) -> usize {
    if memmem::find(bytes, PY_REQUIRES_PYTHON).is_some() {
        return 0;
    }
    1
}

/// No test runner declared in `[tool.pytest.ini_options]` / `[tool.pytest]`
/// or `[tool.poetry.group.test.dependencies]`. Absence means ad-hoc testing
/// (or no tests at all).
fn detect_py_no_test_runner(bytes: &[u8], _regions: &[(usize, usize)]) -> usize {
    let has_pytest = memmem::find(bytes, b"tool.pytest").is_some();
    let has_tox = memmem::find(bytes, b"[tool.tox]").is_some();
    let has_nose = memmem::find(bytes, b"tool.nose2").is_some();
    if has_pytest || has_tox || has_nose {
        0
    } else {
        1
    }
}

fn push_python_findings(report: &mut BuildConfigReport, bytes: &[u8], regions: &[(usize, usize)]) {
    report.push(
        "no `[build-system]` block in pyproject.toml (PEP 517 requires this for wheel builds)",
        detect_py_no_build_system(bytes, regions),
        0.7,
    );
    report.push(
        "build-system `requires = [...]` unpinned (`>=` instead of `==` -- non-reproducible builds)",
        detect_py_unpinned_build_requires(bytes, regions),
        0.8,
    );
    report.push(
        "no `requires-python` in pyproject.toml (no declared Python-version compat)",
        detect_py_no_requires_python(bytes, regions),
        0.6,
    );
    report.push(
        "no test runner declared in pyproject.toml (pytest/tox/nose -- ad-hoc or no tests)",
        detect_py_no_test_runner(bytes, regions),
        0.5,
    );
}

// ── JS/TS: package.json profile smells ────────────────────────────────────

const PKG_ENGINES: &[u8] = b"\"engines\"";
const PKG_NODE: &[u8] = b"\"node\"";
const PKG_SCRIPTS: &[u8] = b"\"scripts\"";
const PKG_BUILD: &[u8] = b"\"build\"";
const PKG_TEST: &[u8] = b"\"test\"";

/// `package.json` has no `engines.node` — no declared Node-version
/// compatibility.
fn detect_pkg_no_engines_node(bytes: &[u8], _regions: &[(usize, usize)]) -> usize {
    if memmem::find(bytes, PKG_ENGINES).is_none() {
        return 1; // no engines at all
    }
    // has engines — check if it mentions node
    if memmem::find(bytes, PKG_NODE).is_some() {
        0
    } else {
        1
    }
}

/// `package.json` has no `scripts.build` and no `scripts.test` — no build
/// or test command declared.
fn detect_pkg_no_scripts_build_test(bytes: &[u8], _regions: &[(usize, usize)]) -> usize {
    if memmem::find(bytes, PKG_SCRIPTS).is_none() {
        return 1; // no scripts at all
    }
    // check both build and test are present
    let has_build = memmem::find(bytes, PKG_BUILD).is_some();
    let has_test = memmem::find(bytes, PKG_TEST).is_some();
    if has_build && has_test { 0 } else { 1 }
}

/// A `dependencies` or `devDependencies` block contains a `*` or a missing
/// version literal (e.g. `"foo":` followed by no value). Unpinned deps
/// are non-reproducible.
fn detect_pkg_unpinned_dependencies(bytes: &[u8], _regions: &[(usize, usize)]) -> usize {
    let deps_block = find_pkg_block(bytes, b"\"dependencies\"");
    let dev_block = find_pkg_block(bytes, b"\"devDependencies\"");
    let (combined_start, combined_end) = match (deps_block, dev_block) {
        (Some((ds, de)), Some((vs, ve))) => (ds.min(vs), de.max(ve)),
        (Some((ds, de)), None) => (ds, de),
        (None, Some((vs, ve))) => (vs, ve),
        (None, None) => return 0,
    };
    let block = &bytes[combined_start..combined_end];
    // Match `"name": "*"`
    let mut off = 0;
    let mut unpinned = 0;
    while let Some(rel) = memmem::find(&block[off..], b": \"*\"") {
        unpinned += 1;
        off += rel + 4;
    }
    if unpinned > 0 { unpinned } else { 0 }
}

/// Find a top-level package.json block by header (e.g. `"dependencies"`).
/// Returns the byte range from `{` after the header to the matching `}`.
fn find_pkg_block(bytes: &[u8], header: &[u8]) -> Option<(usize, usize)> {
    let h = memmem::find(bytes, header)?;
    let after = h + header.len();
    let rest = &bytes[after..];
    // Find the opening `{` (skip whitespace)
    let open = rest.iter().position(|&b| b == b'{')? + after;
    // Find the matching `}` by counting braces from `open` forward.
    let mut depth = 0i32;
    let mut end = open;
    for (i, &b) in bytes[open..].iter().enumerate() {
        if b == b'{' {
            depth += 1;
        } else if b == b'}' {
            depth -= 1;
            if depth == 0 {
                end = open + i;
                break;
            }
        }
    }
    if depth != 0 { None } else { Some((open, end)) }
}

fn push_jsts_findings(report: &mut BuildConfigReport, bytes: &[u8], regions: &[(usize, usize)]) {
    report.push(
        "package.json has no `engines.node` (no declared Node-version compat)",
        detect_pkg_no_engines_node(bytes, regions),
        0.5,
    );
    report.push(
        "package.json has no `scripts.build` or `scripts.test` (no build or test command declared)",
        detect_pkg_no_scripts_build_test(bytes, regions),
        0.7,
    );
    report.push(
        "package.json has unpinned dependency (`*` literal -- non-reproducible installs)",
        detect_pkg_unpinned_dependencies(bytes, regions),
        0.8,
    );
}

// ── Go: go.mod profile smells ─────────────────────────────────────────────

const GO_MODULE: &[u8] = b"module ";
const GO_GO: &[u8] = b"\ngo ";
const GO_REQUIRE: &[u8] = b"require (";
const GO_LATEST: &[u8] = b" latest\n";

/// `go.mod` has no `module` directive (Go requires this).
fn detect_go_no_module(bytes: &[u8], _regions: &[(usize, usize)]) -> usize {
    if memmem::find(bytes, GO_MODULE).is_some() {
        0
    } else {
        1
    }
}

/// `go.mod` has `go 1` (not pinned to a minor: `1.21.X`). Unpinned go
/// version means every toolchain fetch may pick a different compiler.
fn detect_go_unpinned_go_version(bytes: &[u8], _regions: &[(usize, usize)]) -> usize {
    // The directive is `go 1.21` (or `go 1.21.0` etc.). We want
    // exactly the `go 1\n` or `go 1 ` case (no minor).
    if let Some(rel) = memmem::find(bytes, GO_GO) {
        let after_dot = rel + GO_GO.len();
        let rest = &bytes[after_dot..];
        // Skip past the major version `1`
        if rest.first() == Some(&b'1') {
            // Next char should be `.` (pinned)
            if rest.get(1) == Some(&b'.') {
                return 0;
            }
            return 1;
        }
    }
    0
}

/// `require` block contains a `latest` pseudo-version. Go modules should
/// use real semver (or commit hash) -- `latest` defeats reproducibility.
fn detect_go_require_latest(bytes: &[u8], _regions: &[(usize, usize)]) -> usize {
    if memmem::find(bytes, GO_LATEST).is_some() {
        1
    } else {
        0
    }
}

/// `require ( ... )` block exists but is empty (no deps declared) — usually
/// means an empty module; no build to do.
fn detect_go_empty_require(bytes: &[u8], _regions: &[(usize, usize)]) -> usize {
    if let Some(rel) = memmem::find(bytes, GO_REQUIRE) {
        let after = rel + GO_REQUIRE.len();
        let rest = &bytes[after..];
        let close = memmem::find(rest, b")").unwrap_or(rest.len());
        let body = &rest[..close];
        // empty require block: only whitespace inside
        if body
            .iter()
            .all(|&b| b == b' ' || b == b'\n' || b == b'\t' || b == b'\r')
        {
            return 1;
        }
    }
    0
}

fn push_go_findings(report: &mut BuildConfigReport, bytes: &[u8], regions: &[(usize, usize)]) {
    report.push(
        "go.mod has no `module` directive (Go requires this)",
        detect_go_no_module(bytes, regions),
        0.7,
    );
    report.push(
        "go.mod `go` directive not pinned to minor (e.g. `go 1` instead of `go 1.21`)",
        detect_go_unpinned_go_version(bytes, regions),
        0.6,
    );
    report.push(
        "go.mod `require ( ... )` block uses `latest` pseudo-version (non-reproducible builds)",
        detect_go_require_latest(bytes, regions),
        0.9,
    );
    report.push(
        "go.mod `require` block is empty (no dependencies declared)",
        detect_go_empty_require(bytes, regions),
        0.4,
    );
}

// ── Public entry point ───────────────────────────────────────────────────

/// Analyze build-configuration smells in `source` for the given language.
/// Polyglot: Rust (Cargo.toml), Python (pyproject.toml), JS/TS
/// (package.json), Go (go.mod). Other langs report no findings.
pub fn analyze_build_config(source: &str, lang: &str) -> BuildConfigReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, "rust");
    let mut report = BuildConfigReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    let l = canonical_lang(lang);
    match l {
        Lang::Rust => push_rust_findings(&mut report, bytes, &regions),
        Lang::Python => push_python_findings(&mut report, bytes, &regions),
        Lang::JsTs => push_jsts_findings(&mut report, bytes, &regions),
        Lang::Go => push_go_findings(&mut report, bytes, &regions),
        Lang::Other => {}
    }
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`BuildConfigReport`] as `1 - density * SCALE`, clamped to `[0, 1]`.
pub fn score_build_config(report: &BuildConfigReport) -> f32 {
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rep(src: &str, lang: &str) -> BuildConfigReport {
        analyze_build_config(src, lang)
    }

    // ── Rust: existing tests (preserved) ────────────────────────────────

    #[test]
    fn empty_file_clean() {
        let r = rep("", "rust");
        // empty file is a finding (no [profile.release] at all)
        assert!(r.violations >= 1, "empty file: {:?} ", r.findings);
    }

    #[test]
    fn release_no_lto_flagged() {
        let src = r#"[package]
name = "x"

[profile.release]
opt-level = 3
"#;
        let r = rep(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("lto")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn release_lto_set_clean() {
        let src = r#"[profile.release]
lto = "fat"
codegen-units = 1
panic = "abort"
strip = "symbols"
"#;
        let r = rep(src, "rust");
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("lto")
                || m.contains("codegen")
                || m.contains("panic")
                || m.contains("strip")),
            "fully-tuned release is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn release_codegen_units_flagged() {
        let src = r#"[profile.release]
lto = "fat"
"#;
        let r = rep(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("codegen-units")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn release_panic_unwind_flagged() {
        let src = r#"[profile.release]
lto = "fat"
codegen-units = 1
"#;
        let r = rep(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("panic")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn release_strip_missing_flagged() {
        let src = r#"[profile.release]
lto = "fat"
codegen-units = 1
panic = "abort"
"#;
        let r = rep(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("strip")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn release_incremental_true_flagged() {
        let src = r#"[profile.release]
lto = "fat"
incremental = true
"#;
        let r = rep(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("incremental")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn dev_debug_full_flagged() {
        let src = r#"[profile.dev]
opt-level = 0
"#;
        let r = rep(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("line-tables-only")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn dev_no_package_debug_false_flagged() {
        let src = r#"[profile.dev]
debug = "line-tables-only"
"#;
        let r = rep(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("package")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn dev_with_package_debug_false_clean() {
        let src = r#"[profile.dev]
debug = "line-tables-only"

[profile.dev.package."*"]
debug = false
"#;
        let r = rep(src, "rust");
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("package")),
            "package debug=false is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn debugging_profile_no_debug_true_flagged() {
        let src = r#"[profile.debugging]
inherits = "dev"
"#;
        let r = rep(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("debugging") && m.contains("broken")),
            "debugging profile missing debug = true: {:?}",
            r.findings
        );
    }

    #[test]
    fn no_profile_release_flagged() {
        let src = r#"[package]
name = "x"
version = "0.1.0"
"#;
        let r = rep(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("no [profile.release]")),
            "no [profile.release] flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = rep(
            r#"[profile.release]
opt-level = 3
"#,
            "rust",
        );
        let good = rep(
            r#"[profile.release]
lto = "fat"
codegen-units = 1
panic = "abort"
strip = "symbols"

[profile.dev]
debug = "line-tables-only"

[profile.dev.package."*"]
debug = false
"#,
            "rust",
        );
        assert!(
            score_build_config(&bad) < score_build_config(&good),
            "untuned ({:.3}) must score below tuned ({:.3})",
            score_build_config(&bad),
            score_build_config(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = rep(
            r#"[profile.release]
opt-level = 3
"#,
            "rust",
        );
        let s = score_build_config(&r);
        assert!(s > 0.0, "short untuned file must not score 0.0: {s}");
    }

    // ── Python: pyproject.toml ──────────────────────────────────────────

    #[test]
    fn py_no_build_system_flagged() {
        let src = r#"[project]
name = "x"
version = "0.1.0"
"#;
        let r = rep(src, "python");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("build-system")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn py_with_build_system_clean() {
        let src = r#"[build-system]
requires = ["setuptools==69.0", "wheel==0.42"]

[project]
name = "x"
requires-python = ">=3.10"

[tool.pytest.ini_options]
testpaths = ["tests"]
"#;
        let r = rep(src, "python");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("build-system") && m.contains("PEP 517")),
            "tuned pyproject is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn py_unpinned_build_requires_flagged() {
        let src = r#"[build-system]
requires = ["setuptools>=69.0"]
"#;
        let r = rep(src, "python");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("unpinned")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn py_no_requires_python_flagged() {
        let src = r#"[build-system]
requires = ["setuptools==69.0"]

[project]
name = "x"
"#;
        let r = rep(src, "python");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("requires-python")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn py_with_test_runner_clean() {
        let src = r#"[build-system]
requires = ["setuptools==69.0"]
requires-python = ">=3.10"

[tool.pytest.ini_options]
testpaths = ["tests"]
"#;
        let r = rep(src, "python");
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("test runner")),
            "pytest declared: {:?}",
            r.findings
        );
    }

    // ── JS/TS: package.json ─────────────────────────────────────────────

    #[test]
    fn pkg_no_engines_flagged() {
        let src = r#"{
  "name": "x",
  "version": "0.1.0",
  "scripts": {
    "build": "tsc",
    "test": "jest"
  }
}
"#;
        let r = rep(src, "typescript");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("engines")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn pkg_with_engines_node_clean() {
        let src = r#"{
  "name": "x",
  "engines": { "node": ">=20" },
  "scripts": { "build": "tsc", "test": "jest" }
}
"#;
        let r = rep(src, "typescript");
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("engines")),
            "engines.node declared: {:?}",
            r.findings
        );
    }

    #[test]
    fn pkg_no_scripts_test_flagged() {
        let src = r#"{
  "name": "x",
  "engines": { "node": ">=20" }
}
"#;
        let r = rep(src, "javascript");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("scripts")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn pkg_unpinned_dep_flagged() {
        let src = r#"{
  "name": "x",
  "engines": { "node": ">=20" },
  "scripts": { "build": "tsc", "test": "jest" },
  "dependencies": { "foo": "*" }
}
"#;
        let r = rep(src, "javascript");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("unpinned")),
            "{:?}",
            r.findings
        );
    }

    // ── Go: go.mod ──────────────────────────────────────────────────────

    #[test]
    fn go_no_module_flagged() {
        let src = r#"go 1.21

require (
    github.com/foo/bar v1.0.0
)
"#;
        let r = rep(src, "go");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("module")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn go_pinned_version_clean() {
        let src = r#"module example.com/foo

go 1.21.0

require (
    github.com/foo/bar v1.0.0
)
"#;
        let r = rep(src, "go");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("not pinned") || m.contains("module")),
            "pinned go.mod: {:?}",
            r.findings
        );
    }

    #[test]
    fn go_unpinned_version_flagged() {
        let src = r#"module example.com/foo

go 1

require (
    github.com/foo/bar v1.0.0
)
"#;
        let r = rep(src, "go");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("not pinned")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn go_require_latest_flagged() {
        let src = r#"module example.com/foo

go 1.21.0

require (
    github.com/foo/bar latest
)
"#;
        let r = rep(src, "go");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("latest")),
            "{:?}",
            r.findings
        );
    }

    // ── Cross-language tests ─────────────────────────────────────────────

    #[test]
    fn other_lang_no_findings() {
        let r = rep("anything", "ruby");
        assert_eq!(
            r.violations, 0,
            "unsupported lang reports no findings: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_engine_does_not_fire_on_python() {
        // When lang="python", the Rust profile detectors should NOT fire on
        // Rust-looking content (e.g. a pyproject.toml that happens to
        // contain `[profile.release]`).
        let src = r#"[build-system]
requires = ["setuptools==69.0"]
"#;
        let r = rep(src, "python");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("profile.release")),
            "Rust detectors do not fire on Python lang: {:?}",
            r.findings
        );
    }
}
