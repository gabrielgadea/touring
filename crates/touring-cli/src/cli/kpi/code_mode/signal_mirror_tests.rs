//! Characterization of the two KPI readings of the signal mirror
//! (`code_mode_signal_use`, `hooks_complement`).
//!
//! C5 (12/11/2026, aprofundamento do touring-code): the mirror's line format and path
//! moved behind `touring_code::sdk_signal_mirror::Mirror`. The GOLDEN strings below were
//! computed by the code BEFORE that move, on these very fixtures, and the move must not
//! change a byte of them: only the two helpers at the top know how the reading is reached.

use std::collections::BTreeMap;
use std::path::Path;

use super::*;

/// The KPI JSON for the mirror at `path`, serialized.
fn signal_use_json(path: &Path) -> String {
    serde_json::to_string(&code_mode_signal_use_at(&Mirror::at(path))).expect("serializes")
}

/// The complement JSON over a mirror holding `lines`, read at the `since` window, serialized.
fn complement_json(by_name: &BTreeMap<String, u64>, lines: &[&str], since: Option<u64>) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let mirror = mirror_holding(dir.path(), lines);
    serde_json::to_string(&hooks_complement_from(by_name, Some(&mirror), since))
        .expect("serializes")
}

/// What the writers produce — the canonical, legacy (no origin), alias and non-canonical
/// shapes of the real mirror, with a blank line in the middle.
const CLEAN: [&str; 8] = [
    r#"{"ts":1788299999,"hook_name":"index_find","duration_ms":4,"success":true,"origin":"post_bash"}"#,
    r#"{"ts":1788300010,"hook_name":"index_find","duration_ms":8,"success":true,"origin":"post_bash"}"#,
    r#"{"ts":1788300011,"hook_name":"ast_meta","duration_ms":0,"success":false,"origin":"sdk"}"#,
    "",
    r#"{"ts":1788300020,"hook_name":"memory_recall","duration_ms":42,"success":true}"#,
    r#"{"ts":1788300030,"hook_name":"cli-index-find","duration_ms":0,"success":true,"origin":"post_bash"}"#,
    r#"{"ts":1788300031,"hook_name":"ast_tdg","duration_ms":3,"success":true,"origin":"sdk"}"#,
    r#"{"ts":1788300040,"hook_name":"parallel","duration_ms":100,"success":false,"origin":"sdk"}"#,
];

/// Garbage, a line with no hook name, a line before the window, an origin nobody writes.
const NOISY: [&str; 4] = [
    "not json at all",
    r#"{"ts":1788300050}"#,
    r#"{"ts":1788299000,"hook_name":"index_find"}"#,
    r#"{"ts":1788300060,"hook_name":"index_find","origin":"carrier-pigeon"}"#,
];

const SINCE: Option<u64> = Some(1_788_300_000);

fn mirror_file(dir: &Path, name: &str, lines: &[&str], eol: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    let body: String = lines.iter().map(|l| format!("{l}{eol}")).collect();
    std::fs::write(&path, body).expect("write fixture");
    path
}

fn dispatched() -> BTreeMap<String, u64> {
    let mut by_name = BTreeMap::new();
    by_name.insert("post-bash".to_string(), 4u64);
    by_name.insert("cli-suggest".to_string(), 30u64);
    by_name
}

const GOLDEN_SIGNAL_USE_CLEAN: &str = r#"{"available":true,"duration_ms_p50":4,"duration_ms_p99":100,"non_canonical_calls":2,"ratio":0.5,"total":8,"total_calls":7,"used":4}"#;
const GOLDEN_SIGNAL_USE_ABSENT: &str = r#"{"available":false,"reason":"no mirror yet"}"#;
const GOLDEN_SIGNAL_USE_EMPTY: &str = r#"{"available":true,"duration_ms_p50":0,"duration_ms_p99":0,"non_canonical_calls":0,"ratio":0.0,"total":8,"total_calls":0,"used":0}"#;
const GOLDEN_COMPLEMENT_CLEAN: &str = r#"{"available":true,"dispatched":{"cli-suggest":30,"post-bash":4},"mirror_deliveries_by_origin":{"post_bash":1,"sdk":2,"unknown":1},"mirror_deliveries_since":4,"mirror_non_canonical_since":2,"post_bash_delivery_ratio":0.25,"post_bash_dispatched":4,"post_bash_origin_deliveries_since":1,"since_epoch":1788300000}"#;
const GOLDEN_COMPLEMENT_NOISY: &str = r#"{"available":true,"dispatched":{"cli-suggest":30,"post-bash":4},"mirror_deliveries_by_origin":{"post_bash":0,"sdk":0,"unknown":1},"mirror_deliveries_since":1,"mirror_non_canonical_since":0,"post_bash_delivery_ratio":0.0,"post_bash_dispatched":4,"post_bash_origin_deliveries_since":0,"since_epoch":1788300000}"#;
const GOLDEN_COMPLEMENT_NO_DISPATCH: &str =
    r#"{"available":false,"reason":"no hook dispatched yet in this daemon"}"#;

