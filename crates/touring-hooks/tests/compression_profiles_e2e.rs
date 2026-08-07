//! NEW-1 — End-to-end coverage of all 30 compression profiles.
//!
//! Each test feeds a realistic fixture per profile and asserts:
//!   1. The compress() output has correct shape (key info preserved).
//!   2. The compression ratio is below the per-profile target (where applicable).
//!
//! Profiles must be PURE (no I/O, no globals) which lets these run in parallel.

#![cfg(feature = "tantivy-fts")]

use serde_json::json;
use touring_hooks::compression_profiles::{compress_for, registry};

// ─── Helpers ───────────────────────────────────────────────────────────────

fn ratio(out: &str, raw: &str) -> f64 {
    if raw.is_empty() {
        return 0.0;
    }
    out.len() as f64 / raw.len() as f64
}

fn bash(cmd: &str) -> serde_json::Value {
    json!({"command": cmd})
}

// ─── 30 profile audits — one per registered profile ────────────────────────

#[test]
fn audit_registry_count_30() {
    assert_eq!(registry().len(), 30, "must ship exactly 30 profiles");
}

#[test]
fn audit_profile_names_unique() {
    let mut names: Vec<&'static str> = registry().iter().map(|p| p.name()).collect();
    names.sort_unstable();
    let len_before = names.len();
    names.dedup();
    assert_eq!(
        names.len(),
        len_before,
        "no duplicate profile names allowed"
    );
}

// ─── 1. cargo_test ─────────────────────────────────────────────────────────
#[test]
fn audit_p01_cargo_test_keeps_failures() {
    let raw = "running 200 tests\ntest a ... ok\ntest b ... ok\ntest c ... FAILED\nfailures:\n    c\ntest result: FAILED. 1 failed; 199 passed\n";
    let out = compress_for("Bash", &bash("cargo test"), raw);
    assert!(out.contains("FAILED"), "must keep FAILED");
    assert!(out.contains("test result"), "must keep summary");
}

// ─── 2. cargo_clippy ───────────────────────────────────────────────────────
#[test]
fn audit_p02_cargo_clippy_keeps_warnings() {
    let raw = "warning: unused variable: `x`\n  --> src/foo.rs:10:5\n   = help: rename to `_x`\nfinished\n";
    let out = compress_for("Bash", &bash("cargo clippy"), raw);
    assert!(out.contains("warning:"));
    assert!(out.contains("--> "));
}

// ─── 3. cargo_build ────────────────────────────────────────────────────────
#[test]
fn audit_p03_cargo_build_keeps_errors() {
    let raw = "Compiling foo v0.1.0\nerror[E0382]: borrow of moved value\nFinished release\n";
    let out = compress_for("Bash", &bash("cargo build --release"), raw);
    assert!(out.contains("error[E0382]"));
    assert!(out.contains("Finished"));
}

// ─── 4. git_log ────────────────────────────────────────────────────────────
#[test]
fn audit_p04_git_log_collapses_to_one_line() {
    let raw = "commit abc123def456\nAuthor: gg\nDate: 2026-05-08\n\n    fix: foo\n\ncommit def4567890ab\nAuthor: gg\nDate: 2026-05-08\n\n    feat: bar\n";
    let out = compress_for("Bash", &bash("git log"), raw);
    assert!(!out.contains("Author:"));
    assert!(out.contains("fix: foo"));
    assert!(
        ratio(&out, raw) < 1.0,
        "git log compression must shrink the output"
    );
}

// ─── 5. git_diff ───────────────────────────────────────────────────────────
#[test]
fn audit_p05_git_diff_keeps_headers() {
    let raw = "diff --git a/foo.rs b/foo.rs\n--- a/foo.rs\n+++ b/foo.rs\n@@ -1,3 +1,5 @@\n line1\n line2\n+new\n line3\n";
    let out = compress_for("Bash", &bash("git diff HEAD~1"), raw);
    assert!(out.contains("diff --git"));
    assert!(out.contains("@@"));
}

// ─── 6. git_status ─────────────────────────────────────────────────────────
#[test]
fn audit_p06_git_status_compact() {
    let raw = "On branch main\nnothing to commit, working tree clean\n";
    let out = compress_for("Bash", &bash("git status"), raw);
    assert!(out.len() <= raw.len());
}

// ─── 7. gh_pr_list ─────────────────────────────────────────────────────────
#[test]
fn audit_p07_gh_pr_list_keeps_essentials() {
    let raw = "[{\"number\":1,\"title\":\"foo\",\"state\":\"OPEN\",\"body\":\"long body\"}]";
    let out = compress_for("Bash", &bash("gh pr list --json"), raw);
    assert!(!out.is_empty());
}

// ─── 8. gh_issue_list ──────────────────────────────────────────────────────
#[test]
fn audit_p08_gh_issue_list() {
    let raw = "ISSUE 1: open\nISSUE 2: closed\n";
    let out = compress_for("Bash", &bash("gh issue list"), raw);
    assert!(!out.is_empty());
}

