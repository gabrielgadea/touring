//! Environment Management (D52 / F4.12) — secrets-from-env / 12-factor
//! compliance. Secrets must come from a secret manager (Vault/SOPS), not
//! from hardcoded literals or `.env` files committed to git. Config that
//! varies by environment must come from env vars, not hardcoded literals.
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | `env-file-committed` | `.env` referenced in non-`.gitignore` (secrets risk) | file-tree |
//! | `no-secret-manager` | project with `std::env::var` but no Vault / SOPS / `dotenv` reference | Rust |
//! | `hardcoded-url` | `http://` or `https://` literal in source (non-12-factor) | Rust/JS/TS/Python |
//! | `hardcoded-port` | `:8080` or `:3000` literal (non-12-factor — config should come from env) | Rust/JS/TS/Python |
//! | `no-config-layer` | Rust project with no `figment` / `config` / `dotenvy` reference (config is hardcoded) | Rust |
//! | `py-os-environ-no-config` | Python using `os.environ[...]` without `pydantic_settings` (config layer missing) | Python |
//! | `no-12-factor-config` | no `std::env::var` / `os.environ` / `process.env` reads at all (config 100% hardcoded) — Python: only in an entry point (`if __name__ ==`), reads in any spelling (S512) | polyglot |
//!
//! **Disjoint** from D17 secrets (which detects *literal* secrets; F4.12
//! detects the *absence* of a secret-management layer) and D19 config
//! (which keys on debug/CORS/headers; F4.12 keys on the env-vs-config
//! management process).
//!
//! **Sources (context7, `/hashicorp/vault`, High reputation, bench 77.7 +
//! `/getsops/sops`):** Vault centrally manages secrets, rotates old
//! credentials, generates credentials on demand. SOPS encrypts YAML/JSON
//! secrets with KMS/age (cleartext never enters Git). 12-factor III:
//! config that varies by environment comes from env vars / secret
//! manager, not from per-environment files in the code.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

const SCALE: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    Python,
    JsTs,
    Other,
}

fn canonical_lang(lang: &str) -> Lang {
    match lang {
        "rust" | "rs" => Lang::Rust,
        "python" | "py" => Lang::Python,
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" | "mjs" | "cjs" => Lang::JsTs,
        _ => Lang::Other,
    }
}

/// Environment-management findings for one file.
pub type EnvReport = crate::quality::SmellReport;

const STDLIB_ENV_VAR: &[u8] = b"std::env::var";
const STDLIB_ENV: &[u8] = b"std::env::";
const VAULT_ADDR: &[u8] = b"VAULT_ADDR";
const VAULT_TOKEN: &[u8] = b"VAULT_TOKEN";
const SOPS: &[u8] = b"sops";
const DOTENV: &[u8] = b"dotenv";
const FIGMENT: &[u8] = b"figment";
const CONFIG_RS: &[u8] = b"config::Config";
const HTTP_LITERAL: &[u8] = b"http://";
const HTTPS_LITERAL: &[u8] = b"https://";
const PY_OS_ENVIRON: &[u8] = b"os.environ[";
/// Every spelling of a Python env read (S512, 26/09/2026): `os.environ.get` and
/// `os.getenv` were read as "no env" and flagged.
const PY_ENV_READS: &[&[u8]] = &[
    b"os.environ",
    b"environ[",
    b"environ.get(",
    b"getenv(",
    b"BaseSettings",
    b"pydantic_settings",
];
/// A CLI takes its config from the invocation (S512: 69,4% of the analise's
/// flagged entry points parse arguments) — external config, like the env.
const PY_CLI_CONFIG: &[&[u8]] = &[
    b"argparse",
    b"import click",
    b"from click",
    b"import typer",
    b"from typer",
    b"sys.argv",
];
/// The module-level guard that makes a Python file an entry point.
const PY_MAIN_GUARD: &[u8] = b"if __name__ ==";
const PY_PYDANTIC_SETTINGS: &[u8] = b"pydantic_settings";
const PY_BASE_SETTINGS: &[u8] = b"BaseSettings";
const JS_DOTENV: &[u8] = b"dotenv";
const JS_PROCESS_ENV: &[u8] = b"process.env";

fn count_in_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> usize {
    memmem::find_iter(bytes, needle)
        .filter(|&off| !offset_suppressed(off, regions))
        .count()
}

fn has_in_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> bool {
    count_in_executable(bytes, regions, needle) > 0
}