#[test]
fn signal_use_over_a_writer_shaped_mirror_is_unchanged_byte_for_byte() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lf = mirror_file(dir.path(), "lf.jsonl", &CLEAN, "\n");
    let crlf = mirror_file(dir.path(), "crlf.jsonl", &CLEAN, "\r\n");
    assert_eq!(signal_use_json(&lf), GOLDEN_SIGNAL_USE_CLEAN);
    assert_eq!(signal_use_json(&crlf), GOLDEN_SIGNAL_USE_CLEAN);
}

#[test]
fn signal_use_without_a_mirror_or_with_an_empty_one_keeps_its_two_shapes() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert_eq!(
        signal_use_json(&dir.path().join("absent.jsonl")),
        GOLDEN_SIGNAL_USE_ABSENT
    );
    let empty = mirror_file(dir.path(), "empty.jsonl", &[], "\n");
    assert_eq!(signal_use_json(&empty), GOLDEN_SIGNAL_USE_EMPTY);
}

#[test]
fn hooks_complement_over_a_writer_shaped_mirror_is_unchanged_byte_for_byte() {
    assert_eq!(
        complement_json(&dispatched(), &CLEAN, SINCE),
        GOLDEN_COMPLEMENT_CLEAN
    );
    // Garbage and a line with no hook name are not deliveries; neither is anything
    // before `since`; an origin nobody writes is `unknown`, never `post_bash`.
    assert_eq!(
        complement_json(&dispatched(), &NOISY, SINCE),
        GOLDEN_COMPLEMENT_NOISY
    );
    assert_eq!(
        complement_json(&dispatched(), &CLEAN, None),
        GOLDEN_COMPLEMENT_NO_DISPATCH
    );
}

/// A mirror that EXISTS but `Mirror::read` refuses: a directory where the file should be, and a
/// file the process may not open (left out when it can anyway: root ignores the mode). Bytes that
/// are not UTF-8 are NOT here any more — the reader skips such a line
/// (`a_line_that_is_not_utf8_does_not_make_the_mirror_unreadable`).
fn unreadable_mirrors(dir: &Path) -> Vec<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let as_dir = dir.join("dir.jsonl");
    std::fs::create_dir(&as_dir).expect("mkdir");
    let mut found = vec![as_dir];
    let locked = dir.join("locked.jsonl");
    std::fs::write(&locked, b"{\"hook_name\":\"ast_meta\"}\n").expect("write");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    if std::fs::File::open(&locked).is_err() {
        found.push(locked);
    }
    found
}

/// F8 (cross-audit 12/11/2026): a mirror the KPI could not READ came back as `no mirror yet` —
/// the answer for a file nobody has written. A deployment that cannot read its own signal looked
/// like a deployment that had not started, and nothing said otherwise. The absent case keeps its
/// shape byte for byte (the golden above).
#[test]
fn an_unreadable_mirror_reads_unreadable_and_an_absent_one_keeps_no_mirror_yet() {
    let dir = tempfile::tempdir().expect("tempdir");
    for path in unreadable_mirrors(dir.path()) {
        let out = code_mode_signal_use_at(&Mirror::at(path.as_path()));
        assert_eq!(out["available"], json!(false), "{path:?}: {out}");
        let reason = out["reason"].as_str().unwrap_or_default();
        assert!(
            reason.starts_with("unreadable: ") && reason.len() > "unreadable: ".len(),
            "{path:?} must say why it could not be read: {out}"
        );
        assert_ne!(reason, "no mirror yet", "{path:?}");
    }
    assert_eq!(
        signal_use_json(&dir.path().join("absent.jsonl")),
        GOLDEN_SIGNAL_USE_ABSENT
    );
}

