use super::*;

// ── SEC-02: web bind loopback-default + CORS localhost-only ──────────

#[test]
fn web_bind_defaults_to_loopback() {
    let addr = resolve_web_bind(None);
    assert!(addr.ip().is_loopback(), "default bind must be loopback");
    assert_eq!(addr.port(), 3000);
}

#[test]
fn web_bind_honours_valid_override() {
    let addr = resolve_web_bind(Some("0.0.0.0:8080".to_string()));
    assert_eq!(addr.to_string(), "0.0.0.0:8080");
}

#[test]
fn web_bind_invalid_override_falls_back_to_loopback() {
    let addr = resolve_web_bind(Some("not-an-addr".to_string()));
    assert!(
        addr.ip().is_loopback(),
        "invalid bind must fall back to loopback, got {addr}"
    );
}

#[test]
fn cors_allows_localhost_rejects_foreign_and_prefix_injection() {
    assert!(is_localhost_origin(b"http://localhost:3000"));
    assert!(is_localhost_origin(b"http://127.0.0.1:5173"));
    assert!(is_localhost_origin(b"http://[::1]:3000"));
    assert!(is_localhost_origin(b"http://localhost"));
    // Cross-origin attacker must be rejected:
    assert!(!is_localhost_origin(b"http://evil.example.com"));
    // Prefix-injection guard — these must NOT match:
    assert!(!is_localhost_origin(b"http://localhost.evil.com"));
    assert!(!is_localhost_origin(b"http://127.0.0.1.evil.com"));
}

// ── Elite W4: MCP whitelist security gate (SPEC §6.1) ──────────

#[test]
fn mcp_whitelist_rejects_unknown_tools() {
    assert!(resolve_tool_argv("rm_rf", None).is_err());
    assert!(resolve_tool_argv("touring_exec", Some("ls")).is_err());
    assert!(resolve_tool_argv("", None).is_err());
}

#[test]
fn mcp_whitelist_rejects_flag_and_shell_injection() {
    // Leading dash → flag injection.
    assert!(resolve_tool_argv("touring_index_find", Some("--help")).is_err());
    // Shell metacharacters are outside the charset.
    for bad in [
        "a;rm -rf /",
        "$(whoami)",
        "a|b",
        "a&&b",
        "a`b`",
        "a\nb",
        "a\"b",
    ] {
        assert!(
            resolve_tool_argv("touring_index_find", Some(bad)).is_err(),
            "must reject {bad:?}"
        );
    }
    // Oversized arg.
    assert!(resolve_tool_argv("touring_index_find", Some(&"x".repeat(201))).is_err());
}

#[test]
fn mcp_whitelist_resolves_fixed_and_templated_tools() {
    let fixed = resolve_tool_argv("touring_doctor", None).expect("fixed tool");
    assert_eq!(fixed, vec!["doctor", "-j"]);
    let templated =
        resolve_tool_argv("touring_wiring_impact", Some("EliteShell")).expect("templated");
    assert_eq!(
        templated,
        vec![
            "wiring",
            "impact",
            "EliteShell",
            "--depth",
            "3",
            "--format",
            "json"
        ]
    );
    // Templated tool without its arg is refused.
    assert!(resolve_tool_argv("touring_wiring_impact", None).is_err());
}

#[test]
fn mcp_whitelist_is_read_only_surface() {
    // No template may reach mutating subcommands.
    for (name, _, template) in MCP_TOOL_WHITELIST {
        for forbidden in [
            "store", "rebuild", "spawn", "drop", "write", "reward", "exec",
        ] {
            assert!(
                !template.contains(&forbidden),
                "{name} must not invoke `{forbidden}`"
            );
        }
    }
}

/// F-06 (cross-audit 2026-06-11): each error kind maps to a distinct,
/// meaningful HTTP status — not a flat 500.
#[test]
fn app_error_maps_variants_to_status() {
    assert_eq!(
        AppError::TouringCommand("boom".into()).status_code(),
        StatusCode::BAD_GATEWAY
    );
    assert_eq!(
        AppError::TouringParse.status_code(),
        StatusCode::BAD_GATEWAY
    );
    assert_eq!(
        AppError::DotProcess("dot died".into()).status_code(),
        StatusCode::BAD_GATEWAY
    );
    assert_eq!(
        AppError::FileNotFound("x.js".into()).status_code(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        AppError::Io(std::io::Error::other("io")).status_code(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

/// F-06: the rendered response carries a JSON `{"error": ...}` body
/// with `application/json` content-type.
#[test]
fn app_error_renders_json_body() {
    let resp = AppError::FileNotFound("missing.wasm".into()).into_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let ct = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert_eq!(ct, "application/json");
}

/// F-08 (cross-audit 2026-06-11): `name:/path` pairs, bare `/path`
/// entries, blanks and trailing `name:` specs all parse safely.
#[test]
fn federation_roots_parse_named_bare_and_degenerate_specs() {
    assert_eq!(
        parse_federation_roots("ws1:/a,ws2:/b"),
        vec!["/a".to_string(), "/b".to_string()]
    );
    // Bare path (no name) was silently dropped by the old parser.
    assert_eq!(
        parse_federation_roots("/just/a/path"),
        vec!["/just/a/path".to_string()]
    );
    // Mixed + whitespace.
    assert_eq!(
        parse_federation_roots(" ws1:/a , /b "),
        vec!["/a".to_string(), "/b".to_string()]
    );
    // Trailing `name:` (empty root) and empty segments are filtered.
    assert_eq!(parse_federation_roots("ws1:,,"), Vec::<String>::new());
    assert_eq!(parse_federation_roots(""), Vec::<String>::new());
}
