//! Criterion benchmarks for QTable TD(lambda) update throughput.
#![allow(unused_must_use)] // bench callsites don't need to capture TD errors

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use touring_intelligence::rl::rl::qtable::{QLearning, QTable, RewardBreakdown};

fn bench_update_episodes(c: &mut Criterion) {
    let mut group = c.benchmark_group("qtable_update");

    for &episode_count in &[1_000u64, 10_000] {
        group.bench_with_input(
            BenchmarkId::new("episodes", episode_count),
            &episode_count,
            |b, &n| {
                b.iter(|| {
                    let mut qt = QTable::new();
                    for ep in 0..n {
                        let state = ep % 36; // 9 CILA levels * 4 file types
                        let action = ep % 64;
                        let reward = ((ep % 100) as f64) / 100.0;
                        let next_state = (state + 1) % 36;
                        qt.update(
                            black_box(state),
                            black_box(action),
                            black_box(reward),
                            black_box(next_state),
                            None,
                            ep % 10 == 0, // terminal every 10th
                        );
                    }
                    qt
                });
            },
        );
    }

    group.finish();
}

fn bench_best_action(c: &mut Criterion) {
    // Pre-populate a QTable with diverse state-action pairs
    let mut qt = QTable::new();
    for state in 0..100u64 {
        for action in 0..8u64 {
            let reward = ((state * 7 + action * 13) % 100) as f64 / 100.0;
            qt.update(state, action, reward, (state + 1) % 100, None, true);
        }
    }

    c.bench_function("qtable_best_action_100_states", |b| {
        b.iter(|| {
            let mut total = 0u64;
            for state in 0..100u64 {
                if let Some(a) = qt.best_action(black_box(state)) {
                    total += a;
                }
            }
            total
        });
    });
}

fn bench_hook_event_update(c: &mut Criterion) {
    let mut group = c.benchmark_group("qtable_hook_event");

    let scenarios = [
        ("python_edit", 2u8, "python", "Edit", 85.0),
        ("rust_bash", 1, "rust", "Bash", 90.0),
        ("ts_write", 3, "typescript", "Write", 70.0),
    ];

    for (name, cila, ftype, tool, score) in scenarios {
        group.bench_function(name, |b| {
            let mut qt = QTable::new();
            b.iter(|| {
                qt.update_from_hook_event(
                    black_box(cila),
                    black_box(ftype),
                    black_box(tool),
                    black_box(score),
                )
            });
        });
    }

    group.finish();
}

fn bench_granular_reward(c: &mut Criterion) {
    let breakdown = RewardBreakdown {
        compilation: 1.0,
        lint: 0.8,
        type_safe: 0.9,
        tests: 0.7,
        coverage: 0.6,
    };

    c.bench_function("qtable_granular_reward", |b| {
        let mut qt = QTable::new();
        b.iter(|| {
            qt.update_from_granular_reward(
                black_box(2),
                black_box("python"),
                black_box("Edit"),
                black_box(&breakdown),
            )
        });
    });
}

criterion_group!(
    benches,
    bench_update_episodes,
    bench_best_action,
    bench_hook_event_update,
    bench_granular_reward,
);
criterion_main!(benches);
