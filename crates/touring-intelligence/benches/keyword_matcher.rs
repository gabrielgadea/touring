//! Criterion benchmarks for Aho-Corasick keyword matching.
//!
//! Benchmarks KeywordMatcher with ANTT regulatory patterns on documents
//! of varying sizes, plus Levenshtein distance computation.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use touring_intelligence::ann::keyword_matcher::{
    ANTT_PATTERNS, KeywordMatcher, MatcherConfig, TECHNICAL_KEYWORDS, levenshtein_distance,
};

/// Generate a realistic ANTT regulatory document excerpt of approximately `target_len` bytes.
fn gen_document(target_len: usize) -> String {
    let paragraphs = [
        "A Resolucao ANTT nr 5.950/2021 estabelece os parametros para reequilibrio \
         economico-financeiro dos contratos de concessao de rodovias federais. ",
        "O VPL calculado com TIR de 10% e WACC de 8% demonstra a viabilidade do projeto. \
         O fluxo de caixa projetado contempla investimentos em duplicacao e restauracao. ",
        "Conforme o Processo 50500.123456/2024-01, a concessionaria solicita revisao \
         tarifaria extraordinaria com base na Lei nr 10.233/2001. ",
        "A Nota Tecnica SUROC apresenta o fator D de desempenho calculado segundo \
         o programa de exploracao da rodovia (PER). ",
        "O Parecer PF-ANTT nr 123/2024 analisa a conformidade do Termo Aditivo com \
         o Acordao TCU nr 456/2023. O cronograma de obras foi atualizado. ",
        "A tarifa basica de pedagio sera reajustada conforme deliberacao da diretoria \
         colegiada, observando o anexo 5 do contrato de concessao. ",
    ];

    let mut doc = String::with_capacity(target_len + 200);
    let mut idx = 0;
    while doc.len() < target_len {
        doc.push_str(paragraphs[idx % paragraphs.len()]);
        idx += 1;
    }
    doc.truncate(target_len);
    doc
}

fn bench_antt_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("antt_patterns");

    for doc_size in [1_000, 10_000, 100_000] {
        let doc = gen_document(doc_size);
        group.bench_with_input(
            BenchmarkId::new("find_matches", doc_size),
            &doc,
            |bencher, doc| {
                bencher.iter(|| ANTT_PATTERNS.find_matches(black_box(doc)));
            },
        );
    }

    group.finish();
}

fn bench_technical_keywords(c: &mut Criterion) {
    let mut group = c.benchmark_group("technical_keywords");

    for doc_size in [1_000, 10_000, 100_000] {
        let doc = gen_document(doc_size);
        group.bench_with_input(
            BenchmarkId::new("find_matches", doc_size),
            &doc,
            |bencher, doc| {
                bencher.iter(|| TECHNICAL_KEYWORDS.find_matches(black_box(doc)));
            },
        );
    }

    group.finish();
}

fn bench_custom_matcher(c: &mut Criterion) {
    let patterns: Vec<String> = (0..50).map(|i| format!("pattern_{}", i)).collect();
    let matcher = KeywordMatcher::new(patterns, MatcherConfig::default());
    let text = "This text contains pattern_7 and pattern_23 and pattern_42 somewhere.";

    c.bench_function("custom_50_patterns", |b| {
        b.iter(|| matcher.find_matches(black_box(text)));
    });
}

fn bench_matcher_build(c: &mut Criterion) {
    let patterns: Vec<String> = (0..100).map(|i| format!("keyword_{}", i)).collect();

    c.bench_function("build_100_patterns", |b| {
        // Clear cache to force rebuild each time
        touring_intelligence::ann::clear_cache();
        b.iter(|| KeywordMatcher::new(black_box(patterns.clone()), MatcherConfig::default()));
    });
}

fn bench_levenshtein(c: &mut Criterion) {
    let mut group = c.benchmark_group("levenshtein");

    group.bench_function("short_5char", |b| {
        b.iter(|| levenshtein_distance(black_box("hello"), black_box("hallo")));
    });

    group.bench_function("medium_20char", |b| {
        b.iter(|| {
            levenshtein_distance(
                black_box("reequilibrio financ"),
                black_box("reequilibrio finance"),
            )
        });
    });

    let long_a: String = "abcdefghij".repeat(10);
    let long_b: String = "abcdefghik".repeat(10);
    group.bench_function("long_100char", |b| {
        b.iter(|| levenshtein_distance(black_box(&long_a), black_box(&long_b)));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_antt_patterns,
    bench_technical_keywords,
    bench_custom_matcher,
    bench_matcher_build,
    bench_levenshtein,
);
criterion_main!(benches);
