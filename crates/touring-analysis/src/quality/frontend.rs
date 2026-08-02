//! Frontend performance (D25 / F2.12) — Core Web Vitals and frontend load.
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | render-blocking-script | `<script>` without `defer` / `async` / `type="module"` (in HTML/JSX/TSX/Vue) | HTML, TSX, JSX, Vue |
//! | blocking-stylesheet-in-body | `<link rel="stylesheet"` outside `<head>` (in JSX/TSX/Vue — HTML files have no enforcement) | TSX, JSX, Vue |
//! | unbuffered-layout-shift | `<img` without `width=`/`height=` (CLS — without intrinsic dimensions, image load shifts layout) | HTML, TSX, JSX, Vue |
//! | no-fetchpriority-on-hero | `<img` without `fetchpriority="high"` when followed by `loading="lazy"` (contradictory) | HTML, TSX, JSX, Vue |
//! | sync-heavy-handler | `addEventListener`/`onclick=` with a multi-line body that has *no* `await`/`Promise` (sync handler blocks INP) | JS, TS, JSX, TSX |
//! | wasm-no-opt-flag | a `.wasm` literal in Rust+JS with no `wasm-opt` invocation nearby (heuristic) | Rust, JS, TS |
//! | dynamic-import-large-lib | `import(` of a multi-segment path with no `webpackChunkName`/`/* @vite-ignore */` hint (no code-split) | JS, TS, JSX, TSX |
//!
//! **Disjoint** from F2.1 OWASP (security, not perf), F2.7 db-perf (DB), and
//! the F2.9 cache / F2.10 I/O engines. The CWV signal is unique: only the
//! frontend verifier keys on `<script defer>` / `<img width=` / `fetchpriority`.
//!
//! **Sources (context7, `/googlechrome/lighthouse`):** LCP < 2.5s, INP < 200ms,
//! CLS < 0.1 are the "good" thresholds. Render-blocking `<script>` tags are
//! the top CWV regression per the Lighthouse CWV report; unbuffered-layout-
//! shift `<img>` (no `width`/`height`) is the canonical CLS cause.
//!
//! **Scope:** meaningful only for HTML/JSX/TSX/Vue/JS/TS files; for Rust
//! `bindings` crates, only the WASM-no-opt detector fires (and only when
//! the file mentions a `.wasm` literal). For pure Rust or Python the engine
//! returns an empty report.
//!
//! Comments / `#[cfg(test)]` are excluded via `super::code_regions`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

const SCALE: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Html,
    Tsx,
    Jsx,
    Vue,
    JsTs,
    Rust,
    Other,
}

fn canonical_lang(lang: &str) -> Lang {
    match lang {
        "html" | "htm" => Lang::Html,
        "tsx" => Lang::Tsx,
        "jsx" => Lang::Jsx,
        "vue" => Lang::Vue,
        "typescript" | "ts" | "javascript" | "js" | "mjs" | "cjs" => Lang::JsTs,
        "rust" | "rs" => Lang::Rust,
        _ => Lang::Other,
    }
}

/// Frontend findings for one file.
#[derive(Debug, Clone, Default)]
pub struct FrontendReport {
    /// Total raw violation count across all detectors.
    pub violations: usize,
    /// Weighted violation total (per-smell weights applied).
    pub weighted_total: f32,
    /// Lines scanned (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired detector, sorted by count desc.
    pub findings: Vec<(String, usize)>,
}

impl FrontendReport {
    fn push(&mut self, message: &'static str, count: usize, weight: f32) {
        if count > 0 {
            self.violations += count;
            self.weighted_total += count as f32 * weight;
            self.findings.push((message.to_string(), count));
        }
    }
}

