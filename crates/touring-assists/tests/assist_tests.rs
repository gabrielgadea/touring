//! Integration tests for touring-assists framework.

use touring_assists::{
    Assist, AssistCatalog, AssistContext, AssistGroup, AssistHandler, AssistId, AssistTarget,
    Assists, LazySourceChange,
};
use touring_generator::source_change::{Indel, SourceChange, TextEdit};

fn make_ctx(content: &str, range: std::ops::Range<usize>) -> AssistContext<'_> {
    AssistContext::new(1usize, "test.rs", content, range)
}

fn make_target(file_id: usize, range: std::ops::Range<usize>) -> AssistTarget {
    AssistTarget { file_id, range }
}

fn dummy_source_change() -> SourceChange {
    SourceChange::new()
}

#[test]
fn assist_context_selected_text() {
    let ctx = make_ctx("fn foo() {}", 3..6);
    assert_eq!(ctx.selected_text(), "foo");
}

#[test]
fn assist_context_is_cursor_empty_range() {
    let ctx = make_ctx("fn foo() {}", 3..3);
    assert!(ctx.is_cursor());
}

#[test]
fn assist_context_is_cursor_non_empty() {
    let ctx = make_ctx("fn foo() {}", 3..6);
    assert!(!ctx.is_cursor());
}

#[test]
fn lazy_source_change_evaluate() {
    let lazy = LazySourceChange::new(dummy_source_change);
    let result = lazy.evaluate();
    assert!(result.is_empty());
}

#[test]
fn assists_add_single() {
    let mut assists = Assists::new();
    let lazy = LazySourceChange::new(dummy_source_change);
    assists.add(Assist::new(
        "test_assist",
        "Test Assist".to_string(),
        AssistGroup("Test"),
        make_target(1, 0..0),
        lazy,
    ));
    assert_eq!(assists.len(), 1);
    assert!(!assists.is_empty());
}

#[test]
fn assists_add_with_group() {
    let mut assists = Assists::new();
    let lazy = LazySourceChange::new(dummy_source_change);
    assists.add_with_group(
        "group_test",
        "Group Test".to_string(),
        AssistGroup("Testing"),
        make_target(1, 0..0),
        lazy,
    );
    assert_eq!(assists.len(), 1);
}

#[test]
fn assists_finish_sorted() {
    let mut assists = Assists::new();
    let make_assist = |priority: i32| {
        let lazy = LazySourceChange::new(dummy_source_change);
        Assist::new(
            "test",
            "Test".to_string(),
            AssistGroup("Test"),
            make_target(1, 0..0),
            lazy,
        )
        .with_priority(priority)
    };
    assists.add(make_assist(5));
    assists.add(make_assist(1));
    assists.add(make_assist(10));

    let result = assists.finish();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].priority, 10);
    assert_eq!(result[1].priority, 5);
    assert_eq!(result[2].priority, 1);
}

#[test]
fn assists_by_file_groups() {
    let mut assists = Assists::new();
    let make_file_assist = |file_id: usize| {
        let lazy = LazySourceChange::new(dummy_source_change);
        Assist::new(
            "test",
            "Test".to_string(),
            AssistGroup("Test"),
            make_target(file_id, 0..0),
            lazy,
        )
    };
    assists.add(make_file_assist(1));
    assists.add(make_file_assist(2));
    assists.add(make_file_assist(1));

    let by_file = assists.by_file();
    assert_eq!(by_file.len(), 2);
    assert_eq!(by_file[&1usize].len(), 2);
    assert_eq!(by_file[&2usize].len(), 1);
}

#[test]
fn assists_filter_group() {
    let mut combined = Assists::new();
    let lazy_a = LazySourceChange::new(dummy_source_change);
    let lazy_b = LazySourceChange::new(dummy_source_change);
    let lazy_c = LazySourceChange::new(dummy_source_change);

    combined.add_with_group(
        "a",
        "A".to_string(),
        AssistGroup("GroupA"),
        make_target(1, 0..0),
        lazy_a,
    );
    combined.add_with_group(
        "b",
        "B".to_string(),
        AssistGroup("GroupB"),
        make_target(1, 0..0),
        lazy_b,
    );
    combined.add_with_group(
        "c",
        "C".to_string(),
        AssistGroup("GroupA"),
        make_target(1, 0..0),
        lazy_c,
    );

    let group_a = combined.filter_group(&AssistGroup("GroupA"));
    assert_eq!(group_a.len(), 2);
}

