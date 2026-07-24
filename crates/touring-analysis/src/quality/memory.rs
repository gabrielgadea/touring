//! Memory management (D21 / F2.8) — unbounded growth, refcount cycles, hot-path
//! clones.
//!
//! D21 asks whether memory is bounded, leak-free, and not needlessly copied.
//! Rust's ownership eliminates use-after-free, but not three real smells:
//! **unbounded growth** (an `unbounded_channel()` / `unbounded()` cache with no
//! capacity — tokio's own docs warn it "has the ability of causing the process to
//! run out of memory"), **leaks** (`Box::leak` / `mem::forget` / `.leak()` skip
//! `Drop`), **refcount cycles** (a `parent`/`prev`/`owner` back-reference held as
//! a strong `Rc`/`Arc` — or C++ `shared_ptr` — instead of `Weak`, so the count
//! never reaches zero), and **hot-path allocation** (an allocating clone
//! `.to_vec()`/`.to_owned()` inside a loop). The idiomatic *good* forms (a bounded
//! `channel(N)`, RAII, a `Weak` back-reference, borrowing) cannot be proven
//! present, so the engine scores the anti-patterns.
//!
//! | Detector | Signal | Idiomatic fix |
//! |----------|--------|---------------|
//! | unbounded growth | `unbounded_channel(`/`unbounded(` (Rust), `maxsize=None` (Python lru_cache) | a bounded `channel(N)` / `maxsize=N` |
//! | leak | `Box::leak(`/`mem::forget(`/`.leak(` (Rust) | own the value / `ManuallyDrop` / an arena |
//! | refcount cycle | `parent`/`prev`/`owner`/`root`/`back` : `Rc<`/`Arc<` (Rust) or `shared_ptr<` (C++) | a `Weak<`/`weak_ptr` back-reference |
//! | hot-path clone | `.to_vec()`/`.to_owned()` inside a loop (Rust) | hoist / borrow / reuse a buffer |
//!
//! It is **disjoint from F1.11 design-patterns** (which flags `Rc<RefCell<` as an
//! ownership *pattern* smell, and `Cloneable`): F2.8 keys on the *back-reference
//! field name* + a strong refcount (a different needle — `parent: Rc<Node>` with
//! no `RefCell` is a cycle risk F1.11 does not see), and on unbounded/leak/alloc
//! which F1.11 does not touch. `mem::forget` lives in the legacy `antipatterns`
//! pipeline (not wired to any 50-dim verifier), so F2.8 is its dim-level owner.
//! F2.8 is heaviest on Rust / C++ (manual refcounting + explicit leaks); GC
//! languages (TS/JS/Go/Java) manage memory automatically, so the detectable
//! manual-memory surface is small — the engine covers the language-specific
//! signals it can (Python's unbounded `lru_cache`). Comments and
//! `#[cfg(test)]`/test regions are excluded via [`super::code_regions`]. Rolls up
//! as `AggKind::WeightedLoc`. ADVISORY-tier. Zero non-std deps beyond `memchr`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};
use super::loop_blocks::loop_bodies;

/// Canonical language bucket (collapses extension aliases).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    Python,
    Cpp,
    Other,
}

fn canonical_lang(lang: &str) -> Lang {
    match lang {
        "rust" | "rs" => Lang::Rust,
        "python" | "py" => Lang::Python,
        "cpp" | "c++" | "cc" | "cxx" | "c" | "h" | "hpp" => Lang::Cpp,
        _ => Lang::Other,
    }
}

/// Back-reference field names that should be `Weak`, never a strong `Rc`/`Arc`/
/// `shared_ptr` (a strong back-reference is the canonical refcount cycle).
const BACKREF_WORDS: &[&[u8]] = &[b"parent", b"prev", b"previous", b"owner", b"back", b"root"];

#[inline]
fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

#[inline]
fn ident_at(bytes: &[u8], idx: usize) -> bool {
    bytes.get(idx).is_some_and(|&c| is_ident(c))
}

fn is_backref(w: &[u8]) -> bool {
    BACKREF_WORDS.contains(&w)
}

/// The identifier to the left of `start`, skipping whitespace and `:` separators
/// (so `parent: Rc<…>` yields `parent`). `None` if no identifier precedes.
fn ident_before(bytes: &[u8], start: usize) -> Option<&[u8]> {
    let mut i = start;
    while i > 0 {
        let c = bytes[i - 1];
        if c == b' ' || c == b'\t' || c == b':' {
            i -= 1;
        } else {
            break;
        }
    }
    let end = i;
    while i > 0 && is_ident(bytes[i - 1]) {
        i -= 1;
    }
    if i < end { Some(&bytes[i..end]) } else { None }
}