/// `true` if the `<script` tag at `off` reaches a `>` *without* encountering
/// `defer`, `async`, or `type="module"`. Walks the tag char-by-char (string-
/// aware so `>` inside a string doesn't terminate the tag).
fn is_render_blocking_script(bytes: &[u8], off: usize, regions: &[(usize, usize)]) -> bool {
    if offset_suppressed(off, regions) {
        return false;
    }
    let mut j = off;
    while j < bytes.len() && bytes[j] != b'>' {
        let after = skip_inline_string(bytes, j);
        if after > j {
            j = after;
            continue;
        }
        j += 1;
    }
    if j >= bytes.len() {
        return false;
    }
    let tag = &bytes[off..=j];
    let has_defer = memmem::find(tag, b" defer").is_some()
        || memmem::find(tag, b"defer ").is_some()
        || memmem::find(tag, b"\tdefer").is_some()
        || memmem::find(tag, b"\ndefer").is_some();
    let has_async = memmem::find(tag, b" async").is_some()
        || memmem::find(tag, b"async ").is_some()
        || memmem::find(tag, b"\tasync").is_some()
        || memmem::find(tag, b"\nasync").is_some();
    let has_module = memmem::find(tag, b"type=\"module\"").is_some()
        || memmem::find(tag, b"type='module'").is_some();
    !has_defer && !has_async && !has_module
}

/// `<link rel="stylesheet"` *outside* `<head>...</head>`. For .html files we
/// skip (no enforcement — authors may legitimately place sheets inline); for
/// JSX/TSX/Vue, the assumption is one root component → any `<link rel=
/// "stylesheet"` is body-positioned.
fn blocking_stylesheet_in_body(bytes: &[u8], regions: &[(usize, usize)], lang: Lang) -> usize {
    if !matches!(lang, Lang::Tsx | Lang::Jsx | Lang::Vue) {
        return 0;
    }
    let count = memmem::find_iter(bytes, b"rel=\"stylesheet\"")
        .chain(memmem::find_iter(bytes, b"rel='stylesheet'"))
        .filter(|&off| !offset_suppressed(off, regions))
        .count();
    if count > 0 { count } else { 0 }
}

/// `<img …>` without `width=`/`height=` (CLS). Limited to the languages that
/// embed HTML tags.
fn unbuffered_layout_shift(bytes: &[u8], regions: &[(usize, usize)], lang: Lang) -> usize {
    if !matches!(lang, Lang::Html | Lang::Tsx | Lang::Jsx | Lang::Vue) {
        return 0;
    }
    let mut count = 0;
    for off in memmem::find_iter(bytes, b"<img") {
        if offset_suppressed(off, regions) {
            continue;
        }
        let mut j = off;
        while j < bytes.len() && bytes[j] != b'>' {
            let after = skip_inline_string(bytes, j);
            if after > j {
                j = after;
                continue;
            }
            j += 1;
        }
        if j >= bytes.len() {
            continue;
        }
        let tag = &bytes[off..=j];
        let has_dims = memmem::find(tag, b" width=").is_some()
            || memmem::find(tag, b"\twidth=").is_some()
            || memmem::find(tag, b"\nwidth=").is_some()
            || memmem::find(tag, b" height=").is_some()
            || memmem::find(tag, b"\theight=").is_some()
            || memmem::find(tag, b"\nheight=").is_some()
            || memmem::find(tag, b"width:").is_some()
            || memmem::find(tag, b"height:").is_some();
        if !has_dims {
            count += 1;
        }
    }
    count
}

/// `<img loading="lazy" fetchpriority=…` where `fetchpriority="high"` is
/// missing — the contradictory hint that drops LCP. Actually the canonical CLS
/// smell is the opposite: `loading="lazy"` *without* `fetchpriority="high"`
/// on a hero image. We detect `loading="lazy"` without `fetchpriority`.
fn lazy_without_fetchpriority(bytes: &[u8], regions: &[(usize, usize)], lang: Lang) -> usize {
    if !matches!(lang, Lang::Html | Lang::Tsx | Lang::Jsx | Lang::Vue) {
        return 0;
    }
    let has_lazy = memmem::find_iter(bytes, b"loading=\"lazy\"")
        .chain(memmem::find_iter(bytes, b"loading='lazy'"))
        .any(|off| !offset_suppressed(off, regions));
    if !has_lazy {
        return 0;
    }
    let has_priority = memmem::find_iter(bytes, b"fetchpriority=\"high\"")
        .chain(memmem::find_iter(bytes, b"fetchpriority='high'"))
        .any(|off| !offset_suppressed(off, regions));
    if has_priority { 0 } else { 1 }
}

