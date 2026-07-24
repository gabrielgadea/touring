//! E2E tests proving AhoCorasick-backed reranker functionality (T0.1).
//!
//! Verifies that:
//! 1. `get_authority()` returns correct scores for all ANTT doc categories
//!    via `ANTT_PATTERNS` AhoCorasick (not sequential contains).
//! 2. `compute_keyword_match()` counts keywords correctly with real content.
//! 3. Full ranking pipeline orders documents correctly by authority.

use touring_intelligence::ann::reranker::{ContextualReranker, RerankContext, SearchResult};

// ── helpers ──────────────────────────────────────────────────────────────────

fn reranker() -> ContextualReranker {
    ContextualReranker::new()
}

fn ctx_no_keywords() -> RerankContext {
    RerankContext::default()
}

fn ctx_with_keywords(kws: Vec<&str>) -> RerankContext {
    let mut ctx = RerankContext::default();
    ctx.keywords = kws.into_iter().map(|s| s.to_string()).collect();
    ctx
}

// ── T0.1-A: get_authority via ANTT_PATTERNS AhoCorasick ───────────────────

/// Lei patterns must return authority 1.0 (highest tier).
/// Verifies `ANTT_PATTERNS.find_matches(doc_type)` triggers "Law" category.
#[test]
fn test_get_authority_law_patterns() {
    let r = reranker();
    let ctx = ctx_no_keywords();

    for doc_type in &[
        "Lei nº 10.233/2001",
        "Lei Federal 10.233",
        "Lei n° 8.987/1995",
    ] {
        let result = SearchResult::new("d", 0.5, "content").with_type(doc_type);
        let ranked = r
            .rerank(&[result], &ctx)
            .expect("rerank must succeed for law doc_type");
        assert_eq!(
            ranked[0].ranking_factors.document_authority, 1.0,
            "doc_type={doc_type:?} must yield authority 1.0 (Law)"
        );
    }
}

/// Decreto nº pattern must return authority 0.95.
#[test]
fn test_get_authority_decree_patterns() {
    let r = reranker();
    let ctx = ctx_no_keywords();

    let result = SearchResult::new("d", 0.5, "content").with_type("Decreto nº 2.521/1998");
    let ranked = r.rerank(&[result], &ctx).expect("rerank must succeed");
    assert_eq!(
        ranked[0].ranking_factors.document_authority, 0.95,
        "Decreto nº → Decree category → authority 0.95"
    );
}

/// Resolução ANTT pattern must return authority 0.90.
#[test]
fn test_get_authority_resolution_patterns() {
    let r = reranker();
    let ctx = ctx_no_keywords();

    let result = SearchResult::new("d", 0.5, "content").with_type("Resolução ANTT nº 5.950");
    let ranked = r.rerank(&[result], &ctx).expect("rerank must succeed");
    assert_eq!(
        ranked[0].ranking_factors.document_authority, 0.90,
        "Resolução ANTT → Resolution category → authority 0.90"
    );
}

/// Bare doc_type strings (lowercase, no regulatory prefix) fall through to
/// the contains() fallback — must still return correct authority.
#[test]
fn test_get_authority_bare_doc_types_fallback() {
    let r = reranker();
    let ctx = ctx_no_keywords();

    let cases: &[(&str, f64)] = &[
        ("lei", 1.0),
        ("decreto", 0.95),
        ("despacho", 0.70),
        ("unknown random type", 0.50),
    ];

    for &(doc_type, expected) in cases {
        let result = SearchResult::new("d", 0.5, "content").with_type(doc_type);
        let ranked = r.rerank(&[result], &ctx).expect("rerank must succeed");
        let got = ranked[0].ranking_factors.document_authority;
        assert!(
            (got - expected).abs() < 1e-9,
            "doc_type={doc_type:?}: expected {expected}, got {got}"
        );
    }
}

/// Unknown doc_type must return the `outros` fallback 0.50.
#[test]
fn test_get_authority_unknown_returns_outros() {
    let r = reranker();
    let ctx = ctx_no_keywords();

    let result = SearchResult::new("d", 0.5, "x").with_type("Processo interno XYZ");
    let ranked = r.rerank(&[result], &ctx).expect("rerank must succeed");
    assert_eq!(
        ranked[0].ranking_factors.document_authority, 0.50,
        "Unknown doc_type → outros authority 0.50"
    );
}

// ── T0.1-B: compute_keyword_match via AhoCorasick ─────────────────────────