#[test]
fn assist_target_clone() {
    let target = make_target(42, 10..20);
    let cloned = target.clone();
    assert_eq!(cloned.file_id, 42);
    assert_eq!(cloned.range, 10..20);
}

#[test]
fn assist_group_debug() {
    let group = AssistGroup("TestGroup");
    let debug = format!("{:?}", group);
    assert!(debug.contains("TestGroup"));
}

#[test]
fn assist_handler_type_alias_compiles() {
    fn handler_fn(_: &mut Assists, _: &AssistContext) -> Option<()> {
        Some(())
    }
    let _: AssistHandler = handler_fn;
    let mut assists = Assists::new();
    let ctx = make_ctx("test", 0..4);
    let result = handler_fn(&mut assists, &ctx);
    assert!(result.is_some());
}

#[test]
fn assist_id_static_str() {
    let id1: AssistId = "test_id";
    let id2: AssistId = "test_id";
    assert_eq!(id1, id2);
}

#[test]
fn source_change_empty_is_empty() {
    let sc = SourceChange::new();
    assert!(sc.is_empty());
    assert_eq!(sc.file_count(), 0);
    assert_eq!(sc.fs_edit_count(), 0);
}

#[test]
fn source_change_with_text_edit() {
    let text_edit = TextEdit::try_from_iter(vec![Indel {
        delete: 0..5,
        insert: "hello".to_string(),
    }])
    .expect("valid indels");

    let sc = SourceChange::new().with_edit(1usize, text_edit);
    assert!(!sc.is_empty());
    assert_eq!(sc.file_count(), 1);
}

#[test]
fn assist_assist_code_action_conversion() {
    use touring_assists::AssistCodeAction;

    let lazy = LazySourceChange::new(|| {
        let text_edit = TextEdit::try_from_iter(vec![Indel {
            delete: 0..5,
            insert: "hello".to_string(),
        }])
        .expect("valid");
        SourceChange::new().with_edit(1, text_edit)
    });

    let assist = Assist::new(
        "test_assist",
        "Test Assist".to_string(),
        AssistGroup("Test"),
        make_target(1, 0..5),
        lazy,
    );

    let code_action: AssistCodeAction = assist.into();
    assert_eq!(code_action.id, "test_assist");
    assert_eq!(code_action.title, "Test Assist");
}

#[test]
fn assist_edit_text_change_serde() {
    use serde_json;

    let change = touring_assists::AssistTextChange {
        range: 0..10,
        new_text: "test".to_string(),
    };

    let json = serde_json::to_string(&change).expect("serialize");
    let decoded: touring_assists::AssistTextChange =
        serde_json::from_str(&json).expect("deserialize");

    assert_eq!(decoded.range, 0..10);
    assert_eq!(decoded.new_text, "test");
}

#[test]
fn assist_edit_struct_serde() {
    use serde_json;

    let edit = touring_assists::AssistEdit {
        file_id: 42,
        changes: vec![touring_assists::AssistTextChange {
            range: 5..15,
            new_text: "replacement".to_string(),
        }],
    };

    let json = serde_json::to_string(&edit).expect("serialize");
    let decoded: touring_assists::AssistEdit = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(decoded.file_id, 42);
    assert_eq!(decoded.changes.len(), 1);
}

#[test]
fn assist_code_action_serde() {
    use serde_json;

    let action = touring_assists::AssistCodeAction {
        id: "inline_fn".to_string(),
        title: "Inline function".to_string(),
        group: "Refactoring".to_string(),
        file_id: 1,
        range: 10..20,
        edit: None,
        command: Some("test.command".to_string()),
    };

    let json = serde_json::to_string(&action).expect("serialize");
    let decoded: touring_assists::AssistCodeAction =
        serde_json::from_str(&json).expect("deserialize");

    assert_eq!(decoded.id, "inline_fn");
    assert!(decoded.command.is_some());
}

#[test]
fn assist_code_action_skips_none_edit() {
    use serde_json;

    let action = touring_assists::AssistCodeAction {
        id: "test".to_string(),
        title: "Test".to_string(),
        group: "Group".to_string(),
        file_id: 1,
        range: 0..5,
        edit: None,
        command: None,
    };

    let json = serde_json::to_string(&action).expect("serialize");
    assert!(!json.contains("\"edit\":null"));
}

#[test]
fn assists_default_constructor() {
    let assists = Assists::new();
    assert_eq!(assists.len(), 0);
    assert!(assists.is_empty());
}

#[test]
fn assist_target_partial_eq() {
    let t1 = make_target(1, 0..5);
    let t2 = make_target(1, 0..5);
    let t3 = make_target(2, 0..5);
    assert_eq!(t1, t2);
    assert_ne!(t1, t3);
}

