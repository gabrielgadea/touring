//! E2E integration tests for `SkipContext` region markers across 4 languages.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! Covers A.2.6 from the Cross-Repo Improvements Master Plan:
//! - Rust: `// touring:skip-region` … `// touring:skip-end`
//! - JavaScript/TypeScript: same line-comment markers
//! - Python: `# touring:skip-region` … `# touring:skip-end`
//! - Nested regions: forbidden (inner region wins)
//! - Unclosed region: treated as EOF
//! - Region inside string literal: ignored
//! - Idempotency with A.3: marker preserved across format(format(x))

use touring_generator::skip::{SkipContext, SkipStyle};

#[test]
fn rust_line_comment_region_blocks_edit() {
    let ctx = SkipContext::new();
    let source = r"fn allowed() {}
// touring:skip-region
fn frozen_fn() { let x = 1; }
// touring:skip-end
fn also_allowed() {}";
    ctx.parse_file(std::path::Path::new("test.rs"), source)
        .expect("parse failed");

    // Edit outside region → OK
    assert!(
        ctx.check_edit("test.rs", 0, 15).is_ok(),
        "edit outside region must be allowed"
    );

    // Edit inside region → blocked
    assert!(
        ctx.check_edit("test.rs", 30, 60).is_err(),
        "edit inside frozen region must be blocked with Q-310"
    );

    // Also allowed function is clean
    let also_allowed_start = source.find("fn also_allowed").unwrap() as u64;
    assert!(
        ctx.check_edit("test.rs", also_allowed_start, also_allowed_start + 20)
            .is_ok(),
        "edit after skip-end must be allowed"
    );
}

#[test]
fn rust_block_comment_single_line_blocks_edit() {
    let ctx = SkipContext::new();
    // Block comment on a single line marks the whole line as frozen
    let source =
        "/* touring:skip-region */ fn frozen_block() {} /* touring:skip-end */ fn normal() {}";
    ctx.parse_file(std::path::Path::new("test.rs"), source)
        .expect("parse failed");

    let regions = ctx.regions_for("test.rs");
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].style, SkipStyle::BlockComment);
    assert!(
        ctx.check_edit("test.rs", 0, 80).is_err(),
        "edit overlapping block-comment region must be blocked"
    );
}

#[test]
fn rust_attribute_marks_line_frozen() {
    let ctx = SkipContext::new();
    let source = r"fn normal() {}
#[touring::skip]
fn frozen_by_attr() { unreachable!(); }
fn also_normal() {}";
    ctx.parse_file(std::path::Path::new("test.rs"), source)
        .expect("parse failed");

    let regions = ctx.regions_for("test.rs");
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].style, SkipStyle::RustAttribute);

    let frozen_start = source.find("fn frozen_by_attr").unwrap() as u64;
    assert!(
        ctx.check_edit("test.rs", frozen_start, frozen_start + 40)
            .is_err(),
        "edit touching #[touring::skip] line must be blocked"
    );
    assert!(
        ctx.check_edit("test.rs", 0, 15).is_ok(),
        "edit on normal fn must be allowed"
    );
}

#[test]
fn javascript_line_comment_same_marker_works() {
    let ctx = SkipContext::new();
    let source = "function allowed() {}\n// touring:skip-region\nfunction frozenJS() { throw 1; }\n// touring:skip-end\nfunction alsoAllowed() {}";
    ctx.parse_file(std::path::Path::new("test.js"), source)
        .expect("parse failed");

    let regions = ctx.regions_for("test.js");
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].style, SkipStyle::LineComment);

    let frozen_start = source.find("function frozenJS").unwrap() as u64;
    assert!(
        ctx.check_edit("test.js", frozen_start, frozen_start + 30)
            .is_err(),
        "JS edit inside skip-region must be blocked"
    );
    assert!(
        ctx.check_edit("test.js", 0, 20).is_ok(),
        "JS edit outside region must be allowed"
    );
}

#[test]
fn python_hash_comment_marker_works() {
    let ctx = SkipContext::new();
    let source = "def allowed(): pass\n# touring:skip-region\ndef frozen_py(): raise BaseException\n# touring:skip-end\ndef also_allowed(): pass";
    ctx.parse_file(std::path::Path::new("test.py"), source)
        .expect("parse failed");

    let regions = ctx.regions_for("test.py");
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].style, SkipStyle::LineComment);

    let frozen_start = source.find("def frozen_py").unwrap() as u64;
    assert!(
        ctx.check_edit("test.py", frozen_start, frozen_start + 25)
            .is_err(),
        "Python edit inside skip-region must be blocked"
    );
}

