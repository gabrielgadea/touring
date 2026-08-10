//! Intent → prior art, with the anti-anchor contract.
//!
//! Ranking is BM25 ([`crate::text_rank`]) over the mined purpose prose, with a
//! field boost for terms that hit an artifact's name or keywords — the
//! hand-rolled analog of tantivy's `QueryParser::set_field_boost`
//! (Context7 `/websites/rs_tantivy_tantivy`, consulted 2026-08-08).
//!
//! The scoring floors below are **policy**, not measurement: they decide how
//! thin an answer is allowed to be. They are named and documented rather than
//! buried, and every answer reports `corpus_size` so a thin result reads as
//! thin instead of as authoritative.

use super::lexicon;
use super::store::PortfolioIndex;
use super::{CapabilityEntry, ExternalLens, PortfolioAnswer, ScoredCapability, Verdict};
use crate::text_rank::{Bm25Corpus, bm25_score_doc};

/// Multiplier applied to terms hitting an entry's name or keywords.
const FIELD_BOOST: f64 = 2.5;

/// A candidate must reach this fraction of the best score to be shown.
/// Relative rather than absolute so the floor scales with query length.
const RELATIVE_FLOOR: f64 = 0.35;

/// A candidate must match at least this fraction of the query's content terms.
///
/// This replaced an absolute score floor, which was wrong for a reason worth
/// recording: BM25 gives a term appearing in *every* document an IDF near zero,
/// so in a small corpus the single correct answer scored below any fixed
/// threshold and vanished. Term coverage is invariant to corpus size and to
/// term commonality — it asks "did this candidate actually address what I
/// asked?" rather than "did it clear an arbitrary number?".
const MIN_TERM_COVERAGE: f64 = 0.5;

/// How many distinct query terms a candidate must match to be shown at all.
fn required_matches(n_terms: usize) -> usize {
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let need = (n_terms as f64 * MIN_TERM_COVERAGE).ceil() as usize;
    need.max(1)
}

/// Number of distinct query terms present in a document.
fn matched_terms(doc: &[String], terms: &[String]) -> usize {
    terms.iter().filter(|t| doc.contains(t)).count()
}

/// Score multiplier for symbol-grained entries.
///
/// A function is supporting detail; the script/skill/ADW that contains it is
/// what a caller can actually run or extend. At comparable relevance the
/// artifact should come first, so symbols carry a mild penalty rather than
/// being excluded — they are exactly what answers "is there already a function
/// that does X?".
const SYMBOL_WEIGHT: f64 = 0.8;

/// Beyond this age a candidate is called out as possibly stale.
const STALE_DAYS: u64 = 365;

/// The tokens BM25 ranks for one entry: purpose + name + keywords + provenance.
fn entry_tokens(e: &CapabilityEntry) -> Vec<String> {
    let mut text = String::with_capacity(e.purpose.len() + 64);
    text.push_str(&e.purpose);
    text.push(' ');
    text.push_str(&e.name);
    text.push(' ');
    text.push_str(&e.provenance);
    for kw in &e.keywords {
        text.push(' ');
        text.push_str(kw);
    }
    lexicon::tokenize(&text)
}

/// True iff `term` hits the entry's name or one of its keywords (field boost).
fn term_in_name_or_keywords(e: &CapabilityEntry, term: &str) -> bool {
    lexicon::normalize_term(&e.name).as_deref() == Some(term)
        || e.name.to_lowercase().contains(term)
        || e
            .keywords
            .iter()
            .any(|kw| lexicon::normalize_term(kw).as_deref() == Some(term))
}