#[test]
fn assist_group_equality() {
    let g1 = AssistGroup("Test");
    let g2 = AssistGroup("Test");
    let g3 = AssistGroup("Other");
    assert_eq!(g1, g2);
    assert_ne!(g1, g3);
}

#[test]
fn assist_new_with_all_fields() {
    let lazy = LazySourceChange::new(dummy_source_change);
    let assist = Assist::new(
        "full_test",
        "Full Test Label".to_string(),
        AssistGroup("Comprehensive"),
        make_target(99, 100..200),
        lazy,
    );
    assert_eq!(assist.id, "full_test");
    assert_eq!(assist.label, "Full Test Label");
    assert_eq!(assist.group, AssistGroup("Comprehensive"));
    assert_eq!(assist.target.file_id, 99);
    assert_eq!(assist.target.range, 100..200);
    assert_eq!(assist.priority, 0);
}

#[test]
fn assist_with_priority_changes_priority() {
    let lazy = LazySourceChange::new(dummy_source_change);
    let assist = Assist::new(
        "priority_test",
        "Priority Test".to_string(),
        AssistGroup("Test"),
        make_target(1, 0..0),
        lazy,
    )
    .with_priority(42);
    assert_eq!(assist.priority, 42);
}

#[test]
fn assist_target_file_id_range_access() {
    let target = make_target(123, 50..100);
    assert_eq!(target.file_id, 123);
    assert_eq!(target.range.start, 50);
    assert_eq!(target.range.end, 100);
}

#[test]
fn assist_catalog_register_and_get() {
    let mut catalog = AssistCatalog::new();
    let _lazy = LazySourceChange::new(dummy_source_change);

    let handler: AssistHandler = |_, _| Some(());
    catalog.register("test_handler", handler);

    assert!(catalog.get("test_handler").is_some());
    assert!(catalog.get("nonexistent").is_none());
}

#[test]
fn assist_catalog_ids() {
    let mut catalog = AssistCatalog::new();
    let handler: AssistHandler = |_, _| Some(());
    catalog.register("handler_a", handler);
    catalog.register("handler_b", handler);

    let ids = catalog.ids();
    assert!(ids.contains(&"handler_a"));
    assert!(ids.contains(&"handler_b"));
}

#[test]
fn assist_catalog_get_returns_handler() {
    let mut catalog = AssistCatalog::new();
    let handler: AssistHandler = |_, _| Some(());
    catalog.register("specific", handler);

    let retrieved = catalog.get("specific");
    assert!(retrieved.is_some());

    let mut assists = Assists::new();
    let ctx = make_ctx("test", 0..4);
    let result = retrieved.unwrap()(&mut assists, &ctx);
    assert!(result.is_some());
}

#[test]
fn assist_catalog_get_none_for_unknown() {
    let catalog = AssistCatalog::new();
    assert!(catalog.get("unknown").is_none());
}

#[test]
fn assist_catalog_len_zero_empty() {
    let catalog = AssistCatalog::new();
    assert_eq!(catalog.len(), 0);
    assert!(catalog.ids().is_empty());
}

#[test]
fn assist_group_serialize() {
    use serde_json;
    let group = AssistGroup("SerializeTest");
    let json = serde_json::to_string(&group).expect("serialize");
    assert!(json.contains("SerializeTest"));
}

#[test]
fn assist_id_type_equality() {
    let id1: AssistId = "handler";
    let id2: AssistId = "handler";
    assert_eq!(id1, id2);
}

#[test]
fn assist_target_file_id_set_accessible() {
    let target = AssistTarget {
        file_id: 777,
        range: 0..0,
    };
    assert_eq!(target.file_id, 777);
}

#[test]
fn assist_target_range_clone_eq() {
    let target = make_target(1, 5..15);
    let cloned_range = target.range.clone();
    assert_eq!(cloned_range, 5..15);
}

#[test]
fn assists_len_and_is_empty() {
    let mut assists = Assists::new();
    assert!(assists.is_empty());
    assert_eq!(assists.len(), 0);

    let lazy = LazySourceChange::new(dummy_source_change);
    assists.add(Assist::new(
        "test",
        "Test".to_string(),
        AssistGroup("Test"),
        make_target(1, 0..0),
        lazy,
    ));
    assert!(!assists.is_empty());
    assert_eq!(assists.len(), 1);
}

#[test]
fn assist_context_selected_text_multiline() {
    let ctx = make_ctx("fn foo() {\n    bar()\n}", 3..6);
    assert_eq!(ctx.selected_text(), "foo");
}