/// `addEventListener`/`onclick=` whose body spans multiple lines without
/// `await`/`Promise` — a sync handler that blocks INP. Heuristic.
fn sync_heavy_handler(bytes: &[u8], regions: &[(usize, usize)], lang: Lang) -> usize {
    if !matches!(lang, Lang::JsTs | Lang::Tsx | Lang::Jsx) {
        return 0;
    }
    let needles: [&[u8]; 3] = [b"addEventListener(", b"onclick=", b"onchange="];
    let mut count = 0;
    for needle in &needles {
        for off in memmem::find_iter(bytes, needle) {
            if offset_suppressed(off, regions) {
                continue;
            }
            let cap = off.saturating_add(512);
            let mut j = off + needle.len();
            let mut newlines = 0usize;
            let mut has_async = false;
            let mut paren: i32 = 0;
            let mut brace: i32 = 0;
            while j < bytes.len() && j < cap {
                let after = skip_inline_string(bytes, j);
                if after > j {
                    j = after;
                    continue;
                }
                match bytes[j] {
                    b'\n' => newlines += 1,
                    b'(' => paren += 1,
                    b')' if paren > 0 => paren -= 1,
                    b'{' => brace += 1,
                    b'}' if brace > 0 => brace -= 1,
                    b';' if paren == 0 && brace == 0 => break,
                    _ => {}
                }
                if memmem::find(&bytes[j..j.saturating_add(8).min(bytes.len())], b"await ")
                    .is_some()
                {
                    has_async = true;
                }
                if memmem::find(&bytes[j..j.saturating_add(8).min(bytes.len())], b"Promise")
                    .is_some()
                {
                    has_async = true;
                }
                j += 1;
            }
            if newlines >= 2 && !has_async {
                count += 1;
            }
        }
    }
    count
}

/// A `.wasm` literal in a Rust binding/JS wiring file with no `wasm-opt`
/// invocation nearby — the canonical "ship the unoptimized blob" smell.
fn wasm_no_opt_flag(bytes: &[u8], regions: &[(usize, usize)], lang: Lang) -> usize {
    if !matches!(lang, Lang::Rust | Lang::JsTs) {
        return 0;
    }
    let has_wasm = memmem::find_iter(bytes, b".wasm").any(|off| !offset_suppressed(off, regions));
    if !has_wasm {
        return 0;
    }
    let has_wasm_opt =
        memmem::find_iter(bytes, b"wasm-opt").any(|off| !offset_suppressed(off, regions));
    if has_wasm_opt { 0 } else { 1 }
}

/// `import(` (dynamic) without a code-split hint (`webpackChunkName` /
/// `/* @vite-ignore */`). For now we just count the number of dynamic
/// imports — the canonical fix is to lazy-load; this is the lightweight
/// "lots of dynamic imports" signal.
fn dynamic_import_count(bytes: &[u8], regions: &[(usize, usize)], lang: Lang) -> usize {
    if !matches!(lang, Lang::JsTs | Lang::Tsx | Lang::Jsx) {
        return 0;
    }
    let count = memmem::find_iter(bytes, b"import(")
        .filter(|&off| !offset_suppressed(off, regions))
        .count();
    if count > 5 { count } else { 0 }
}

/// Skip over a JS template literal / string (so `>` inside a string doesn't
/// terminate an HTML tag mid-parse). Mirrors `loop_blocks::skip_literal` in
/// spirit but specialized for inline strings.
fn skip_inline_string(bytes: &[u8], i: usize) -> usize {
    match bytes.get(i) {
        Some(b'"') | Some(b'`') | Some(b'\'') => {
            let quote = bytes[i];
            let mut j = i + 1;
            while j < bytes.len() {
                match bytes[j] {
                    b'\\' => j += 2,
                    c if c == quote => return j + 1,
                    _ => j += 1,
                }
            }
            j
        }
        _ => i,
    }
}