// ─── 9. kubectl_get ────────────────────────────────────────────────────────
#[test]
fn audit_p09_kubectl_get_pods() {
    let raw =
        "NAME       READY   STATUS    RESTARTS   AGE\npod-foo    1/1     Running   0          5d\n";
    let out = compress_for("Bash", &bash("kubectl get pods"), raw);
    assert!(out.contains("Running") || out.contains("NAME"));
}

// ─── 10. docker_ps ─────────────────────────────────────────────────────────
#[test]
fn audit_p10_docker_ps() {
    let raw =
        "CONTAINER ID   IMAGE     COMMAND     STATUS\nabc            redis     redis-svr   Up 2h\n";
    let out = compress_for("Bash", &bash("docker ps"), raw);
    assert!(out.contains("CONTAINER") || out.contains("Up"));
}

// ─── 11. pytest ────────────────────────────────────────────────────────────
#[test]
fn audit_p11_pytest() {
    let raw = "test_a PASSED\ntest_b FAILED\nassertion error: x != y\n=== 1 failed, 1 passed ===\n";
    let out = compress_for("Bash", &bash("pytest -v"), raw);
    assert!(out.contains("FAILED"));
    assert!(out.contains("passed"));
}

// ─── 12. ruff ──────────────────────────────────────────────────────────────
#[test]
fn audit_p12_ruff() {
    let raw = "foo.py:10:5: F401 'os' imported but unused\nFound 1 error\n";
    let out = compress_for("Bash", &bash("ruff check"), raw);
    assert!(!out.is_empty());
}

// ─── 13. tsc ───────────────────────────────────────────────────────────────
#[test]
fn audit_p13_tsc() {
    let raw = "src/foo.ts:10:5 - error TS2304: Cannot find name 'foo'.\nFound 1 error.\n";
    let out = compress_for("Bash", &bash("tsc --noEmit"), raw);
    assert!(out.contains("error TS"));
}

// ─── 14. npm_list ──────────────────────────────────────────────────────────
#[test]
fn audit_p14_npm_list() {
    let raw = "+-- foo@1.0.0\n+-- bar@2.0.0\n+-- baz@3.0.0\n";
    let out = compress_for("Bash", &bash("npm list"), raw);
    assert!(!out.is_empty());
}

// ─── 15. pip_list ──────────────────────────────────────────────────────────
#[test]
fn audit_p15_pip_list() {
    let raw = "Package    Version\n---------  -------\nrequests   2.28.0\nsetuptools 61.2.0\n";
    let out = compress_for("Bash", &bash("pip list"), raw);
    assert!(!out.is_empty());
}

// ─── 16. eslint (NEW-1 batch 2) ────────────────────────────────────────────
#[test]
fn audit_p16_eslint() {
    let raw = "/foo.js:10:5  error  Missing semicolon\n3 problems (3 errors, 0 warnings)\n";
    let out = compress_for("Bash", &bash("eslint src/"), raw);
    assert!(out.contains("error") || out.contains("problem"));
}

// ─── 17. mocha ─────────────────────────────────────────────────────────────
#[test]
fn audit_p17_mocha() {
    let raw = "  ✓ foo passing\n  ✗ bar failing\n  AssertionError\n  1 passing 1 failing\n";
    let out = compress_for("Bash", &bash("mocha test/"), raw);
    assert!(out.contains("passing") || out.contains("failing"));
}

// ─── 18. jest ──────────────────────────────────────────────────────────────
#[test]
fn audit_p18_jest() {
    let raw = "PASS  src/foo.test.js\nFAIL  src/bar.test.js\n  ✕ should work\nTests: 1 failed, 1 passed\n";
    let out = compress_for("Bash", &bash("jest"), raw);
    assert!(out.contains("PASS") || out.contains("FAIL"));
}

// ─── 19. rustdoc ───────────────────────────────────────────────────────────
#[test]
fn audit_p19_rustdoc() {
    let raw = "Documenting foo v0.1.0\nwarning: unresolved link\nFinished dev\n";
    let out = compress_for("Bash", &bash("cargo doc"), raw);
    assert!(out.contains("warning") || out.contains("Documenting"));
}

// ─── 20. terraform_plan ────────────────────────────────────────────────────
#[test]
fn audit_p20_terraform_plan() {
    let raw = "Plan: 3 to add, 1 to change, 0 to destroy.\n  + resource \"aws_s3_bucket\" \"x\"\n  - resource \"aws_iam_user\" \"y\"\n";
    let out = compress_for("Bash", &bash("terraform plan"), raw);
    assert!(out.contains("Plan:"));
    assert!(out.contains("+ "));
}

// ─── 21. ansible_play ──────────────────────────────────────────────────────
#[test]
fn audit_p21_ansible() {
    let raw = "PLAY [webservers] *\nTASK [install nginx] *\nok: [server1]\nfailed: [server2]\nPLAY RECAP *\n";
    let out = compress_for("Bash", &bash("ansible-playbook playbook.yml"), raw);
    assert!(out.contains("PLAY RECAP") || out.contains("failed:"));
}