#[test]
fn assist_context_selected_text_boundary_start() {
    let ctx = make_ctx("fn foo() {}", 0..2);
    assert_eq!(ctx.selected_text(), "fn");
}

#[test]
fn assist_context_selected_text_boundary_end() {
    let ctx = make_ctx("fn foo() {}", 6..8);
    assert_eq!(ctx.selected_text(), "()");
}

#[test]
fn assist_context_cursor_at_content_start() {
    let ctx = make_ctx("fn foo() {}", 0..0);
    assert!(ctx.is_cursor());
    assert_eq!(ctx.selected_text(), "");
}

#[test]
fn assists_finish_returns_in_priority_order() {
    let mut assists = Assists::new();
    let make = |p: i32| {
        let lazy = LazySourceChange::new(dummy_source_change);
        Assist::new(
            "t",
            "T".to_string(),
            AssistGroup("T"),
            make_target(1, 0..0),
            lazy,
        )
        .with_priority(p)
    };
    assists.add(make(1));
    assists.add(make(100));
    assists.add(make(50));
    let result = assists.finish();
    assert_eq!(result[0].priority, 100);
    assert_eq!(result[1].priority, 50);
    assert_eq!(result[2].priority, 1);
}

#[test]
fn assists_filter_group_no_match() {
    let mut combined = Assists::new();
    let lazy = LazySourceChange::new(dummy_source_change);
    combined.add_with_group(
        "a",
        "A".to_string(),
        AssistGroup("GroupA"),
        make_target(1, 0..0),
        lazy,
    );

    let group_b = combined.filter_group(&AssistGroup("GroupB"));
    assert_eq!(group_b.len(), 0);
}

#[test]
fn assist_target_zero_length_range() {
    let target = make_target(1, 10..10);
    assert_eq!(target.range.start, 10);
    assert_eq!(target.range.end, 10);
    assert_eq!(target.range.start, target.range.end);
}

#[test]
fn assist_context_with_unicode_content() {
    let ctx = make_ctx("fn café() {}", 3..8);
    assert_eq!(ctx.selected_text(), "café");
}

#[test]
fn assist_id_from_static_str() {
    let id: AssistId = "static_id";
    assert_eq!(id, "static_id");
}

#[test]
fn assist_group_from_str() {
    let group = AssistGroup("TestGroup");
    assert_eq!(group.0, "TestGroup");
}

#[test]
fn assists_add_multiple_same_group() {
    let mut assists = Assists::new();
    let make = |id: &'static str| {
        let lazy = LazySourceChange::new(dummy_source_change);
        Assist::new(
            id,
            id.to_string(),
            AssistGroup("G"),
            make_target(1, 0..0),
            lazy,
        )
    };
    assists.add(make("a"));
    assists.add(make("b"));
    assists.add(make("c"));

    let g = assists.filter_group(&AssistGroup("G"));
    assert_eq!(g.len(), 3);
}

#[test]
fn assist_code_action_with_edit_serde() {
    use touring_assists::AssistCodeAction;
    let action = AssistCodeAction {
        id: "test".to_string(),
        title: "Test".to_string(),
        group: "Test".to_string(),
        file_id: 1,
        range: 0..5,
        edit: Some(touring_assists::AssistEdit {
            file_id: 1,
            changes: vec![touring_assists::AssistTextChange {
                range: 0..5,
                new_text: "hello".to_string(),
            }],
        }),
        command: None,
    };

    let json = serde_json::to_string(&action).expect("serialize");
    let decoded: AssistCodeAction = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(decoded.id, "test");
    assert!(decoded.edit.is_some());
}

#[test]
fn assist_text_change_range_access() {
    let tc = touring_assists::AssistTextChange {
        range: 10..20,
        new_text: "test".to_string(),
    };
    assert_eq!(tc.range.start, 10);
    assert_eq!(tc.range.end, 20);
}

#[test]
fn assist_edit_single_change_serde() {
    use touring_assists::{AssistEdit, AssistTextChange};
    let edit = AssistEdit {
        file_id: 1,
        changes: vec![
            AssistTextChange {
                range: 0..5,
                new_text: "hello".to_string(),
            },
            AssistTextChange {
                range: 5..10,
                new_text: "world".to_string(),
            },
        ],
    };
    let json = serde_json::to_string(&edit).expect("serialize");
    let decoded: AssistEdit = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(decoded.file_id, 1);
    assert_eq!(decoded.changes.len(), 2);
}