/// Count `needle` occurrences in non-suppressed positions.
fn count_plain(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> usize {
    memmem::find_iter(bytes, needle)
        .filter(|&off| !offset_suppressed(off, regions))
        .count()
}

/// Count `needle` occurrences whose *preceding* char is a word boundary (so
/// `unbounded(` ≠ `is_unbounded(`).
fn count_word_before(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> usize {
    memmem::find_iter(bytes, needle)
        .filter(|&off| !offset_suppressed(off, regions))
        .filter(|&off| !ident_at(bytes, off.wrapping_sub(1)))
        .count()
}

/// Detector 1 — unbounded growth: unbounded channels / caches with no capacity.
fn unbounded_count(bytes: &[u8], regions: &[(usize, usize)], lang: Lang) -> usize {
    match lang {
        Lang::Rust => {
            count_word_before(bytes, regions, b"unbounded_channel(")
                + count_word_before(bytes, regions, b"unbounded(")
        }
        // Python `functools.lru_cache(maxsize=None)` is an unbounded cache.
        Lang::Python => count_plain(bytes, regions, b"maxsize=None"),
        _ => 0,
    }
}

/// Detector 2 — leak: `Box::leak` / `mem::forget` / `.leak()` skip `Drop`.
fn leak_count(bytes: &[u8], regions: &[(usize, usize)], lang: Lang) -> usize {
    if lang != Lang::Rust {
        return 0;
    }
    count_plain(bytes, regions, b"Box::leak(")
        + count_plain(bytes, regions, b"mem::forget(")
        + count_plain(bytes, regions, b".leak(")
}

/// Detector 3 (Rust) — a `BACKREF_WORDS` field typed as a strong `Rc<`/`Arc<`.
/// `Weak<` does not match the strong token, so the fix is not flagged; the type
/// token must be a whole word (so `MyArc<` is excluded) and the adjacent name an
/// exact back-reference word (so `child: Rc<…>` / `data: Arc<…>` are not).
fn rust_cycle(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let mut n = 0;
    for tok in [b"Rc<".as_slice(), b"Arc<".as_slice()] {
        for off in memmem::find_iter(bytes, tok) {
            if offset_suppressed(off, regions) {
                continue;
            }
            if ident_at(bytes, off.wrapping_sub(1)) {
                continue; // standalone token (not `MyArc<`)
            }
            if ident_before(bytes, off).is_some_and(is_backref) {
                n += 1;
            }
        }
    }
    n
}

/// `true` if `line` contains a whole-word back-reference identifier.
fn line_has_backref(line: &[u8]) -> bool {
    for w in BACKREF_WORDS {
        for off in memmem::find_iter(line, w) {
            let before = off == 0 || !is_ident(line[off - 1]);
            let after = !ident_at(line, off + w.len());
            if before && after {
                return true;
            }
        }
    }
    false
}

/// Detector 3 (C++) — a `shared_ptr<` on a line with a back-reference name and no
/// `weak_ptr` (C++ is type-first, so a line-level proximity check handles the
/// `std::shared_ptr<Node> parent;` form without parsing the generic args).
fn cpp_cycle(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let mut n = 0;
    for off in memmem::find_iter(bytes, b"shared_ptr<") {
        if offset_suppressed(off, regions) {
            continue;
        }
        let ls = bytes[..off]
            .iter()
            .rposition(|&c| c == b'\n')
            .map_or(0, |p| p + 1);
        let mut le = off;
        while le < bytes.len() && bytes[le] != b'\n' {
            le += 1;
        }
        let line = &bytes[ls..le];
        if memmem::find(line, b"weak_ptr").is_some() {
            continue; // already a weak back-reference
        }
        if line_has_backref(line) {
            n += 1;
        }
    }
    n
}

fn cycle_count(bytes: &[u8], regions: &[(usize, usize)], lang: Lang) -> usize {
    match lang {
        Lang::Rust => rust_cycle(bytes, regions),
        Lang::Cpp => cpp_cycle(bytes, regions),
        _ => 0,
    }
}

/// Detector 4 — hot-path allocation: an allocating clone (`.to_vec()`/
/// `.to_owned()`) inside a loop body. Restricted to Rust, where these are
/// unambiguous deep copies (bare `.clone()` is excluded — in an `Arc`-heavy
/// codebase it is dominated by cheap refcount bumps).
fn hot_path_alloc(bytes: &[u8], regions: &[(usize, usize)], lang: Lang) -> usize {
    if lang != Lang::Rust {
        return 0;
    }
    loop_bodies(bytes, regions, "rust")
        .into_iter()
        .filter(|&(s, e)| body_has_alloc(bytes, regions, s, e))
        .count()
}

fn body_has_alloc(bytes: &[u8], regions: &[(usize, usize)], s: usize, e: usize) -> bool {
    let body = &bytes[s..e];
    for tok in [b".to_vec(".as_slice(), b".to_owned(".as_slice()] {
        for off in memmem::find_iter(body, tok) {
            if !offset_suppressed(s + off, regions) {
                return true;
            }
        }
    }
    false
}

/// Per-file memory-management analysis (parallel shape to [`super::idioms::IdiomReport`]).
#[derive(Debug, Clone, Default)]
pub struct MemoryMgmtReport {
    /// Total memory-management anti-patterns found in production code.
    pub violations: usize,
    /// Weighted sum (each anti-pattern scaled by its category weight).
    pub weighted_total: f32,
    /// Production lines considered (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired category, for evidence (highest count first).
    pub findings: Vec<(String, usize)>,
}

impl MemoryMgmtReport {
    /// Record `count` occurrences of one anti-pattern category with `weight`.
    /// A zero count is a no-op.
    fn push(&mut self, message: &'static str, count: usize, weight: f32) {
        if count == 0 {
            return;
        }
        self.violations += count;
        self.weighted_total += weight * count as f32;
        self.findings.push((message.to_string(), count));
    }
}

/// Analyze memory-management anti-patterns for `lang`. Unknown / GC languages
/// yield an empty report (no detectable manual-memory smell → score 1.0).
#[must_use]
pub fn analyze_memory_mgmt(source: &str, lang: &str) -> MemoryMgmtReport {
    let regions = non_executable_regions(source, lang);
    let bytes = source.as_bytes();
    let canon = canonical_lang(lang);
    let mut report = MemoryMgmtReport {
        total_lines: source.lines().count(),
        ..MemoryMgmtReport::default()
    };

    report.push(
        "unbounded growth (`unbounded_channel(`/`unbounded(`/`maxsize=None`) -> a bounded `channel(N)` / `maxsize=N`",
        unbounded_count(bytes, &regions, canon),
        1.0,
    );
    report.push(
        "intentional leak (`Box::leak`/`mem::forget`/`.leak()` skips Drop) -> own the value / `ManuallyDrop` / an arena",
        leak_count(bytes, &regions, canon),
        0.8,
    );
    report.push(
        "strong-`Rc`/`Arc`/`shared_ptr` back-reference (refcount cycle leak) -> a `Weak`/`weak_ptr` back-reference",
        cycle_count(bytes, &regions, canon),
        1.0,
    );
    report.push(
        "allocating clone (`.to_vec()`/`.to_owned()`) inside a loop (hot-path) -> hoist / borrow / reuse a buffer",
        hot_path_alloc(bytes, &regions, canon),
        0.4,
    );

    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// D21 memory-management score: `1 - density * SCALE`, where density is the
/// weighted anti-pattern count per production line. A bounded, leak-free,
/// cycle-free file is `1.0`. SCALE is the style-tier `6.0` (ADVISORY);
/// `WeightedLoc` roll-up.
#[must_use]
pub fn score_memory_mgmt(report: &MemoryMgmtReport) -> f32 {
    const SCALE: f32 = 6.0;
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Unbounded growth ───────────────────────────────────────────────────────
    #[test]
    fn rust_unbounded_channel_flagged_bounded_clean() {
        let bad = analyze_memory_mgmt("let (tx, rx) = mpsc::unbounded_channel();\n", "rust");
        assert!(
            bad.violations >= 1,
            "unbounded_channel must flag: {:?}",
            bad.findings
        );
        let good = analyze_memory_mgmt("let (tx, rx) = mpsc::channel(64);\n", "rust");
        assert_eq!(
            good.violations, 0,
            "bounded channel(N) is clean: {:?}",
            good.findings
        );
        assert!((score_memory_mgmt(&good) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rust_unbounded_cache_flagged_word_boundary() {
        // The real workspace pattern: an "LRU" with no bound.
        let bad = analyze_memory_mgmt("let cache = LruCache::unbounded();\n", "rust");
        assert!(
            bad.violations >= 1,
            "LruCache::unbounded() must flag: {:?}",
            bad.findings
        );
        // `is_unbounded(` must NOT match (word boundary before).
        let good = analyze_memory_mgmt("if q.is_unbounded() { warn(); }\n", "rust");
        assert_eq!(
            good.violations, 0,
            "is_unbounded() is a predicate, not a constructor: {:?}",
            good.findings
        );
    }

    // ── Leak ───────────────────────────────────────────────────────────────────
    #[test]
    fn rust_leak_primitives_flagged() {
        let bad = analyze_memory_mgmt(
            "let s: &'static str = Box::leak(boxed);\nstd::mem::forget(guard);\nlet p = v.leak();\n",
            "rust",
        );
        assert!(
            bad.violations >= 3,
            "Box::leak + mem::forget + .leak must flag: {:?}",
            bad.findings
        );
    }

    // ── Refcount cycle ─────────────────────────────────────────────────────────
    #[test]
    fn rust_strong_backref_flagged_weak_and_nonbackref_clean() {
        let bad = analyze_memory_mgmt(
            "struct Node {\n    parent: Rc<Node>,\n    owner: Arc<Graph>,\n}\n",
            "rust",
        );
        assert!(
            bad.violations >= 2,
            "parent/owner strong Rc/Arc must flag: {:?}",
            bad.findings
        );
        // `Weak` back-reference (the fix) and a non-backref field are clean.
        let good = analyze_memory_mgmt(
            "struct Node {\n    parent: Weak<Node>,\n    children: Vec<Rc<Node>>,\n    data: Arc<Mutex<T>>,\n}\n",
            "rust",
        );
        assert_eq!(
            good.violations, 0,
            "Weak parent + non-backref Rc/Arc clean: {:?}",
            good.findings
        );
    }

    // ── Hot-path allocation ────────────────────────────────────────────────────
    #[test]
    fn rust_alloc_in_loop_flagged_outside_clean() {
        let bad = analyze_memory_mgmt("for x in xs {\n    sink(x.to_vec());\n}\n", "rust");
        assert!(
            bad.violations >= 1,
            "to_vec in a loop must flag: {:?}",
            bad.findings
        );
        let good = analyze_memory_mgmt("let owned = slice.to_vec();\nuse_it(owned);\n", "rust");
        assert_eq!(
            good.violations, 0,
            "a single to_vec outside a loop is fine: {:?}",
            good.findings
        );
    }

    // ── Python ─────────────────────────────────────────────────────────────────
    #[test]
    fn python_unbounded_lru_cache_flagged_bounded_clean() {
        let bad = analyze_memory_mgmt(
            "@lru_cache(maxsize=None)\ndef f(x):\n    return x\n",
            "python",
        );
        assert!(
            bad.violations >= 1,
            "lru_cache(maxsize=None) must flag: {:?}",
            bad.findings
        );
        let good = analyze_memory_mgmt(
            "@lru_cache(maxsize=128)\ndef f(x):\n    return x\n",
            "python",
        );
        assert_eq!(
            good.violations, 0,
            "a bounded maxsize is clean: {:?}",
            good.findings
        );
    }

    // ── C++ ────────────────────────────────────────────────────────────────────
    #[test]
    fn cpp_shared_ptr_backref_flagged_weak_clean() {
        let bad = analyze_memory_mgmt(
            "struct Node {\n    std::shared_ptr<Node> parent;\n};\n",
            "cpp",
        );
        assert!(
            bad.violations >= 1,
            "shared_ptr parent must flag: {:?}",
            bad.findings
        );
        let good = analyze_memory_mgmt(
            "struct Node {\n    std::weak_ptr<Node> parent;\n    std::shared_ptr<Node> child;\n};\n",
            "cpp",
        );
        assert_eq!(
            good.violations, 0,
            "weak_ptr parent + shared_ptr child clean: {:?}",
            good.findings
        );
    }

    // ── Cross-cutting ──────────────────────────────────────────────────────────
    #[test]
    fn comments_and_tests_excluded() {
        let src = "// parent: Rc<Node> and Box::leak() and unbounded_channel() in docs\nfn prod() -> bool { true }\n#[cfg(test)]\nmod tests {\n    fn t() { let _ = mpsc::unbounded_channel(); }\n}\n";
        let r = analyze_memory_mgmt(src, "rust");
        assert_eq!(
            r.violations, 0,
            "comment/test smells excluded: {:?}",
            r.findings
        );
    }

    #[test]
    fn gc_language_is_empty() {
        let r = analyze_memory_mgmt(
            "const m = new Map(); for (const x of xs) { m.set(x, x); }",
            "typescript",
        );
        assert_eq!(
            r.violations, 0,
            "GC language has no manual-memory smell here: {:?}",
            r.findings
        );
        assert!((score_memory_mgmt(&r) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn score_is_monotone_in_density() {
        let mk = |w: f32| MemoryMgmtReport {
            weighted_total: w,
            total_lines: 100,
            ..Default::default()
        };
        let mut prev = 2.0f32;
        for w in [0.0, 1.0, 3.0, 6.0, 12.0] {
            let s = score_memory_mgmt(&mk(w));
            assert!(s <= prev, "more memory debt must not raise the score");
            prev = s;
        }
    }
}
