//! Criterion benchmarks for LinUCB contextual bandit arm selection.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use touring_intelligence::rl::bandit::linucb::{LinUCBBandit, NUM_ARMS, extract_features};

fn bench_select_arm(c: &mut Criterion) {
    let mut group = c.benchmark_group("linucb_select_arm");

    let contexts = [
        ("python_small_early", "python", 50, 5, 0, 2u8),
        ("rust_medium_mid", "rust", 500, 30, 2, 4),
        ("ts_large_late", "typescript", 2000, 60, 1, 5),
    ];

    for (name, ftype, size, turn, errs, cila) in contexts {
        group.bench_function(name, |b| {
            let mut bandit = LinUCBBandit::new();
            let features = extract_features(ftype, size, turn, errs, cila);
            b.iter(|| bandit.select_arm(black_box(&features)));
        });
    }

    group.finish();
}

fn bench_select_arm_after_training(c: &mut Criterion) {
    let mut group = c.benchmark_group("linucb_select_trained");

    for &train_rounds in &[10usize, 100, 500] {
        group.bench_with_input(
            BenchmarkId::new("rounds", train_rounds),
            &train_rounds,
            |b, &rounds| {
                let mut bandit = LinUCBBandit::new();
                let features = extract_features("python", 200, 20, 0, 2);

                // Train the bandit
                for i in 0..rounds {
                    let arm = i % NUM_ARMS;
                    let reward = if arm == 3 { 1.0 } else { 0.2 };
                    bandit.update(arm, &features, reward);
                }

                b.iter(|| bandit.select_arm(black_box(&features)));
            },
        );
    }

    group.finish();
}

fn bench_update_cycle(c: &mut Criterion) {
    c.bench_function("linucb_update_cycle", |b| {
        let mut bandit = LinUCBBandit::new();
        let features = extract_features("python", 200, 20, 0, 2);
        let mut arm_idx = 0usize;

        b.iter(|| {
            let (arm, _) = bandit.select_arm(black_box(&features));
            arm_idx = arm;
            bandit.update(black_box(arm_idx), black_box(&features), black_box(0.8));
        });
    });
}

fn bench_extract_features(c: &mut Criterion) {
    c.bench_function("linucb_extract_features", |b| {
        b.iter(|| {
            extract_features(
                black_box("rust"),
                black_box(500),
                black_box(30),
                black_box(2),
                black_box(4),
            )
        });
    });
}

fn bench_export_import(c: &mut Criterion) {
    let mut group = c.benchmark_group("linucb_persistence");

    // Build a trained bandit
    let mut bandit = LinUCBBandit::new();
    let features = extract_features("python", 200, 20, 0, 2);
    for i in 0..100 {
        bandit.update(i % NUM_ARMS, &features, 0.5 + (i % 10) as f64 / 20.0);
    }

    group.bench_function("export", |b| {
        b.iter(|| bandit.export());
    });

    let exported = bandit.export();
    group.bench_function("import", |b| {
        b.iter(|| {
            let mut fresh = LinUCBBandit::new();
            fresh.import(black_box(&exported));
            fresh
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_select_arm,
    bench_select_arm_after_training,
    bench_update_cycle,
    bench_extract_features,
    bench_export_import,
);
criterion_main!(benches);