fn push_rust_findings(report: &mut EnvReport, bytes: &[u8], regions: &[(usize, usize)]) {
    let has_env_var = has_in_executable(bytes, regions, STDLIB_ENV_VAR)
        || has_in_executable(bytes, regions, STDLIB_ENV);
    // If project reads env vars but has no secret manager / config layer,
    // env vars are likely the only source -- fragile.
    if has_env_var {
        let has_secret_mgr = has_in_executable(bytes, regions, VAULT_ADDR)
            || has_in_executable(bytes, regions, VAULT_TOKEN)
            || has_in_executable(bytes, regions, SOPS)
            || has_in_executable(bytes, regions, DOTENV)
            || has_in_executable(bytes, regions, FIGMENT)
            || has_in_executable(bytes, regions, CONFIG_RS);
        if !has_secret_mgr {
            report.push(
                "uses `std::env::var` but no Vault / SOPS / `dotenv` / `figment` / `config` reference (no secret manager)",
                1, 0.9,
            );
        }
    }
    // No config layer at all
    if !has_in_executable(bytes, regions, FIGMENT)
        && !has_in_executable(bytes, regions, CONFIG_RS)
        && !has_in_executable(bytes, regions, DOTENV)
    {
        report.push(
            "no `figment` / `config` / `dotenvy` reference (no structured config layer)",
            1,
            0.6,
        );
    }
    // Hardcoded URLs
    let http_count = count_in_executable(bytes, regions, HTTP_LITERAL)
        + count_in_executable(bytes, regions, HTTPS_LITERAL);
    if http_count > 0 {
        report.push(
            "hardcoded `http://` or `https://` URL (non-12-factor -- should come from env)",
            http_count,
            0.5,
        );
    }
}

fn push_python_findings(report: &mut EnvReport, bytes: &[u8], regions: &[(usize, usize)]) {
    let has_environ = has_in_executable(bytes, regions, PY_OS_ENVIRON);
    if has_environ
        && !has_in_executable(bytes, regions, PY_PYDANTIC_SETTINGS)
        && !has_in_executable(bytes, regions, PY_BASE_SETTINGS)
    {
        report.push(
            "uses `os.environ[...]` without `pydantic_settings` / `BaseSettings` (no typed config layer)",
            1, 0.7,
        );
    }
    // No env reads at all (12-factor violation) — S512 (26/09/2026): only in an
    // entry point, where the process reads its config. Gated on size alone it
    // flagged 97,3% of the analise's eligible scripts (a pure module dropped
    // 1.0 -> 0.88 by growing from 29 to 30 lines): a constant is not a signal.
    let reads_env = PY_ENV_READS
        .iter()
        .chain(PY_CLI_CONFIG)
        .any(|needle| has_in_executable(bytes, regions, needle));
    if !reads_env && has_in_executable(bytes, regions, PY_MAIN_GUARD) && report.total_lines >= 30 {
        report.push(
            "no `os.environ` reads nor CLI arguments in an entry point (12-factor: config that \
             varies must come from outside the code)",
            1,
            0.6,
        );
    }
    // Hardcoded URLs
    let http_count = count_in_executable(bytes, regions, HTTP_LITERAL)
        + count_in_executable(bytes, regions, HTTPS_LITERAL);
    if http_count > 0 {
        report.push(
            "hardcoded `http://` / `https://` URL (non-12-factor)",
            http_count,
            0.5,
        );
    }
}

fn push_jsts_findings(report: &mut EnvReport, bytes: &[u8], regions: &[(usize, usize)]) {
    let has_env = has_in_executable(bytes, regions, JS_PROCESS_ENV);
    let has_dotenv = has_in_executable(bytes, regions, JS_DOTENV);
    if has_env && !has_dotenv {
        report.push(
            "uses `process.env` without `dotenv` / similar (no structured env loading)",
            1,
            0.6,
        );
    }
    if !has_env && report.total_lines >= 30 {
        report.push(
            "no `process.env` reads (12-factor: config that varies by env must come from env vars)",
            1,
            0.5,
        );
    }
    let http_count = count_in_executable(bytes, regions, HTTP_LITERAL)
        + count_in_executable(bytes, regions, HTTPS_LITERAL);
    if http_count > 0 {
        report.push("hardcoded `http://` / `https://` URL", http_count, 0.5);
    }
}