// ─── 22. make ──────────────────────────────────────────────────────────────
#[test]
fn audit_p22_make() {
    let raw = "make: *** [Makefile:5: all] Error 1\nundefined reference to `foo'\n";
    let out = compress_for("Bash", &bash("make all"), raw);
    assert!(out.contains("Error") || out.contains("undefined"));
}

// ─── 23. jq ────────────────────────────────────────────────────────────────
#[test]
fn audit_p23_jq_small_passthrough() {
    let raw = "{\"a\": 1, \"b\": 2}";
    let out = compress_for("Bash", &bash("cat foo.json | jq ."), raw);
    assert_eq!(&*out, raw, "small jq output passes through");
}

// ─── 24. curl_verbose ──────────────────────────────────────────────────────
#[test]
fn audit_p24_curl_verbose() {
    let raw = "* Trying 1.2.3.4...\n> GET / HTTP/1.1\n> Host: foo\n< HTTP/1.1 200 OK\n< Content-Type: text/html\n";
    let out = compress_for("Bash", &bash("curl -v http://foo/"), raw);
    assert!(out.contains("HTTP/") || out.contains("> GET"));
}

// ─── 25. rsync ─────────────────────────────────────────────────────────────
#[test]
fn audit_p25_rsync() {
    let raw = "sending file list\nfile1.txt\nfile2.txt\nsent 100 bytes  received 50 bytes\ntotal size is 5000\n";
    let out = compress_for("Bash", &bash("rsync -av src/ dst/"), raw);
    assert!(out.contains("sent") || out.contains("total size"));
}

// ─── 26. tar ───────────────────────────────────────────────────────────────
#[test]
fn audit_p26_tar() {
    let raw = "tar: warning: file changed as we read it\ntar: Error: cannot open archive\n";
    let out = compress_for("Bash", &bash("tar cvf foo.tar src/"), raw);
    assert!(out.contains("tar:") || out.contains("Error"));
}

// ─── 27. ffmpeg ────────────────────────────────────────────────────────────
#[test]
fn audit_p27_ffmpeg() {
    let raw = "Input #0, mov, from 'in.mov':\nStream #0:0: Video: h264\nOutput #0, mp4, to 'out.mp4':\nvideo:1024kB audio:128kB\n";
    let out = compress_for("Bash", &bash("ffmpeg -i in.mov out.mp4"), raw);
    assert!(out.contains("Input") || out.contains("video:"));
}

// ─── 28. ripgrep ───────────────────────────────────────────────────────────
#[test]
fn audit_p28_ripgrep() {
    let raw: String = (0..150)
        .map(|i| format!("foo.rs:{i}:match line {i}\n"))
        .collect();
    let out = compress_for("Bash", &bash("rg pattern src/"), &raw);
    let line_count = out.lines().count();
    assert!(line_count <= 105, "ripgrep truncates at 100 hits + marker");
}

// ─── 29. systemctl_status ──────────────────────────────────────────────────
#[test]
fn audit_p29_systemctl_status() {
    let raw = "● foo.service - Foo Service\nLoaded: loaded\nActive: active (running) since Mon\nMain PID: 1234\nMemory: 12M\nTasks: 5\n   May 8 18:00 foo: started\n";
    let out = compress_for("Bash", &bash("systemctl status foo"), raw);
    assert!(out.contains("Active:"));
}

// ─── 30. journalctl ────────────────────────────────────────────────────────
#[test]
fn audit_p30_journalctl() {
    let raw = "May 8 info: starting\nMay 8 ERROR: connection refused\nMay 8 FATAL: out of memory\nMay 8 info: shutdown\n";
    let out = compress_for("Bash", &bash("journalctl -u foo"), raw);
    assert!(out.contains("ERROR") || out.contains("FATAL"));
}

// ─── Falsifiability + REGRA #0 audit ───────────────────────────────────────

#[test]
fn audit_unknown_tool_passthrough() {
    let raw = "internal_cli output line A\nline B";
    let out = compress_for("Bash", &bash("rare_internal_cli --foo"), raw);
    assert_eq!(&*out, raw, "unknown tool → unchanged passthrough");
}

// `audit_disabled_flag_passthrough` lived here until 2026-08-02 and MUTATED the
// process environment (`set_var("TOURING_COMPRESSION_PROFILES", "0")`) — its own
// TODO admitted the hazard. Env vars are process-global and libtest runs this
// file's tests concurrently, so during that window every neighbour saw
// compression disabled and got raw passthrough. That is what made
// `audit_p04_git_log_collapses_to_one_line` fail under `cargo test --workspace`
// while passing in isolation. The test now lives in its own integration file,
// `compression_kill_switch_e2e.rs`: cargo compiles each `tests/*.rs` as a
// separate BINARY, so the mutation can no longer reach anything else.
