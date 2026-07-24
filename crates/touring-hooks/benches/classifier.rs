//! Criterion benchmarks for CILA intent classification.
//!
//! Benchmarks the IntentClassifier (48-pattern RegexSet) and CachedIntentClassifier
//! across different CILA levels with realistic prompt texts.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use touring_hooks::classifier::{CachedIntentClassifier, IntentClassifier};

/// Representative prompts for each CILA level.
const PROMPTS: &[(&str, &str)] = &[
    ("L0_greeting", "ola, como vai voce?"),
    ("L0_question", "O que significa ANTT?"),
    ("L1_calcule", "calcule o VPL do projeto com TIR de 12%"),
    ("L1_quantos", "quantos processos existem no sistema?"),
    ("L2_search", "buscar na base de dados os documentos"),
    ("L2_rag", "consulte o RAG para encontrar precedentes"),
    ("L3_process", "analisar 50500.123456/2024-01"),
    (
        "L3_reequilibrio",
        "reequilibrio economico-financeiro do contrato",
    ),
    ("L4_aco", "execute --aco completo para validacao"),
    ("L4_deep", "analise completa do processo administrativo"),
    ("L5_drift", "execute drift detection no modelo"),
    ("L6_swarm", "execute --swarm completo"),
    ("L6_team", "crie uma equipe de agentes paralela"),
];

fn bench_classify_raw(c: &mut Criterion) {
    let classifier = IntentClassifier::build();
    let mut group = c.benchmark_group("classify_intent");

    for (name, prompt) in PROMPTS {
        group.bench_function(*name, |b| {
            b.iter(|| classifier.classify(black_box(prompt)));
        });
    }

    group.finish();
}

fn bench_classify_batch(c: &mut Criterion) {
    let classifier = IntentClassifier::build();
    let prompts: Vec<String> = PROMPTS.iter().map(|(_, p)| p.to_string()).collect();

    c.bench_function("classify_batch_13", |b| {
        b.iter(|| {
            for p in black_box(&prompts) {
                classifier.classify(p);
            }
        });
    });
}

fn bench_classify_long_prompt(c: &mut Criterion) {
    let classifier = IntentClassifier::build();
    // Simulate a long prompt (typical user message ~500 chars)
    let long_prompt = format!(
        "Preciso de uma analise completa e detalhada do processo 50500.123456/2024-01 \
         que envolve o reequilibrio economico-financeiro do contrato de concessao. \
         O VPL e a TIR devem ser calculados com base nos fluxos de caixa projetados. \
         Favor buscar na base de dados os documentos relevantes e preparar um \
         relatorio a diretoria colegiada. {}",
        "Contexto adicional. ".repeat(20)
    );

    c.bench_function("classify_long_prompt", |b| {
        b.iter(|| classifier.classify(black_box(&long_prompt)));
    });
}

fn bench_cached_classifier(c: &mut Criterion) {
    let cached = CachedIntentClassifier::new(256);
    let mut group = c.benchmark_group("cached_classifier");

    // Warm up cache with all prompts
    for (_, prompt) in PROMPTS {
        cached.classify(prompt);
    }

    // Benchmark cache hits
    for (name, prompt) in PROMPTS {
        group.bench_function(format!("hit_{}", name), |b| {
            b.iter(|| cached.classify(black_box(prompt)));
        });
    }

    // Benchmark cache miss (new prompt each iteration)
    group.bench_function("miss_unique", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            counter += 1;
            let prompt = format!("unique prompt number {}", counter);
            cached.classify(black_box(&prompt))
        });
    });

    group.finish();
}

fn bench_classifier_build(c: &mut Criterion) {
    c.bench_function("classifier_build", |b| {
        b.iter(IntentClassifier::build);
    });
}

criterion_group!(
    benches,
    bench_classify_raw,
    bench_classify_batch,
    bench_classify_long_prompt,
    bench_cached_classifier,
    bench_classifier_build,
);
criterion_main!(benches);