/// The sibling reading swallowed the same error (`.ok().flatten().unwrap_or_default()`): an
/// unreadable mirror counted as zero deliveries, and a `post_bash_delivery_ratio` of 0.0 pointed
/// at the feeder as the suspect. Unreadable is stated, and the ratio — unknown — is `null`.
#[test]
fn hooks_complement_over_an_unreadable_mirror_says_so_and_invents_no_zero_ratio() {
    let dir = tempfile::tempdir().expect("tempdir");
    for path in unreadable_mirrors(dir.path()) {
        let mirror = Mirror::at(path.as_path());
        let out = hooks_complement_from(&dispatched(), Some(&mirror), SINCE);
        assert!(
            out["mirror_unreadable"]
                .as_str()
                .is_some_and(|e| !e.is_empty()),
            "{path:?}: {out}"
        );
        assert_eq!(
            out["post_bash_delivery_ratio"],
            Value::Null,
            "{path:?}: {out}"
        );
        assert_eq!(
            out["mirror_deliveries_since"],
            Value::Null,
            "{path:?}: unknown, not 0: {out}"
        );
        assert!(out.get("mirror_absent").is_none(), "{path:?}: {out}");
    }
    // absent is another state, with its own key and the same null numbers (see the test below)
    let absent = Mirror::at(dir.path().join("absent.jsonl"));
    let out = hooks_complement_from(&dispatched(), Some(&absent), SINCE);
    assert!(out.get("mirror_unreadable").is_none(), "{out}");
}

/// RED→GREEN (C5): ONE torn line (a crash mid-append) made the KPI drop
/// `duration_ms_p50/p99` while the counts, read by a second, tolerant parser, went on.
#[test]
fn a_torn_line_does_not_hide_the_durations_from_the_kpi() {
    let dir = tempfile::tempdir().expect("tempdir");
    let torn = mirror_file(
        dir.path(),
        "torn.jsonl",
        &[
            r#"{"ts":1,"hook_name":"index_find","duration_ms":8,"success":true}"#,
            r#"{"ts":2,"hook_name":"ast_me"#,
            r#"{"ts":4,"hook_name":"memory_recall","duration_ms":40,"success":true}"#,
        ],
        "\n",
    );
    let v = code_mode_signal_use_at(&Mirror::at(torn));
    assert_eq!(v["total_calls"], 2, "{v}");
    assert_eq!(v["used"], 2, "{v}");
    assert_eq!(v["duration_ms_p50"], 40, "p50 over [8, 40]: {v}");
    assert_eq!(v["duration_ms_p99"], 40, "{v}");
    assert_eq!(
        v["skipped_lines"], 1,
        "the torn line is shown, not hidden: {v}"
    );
}

/// RED→GREEN (review 12/11/2026, C2): `Mirror::read` took the file as one `String`, so ONE line
/// with bytes that are not UTF-8 made the whole mirror unreadable — the KPI then said so
/// (`unreadable: …`) about a mirror whose other lines were fine. A bad line is skipped; the rest
/// is read.
#[test]
fn a_line_that_is_not_utf8_does_not_make_the_mirror_unreadable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("utf8.jsonl");
    let mut body = br#"{"ts":1,"hook_name":"index_find","duration_ms":8,"success":true}"#.to_vec();
    body.extend_from_slice(b"\n{\"ts\":2,\"hook_name\":\"ast_\xff\xfe\"}\n");
    body.extend_from_slice(
        br#"{"ts":4,"hook_name":"memory_recall","duration_ms":40,"success":true}"#,
    );
    body.push(b'\n');
    std::fs::write(&path, body).expect("write fixture");
    let v = code_mode_signal_use_at(&Mirror::at(path));
    assert_eq!(v["available"], true, "{v}");
    assert_eq!(
        v["total_calls"], 2,
        "the two readable lines are the calls: {v}"
    );
    assert_eq!(v["duration_ms_p50"], 40, "p50 over [8, 40]: {v}");
    assert_eq!(
        v["skipped_lines"], 1,
        "the line that is not UTF-8 is counted: {v}"
    );
}

/// RED→GREEN (review 12/11/2026, D4): `hooks_complement_from` dropped the `None` that
/// `Mirror::read` returns for "no file yet" and read it as an empty aggregate: zero deliveries and,
/// with post-bash dispatched, a `post_bash_delivery_ratio` of 0.0 — the same number as a feeder
/// that wrote nothing. An absent mirror is stated as absent and its numbers are `null`; the
/// daemon's own half (the dispatch counts) is real and stays.
#[test]
fn hooks_complement_over_an_absent_mirror_says_absent_and_invents_no_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    let no_file = Mirror::at(dir.path().join("absent.jsonl"));
    for mirror in [Some(&no_file), None] {
        let out = hooks_complement_from(&dispatched(), mirror, SINCE);
        assert_eq!(out["mirror_absent"], json!(true), "{out}");
        for key in [
            "post_bash_delivery_ratio",
            "mirror_deliveries_since",
            "mirror_deliveries_by_origin",
            "post_bash_origin_deliveries_since",
            "mirror_non_canonical_since",
        ] {
            assert_eq!(out[key], Value::Null, "{key} must be unknown, not 0: {out}");
        }
        assert_eq!(out["post_bash_dispatched"], 4, "{out}");
    }
}