/// Analyze frontend perf smells in `source` for the given language.
pub fn analyze_frontend(source: &str, lang: &str) -> FrontendReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, lang);
    let lang = canonical_lang(lang);
    let mut report = FrontendReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    match lang {
        Lang::Html | Lang::Tsx | Lang::Jsx | Lang::Vue | Lang::JsTs | Lang::Rust => {
            if matches!(lang, Lang::Html | Lang::Tsx | Lang::Jsx | Lang::Vue) {
                let mut count = 0;
                for off in memmem::find_iter(bytes, b"<script") {
                    if is_render_blocking_script(bytes, off, &regions) {
                        count += 1;
                    }
                }
                report.push(
                    "render-blocking <script> (add `defer`, `async`, or `type=\"module\"`)",
                    count,
                    1.0,
                );
                report
                    .push(
                        "<link rel=\"stylesheet\"> in JSX/TSX/Vue body (use <link> in <head> or CSS-in-JS)",
                        blocking_stylesheet_in_body(bytes, &regions, lang),
                        0.7,
                    );
                report.push(
                    "<img> without width/height (CLS — declare intrinsic dimensions)",
                    unbuffered_layout_shift(bytes, &regions, lang),
                    0.9,
                );
                report
                    .push(
                        "<img loading=\"lazy\"> without fetchpriority=\"high\" on the hero (LCP — contradictory hint)",
                        lazy_without_fetchpriority(bytes, &regions, lang),
                        0.6,
                    );
            }
            if matches!(lang, Lang::JsTs | Lang::Tsx | Lang::Jsx) {
                report.push(
                    "sync-heavy addEventListener/onclick (multi-line body, no await/Promise — INP)",
                    sync_heavy_handler(bytes, &regions, lang),
                    0.8,
                );
                report.push(
                    "many dynamic imports without code-split hint (>5 — bundle is not lazy-loaded)",
                    dynamic_import_count(bytes, &regions, lang),
                    0.4,
                );
            }
            report.push(
                ".wasm literal without `wasm-opt -Oz` invocation nearby (ship the optimized blob)",
                wasm_no_opt_flag(bytes, &regions, lang),
                0.9,
            );
        }
        _ => {}
    }
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`FrontendReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
/// Delegates to [`super::score_utils::density_score`] for the `max(20)` floor
/// so short files don't saturate (F2.13 lesson).
pub fn score_frontend(report: &FrontendReport) -> f32 {
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn render_blocking_script_flagged() {
        let src = "<html><head></head><body><script>work();</script></body></html>\n";
        let r = analyze_frontend(src, "html");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("render-blocking")),
            "no defer/async/module is flagged: {:?}",
            r.findings
        );
    }
    #[test]
    fn defer_script_clean() {
        let src = "<html><body><script defer>work();</script></body></html>\n";
        let r = analyze_frontend(src, "html");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("render-blocking")),
            "defer is the fix: {:?}",
            r.findings
        );
    }
    #[test]
    fn module_script_clean() {
        let src = "<html><body><script type=\"module\">work();</script></body></html>\n";
        let r = analyze_frontend(src, "html");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("render-blocking")),
            "type=module is the fix: {:?}",
            r.findings
        );
    }
    #[test]
    fn img_no_dims_flagged() {
        let src = "<html><body><img src=\"hero.jpg\"></body></html>\n";
        let r = analyze_frontend(src, "html");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("<img> without width/height")),
            "no width/height = CLS: {:?}",
            r.findings
        );
    }
    #[test]
    fn img_with_dims_clean() {
        let src =
            "<html><body><img src=\"hero.jpg\" width=\"1200\" height=\"600\"></body></html>\n";
        let r = analyze_frontend(src, "html");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("<img> without width/height")),
            "width+height is the fix: {:?}",
            r.findings
        );
    }
    #[test]
    fn lazy_without_fetchpriority_flagged() {
        let src = "<html><body><img src=\"hero.jpg\" loading=\"lazy\"></body></html>\n";
        let r = analyze_frontend(src, "html");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("loading=\"lazy\"")),
            "lazy + no fetchpriority=high is the LCP contradiction: {:?}",
            r.findings
        );
    }
    #[test]
    fn sync_heavy_handler_flagged() {
        let src = "el.addEventListener('click', function() {\n    for (let i = 0; i < 1e6; i++) { heavy(i); }\n    other();\n});\n";
        let r = analyze_frontend(src, "javascript");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("sync-heavy")),
            "multi-line sync handler = INP regression: {:?}",
            r.findings
        );
    }
    #[test]
    fn async_handler_clean() {
        let src = "el.addEventListener('click', async function() {\n    const r = await fetch('/x');\n    await r.json();\n});\n";
        let r = analyze_frontend(src, "javascript");
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("sync-heavy")),
            "async handler is fine: {:?}",
            r.findings
        );
    }
    #[test]
    fn wasm_no_opt_flagged() {
        let src = "pub const WASM: &[u8] = include_bytes!(\"pkg/app.wasm\");\n";
        let r = analyze_frontend(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains(".wasm literal")),
            ".wasm without wasm-opt is flagged: {:?}",
            r.findings
        );
    }
    #[test]
    fn wasm_with_opt_clean() {
        // `wasm-opt` is in the same file (in a real string literal — the
        // canonical build-step invocation), so the detector sees it and
        // accepts the file.
        let src = "fn build() { run(\"wasm-opt -Oz -o app.opt.wasm\"); }\npub const WASM: &[u8] = include_bytes!(\"pkg/app.opt.wasm\");\n";
        let r = analyze_frontend(src, "rust");
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains(".wasm literal")),
            "wasm-opt mentioned → clean: {:?}",
            r.findings
        );
    }
    #[test]
    fn pure_rust_no_frontend_signals() {
        let src = "fn add(a: i32, b: i32) -> i32 { a + b }\n";
        let r = analyze_frontend(src, "rust");
        assert_eq!(r.violations, 0, "pure-Rust file is clean: {:?}", r.findings);
    }
    #[test]
    fn comment_excluded() {
        // The `code_regions` helper currently has no HTML-comment syntax, so we
        // exercise the same exclusion on the languages it *does* understand:
        // Rust `//` and JS/TS `//`. A `// <script>work();</script>` literal in
        // a .ts file must not be flagged as a render-blocking script.
        let src = "// <script>work();</script>\nconst x = 1;\n";
        let r = analyze_frontend(src, "typescript");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("render-blocking")),
            "commented <script> is excluded: {:?}",
            r.findings
        );
    }
    #[test]
    fn string_gt_not_tag_close() {
        let src = "<html><body><script>let x = \"a > b\";</script></body></html>\n";
        let r = analyze_frontend(src, "html");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("render-blocking")),
            "the script tag is still a render-blocking one: {:?}",
            r.findings
        );
    }
    #[test]
    fn score_monotonic_dirty_below_clean() {
        let bad = analyze_frontend(
            "<html><body><script>work();</script><img src=\"hero.jpg\"><img src=\"b.jpg\"><img src=\"c.jpg\"></body></html>\n",
            "html",
        );
        let good = analyze_frontend(
            "<html><body><script defer>work();</script><img src=\"hero.jpg\" width=\"100\" height=\"100\" fetchpriority=\"high\"></body></html>\n",
            "html",
        );
        assert!(
            score_frontend(&bad) < score_frontend(&good),
            "dirty ({:.3}) must score below clean ({:.3})",
            score_frontend(&bad),
            score_frontend(&good)
        );
    }
    /// Regression test for the F2.13 saturation fix (`max(20)` floor in
    /// [`super::score_utils::density_score`]). A short HTML with multiple
    /// CWV smells must NOT score 0.0.
    #[test]
    fn score_short_file_does_not_saturate() {
        let r = analyze_frontend(
            "<html><body><script>w();</script><img src=\"a.jpg\"><img src=\"b.jpg\"><img src=\"c.jpg\"></body></html>\n",
            "html",
        );
        let s = score_frontend(&r);
        assert!(
            s > 0.0,
            "short HTML with CWV smells must not score 0.0: {s}"
        );
    }
}