/// With explicit caller keywords, content containing all keywords → score 1.0.
#[test]
fn test_compute_keyword_match_all_keywords_present_returns_one() {
    let r = reranker();
    let content = "the document discusses tarifa and pedágio and outorga in detail";
    let ctx = ctx_with_keywords(vec!["tarifa", "pedágio", "outorga"]);

    let result = SearchResult::new("d", 0.5, content).with_type("lei");
    let ranked = r.rerank(&[result], &ctx).expect("rerank must succeed");
    assert!(
        (ranked[0].ranking_factors.keyword_match - 1.0).abs() < 1e-9,
        "all 3 keywords present → score must be 1.0, got {}",
        ranked[0].ranking_factors.keyword_match
    );
}

/// Content missing all keywords → score 0.0.
#[test]
fn test_compute_keyword_match_no_keywords_present_returns_zero() {
    let r = reranker();
    let content = "irrelevant content about something completely different";
    let ctx = ctx_with_keywords(vec!["tarifa", "pedágio", "outorga"]);

    let result = SearchResult::new("d", 0.5, content).with_type("lei");
    let ranked = r.rerank(&[result], &ctx).expect("rerank must succeed");
    assert_eq!(
        ranked[0].ranking_factors.keyword_match, 0.0,
        "none of the keywords present → score must be 0.0"
    );
}

/// Only 1 of 3 keywords present → fractional score 1/3.
#[test]
fn test_compute_keyword_match_partial_match_fractional_score() {
    let r = reranker();
    let content = "this document discusses tarifa only";
    let ctx = ctx_with_keywords(vec!["tarifa", "pedágio", "outorga"]);

    let result = SearchResult::new("d", 0.5, content).with_type("lei");
    let ranked = r.rerank(&[result], &ctx).expect("rerank must succeed");
    let score = ranked[0].ranking_factors.keyword_match;
    assert!(
        (score - 1.0 / 3.0).abs() < 1e-9,
        "1/3 keywords present → score ~0.333, got {score}"
    );
}

// ── T0.1-C: End-to-end ranking order by authority ─────────────────────────

/// Documents with higher authority categories must rank above lower authority
/// ones when all other factors (semantic score, date) are equal.
#[test]
fn test_reranker_e2e_ranking_order_by_authority() {
    let r = reranker();
    let ctx = ctx_no_keywords();

    let results = vec![
        // equal semantic score, different doc_type authority
        SearchResult::new("outros", 0.80, "content").with_type("other"),
        SearchResult::new("lei", 0.80, "content").with_type("Lei nº 10.233/2001"),
        SearchResult::new("nota", 0.80, "content").with_type("Nota Técnica nº 5/2024"),
        SearchResult::new("decreto", 0.80, "content").with_type("Decreto nº 2.521/1998"),
    ];

    let ranked = r
        .rerank(&results, &ctx)
        .expect("rerank must succeed on 4 documents");

    // Authority order: Law(1.0) > Decree(0.95) > TechnicalNote(0.75) > outros(0.50)
    let ids: Vec<&str> = ranked.iter().map(|rr| rr.document_id.as_str()).collect();

    let pos_lei = ids
        .iter()
        .position(|&id| id == "lei")
        .expect("lei must be in ranked output");
    let pos_decreto = ids
        .iter()
        .position(|&id| id == "decreto")
        .expect("decreto must be in ranked output");
    let pos_nota = ids
        .iter()
        .position(|&id| id == "nota")
        .expect("nota must be in ranked output");
    let pos_outros = ids
        .iter()
        .position(|&id| id == "outros")
        .expect("outros must be in ranked output");

    assert!(
        pos_lei < pos_decreto,
        "Lei must rank above Decreto (got pos {} vs {})",
        pos_lei,
        pos_decreto
    );
    assert!(
        pos_decreto < pos_nota,
        "Decreto must rank above Nota Técnica (got pos {} vs {})",
        pos_decreto,
        pos_nota
    );
    assert!(
        pos_nota < pos_outros,
        "Nota Técnica must rank above outros (got pos {} vs {})",
        pos_nota,
        pos_outros
    );
}

/// rerank() on empty input must return EmptyResults error (not panic).
#[test]
fn test_reranker_empty_input_returns_error() {
    let r = reranker();
    let result = r.rerank(&[], &ctx_no_keywords());
    assert!(result.is_err(), "empty input must return Err(EmptyResults)");
}
