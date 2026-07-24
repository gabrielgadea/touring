//! ConfigSecurityAnalyzer — OWASP A05:2021 Security Misconfiguration detector.
//!
//! Sibling to [`super::security::SecurityAnalyzer`] (which covers the injection
//! CWE class via the offensive `PatternRegistry`). This analyzer targets the
//! *misconfiguration* class — settings insecure regardless of input: disabled
//! TLS/cert verification (CWE-295), permissive CORS (CWE-942), active debug /
//! verbose runtime in production (CWE-489), insecure cookie flags (CWE-614),
//! unsafe Content-Security-Policy (CWE-693), and world-writable permissions
//! (CWE-732). Grounded on the D19 (F2.6) catalog + OWASP A05 + Trivy/checkov
//! misconfiguration families.
//!
//! Design mirrors `SecurityAnalyzer` so F2.6 composes consistently with F2.1:
//! - emits [`touring_offensive::vuln::VulnMatch`] (shared shape: pattern_name,
//!   span, severity, cwe_id), so the same AST-aware [`super::code_regions`] pass
//!   drops matches that live in comments / `#[cfg(test)]` corpora;
//! - severity-summed score `1.0 - sum/10` clamped — identical to the vuln-score
//!   half of `SecurityReport`.
//!
//! Zero-dependency line scanner (no `regex` dep, matching `code_regions`): each
//! rule is a `fn(lowercased_trimmed, trimmed) -> bool`. Precision over recall —
//! every rule is a high-signal, low-false-positive misconfiguration, never a
//! broad keyword. Pure-comment lines are skipped before matching (the dominant
//! false-positive source across `//` and `#` comment families), and the
//! `code_regions` pass additionally suppresses `#[cfg(test)]` / block-comment
//! interiors for code languages.

use touring_offensive::vuln::VulnMatch;

/// One misconfiguration rule.
struct ConfigRule {
    /// Stable rule name surfaced in evidence.
    name: &'static str,
    /// Associated CWE id.
    cwe_id: u32,
    /// Severity in `[0.0, 10.0]`. Critical (TLS off) ~6.0 → a single hit fails
    /// the F2.6 BLOCK gate (`1.0 - 6.0/10 = 0.4 < 0.5`).
    severity: f32,
    /// Matcher over `(lowercased_trimmed_line, trimmed_line)`.
    matches: fn(&str, &str) -> bool,
}

/// Report of configuration-security misconfigurations in a single source.
#[derive(Debug, Clone)]
pub struct ConfigReport {
    /// Language / format id used for analysis.
    pub lang: String,
    /// Misconfiguration matches (post region-suppression).
    pub misconfigs: Vec<VulnMatch>,
    /// Score in `[0.0, 1.0]`: `1.0 - sum(severity)/10`, clamped. `1.0` = clean.
    pub score: f32,
}

/// OWASP A05 Security Misconfiguration analyzer.
pub struct ConfigSecurityAnalyzer {
    rules: &'static [ConfigRule],
}

impl ConfigSecurityAnalyzer {
    /// New analyzer with the built-in OWASP A05 rule catalog.
    #[must_use]
    pub fn new() -> Self {
        Self { rules: RULES }
    }

    /// Scan `source` for configuration-security misconfigurations.
    ///
    /// Line-granular: pure-comment lines are skipped; each remaining line is
    /// matched against every rule. A hit records a [`VulnMatch`] whose `span.0`
    /// is the offset of the line's first non-whitespace byte, so the AST-aware
    /// [`super::code_regions`] pass can drop hits inside `#[cfg(test)]` corpora
    /// or block comments (mirrors `SecurityAnalyzer::analyze`).
    #[must_use]
    pub fn analyze(&self, source: &str, lang: &str) -> ConfigReport {
        let mut misconfigs: Vec<VulnMatch> = Vec::new();
        let mut offset = 0usize;
        for raw_line in source.lines() {
            let trimmed = raw_line.trim_start();
            let indent = raw_line.len() - trimmed.len();
            // Skip pure-comment lines (format-agnostic): the dominant FP source.
            let is_comment = trimmed.starts_with("//")
                || trimmed.starts_with('#')
                || trimmed.starts_with("/*")
                || trimmed.starts_with('*');
            if !is_comment {
                let lower = trimmed.to_ascii_lowercase();
                for rule in self.rules {
                    if (rule.matches)(&lower, trimmed) {
                        let start = offset + indent;
                        misconfigs.push(VulnMatch::new(
                            rule.name.to_string(),
                            (start, offset + raw_line.len()),
                            rule.severity,
                            rule.cwe_id,
                        ));
                    }
                }
            }
            // `lines()` strips the '\n'; +1 restores it (LF-assumed, like the
            // rest of the quality pipeline). Offsets feed monotonic region
            // containment only.
            offset += raw_line.len() + 1;
        }

        // AST-aware precision pass (mirrors SecurityAnalyzer): drop a misconfig
        // inside a comment or `#[cfg(test)]` corpus — doc/fixture, not a setting.
        if !misconfigs.is_empty() {
            let regions = super::code_regions::non_executable_regions(source, lang);
            if !regions.is_empty() {
                misconfigs.retain(|m| !super::code_regions::offset_suppressed(m.span.0, &regions));
            }
        }

        let score = if misconfigs.is_empty() {
            1.0
        } else {
            let sum: f32 = misconfigs.iter().map(|m| m.severity).sum();
            (1.0 - sum / 10.0).clamp(0.0, 1.0)
        };

        ConfigReport {
            lang: lang.to_string(),
            misconfigs,
            score,
        }
    }
}