/// Known external references per topic, so the `external` section names a real
/// library instead of a placeholder.
const EXTERNAL_SUBJECTS: &[(&str, &str)] = &[
    ("pdf", "WeasyPrint"),
    ("chart", "Matplotlib"),
    ("plot", "Matplotlib"),
    ("graph", "Graphviz"),
    ("map", "Graphviz"),
    ("diagram", "Mermaid"),
    ("spreadsheet", "openpyxl"),
    ("excel", "openpyxl"),
    ("template", "Jinja"),
    ("html", "Jinja"),
    ("slide", "python-pptx"),
    ("presentation", "python-pptx"),
    ("search", "Tantivy"),
    ("index", "Tantivy"),
    ("embedding", "fastembed"),
    ("async", "Tokio"),
    ("http", "reqwest"),
    ("api", "FastAPI"),
    ("dataframe", "Polars"),
    ("data", "Polars"),
];

/// Build the external lenses for an intent from its own terms.
///
/// Never emits a placeholder: the question always carries the verbatim intent,
/// and the subject is either a matched library or the strongest content term.
fn external_lenses(intent: &str, terms: &[String]) -> Vec<ExternalLens> {
    let mut lenses: Vec<ExternalLens> = Vec::new();
    for term in terms {
        if let Some((_, subject)) = EXTERNAL_SUBJECTS.iter().find(|(k, _)| k == term)
            && !lenses.iter().any(|l| l.subject == *subject)
        {
            lenses.push(ExternalLens {
                source: "context7".to_string(),
                subject: (*subject).to_string(),
                question: format!("melhores práticas atuais para: {intent}"),
            });
        }
    }
    if lenses.is_empty() {
        // No curated match — name the strongest term rather than a placeholder.
        let subject = terms
            .iter()
            .max_by_key(|t| t.len())
            .cloned()
            .unwrap_or_else(|| intent.trim().to_string());
        lenses.push(ExternalLens {
            source: "context7".to_string(),
            subject,
            question: format!("melhores práticas atuais para: {intent}"),
        });
    }
    lenses.truncate(3);
    lenses
}

/// Shortest shared prefix that counts as the same word family for gap reporting.
const STEM_PREFIX: usize = 4;

/// Is `term` mentioned by any of the candidates' tokens?
///
/// Deliberately more permissive than the exact match BM25 uses. A gap is an
/// **assertion about absence**, so it must not fire on a morphological variant:
/// claiming "no candidate mentions professional" when one says "professionally
/// formatted" is a false statement, and a portfolio that makes false statements
/// is worse than none. Ranking stays exact; only the absence claim is lenient.
fn term_is_covered(term: &str, covered: &[String]) -> bool {
    covered.iter().any(|tok| {
        tok == term
            || (term.len() >= STEM_PREFIX
                && tok.len() >= STEM_PREFIX
                && (tok.starts_with(term) || term.starts_with(tok.as_str())))
    })
}

/// Derive the `gaps` section from the data — never invented.
///
/// Each clause is a fact about the candidate set: a query term nobody covers,
/// an absent test, an inherited description, an aged artifact.
fn derive_gaps(
    intent: &str,
    terms: &[String],
    hits: &[ScoredCapability],
    corpus_size: usize,
) -> Vec<String> {
    let mut gaps = Vec::new();
    if hits.is_empty() {
        gaps.push(format!(
            "nenhum artefato conhecido cobre \"{intent}\" ({corpus_size} registros varridos) — \
             o portfólio não tem prior-art para este intento"
        ));
        return gaps;
    }

    let covered: Vec<String> = hits.iter().flat_map(|h| entry_tokens(&h.entry)).collect();
    let missing: Vec<&String> = terms
        .iter()
        .filter(|t| !term_is_covered(t, &covered))
        .collect();
    if !missing.is_empty() {
        let list = missing
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        gaps.push(format!("nenhum candidato menciona: {list}"));
    }

    if hits.iter().all(|h| h.entry.evidence.has_tests != Some(true)) {
        gaps.push(
            "nenhum candidato tem teste conhecido — reuso não verificado por execução".to_string(),
        );
    }

    if hits.iter().all(|h| h.entry.purpose_inherited) {
        gaps.push(
            "a descrição de todos os candidatos foi herdada do bundle, não do próprio arquivo — \
             confirme o comportamento real antes de reusar"
                .to_string(),
        );
    }

    if hits
        .iter()
        .all(|h| h.entry.evidence.modified_days_ago.is_some_and(|d| d > STALE_DAYS))
    {
        gaps.push(format!(
            "todos os candidatos têm mais de {STALE_DAYS} dias sem modificação — podem estar defasados"
        ));
    }

    gaps
}

