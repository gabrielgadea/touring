//! Criterion benchmarks for FileKnowledgeDB operations.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use tempfile::TempDir;
use touring_hooks::knowledge::{BashOutcome, FileKnowledge, FileKnowledgeDB};

/// Create a fresh DB in a temp directory.
fn fresh_db() -> (TempDir, FileKnowledgeDB) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("bench_knowledge.db");
    let db = FileKnowledgeDB::new(&db_path).unwrap();
    (tmp, db)
}

/// Create a DB pre-populated with `n` file entries.
fn populated_db(n: usize) -> (TempDir, FileKnowledgeDB) {
    let (tmp, db) = fresh_db();
    for i in 0..n {
        db.upsert(&FileKnowledge {
            file_path: format!("src/module_{}/file_{}.py", i / 10, i),
            language: Some("python".to_string()),
            line_count: (i * 10 + 50) as i64,
            symbol_count: (i % 20 + 5) as i64,
            read_count: 0,
            last_read_at: None,
            content_hash: Some(format!("hash_{i}")),
            imports_json: Some(format!("[\"mod_{}\"]", i % 10)),
            symbols_json: Some(format!("[\"func_{}\", \"Class_{}\"]", i, i)),
            notes: None,
        })
        .unwrap();
    }
    (tmp, db)
}

fn bench_upsert(c: &mut Criterion) {
    let mut group = c.benchmark_group("knowledge_db_upsert");

    for &count in &[10usize, 100] {
        group.bench_with_input(BenchmarkId::new("files", count), &count, |b, &n| {
            b.iter_batched(
                fresh_db,
                |(_tmp, db)| {
                    for i in 0..n {
                        db.upsert(&FileKnowledge {
                            file_path: format!("file_{i}.py"),
                            language: Some("python".to_string()),
                            line_count: 100,
                            symbol_count: 10,
                            read_count: 0,
                            last_read_at: None,
                            content_hash: Some(format!("h_{i}")),
                            imports_json: None,
                            symbols_json: None,
                            notes: None,
                        })
                        .unwrap();
                    }
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn bench_lookup(c: &mut Criterion) {
    let (_tmp, db) = populated_db(500);

    let mut group = c.benchmark_group("knowledge_db_lookup");

    group.bench_function("existing_file", |b| {
        b.iter(|| db.lookup(black_box("src/module_5/file_50.py")).unwrap());
    });

    group.bench_function("missing_file", |b| {
        b.iter(|| db.lookup(black_box("nonexistent/path.py")).unwrap());
    });

    group.finish();
}

fn bench_get_relations_from(c: &mut Criterion) {
    let (_tmp, db) = fresh_db();
    // Populate file knowledge entries
    for i in 0..100 {
        db.upsert(&FileKnowledge {
            file_path: format!("file_{i}.py"),
            language: Some("python".to_string()),
            line_count: 100,
            symbol_count: 5,
            ..Default::default()
        })
        .unwrap();
    }

    c.bench_function("knowledge_db_get_relations_from", |b| {
        b.iter(|| db.get_relations_from(black_box("file_5.py")).unwrap());
    });
}

fn bench_record_bash_outcome(c: &mut Criterion) {
    c.bench_function("knowledge_db_record_bash", |b| {
        b.iter_batched(
            fresh_db,
            |(_tmp, db)| {
                for i in 0..50 {
                    db.record_bash_outcome(&BashOutcome {
                        command: format!("cargo test -p touring-hooks -- test_{i}"),
                        command_short: "cargo test".to_string(),
                        command_hash: String::new(),
                        exit_code: if i % 5 == 0 { 1 } else { 0 },
                        success: i % 5 != 0,
                        error_pattern: if i % 5 == 0 {
                            Some("test_failure".to_string())
                        } else {
                            None
                        },
                        file_context: Some(format!("file_{i}.rs")),
                        executed_at: "2026-03-20T12:00:00".to_string(),
                    })
                    .unwrap();
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_record_edit(c: &mut Criterion) {
    c.bench_function("knowledge_db_record_edit", |b| {
        b.iter_batched(
            fresh_db,
            |(_tmp, db)| {
                for i in 0..50 {
                    db.record_edit(
                        &format!("file_{i}.py"),
                        "edit",
                        Some(&format!("replaced function body {i}")),
                    )
                    .unwrap();
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_record_coedit(c: &mut Criterion) {
    c.bench_function("knowledge_db_record_coedit", |b| {
        b.iter_batched(
            fresh_db,
            |(_tmp, db)| {
                for i in 0..50 {
                    db.record_coedit(
                        &format!("file_{i}.py"),
                        &format!("file_{}.py", (i + 1) % 50),
                    )
                    .unwrap();
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_get_coedit_neighbors(c: &mut Criterion) {
    let (_tmp, db) = fresh_db();
    // Populate co-edits
    for i in 0..100 {
        for j in 1..=3 {
            db.record_coedit(
                &format!("file_{i}.py"),
                &format!("file_{}.py", (i + j) % 100),
            )
            .unwrap();
        }
    }

    c.bench_function("knowledge_db_coedit_neighbors", |b| {
        b.iter(|| db.get_coedit_neighbors(black_box("file_50.py"), black_box(10)));
    });
}

criterion_group!(
    benches,
    bench_upsert,
    bench_lookup,
    bench_get_relations_from,
    bench_record_bash_outcome,
    bench_record_edit,
    bench_record_coedit,
    bench_get_coedit_neighbors,
);
criterion_main!(benches);