impl Default for ConfigSecurityAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Built-in OWASP A05 misconfiguration catalog. Precision-tuned: each matcher
/// targets a settings-level insecurity that holds regardless of input.
static RULES: &[ConfigRule] = &[
    // CWE-295 — TLS / certificate verification disabled. Critical: silently
    // defeats HTTPS. Covers rust (reqwest/native-tls), js (`rejectUnauthorized`),
    // go (`InsecureSkipVerify`), python-requests (`verify=False`).
    ConfigRule {
        name: "tls_verification_disabled",
        cwe_id: 295,
        severity: 6.0,
        matches: |l, _r| {
            l.contains("danger_accept_invalid_certs(true)")
                || l.contains("danger_accept_invalid_hostnames(true)")
                || l.contains("accept_invalid_certs(true)")
                || l.contains("accept_invalid_hostnames(true)")
                || l.contains("rejectunauthorized: false")
                || l.contains("rejectunauthorized:false")
                || l.contains("insecureskipverify: true")
                || l.contains("insecureskipverify:true")
                || l.contains("insecureskipverify = true")
                || l.contains("verify=false")
                || l.contains("verify = false")
                // YAML/config-file forms (W2 2026-07-02): now that F2.6 resolves
                // real config artifacts, recognise the declarative keys too. The
                // `:` forms are config-file-only and cannot collide with the Cargo
                // `[profile] debug = true` false positive (that uses `=`).
                || l.contains("tls_enabled: false")
                || l.contains("tls_enabled:false")
                || l.contains("ssl_verify: false")
                || l.contains("ssl_verify:false")
                || l.contains("verify_ssl: false")
                || l.contains("verify_ssl:false")
                || l.contains("sslverify: false")
                || l.contains("ssl: false")
                || l.contains("tls: false")
        },
    },
    // CWE-942 — permissive CORS (wildcard origin / any-origin). High value: a
    // wildcard ACAO (esp. with credentials) enables cross-origin theft.
    ConfigRule {
        name: "cors_permissive_wildcard",
        cwe_id: 942,
        severity: 4.0,
        matches: |l, _r| {
            (l.contains("access-control-allow-origin") && l.contains('*'))
                || l.contains(".allow_any_origin(")
                || l.contains("alloworigin::any(")
                || l.contains(".allow_origin(any")
                || l.contains("cors::permissive(")
                || l.contains(".allow_origin(\"*\")")
                // YAML/config forms (W2 2026-07-02).
                || (l.contains("allow_origin") && l.contains('*'))
                || (l.contains("allowed_origins") && l.contains('*'))
                || (l.contains("allow-origin") && l.contains('*'))
        },
    },
    // CWE-489 — active debug / verbose runtime in production. Framework-scoped
    // (Flask) to avoid the Cargo `[profile] debug = true` (debuginfo) false hit.
    ConfigRule {
        name: "active_debug_production",
        cwe_id: 489,
        severity: 3.5,
        matches: |l, _r| {
            l.contains("app.run(debug=true")
                || l.contains("app.debug = true")
                || l.contains("app.debug=true")
                || l.contains("app.config['debug'] = true")
                || l.contains("flask_debug=1")
                || l.contains("flask_env=development")
        },
    },
    // CWE-614 / CWE-1004 — insecure cookie flags (Secure/HttpOnly off, SameSite
    // weakened).
    ConfigRule {
        name: "insecure_cookie_flags",
        cwe_id: 614,
        severity: 3.0,
        matches: |l, _r| {
            l.contains(".secure(false)")
                || l.contains(".http_only(false)")
                || l.contains(".httponly(false)")
                || l.contains("httponly: false")
                || l.contains("httponly:false")
                || l.contains("secure: false")
                || (l.contains("samesite") && l.contains("none"))
        },
    },
    // CWE-693 — Content-Security-Policy weakened with unsafe directives. The
    // `unsafe-inline` / `unsafe-eval` tokens appear only in a CSP context.
    ConfigRule {
        name: "csp_unsafe_directive",
        cwe_id: 693,
        severity: 2.5,
        matches: |l, _r| l.contains("unsafe-inline") || l.contains("unsafe-eval"),
    },
    // CWE-732 — world-writable / over-permissive file mode.
    ConfigRule {
        name: "world_writable_permissions",
        cwe_id: 732,
        severity: 2.5,
        matches: |l, _r| {
            l.contains("0o777")
                || l.contains("0o666")
                || l.contains("chmod 777")
                || l.contains("chmod(0o777")
                || l.contains("set_mode(0o777")
                || l.contains("from_mode(0o777")
        },
    },
    // CWE-250 — container/pod running privileged or as root (W2 2026-07-02).
    // High signal in k8s manifests / Dockerfiles now that F2.6 resolves them.
    ConfigRule {
        name: "container_privileged_or_root",
        cwe_id: 250,
        severity: 5.0,
        matches: |l, _r| {
            l.contains("privileged: true")
                || l.contains("privileged:true")
                || l.contains("runasuser: 0")
                || l.contains("run_as_user: 0")
                || l.contains("allowprivilegeescalation: true")
                || l.trim() == "user root"
                || l.trim_start().starts_with("user root")
        },
    },
    // CWE-798 — hardcoded / default credentials in a config file. Precision-tuned
    // to well-known placeholder secrets to avoid overlap with F2.4 (which owns
    // high-entropy real secrets) and false hits on prose (W2 2026-07-02).
    ConfigRule {
        name: "default_or_weak_credentials",
        cwe_id: 798,
        severity: 4.0,
        matches: |l, _r| {
            let has_cred_key = l.contains("password")
                || l.contains("passwd")
                || l.contains("secret_key")
                || l.contains("secret-key")
                || l.contains("api_key")
                || l.contains("api-key");
            has_cred_key
                && (l.contains("changeme")
                    || l.contains("change_me")
                    || l.contains("password123")
                    || l.contains(": admin")
                    || l.contains(":admin")
                    || l.contains("\"admin\"")
                    || l.contains("'admin'")
                    || l.contains(": password")
                    || l.contains("=password")
                    || l.contains("= \"password\""))
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze_rust(src: &str) -> ConfigReport {
        ConfigSecurityAnalyzer::new().analyze(src, "rust")
    }

    #[test]
    fn clean_source_scores_one() {
        let r = analyze_rust("fn handler() -> Result<(), ()> { Ok(()) }\n");
        assert!(r.misconfigs.is_empty());
        assert!((r.score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn empty_source_is_clean() {
        let r = analyze_rust("");
        assert!((r.score - 1.0).abs() < 1e-6);
    }

    // ── True positives (must fail / flag) ────────────────────────────────
    #[test]
    fn tls_disabled_is_critical_block() {
        let r = analyze_rust("let c = builder.danger_accept_invalid_certs(true).build();");
        assert_eq!(r.misconfigs.len(), 1, "{:?}", r.misconfigs);
        assert_eq!(r.misconfigs[0].cwe_id, 295);
        assert!(r.score < 0.5, "critical TLS-off must BLOCK: {}", r.score);
    }

    #[test]
    fn requests_verify_false_flagged() {
        let r = ConfigSecurityAnalyzer::new()
            .analyze("resp = requests.get(url, verify=False)\n", "python");
        assert!(r.misconfigs.iter().any(|m| m.cwe_id == 295));
    }

    // ── W2 (2026-07-02): YAML / container config-file rules ──────────────────
    #[test]
    fn yaml_tls_disabled_flagged() {
        let r = ConfigSecurityAnalyzer::new().analyze("server:\n  tls_enabled: false\n", "yaml");
        assert!(
            r.misconfigs.iter().any(|m| m.cwe_id == 295),
            "YAML tls_enabled:false must flag CWE-295: {:?}",
            r.misconfigs
        );
    }

    #[test]
    fn yaml_cors_wildcard_flagged() {
        let r =
            ConfigSecurityAnalyzer::new().analyze("cors:\n  allowed_origins: [\"*\"]\n", "yaml");
        assert!(r.misconfigs.iter().any(|m| m.cwe_id == 942));
    }

    #[test]
    fn k8s_privileged_container_flagged() {
        let r =
            ConfigSecurityAnalyzer::new().analyze("securityContext:\n  privileged: true\n", "yaml");
        assert!(r.misconfigs.iter().any(|m| m.cwe_id == 250));
    }

    #[test]
    fn default_credentials_flagged() {
        let r = ConfigSecurityAnalyzer::new().analyze("db:\n  password: changeme\n", "yaml");
        assert!(r.misconfigs.iter().any(|m| m.cwe_id == 798));
    }

    #[test]
    fn benign_config_key_not_flagged() {
        // A cred key with a real (non-placeholder) value must NOT trip the
        // default-credentials rule (F2.4 owns real high-entropy secrets).
        let r = ConfigSecurityAnalyzer::new()
            .analyze("db:\n  password_hash: a9f3e1c7b2d84f60\n", "yaml");
        assert!(
            !r.misconfigs.iter().any(|m| m.cwe_id == 798),
            "non-placeholder credential must not trip the default-creds rule"
        );
    }

    #[test]
    fn cors_wildcard_flagged() {
        let r = analyze_rust("headers.insert(\"Access-Control-Allow-Origin\", \"*\");");
        assert!(r.misconfigs.iter().any(|m| m.cwe_id == 942));
    }

    #[test]
    fn flask_debug_flagged() {
        let r = ConfigSecurityAnalyzer::new().analyze("app.run(debug=True)\n", "python");
        assert!(r.misconfigs.iter().any(|m| m.cwe_id == 489));
    }

    #[test]
    fn insecure_cookie_flagged() {
        let r =
            analyze_rust("let cookie = Cookie::build(\"k\", v).http_only(false).secure(false);");
        assert!(r.misconfigs.iter().any(|m| m.cwe_id == 614));
    }

    #[test]
    fn js_reject_unauthorized_flagged() {
        let r = ConfigSecurityAnalyzer::new().analyze(
            "const agent = new https.Agent({ rejectUnauthorized: false });",
            "javascript",
        );
        assert!(r.misconfigs.iter().any(|m| m.cwe_id == 295));
    }

    // ── False-positive controls (must NOT flag) ──────────────────────────
    #[test]
    fn cfg_debug_assertions_not_flagged() {
        // `debug_assertions` is a legitimate cfg, not an app-debug setting.
        let r = analyze_rust("if cfg!(debug_assertions) { log(); }");
        assert!(r.misconfigs.is_empty(), "{:?}", r.misconfigs);
    }

    #[test]
    fn cargo_profile_debug_not_flagged() {
        // Cargo's `[profile] debug = true` is debuginfo, not an app-debug flag.
        let r = ConfigSecurityAnalyzer::new().analyze("debug = true\n", "toml");
        assert!(r.misconfigs.is_empty(), "{:?}", r.misconfigs);
    }

    #[test]
    fn commented_misconfig_suppressed() {
        // A misconfig token inside a comment is doc, not a deployed setting.
        let r =
            analyze_rust("// builder.danger_accept_invalid_certs(true) is forbidden\nfn f() {}");
        assert!(r.misconfigs.is_empty(), "{:?}", r.misconfigs);
    }

    #[test]
    fn cfg_test_corpus_suppressed() {
        // Misconfig literal in a `#[cfg(test)]` module is a fixture, not a sink.
        let src = "fn prod() {}\n#[cfg(test)]\nmod tests {\n    fn t() { let _ = c.danger_accept_invalid_certs(true); }\n}\n";
        let r = analyze_rust(src);
        assert!(
            r.misconfigs.is_empty(),
            "test-corpus misconfig must be suppressed: {:?}",
            r.misconfigs
        );
    }

    #[test]
    fn score_clamps_to_zero_on_many() {
        let src = "danger_accept_invalid_certs(true)\nrejectUnauthorized: false\ninsecureskipverify = true\n";
        let r = analyze_rust(src);
        assert!((0.0..=1.0).contains(&r.score));
        assert!(r.score < 0.5);
    }
}