/// Analyze environment-management smells in `source` for the given language.
/// Polyglot: Rust + Python + JS/TS.
pub fn analyze_env(source: &str, lang: &str) -> EnvReport {
    let bytes = source.as_bytes();
    // S512: each language masks its OWN comments — every language was lexed as
    // Rust here, so a Python `# os.environ[...]` comment counted as a read.
    let region_lang = match canonical_lang(lang) {
        Lang::Python => "python",
        Lang::JsTs => "javascript",
        Lang::Rust | Lang::Other => "rust",
    };
    let regions = non_executable_regions(source, region_lang);
    let mut report = EnvReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    match canonical_lang(lang) {
        Lang::Rust => push_rust_findings(&mut report, bytes, &regions),
        Lang::Python => push_python_findings(&mut report, bytes, &regions),
        Lang::JsTs => push_jsts_findings(&mut report, bytes, &regions),
        Lang::Other => {}
    }
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`EnvReport`] as `1 - density * SCALE`, clamped to `[0, 1]`.
pub fn score_env(report: &EnvReport) -> f32 {
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rep(src: &str, lang: &str) -> EnvReport {
        analyze_env(src, lang)
    }

    #[test]
    fn empty_file_clean() {
        let r = rep("", "rust");
        // Empty Rust file legitimately fires the "no config layer" finding
        // (no figment / config / dotenvy referenced).
        assert!(r.violations >= 1, "empty Rust file: {:?} ", r.findings);
    }

    #[test]
    fn rust_env_no_secret_mgr_flagged() {
        let src = r#"fn main() {
    let url = std::env::var("DATABASE_URL").unwrap();
}
"#;
        let r = rep(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("secret manager")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn rust_env_with_vault_clean() {
        let src = r#"fn main() {
    let token = std::env::var("VAULT_TOKEN").unwrap();
    let addr = std::env::var("VAULT_ADDR").unwrap();
}
"#;
        let r = rep(src, "rust");
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("secret manager")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn rust_hardcoded_url_flagged() {
        let src = r#"fn main() {
    let url = "https://api.example.com/v1";
}
"#;
        let r = rep(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("http")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn py_environ_no_settings_flagged() {
        let src = r#"import os

def get_url():
    return os.environ["DATABASE_URL"]
"#;
        let r = rep(src, "python");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("pydantic")),
            "{:?}",
            r.findings
        );
    }

    /// An `n`-line pure Python module: arithmetic helpers, no config, no I/O.
    fn pure_py(n: usize) -> String {
        let mut out = String::from("\"\"\"Pure helpers.\"\"\"\n");
        let mut i = 0;
        while out.lines().count() < n {
            out.push_str(&format!("def f{i}(x):\n    return x + {i}\n\n"));
            i += 1;
        }
        out
    }

    fn flags_no_env(src: &str) -> bool {
        rep(src, "python")
            .findings
            .iter()
            .any(|(m, _)| m.contains("no `os.environ` reads"))
    }

    /// S512 (26/09/2026): the absence smell fired on every Python file of 30+
    /// lines without `os.environ[` — 97,3% of the analise's eligible scripts, a
    /// near-constant signal; a pure module dropped 1.0 -> 0.88 by growing from 29
    /// to 30 lines. 12-factor config is read where the process starts: an entry
    /// point. And a read is a read in any spelling — `os.environ.get` and
    /// `os.getenv` were false positives. A comment is not a read.
    #[test]
    fn py_env_absence_is_an_entry_point_smell_and_any_read_counts() {
        let module = pure_py(40);
        assert!(!flags_no_env(&module), "a library module is no entry point");
        let entry = format!("{module}\nif __name__ == \"__main__\":\n    f0(1)\n");
        assert!(flags_no_env(&entry), "an entry point that reads no env");
        for read in [
            "os.environ.get(\"DB\")",
            "os.getenv(\"DB\")",
            "os.environ[\"DB\"]",
            "environ.get(\"DB\")",
        ] {
            let with = format!("import os\nDB = {read}\n{entry}");
            assert!(!flags_no_env(&with), "{read} is an env read");
        }
        let commented = format!("# DB = os.environ[\"DB\"]\n{entry}");
        assert!(flags_no_env(&commented), "a comment is not a read");
        // A CLI takes its config from the invocation: 69,4% of the analise's
        // flagged entry points parse arguments — the smell is "config 100%
        // hardcoded", not "no env var".
        for cli in [
            "import argparse",
            "import click",
            "import typer",
            "import sys\nARGS = sys.argv[1:]",
        ] {
            let with = format!("{cli}\n{entry}");
            assert!(!flags_no_env(&with), "{cli} is external config");
        }
    }

    #[test]
    fn py_with_pydantic_clean() {
        let src = r#"import os
from pydantic_settings import BaseSettings

class Settings(BaseSettings):
    database_url: str
"#;
        let r = rep(src, "python");
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("pydantic")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn js_process_env_no_dotenv_flagged() {
        let src = r#"function getConfig() {
  return process.env.DATABASE_URL;
}
"#;
        let r = rep(src, "javascript");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("dotenv")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn other_lang_no_findings() {
        let r = rep("anything", "ruby");
        assert_eq!(r.violations, 0, "unsupported lang: {:?}", r.findings);
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = rep(
            r#"fn main() {
    let url = "https://api.example.com/v1";
    let token = std::env::var("TOKEN").unwrap();
}
"#,
            "rust",
        );
        let good = rep(
            r#"fn main() {
    let token = std::env::var("VAULT_TOKEN").unwrap();
}
"#,
            "rust",
        );
        assert!(
            score_env(&bad) < score_env(&good),
            "hardcoded+env ({:.3}) must score below vault-clean ({:.3})",
            score_env(&bad),
            score_env(&good)
        );
    }
}