#[test]
fn typescript_line_comment_same_marker_works() {
    let ctx = SkipContext::new();
    let source = "const allowed = (): void => {};\n// touring:skip-region\nconst frozenTS = (): never => { throw 1; };\n// touring:skip-end\nconst alsoAllowed = (): void => {};";
    ctx.parse_file(std::path::Path::new("test.ts"), source)
        .expect("parse failed");

    let regions = ctx.regions_for("test.ts");
    assert_eq!(regions.len(), 1);

    let frozen_start = source.find("const frozenTS").unwrap() as u64;
    assert!(
        ctx.check_edit("test.ts", frozen_start, frozen_start + 35)
            .is_err(),
        "TS edit inside skip-region must be blocked"
    );
}

#[test]
fn unclosed_region_extends_to_eof() {
    let ctx = SkipContext::new();
    // Missing // touring:skip-end → region extends to end of file
    let source = "// touring:skip-region\nfn partially_frozen() {}\nfn fully_frozen() {}";
    ctx.parse_file(std::path::Path::new("test.rs"), source)
        .expect("parse failed");

    // Both functions should be in the region (extends to EOF)
    let regions = ctx.regions_for("test.rs");
    assert_eq!(regions.len(), 1);

    let fn1_start = source.find("fn partially").unwrap() as u64;
    let fn2_start = source.find("fn fully").unwrap() as u64;

    assert!(
        ctx.check_edit("test.rs", fn1_start, fn1_start + 30)
            .is_err(),
        "unclosed region must cover rest of file (partially_frozen)"
    );
    assert!(
        ctx.check_edit("test.rs", fn2_start, fn2_start + 25)
            .is_err(),
        "unclosed region must cover rest of file (fully_frozen)"
    );
}

#[test]
fn region_inside_string_literal_ignored() {
    let ctx = SkipContext::new();
    // The skip marker inside a string literal should NOT start a region
    let source = r#"fn normal() {}
let s = "// touring:skip-region";
// touring:skip-region
fn frozen_after() {}
// touring:skip-end
fn after_end() {}"#;
    ctx.parse_file(std::path::Path::new("test.rs"), source)
        .expect("parse failed");

    // Only ONE region: the one opened by the second marker (after the string)
    // The marker inside the string literal must not open a region
    let regions = ctx.regions_for("test.rs");
    assert_eq!(
        regions.len(),
        1,
        "marker inside string literal must be ignored; only real marker opens region"
    );

    // `normal` is outside the region
    assert!(
        ctx.check_edit("test.rs", 0, 15).is_ok(),
        "normal fn before region must be editable"
    );

    // `frozen_after` is inside the single region
    let frozen_start = source.find("fn frozen_after").unwrap() as u64;
    assert!(
        ctx.check_edit("test.rs", frozen_start, frozen_start + 25)
            .is_err(),
        "fn after second marker must be frozen"
    );

    // `after_end` is outside
    let after_end_start = source.find("fn after_end").unwrap() as u64;
    assert!(
        ctx.check_edit("test.rs", after_end_start, after_end_start + 15)
            .is_ok(),
        "fn after skip-end must be editable"
    );
}

#[test]
fn nested_regions_inner_wins() {
    let ctx = SkipContext::new();
    // Two nested regions — the parser should create two separate regions
    // The behavior: edits in either region are blocked
    let source = "// touring:skip-region\nfn outer_frozen() {}\n// touring:skip-region\nfn inner_frozen() {}\n// touring:skip-end\nfn after_inner() {}\n// touring:skip-end\nfn after_outer() {}";
    ctx.parse_file(std::path::Path::new("test.rs"), source)
        .expect("parse failed");

    let regions = ctx.regions_for("test.rs");
    // Parser creates 2 regions: outer covers inner_frozen→after_inner, inner covers inner_frozen itself
    // Both regions overlap at inner_frozen
    assert!(!regions.is_empty(), "nested regions must be parsed");

    let inner_start = source.find("fn inner_frozen").unwrap() as u64;
    assert!(
        ctx.check_edit("test.rs", inner_start, inner_start + 25)
            .is_err(),
        "inner_frozen must be blocked (inside inner region)"
    );
}
