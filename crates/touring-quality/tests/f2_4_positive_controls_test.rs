//! Positive controls for the F2.4 relaxations and sentinel reviewed in task 4b.
// touring-quality:allow-attack-fixture
// touring-quality:allow-secrets

use touring_quality::verifications::f2_4_secrets::scan_text;

fn assert_detected(cases: &[&str]) {
    let missed: Vec<_> = cases
        .iter()
        .filter(|line| !scan_text(&format!("{line}\n")).strong)
        .collect();
    assert!(missed.is_empty(), "F2.4 missed {missed:#?}");
}

/// 1305ff40: all five false-positive classes retain dangerous neighbors.
#[test]
fn commit_1305ff40_five_classes_neighbors() {
    assert_detected(&[
        // Comparison vs assignment.
        r##"if api_key != "none": api_key = "Tr0ub4dor&3xK9""##,
        r##"if token <= "x": password = "Tr0ub4dor&3xK9""##,
        r##"password := "Tr0ub4dor&3xK9""##,
        // A secret parameter's own default, plus destructuring.
        r##"def __init__(self, api_key: str = "Tr0ub4dor&3xK9", model_name: str = "gpt-4"):"##,
        r##"def __init__(self, model_name: str = "gpt-4", *, password="Tr0ub4dor&3xK9"):"##,
        r##"let (other, password) = ("x", "Tr0ub4dor&3xK9");"##,
        // Placeholder vocabulary does not launder opaque tokens or long numbers.
        r##"apiKey: "test-4eC39HqLyjWDarjtT1zdp7dc","##,
        r##"apiKey: "your-key-Xk9mQ7zR2wY5","##,
        r##"apiKey: "test-api-key-12345678","##,
        // Numeric ordinals are exempt only for token, and only up to four digits.
        r##""password": "1009","##,
        r##""token": "10091","##,
        // A sibling secret binding still belongs to its own name.
        r##"cfg = {"note": "ok", "api_key": "Tr0ub4dor&3xK9"}"##,
        // Message-holder prose does not exempt a credential-shaped value.
        r##"errors.password = "Tr0ub4dor &3 xK9";"##,
        // Readable fields do not exempt an opaque query/path segment.
        r##"u = "page=1&limit=50&sess=aB3xK9mQ7zR2wY5tE8uI1oP4xC2vB""##,
        r##"id = "docs_v2_aB3xK9mQ7zR2wY5tE8uI1oP4xC2vB_final""##,
        // The assigned-name fix keeps a following real credential binding.
        r##"creds = {"x-token": "Tr0ub4dor&3xK9"}"##,
    ]);
}

/// 059b2026: placeholder vocabulary must match whole segments, not substrings.
#[test]
fn commit_059b2026_placeholder_segment_neighbors() {
    assert_detected(&[
        r##"password = "protest-heaven-sample-castle-12""##,
        r##"password = "test-orange-violin-harbor-7""##,
        r##"apiKey: "test-4eC39HqLyjWDarjtT1zdp7dc","##,
    ]);
}

/// FURO 059b2026: the letters-only fallback still accepts placeholder substrings.
#[test]
#[ignore = "FURO 4b/059b2026: placeholder substring without a separator is not detected"]
fn furo_059b2026_placeholder_substring_without_separator() {
    assert_detected(&[
        r##"password = "protest""##,
        r##"api_key = "latest""##,
        r##"password = "protestHeaven""##,
    ]);
}

/// 44869807: Go field tags are metadata, but assignments and embedded secrets are not.
#[test]
fn commit_44869807_go_struct_tag_neighbors() {
    assert_detected(&[
        r##"password := "Tr0ub4dor&3xK9""##,
        r##"cfg.Password = "Tr0ub4dor&3xK9""##,
        r##"Token string `default:"ghp_0123456789abcdef0123456789abcdef0123"`"##,
        r##"APIKey string `default:"aB3xK9mQ7zR2wY5tE8uI1oP4xC2vB"`"##,
        r##"Password string `default:"Tr0ub4dor&3xK9"`"##,
    ]);
}

/// FURO 44869807: a short credential disguised as the JSON wire key is exempt.
#[test]
#[ignore = "FURO 4b/44869807: short credential in a Go json tag is not detected"]
fn furo_44869807_short_credential_in_json_tag() {
    assert_detected(&[r##"Password string `json:"Tr0ub4dor&3xK9"`"##]);
}

/// 910074b6: removing the sentinel's suffix must not disable secret detection.
#[test]
fn commit_910074b6_entropy_sentinel_neighbors() {
    assert_detected(&[
        r##"name = "../../shared/foo.vgpAusente0c2dcab442d3.png?v=2#x""##,
        r##"api_key = "vgpAusente0c2dcab442d3""##,
        r##"password = "vgpAusenteA9b8C7d6E5f4Q2r8""##,
    ]);
}