/// Weight given to the semantic score when a [`SemanticScorer`] is supplied.
///
/// Policy, not measurement: lexical evidence stays the majority signal because
/// it is the one that can be traced back to a literal word in the artifact's
/// own description. The semantic leg breaks ties and rescues synonyms.
const SEMANTIC_WEIGHT: f64 = 0.4;

/// Rank the portfolio against `intent`, optionally re-ranking with a scorer.
///
/// With `scorer: None` this is exactly [`answer`]. With one, the surviving
/// candidates are blended `(1-w)·lexical + w·semantic` on scores normalized to
/// the best in the set — so a synonym match ("draw a diagram" vs "render a
/// chart") can overtake a literal one, without letting the semantic leg
/// invent relevance where the lexical leg found none.
#[must_use]
pub fn answer_with_scorer(
    index: &PortfolioIndex,
    intent: &str,
    top_k: usize,
    scorer: Option<&dyn super::SemanticScorer>,
) -> PortfolioAnswer {
    let mut ans = answer(index, intent, top_k);
    let Some(scorer) = scorer else { return ans };
    let best_lexical = ans.prior_art.first().map_or(0.0, |h| h.score);
    if best_lexical <= 0.0 {
        return ans;
    }
    for hit in &mut ans.prior_art {
        let semantic = scorer.score(intent, &hit.entry.purpose).clamp(0.0, 1.0);
        let lexical = hit.score / best_lexical;
        hit.score = best_lexical
            * ((1.0 - SEMANTIC_WEIGHT).mul_add(lexical, SEMANTIC_WEIGHT * semantic));
    }
    ans.prior_art.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.entry.id.cmp(&b.entry.id))
    });
    ans
}

