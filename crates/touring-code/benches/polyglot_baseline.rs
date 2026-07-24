//! polyglot_baseline.rs — S-2 of Wave 5 CEG Pln2 Final Closure
//!
//! Criterion bench corpus for touring-code::polyglot::{search, rewrite} that
//! captures P50/P99 latency baselines BEFORE the ast-grep 0.36 -> 0.42.x +
//! tree-sitter 0.24 -> 0.26.x grammar bump, then is re-run AFTER (S-11) to
//! validate that the bump does not introduce > 10% regression.
//!
//! Run with:
//!   cargo bench -p touring-code --bench polyglot_baseline
//!   cargo bench -p touring-code --bench polyglot_baseline -- --noplot
//!   cargo bench -p touring-code --bench polyglot_baseline -- --test     // smoke
//!
//! Output: target/criterion/polyglot_search/<lang>/...
//!         target/criterion/polyglot_rewrite/<lang>/...
//!
//! API signatures (verified via VGP `grep -n "pub fn (search|rewrite)"` on 2026-05-23):
//!   pub fn search(lang: Lang, source: &str, pattern: &str) -> Result<Vec<Match>>
//!   pub fn rewrite(lang: Lang, source: &str, pattern: &str, replacement: &str) -> Result<String>

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

use touring_code::polyglot::{Lang, rewrite, search};

// Fixture sources — small but representative.

const JS_SOURCE: &str = "function greet(name) { console.log('hi', name); }\nfunction farewell(name) { console.log('bye', name); }\n";
const JS_PATTERN: &str = "console.log($$$ARGS)";

const TS_SOURCE: &str = "interface User { id: number; name: string; }\nfunction lookup(id: number): User | undefined { return undefined; }\n";
const TS_PATTERN: &str = "function $NAME($$$PARAMS): $RET { $$$BODY }";

const PY_SOURCE: &str = "import os\ndef main():\n    print('hello world')\n    print('done')\n\nif __name__ == '__main__':\n    main()\n";
const PY_PATTERN: &str = "print($MSG)";
const PY_REPLACEMENT: &str = "logger.info($MSG)";

const GO_SOURCE: &str = "package main\n\nimport \"fmt\"\n\nfunc main() {\n    fmt.Println(\"hello\")\n    fmt.Println(\"world\")\n}\n";
const GO_PATTERN: &str = "fmt.Println($ARG)";
const GO_REPLACEMENT: &str = "log.Info($ARG)";

const RUST_SOURCE: &str =
    "fn add(a: i32, b: i32) -> i32 { a + b }\nfn mul(a: i32, b: i32) -> i32 { a * b }\n";
const RUST_PATTERN: &str = "fn $NAME($$$PARAMS) -> $RET { $$$BODY }";

fn bench_polyglot_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("polyglot_search");

    let cases: &[(Lang, &str, &str, &str)] = &[
        (Lang::JavaScript, "js", JS_SOURCE, JS_PATTERN),
        (Lang::TypeScript, "ts", TS_SOURCE, TS_PATTERN),
        (Lang::Python, "py", PY_SOURCE, PY_PATTERN),
        (Lang::Go, "go", GO_SOURCE, GO_PATTERN),
        (Lang::Rust, "rs", RUST_SOURCE, RUST_PATTERN),
    ];

    for (lang, label, src, pat) in cases {
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(label),
            &(*lang, *src, *pat),
            |b, (lang, src, pat)| {
                b.iter(|| {
                    let _ = black_box(search(black_box(*lang), black_box(src), black_box(pat)));
                });
            },
        );
    }

    group.finish();
}

fn bench_polyglot_rewrite(c: &mut Criterion) {
    let mut group = c.benchmark_group("polyglot_rewrite");

    let cases: &[(Lang, &str, &str, &str, &str)] = &[
        (Lang::Python, "py", PY_SOURCE, PY_PATTERN, PY_REPLACEMENT),
        (Lang::Go, "go", GO_SOURCE, GO_PATTERN, GO_REPLACEMENT),
    ];

    for (lang, label, src, pat, repl) in cases {
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(label),
            &(*lang, *src, *pat, *repl),
            |b, (lang, src, pat, repl)| {
                b.iter(|| {
                    let _ = black_box(rewrite(
                        black_box(*lang),
                        black_box(src),
                        black_box(pat),
                        black_box(repl),
                    ));
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_polyglot_search, bench_polyglot_rewrite);
criterion_main!(benches);
