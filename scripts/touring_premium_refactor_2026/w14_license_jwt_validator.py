#!/usr/bin/env python3
"""W14 — Validate JWT ed25519 license flow (offline + expired + corrupted).

Premium/Enterprise tiers gate on JWT ed25519-signed license keys:
  - 30-day offline grace period
  - Signature verification via embedded public key
  - Tier claim must match binary tier (no cross-tier upgrade via license swap)

This validator EMITS test cases (sample JWTs) and a Rust integration test
skeleton — does NOT verify (that's the consumer code's job in W14.4-W14.7).

Outputs:
  - data/w14-license-test-cases.json
  - staging/w14-license-tests/<case>.jwt           (sample tokens)
  - staging/w14-license-tests/integration_test.rs   (Rust skeleton)
"""
from __future__ import annotations

import base64
import json
from datetime import UTC, datetime, timedelta
from pathlib import Path

import script_common as sc

_SCRIPT = "w14_license_jwt_validator"

_ROOT, _DATA_DIR, _STAGING_DIR = sc.ROOT, sc.DATA_DIR, sc.STAGING_DIR
_TESTS_DIR = _STAGING_DIR / "w14-license-tests"


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    return sc.with_output_dir_and_verbose(p, _DATA_DIR)


def b64url(data: bytes) -> str:
    """URL-safe base64 without padding."""
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode("ascii")


def make_jwt_payload(claims: dict) -> str:
    """Render unsigned JWT (header + payload)."""
    header = {"alg": "EdDSA", "typ": "JWT"}
    h64 = b64url(json.dumps(header, separators=(",", ":")).encode())
    p64 = b64url(json.dumps(claims, separators=(",", ":"),
                              sort_keys=True).encode())
    # Signature placeholder (real signing in Rust integration test)
    sig = b64url(b"PLACEHOLDER_SIGNATURE_REAL_RUST_TEST")
    return f"{h64}.{p64}.{sig}"


def emit_test_cases() -> list[dict]:
    """Generate canonical test cases for license validation."""
    now = datetime.now(UTC)
    cases = [
        {
            "name": "valid_premium",
            "expected_outcome": "ACCEPT",
            "claims": {
                "iss": "touring.dev",
                "sub": "user-uuid-premium-001",
                "tier": "premium",
                "iat": int(now.timestamp()),
                "exp": int((now + timedelta(days=365)).timestamp()),
                "nbf": int(now.timestamp()),
            },
        },
        {
            "name": "expired_premium",
            "expected_outcome": "REJECT_EXPIRED",
            "claims": {
                "iss": "touring.dev",
                "sub": "user-uuid-premium-002",
                "tier": "premium",
                "iat": int((now - timedelta(days=400)).timestamp()),
                "exp": int((now - timedelta(days=35)).timestamp()),
                "nbf": int((now - timedelta(days=400)).timestamp()),
            },
        },
        {
            "name": "expired_within_grace",
            "expected_outcome": "ACCEPT_GRACE",
            "claims": {
                "iss": "touring.dev",
                "sub": "user-uuid-premium-003",
                "tier": "premium",
                "iat": int((now - timedelta(days=400)).timestamp()),
                "exp": int((now - timedelta(days=10)).timestamp()),
                "nbf": int((now - timedelta(days=400)).timestamp()),
            },
        },
        {
            "name": "tier_mismatch",
            "expected_outcome": "REJECT_TIER_MISMATCH",
            "claims": {
                "iss": "touring.dev",
                "sub": "user-uuid-tier-mismatch",
                "tier": "free",  # binary built as premium
                "iat": int(now.timestamp()),
                "exp": int((now + timedelta(days=365)).timestamp()),
            },
        },
        {
            "name": "not_yet_valid",
            "expected_outcome": "REJECT_NOT_YET_VALID",
            "claims": {
                "iss": "touring.dev",
                "sub": "future-user",
                "tier": "premium",
                "iat": int((now + timedelta(days=30)).timestamp()),
                "nbf": int((now + timedelta(days=30)).timestamp()),
                "exp": int((now + timedelta(days=395)).timestamp()),
            },
        },
        {
            "name": "wrong_issuer",
            "expected_outcome": "REJECT_ISSUER",
            "claims": {
                "iss": "attacker.example",
                "sub": "spoofed",
                "tier": "enterprise",
                "iat": int(now.timestamp()),
                "exp": int((now + timedelta(days=365)).timestamp()),
            },
        },
    ]
    for c in cases:
        c["jwt"] = make_jwt_payload(c["claims"])
    return cases


def emit_rust_skeleton(cases: list[dict]) -> Path:
    """Emit Rust integration_test.rs skeleton."""
    _TESTS_DIR.mkdir(parents=True, exist_ok=True)
    p = _TESTS_DIR / "integration_test.rs"
    lines = [
        '//! AUTO-GENERATED — W14 JWT license validation integration tests.',
        '//!',
        '//! Each test case corresponds to data/w14-license-test-cases.json.',
        '//! Signature placeholders MUST be replaced with real ed25519 sigs',
        '//! signed by the test private key during W14.4-W14.7 implementation.',
        "",
        "use touring_bindings::license::{LicenseError, validate_license};",
        "",
    ]
    for c in cases:
        outcome = c["expected_outcome"]
        lines.extend([
            "#[test]",
            f"fn test_{c['name']}() {{",
            f'    let jwt = include_str!("./{c["name"]}.jwt");',
            "    let result = validate_license(jwt, \"premium\");",
            f"    // Expected outcome: {outcome}",
            "    // TODO: assert!(matches!(result, ...));",
            "}",
            "",
        ])
    p.write_text("\n".join(lines), encoding="utf-8")
    return p


def run(args: sc.Namespace) -> dict:
    """Emit test cases + skeleton."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _TESTS_DIR.mkdir(parents=True, exist_ok=True)
    cases = emit_test_cases()
    # Emit each JWT to its own file
    jwt_paths: list[str] = []
    for c in cases:
        jwt_file = _TESTS_DIR / f"{c['name']}.jwt"
        jwt_file.write_text(c["jwt"], encoding="utf-8")
        jwt_paths.append(str(jwt_file.relative_to(_ROOT)))
    json_path = args.output_dir / "w14-license-test-cases.json"
    # Strip JWT field from saved JSON (only metadata)
    cases_json = [{k: v for k, v in c.items() if k != "jwt"} for c in cases]
    json_path.write_text(json.dumps(cases_json, indent=2, ensure_ascii=False),
                          encoding="utf-8")
    rust_skeleton = emit_rust_skeleton(cases)
    return {
        "script": "w14_license_jwt_validator",
        "wave": "W14", "subtask_refs": ["W14.license"],
        "timestamp": sc.now_iso(),
        "status": "OK",
        "totals": {
            "test_cases": len(cases),
            "jwt_files_emitted": len(jwt_paths),
        },
        "test_case_names": [c["name"] for c in cases],
        "jwt_paths": jwt_paths,
        "rust_skeleton": str(rust_skeleton.relative_to(_ROOT)),
        "json_path": str(json_path.relative_to(_ROOT)),
    }


def _run_and_report(args: sc.Namespace) -> int:
    """Run w14_license_jwt_validator, print its summary; the return value is the exit code."""
    result = run(args)
    sc.emit_json({
        "status": result["status"], "totals": result["totals"],
        "test_case_names": result["test_case_names"],
        "rust_skeleton": result["rust_skeleton"],
        "json_path": result["json_path"],
    })
    return 0


def main() -> int:
    """Entry point of w14_license_jwt_validator."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