/// Rank the portfolio against `intent` and assemble the three-section answer.
///
/// Pure over `index`: no IO, no clock, no global state — so the same index and
/// intent always produce the same answer.
#[must_use]
pub fn answer(index: &PortfolioIndex, intent: &str, top_k: usize) -> PortfolioAnswer {
    let terms = lexicon::tokenize(intent);
    let corpus_size = index.entries.len();
    let verdict_required: Vec<String> =
        Verdict::all().iter().map(|v| v.tag().to_string()).collect();

    if terms.is_empty() || top_k == 0 || index.entries.is_empty() {
        return PortfolioAnswer {
            intent: intent.to_string(),
            prior_art: Vec::new(),
            gaps: derive_gaps(intent, &terms, &[], corpus_size),
            external: external_lenses(intent, &terms),
            verdict_required,
            corpus_size,
        };
    }

    let docs: Vec<Vec<String>> = index.entries.iter().map(entry_tokens).collect();
    let corpus = Bm25Corpus::new(docs);
    let df = corpus.doc_freq(&terms);

    let need = required_matches(terms.len());
    let mut scored: Vec<ScoredCapability> = index
        .entries
        .iter()
        .enumerate()
        .filter(|(i, _)| matched_terms(corpus.doc(*i), &terms) >= need)
        .map(|(i, entry)| ScoredCapability {
            entry: entry.clone(),
            score: bm25_score_doc(
                corpus.doc(i),
                &terms,
                &df,
                corpus.n(),
                corpus.avgdl(),
                &|term| term_in_name_or_keywords(entry, term),
                FIELD_BOOST,
            ) * if entry.kind == super::CapabilityKind::Symbol {
                SYMBOL_WEIGHT
            } else {
                1.0
            },
        })
        .collect();

    // Highest score first; ties broken by id so the order is deterministic.
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.entry.id.cmp(&b.entry.id))
    });

    let best = scored.first().map_or(0.0, |s| s.score);
    scored.retain(|s| s.score >= best * RELATIVE_FLOOR);
    scored.truncate(top_k);

    let gaps = derive_gaps(intent, &terms, &scored, corpus_size);
    PortfolioAnswer {
        intent: intent.to_string(),
        prior_art: scored,
        gaps,
        external: external_lenses(intent, &terms),
        verdict_required,
        corpus_size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::store::INDEX_VERSION;
    use crate::portfolio::{CapabilityKind, Evidence};

    fn entry(name: &str, purpose: &str, kws: &[&str]) -> CapabilityEntry {
        let disp = format!("~/scripts/{name}.py");
        CapabilityEntry {
            id: CapabilityEntry::make_id(CapabilityKind::Script, &disp),
            display_path: disp.clone(),
            kind: CapabilityKind::Script,
            name: name.to_string(),
            purpose: purpose.to_string(),
            language: "python".to_string(),
            entry_point: Some(format!("python3 {disp}")),
            provenance: "skill:test".to_string(),
            keywords: kws.iter().map(|s| (*s).to_string()).collect(),
            evidence: Evidence::default(),
            purpose_inherited: false,
        }
    }

    fn index_of(entries: Vec<CapabilityEntry>) -> PortfolioIndex {
        PortfolioIndex {
            version: INDEX_VERSION,
            built_at: "epoch:0".to_string(),
            roots: vec![],
            entries,
        }
    }

    fn corpus() -> PortfolioIndex {
        index_of(vec![
            entry("render_map", "Generate the module dependency map as an SVG diagram", &["map", "svg"]),
            entry("html_to_pdf", "Generate a professional PDF document from an HTML template", &["pdf", "html"]),
            entry("parse_config", "Parse and validate a TOML configuration file", &["config", "toml"]),
        ])
    }

    #[test]
    fn portuguese_intent_finds_the_english_artifact() {
        // The end-to-end version of the failure measured against search-tools.
        let a = answer(&corpus(), "gerar PDF profissional", 5);
        assert!(!a.prior_art.is_empty(), "expected a hit, gaps={:?}", a.gaps);
        assert_eq!(a.prior_art[0].entry.name, "html_to_pdf");
    }

    #[test]
    fn map_intent_ranks_the_map_script_first_in_both_languages() {
        for intent in ["gerar um mapa", "generate a map"] {
            let a = answer(&corpus(), intent, 5);
            assert_eq!(
                a.prior_art.first().map(|h| h.entry.name.as_str()),
                Some("render_map"),
                "intent {intent} ranked {:?}",
                a.prior_art.iter().map(|h| &h.entry.name).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn unrelated_intent_returns_empty_prior_art_and_says_so() {
        // The portfolio must never pad. "Nothing found" is a valid answer.
        let a = answer(&corpus(), "treinar uma rede neural convolucional", 5);
        assert!(a.prior_art.is_empty(), "{:?}", a.prior_art);
        assert!(
            a.gaps.iter().any(|g| g.contains("nenhum artefato conhecido")),
            "gaps must state the absence: {:?}",
            a.gaps
        );
        assert_eq!(a.corpus_size, 3, "a thin answer must expose how thin");
    }

    #[test]
    fn every_answer_demands_a_verdict() {
        let a = answer(&corpus(), "gerar mapa", 5);
        for v in Verdict::all() {
            assert!(a.verdict_required.contains(&v.tag().to_string()), "missing {}", v.tag());
        }
    }

    #[test]
    fn external_lens_names_a_real_library_never_a_placeholder() {
        let a = answer(&corpus(), "gerar PDF profissional", 5);
        assert!(!a.external.is_empty());
        let lens = &a.external[0];
        assert_eq!(lens.subject, "WeasyPrint");
        assert!(lens.question.contains("gerar PDF profissional"), "{}", lens.question);
        assert!(!lens.question.contains('<'), "placeholder leaked: {}", lens.question);
    }

    #[test]
    fn external_lens_falls_back_to_a_derived_subject() {
        let a = answer(&corpus(), "orquestrar telemetria distribuida", 5);
        let lens = &a.external[0];
        assert!(!lens.subject.is_empty());
        assert!(!lens.subject.contains('<'), "placeholder leaked: {}", lens.subject);
        assert!(lens.question.contains("orquestrar telemetria"), "{}", lens.question);
    }

    #[test]
    fn gaps_name_the_uncovered_terms() {
        let a = answer(&corpus(), "generate a map with authentication", 5);
        assert!(!a.prior_art.is_empty());
        assert!(
            a.gaps.iter().any(|g| g.contains("authentication")),
            "the uncovered term must be named: {:?}",
            a.gaps
        );
    }

    #[test]
    fn gaps_flag_absent_tests() {
        let a = answer(&corpus(), "gerar mapa", 5);
        assert!(
            a.gaps.iter().any(|g| g.contains("teste conhecido")),
            "{:?}",
            a.gaps
        );
    }

    #[test]
    fn gaps_flag_inherited_descriptions() {
        let mut e = entry("fill_form", "Toolkit for generating professional PDF documents", &["pdf"]);
        e.purpose_inherited = true;
        let a = answer(&index_of(vec![e]), "gerar PDF", 5);
        assert!(!a.prior_art.is_empty());
        assert!(
            a.gaps.iter().any(|g| g.contains("herdada do bundle")),
            "{:?}",
            a.gaps
        );
    }

    #[test]
    fn empty_index_is_honest_not_silent() {
        let a = answer(&PortfolioIndex::empty(), "gerar mapa", 5);
        assert!(a.prior_art.is_empty());
        assert_eq!(a.corpus_size, 0);
        assert!(!a.gaps.is_empty(), "an empty portfolio must still explain itself");
        assert!(!a.external.is_empty(), "external lens survives an empty index");
    }

    #[test]
    fn ranking_is_deterministic_across_runs() {
        let idx = corpus();
        let a = answer(&idx, "gerar documento", 5);
        let b = answer(&idx, "gerar documento", 5);
        assert_eq!(a, b);
    }

    #[test]
    fn field_boost_lifts_a_name_match_over_a_prose_only_match() {
        let idx = index_of(vec![
            entry("pdf_builder", "Assemble output files for distribution", &["pdf"]),
            entry("misc_tool", "A helper that can also produce a pdf when asked nicely", &[]),
        ]);
        let a = answer(&idx, "pdf", 5);
        assert_eq!(
            a.prior_art.first().map(|h| h.entry.name.as_str()),
            Some("pdf_builder"),
            "name/keyword hit must outrank a prose-only mention"
        );
    }

    #[test]
    fn a_term_common_to_every_document_still_returns_its_candidates() {
        // Regression: an absolute score floor hid these. BM25 gives a term
        // present in every doc an IDF near zero, so in a small corpus the
        // correct answers scored below any fixed threshold and vanished.
        let idx = index_of(vec![
            entry("pdf_builder", "Assemble output files for distribution", &["pdf"]),
            entry("pdf_merger", "Combine several pdf files into one", &["pdf"]),
        ]);
        let a = answer(&idx, "pdf", 5);
        assert_eq!(a.prior_art.len(), 2, "both candidates must survive: {:?}", a.gaps);
    }

    #[test]
    fn a_single_incidental_term_does_not_qualify_a_candidate() {
        // Coverage floor: matching only "generate" out of three terms is noise.
        let idx = index_of(vec![entry(
            "unrelated",
            "Generate nothing of interest whatsoever",
            &[],
        )]);
        let a = answer(&idx, "gerar PDF profissional", 5);
        assert!(a.prior_art.is_empty(), "1 of 3 terms must not qualify: {:?}", a.prior_art);
    }

    #[test]
    fn a_gap_never_fires_on_a_morphological_variant() {
        // Measured against the real corpus 2026-08-08: the answer claimed
        // "nenhum candidato menciona: professional" while candidate #1 read
        // "professionally formatted PDFs". A false absence claim is worse than
        // no claim at all.
        let idx = index_of(vec![entry(
            "generate_pdf",
            "Converts markdown files to professionally formatted PDFs with proper styling",
            &["generate", "pdf"],
        )]);
        let a = answer(&idx, "gerar PDF profissional", 5);
        assert!(!a.prior_art.is_empty());
        assert!(
            !a.gaps.iter().any(|g| g.contains("menciona") && g.contains("professional")),
            "false absence claim: {:?}",
            a.gaps
        );
    }

    #[test]
    fn a_genuinely_absent_term_is_still_reported() {
        // The lenient rule must not silence real gaps.
        let idx = index_of(vec![entry(
            "generate_pdf",
            "Converts markdown files to professionally formatted PDFs",
            &["generate", "pdf"],
        )]);
        let a = answer(&idx, "gerar PDF com assinatura digital", 5);
        assert!(
            a.gaps.iter().any(|g| g.contains("sign") || g.contains("digital")),
            "a truly uncovered term must still be named: {:?}",
            a.gaps
        );
    }

    #[test]
    fn short_terms_require_an_exact_match_for_coverage() {
        // A 3-char term must not be "covered" by any word starting with it.
        assert!(!term_is_covered("pdf", &["pdfkit".to_string()]));
        assert!(term_is_covered("pdf", &["pdf".to_string()]));
        assert!(term_is_covered("professional", &["professionally".to_string()]));
    }

    #[test]
    fn an_artifact_outranks_a_symbol_of_equal_relevance() {
        // Measured 2026-08-08: without this weight, stub symbols ("Command-line
        // interface.", 23 chars) outranked the real PDF artifacts, because BM25
        // length normalization favours short documents.
        let mut sym = entry("helper", "Generate a professional PDF from a template", &["pdf"]);
        sym.kind = CapabilityKind::Symbol;
        sym.id = CapabilityEntry::make_symbol_id("~/scripts/helper.py", "helper");
        let art = entry("builder", "Generate a professional PDF from a template", &["pdf"]);
        let a = answer(&index_of(vec![sym, art]), "gerar PDF profissional", 5);
        assert_eq!(
            a.prior_art.first().map(|h| h.entry.kind),
            Some(CapabilityKind::Script),
            "the runnable artifact must come first: {:?}",
            a.prior_art.iter().map(|h| (h.entry.kind, &h.entry.name)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_symbol_still_wins_when_it_is_genuinely_more_relevant() {
        // The weight is a tiebreaker, not an exclusion — the symbol grain is
        // exactly what answers "is there already a function that does X?".
        let mut sym = entry(
            "html_to_markdown",
            "Convert scraped HTML documents to Markdown preserving metadata",
            &["html", "markdown", "convert"],
        );
        sym.kind = CapabilityKind::Symbol;
        sym.id = CapabilityEntry::make_symbol_id("~/scripts/conv.py", "html_to_markdown");
        let art = entry("unrelated_tool", "Convert spreadsheets into CSV exports", &["csv"]);
        let a = answer(&index_of(vec![sym, art]), "converter HTML em markdown", 5);
        assert_eq!(
            a.prior_art.first().map(|h| h.entry.name.as_str()),
            Some("html_to_markdown"),
            "{:?}",
            a.prior_art.iter().map(|h| &h.entry.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn required_matches_is_at_least_one() {
        assert_eq!(required_matches(0), 1);
        assert_eq!(required_matches(1), 1);
        assert_eq!(required_matches(2), 1);
        assert_eq!(required_matches(3), 2);
        assert_eq!(required_matches(4), 2);
    }

    #[test]
    fn top_k_zero_returns_nothing_but_still_answers() {
        let a = answer(&corpus(), "gerar mapa", 0);
        assert!(a.prior_art.is_empty());
        assert!(!a.gaps.is_empty());
    }
}
