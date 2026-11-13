//! C7 — the two producers of the runtime's heat map (`post_edit` records an edit,
//! `pre_read` records an access) and the pipeline's memory budget only work together if
//! they agree on the KEY: the project-relative path the pipeline, the index and the edit
//! history all use. This test drives both producers through their real hook entry points
//! and then the budget, so a producer that goes back to the absolute `file_path` fails it
//! twice: the key assertion, and the eviction that no longer finds the file's heat.

use crate::runtime::HookRuntime;
use serde_json::json;
use touring_code::ast::SharedPipeline;

/// Comment padding: a big document with almost no symbols to extract.
fn padded_rust(bytes: usize) -> String {
    format!(
        "pub fn f() {{}}\n{}",
        "// padding padding padding padding padding padding padding\n".repeat(bytes / 58 + 1)
    )
}

#[test]
#[serial_test::serial(s28_state)]
fn the_heat_producers_key_by_the_relative_path_that_the_pipeline_budget_ranks_by() {
    // `post_edit` runs the S28 gate, which keeps state; give it a private directory.
    let _state = crate::s28_gate::use_state_dir("heat-producers");
    let project = tempfile::TempDir::new().expect("project");
    let src = project.path().join("src");
    std::fs::create_dir_all(&src).expect("src");
    let body = padded_rust(400 * 1024);
    // `a_hot` sorts FIRST by path: were its heat not found, it would be the first victim.
    for name in ["a_hot.rs", "b_read.rs", "c_new.rs"] {
        std::fs::write(src.join(name), &body).expect("write");
    }
    let abs = |name: &str| src.join(name).to_string_lossy().into_owned();

    let mut runtime = HookRuntime::new(project.path()).expect("runtime");
    // 3 x 400 KiB of rope do not fit in 1 MiB: loading all three must evict exactly one.
    runtime.infra.pipeline = Some(SharedPipeline::new().with_memory_budget_mb(1));

    // The two real producers, through the real hook entry points.
    let _ = crate::post_edit::run_returning(
        &mut runtime,
        &json!({"tool_name": "Edit", "tool_input": {"file_path": abs("a_hot.rs")}}),
    );
    let _ = crate::pre_read::run_returning(
        &runtime,
        &json!({"tool_input": {"file_path": abs("b_read.rs")}}),
    );

    // (a) the key is the RELATIVE path — the pipeline's spelling — and never the absolute one.
    {
        let heat = runtime.learning.heat_map.borrow();
        assert_eq!(
            heat.get("src/a_hot.rs").map(|h| h.edits),
            Some(1),
            "post_edit must record the edit under the relative path"
        );
        assert_eq!(
            heat.get("src/b_read.rs").map(|h| h.access_count),
            Some(1),
            "pre_read must record the access under the relative path"
        );
        assert!(
            heat.get(&abs("a_hot.rs")).is_none() && heat.get(&abs("b_read.rs")).is_none(),
            "no entry may be keyed by an absolute path"
        );
    }

    // (b) the budget finds that heat: through the production door (`process_file`), the
    // edited file stays and the file that was only read, the coldest tracked, leaves.
    for name in ["a_hot.rs", "b_read.rs", "c_new.rs"] {
        runtime
            .process_file(&format!("src/{name}"), &body)
            .expect("process_file");
    }
    let pipeline = runtime.infra.pipeline.as_ref().expect("pipeline");
    pipeline.with_read(|p| {
        assert!(
            p.get_document("src/a_hot.rs").is_some(),
            "the edited file's heat, keyed relative, outranks the rest"
        );
        assert!(
            p.get_document("src/b_read.rs").is_none(),
            "the file that was only read is the coldest tracked: it is the victim"
        );
        assert!(
            p.get_document("src/c_new.rs").is_some(),
            "the file just processed never leaves"
        );
    });
    assert_eq!(pipeline.stats().evictions, 1, "exactly one victim");
}
