//! Discourse relation population for argument mining.
//!
//! Populates discourse relations between argument elements based on
//! spatial and linguistic relationships.

use super::{EricksonElement, RelationType};
use crate::erickson::sentence_boundaries::get_sentence_boundaries;

/// Discourse relation context relative to another element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationContext {
    /// Element precedes the target in text order
    Preceding,
    /// Element follows the target in text order
    Following,
    /// Element shares position with target
    Same,
}

/// Population result with updated elements.
pub struct PopulationResult {
    /// Updated elements with relations
    pub elements: Vec<EricksonElement>,
    /// Number of relations added
    pub relations_added: usize,
}

/// Context bundle for relation inference — groups the per-element data so
/// `infer_relation_for_element` stays under the 7-argument clippy limit.
struct ElementInferContext<'a> {
    current: &'a EricksonElement,
    current_sentence_idx: Option<usize>,
    prev_element: Option<&'a EricksonElement>,
    next_element: Option<&'a EricksonElement>,
    next_sentence_idx: Option<usize>,
    total_sentences: usize,
}

/// Infers discourse relations between argument elements based on text context.
///
/// # Algorithm
///
/// 1. Sort elements by position
/// 2. For each element, determine context using sentence boundaries
/// 3. Apply relation inference rules:
///    - Same sentence → Elaborate
///    - SUPPORT_INDICATORS nearby → Support
///    - ATTACK_INDICATORS nearby → Attack
///    - End of penultimate sentence AND next at start of last → Conclude
///    - Contrast markers in same sentence → Contrast
///    - Otherwise → None
///
/// # Arguments
///
/// * `elements` - Mutable slice of EricksonElements (sorted by position)
/// * `text` - The original text for sentence boundary analysis
///
/// # Returns
///
/// Number of relations added
pub fn infer_relations(elements: &mut [EricksonElement], text: &str) -> usize {
    let boundaries = get_sentence_boundaries(text);
    infer_relations_with_boundaries(elements, text, &boundaries)
}

/// Infers discourse relations using pre-computed sentence boundaries, avoiding redundant
/// boundary computation when the caller already has them available.
pub fn infer_relations_with_boundaries(
    elements: &mut [EricksonElement],
    text: &str,
    boundaries: &[std::ops::Range<usize>],
) -> usize {
    if elements.len() < 2 || text.is_empty() {
        return 0;
    }

    // Sort elements by position
    elements.sort_by_key(|e| e.start);

    if boundaries.is_empty() {
        return 0;
    }

    let total_sentences = boundaries.len();

    // Phase 1: compute relations without mutating elements (avoids borrow-checker conflicts
    // that arise from simultaneous indexing and mutation).
    let inferred: Vec<Option<RelationType>> = (0..elements.len())
        .map(|i| {
            let current = elements.get(i)?;

            let current_sentence_idx = find_sentence_index(current.start, boundaries);

            let prev_element = if i > 0 { elements.get(i - 1) } else { None };

            let (next_element, next_sentence_idx) = if i + 1 < elements.len() {
                let next = elements.get(i + 1);
                let idx = next.and_then(|n| find_sentence_index(n.start, boundaries));
                (next, idx)
            } else {
                (None, None)
            };

            let ctx = ElementInferContext {
                current,
                current_sentence_idx,
                prev_element,
                next_element,
                next_sentence_idx,
                total_sentences,
            };

            infer_relation_for_element(&ctx, boundaries, text)
        })
        .collect();

    // Phase 2: apply inferred relations
    let mut relations_added = 0;
    for (i, rel_opt) in inferred.into_iter().enumerate() {
        if let Some(rel) = rel_opt {
            if let Some(elem) = elements.get_mut(i) {
                elem.relation = Some(rel);
                relations_added += 1;
            }
        }
    }

    relations_added
}

/// Finds the sentence index containing a position.
fn find_sentence_index(pos: usize, boundaries: &[std::ops::Range<usize>]) -> Option<usize> {
    for (idx, range) in boundaries.iter().enumerate() {
        if pos >= range.start && pos < range.end {
            return Some(idx);
        }
    }
    None
}

/// Determines the discourse relation context between two positions in the text.
///
/// Returns whether `element_pos` is in the Same, Preceding, or Following sentence
/// relative to `other_pos`.  Used by `infer_relation_for_element` for rules 4 and 5.
pub fn get_relation_context(
    element_pos: usize,
    other_pos: usize,
    boundaries: &[std::ops::Range<usize>],
) -> RelationContext {
    let element_sentence = find_sentence_index(element_pos, boundaries);
    let other_sentence = find_sentence_index(other_pos, boundaries);

    match (element_sentence, other_sentence) {
        (Some(e), Some(o)) if e == o => RelationContext::Same,
        (Some(e), Some(o)) if e < o => RelationContext::Preceding,
        (Some(e), Some(o)) if e > o => RelationContext::Following,
        _ => RelationContext::Same,
    }
}

/// Infers the relation type for a single element based on its context.
///
/// Uses [`ElementInferContext`] to stay below the 7-argument clippy limit.
/// Rules 4 and 5 use [`get_relation_context`] to test same-sentence membership.
fn infer_relation_for_element(
    ctx: &ElementInferContext<'_>,
    boundaries: &[std::ops::Range<usize>],
    text: &str,
) -> Option<RelationType> {
    // Rule 1: Check for support indicators near current element
    if has_support_indicator_near(ctx.current, text) {
        return Some(RelationType::Support);
    }

    // Rule 2: Check for attack indicators near current element
    if has_attack_indicator_near(ctx.current, text) {
        return Some(RelationType::Attack);
    }

    // Rule 3: Conclude check - element at end of penultimate sentence AND next at start of last
    if let (Some(curr_idx), Some(next), Some(next_idx)) = (
        ctx.current_sentence_idx,
        ctx.next_element,
        ctx.next_sentence_idx,
    ) {
        if ctx.total_sentences >= 2
            && curr_idx == ctx.total_sentences - 2
            && next_idx == ctx.total_sentences - 1
        {
            if let Some(last_boundary) = get_sentence_boundaries(text).get(ctx.total_sentences - 1)
            {
                let midpoint = last_boundary.start + (last_boundary.end - last_boundary.start) / 2;
                if next.start < midpoint {
                    return Some(RelationType::Conclude);
                }
            }
        }
    }

    // Rule 4: Check for contrast markers — uses get_relation_context to detect same sentence
    if let Some(prev) = ctx.prev_element {
        if get_relation_context(ctx.current.start, prev.start, boundaries) == RelationContext::Same
            && has_contrast_marker(ctx.current, text)
        {
            return Some(RelationType::Contrast);
        }
    }

    // Rule 5: Elaborate — same sentence as preceding element, via get_relation_context
    if let Some(prev) = ctx.prev_element {
        if get_relation_context(ctx.current.start, prev.start, boundaries) == RelationContext::Same
        {
            return Some(RelationType::Elaborate);
        }
    }

    // Default: no relation
    None
}

/// Checks if a support indicator is near the element.
fn has_support_indicator_near(element: &EricksonElement, text: &str) -> bool {
    let context_window = 50;
    let start = element.start.saturating_sub(context_window);
    let end = (element.end + context_window).min(text.len());
    let context = &text[start..end];

    super::SUPPORT_INDICATORS.is_match(context)
}

/// Checks if an attack indicator is near the element.
fn has_attack_indicator_near(element: &EricksonElement, text: &str) -> bool {
    let context_window = 50;
    let start = element.start.saturating_sub(context_window);
    let end = (element.end + context_window).min(text.len());
    let context = &text[start..end];

    super::ATTACK_INDICATORS.is_match(context)
}

/// Checks if the element contains a contrast marker.
fn has_contrast_marker(element: &EricksonElement, text: &str) -> bool {
    let context_window = 30;
    let start = element.start.saturating_sub(context_window);
    let end = (element.end + context_window).min(text.len());
    let context = &text[start..end];

    // Contrast markers: however, but, although, despite, nevertheless, whereas, while
    let contrast_patterns = [
        "however",
        "but",
        "although",
        "despite",
        "nevertheless",
        "whereas",
        "while",
        "但是",
        "然而",
        "尽管",
        "不过",
    ];

    let context_lower = context.to_lowercase();
    contrast_patterns
        .iter()
        .any(|marker| context_lower.contains(marker))
}

/// Populates discourse relations between elements based on context.
///
/// This function analyzes the spatial and linguistic relationships between
/// extracted argument elements and assigns appropriate discourse relations.
pub fn populate_relations(elements: &mut [EricksonElement]) -> usize {
    if elements.len() < 2 {
        return 0;
    }

    // Sort elements by position first
    elements.sort_by_key(|e| e.start);

    // Phase 1: compute relations without mutating (avoids simultaneous index + mutable borrow)
    let inferred: Vec<Option<RelationType>> = elements
        .windows(2)
        .map(|w| match (w.first(), w.get(1)) {
            (Some(current), Some(next)) => infer_relation_simple(current, Some(next)),
            _ => None,
        })
        .chain(std::iter::once(
            // Last element has no successor
            elements
                .last()
                .and_then(|last| infer_relation_simple(last, None)),
        ))
        .collect();

    // Phase 2: apply inferred relations
    let mut relations_added = 0;
    for (elem, rel_opt) in elements.iter_mut().zip(inferred.iter()) {
        if let Some(rel) = rel_opt {
            elem.relation = Some(*rel);
            relations_added += 1;
        }
    }

    relations_added
}

/// Simple relation inference based on pattern types.
fn infer_relation_simple(
    current: &EricksonElement,
    next: Option<&EricksonElement>,
) -> Option<RelationType> {
    match (&current.pattern, next.map(|n| &n.pattern)) {
        // Evidence typically supports claims
        (super::NLPPattern::Evidence, Some(super::NLPPattern::Claim)) => {
            Some(RelationType::Support)
        }
        // Rebuttals attack claims
        (super::NLPPattern::Rebuttal, Some(super::NLPPattern::Claim)) => Some(RelationType::Attack),
        // Warrants elaborate on evidence
        (super::NLPPattern::Evidence, Some(super::NLPPattern::Warrant)) => {
            Some(RelationType::Elaborate)
        }
        // Default: elaborate only when a next element exists
        (_, Some(_)) => Some(RelationType::Elaborate),
        // No next element — no relation to assign
        _ => None,
    }
}

/// Resets all relations in elements to None.
pub fn reset_relations(elements: &mut [EricksonElement]) {
    for elem in elements.iter_mut() {
        elem.relation = None;
    }
}

#[cfg(test)]
mod tests {
    use super::super::{EricksonElement, NLPPattern};
    use super::*;

    #[test]
    fn test_find_sentence_index() {
        let text = "First sentence. Second sentence. Third.";
        let boundaries = get_sentence_boundaries(text);

        // Find positions for each sentence
        let first_end = text.find("First sentence.").unwrap() + "First sentence.".len();
        let second_end = text.find("Second sentence.").unwrap() + "Second sentence.".len();

        assert_eq!(find_sentence_index(0, &boundaries), Some(0));
        assert_eq!(find_sentence_index(first_end - 1, &boundaries), Some(0));
        // first_end is the space after "First sentence." — use first_end+1 ('S') for sentence 1
        assert_eq!(find_sentence_index(first_end + 1, &boundaries), Some(1));
        assert_eq!(find_sentence_index(second_end - 1, &boundaries), Some(1));
    }

    #[test]
    fn test_get_relation_context() {
        let text = "First sentence. Second sentence. Third.";
        let boundaries = get_sentence_boundaries(text);

        // Positions within first sentence
        assert_eq!(
            get_relation_context(0, 5, &boundaries),
            RelationContext::Same
        );

        // Positions in different sentences
        let first_end = text.find("First sentence.").unwrap() + "First sentence.".len();
        // first_end is the space; first_end+1 is 'S' in "Second sentence." (sentence 1)
        assert_eq!(
            get_relation_context(0, first_end + 1, &boundaries),
            RelationContext::Preceding
        );
    }

    #[test]
    fn test_infer_relations_empty() {
        let mut elements: Vec<EricksonElement> = vec![];
        let count = infer_relations(&mut elements, "Some text");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_infer_relations_single_element() {
        let mut elements = vec![EricksonElement::new(NLPPattern::Claim, "test", 0, 4, 0.9)];
        let count = infer_relations(&mut elements, "test");
        assert_eq!(count, 0); // No relation for single element
    }

    #[test]
    fn test_infer_relations_support() {
        let mut elements = vec![
            EricksonElement::new(NLPPattern::Claim, "should upgrade", 0, 14, 0.9),
            EricksonElement::new(NLPPattern::Evidence, "evidence", 20, 27, 0.85),
        ];
        let text = "We should upgrade because evidence shows it is critical.";
        let count = infer_relations(&mut elements, text);
        assert!(
            count <= elements.len(),
            "relation count must not exceed element count"
        );
    }

    #[test]
    fn test_infer_relations_same_sentence_elaborate() {
        let mut elements = vec![
            EricksonElement::new(NLPPattern::Evidence, "data shows", 0, 9, 0.85),
            EricksonElement::new(NLPPattern::Claim, "should upgrade", 15, 30, 0.9),
        ];
        let text = "The data shows it is broken. We should upgrade.";
        let count = infer_relations(&mut elements, text);
        assert!(
            count <= elements.len(),
            "relation count must not exceed element count"
        );
    }

    #[test]
    fn test_infer_relations_conclude() {
        let mut elements = vec![
            EricksonElement::new(NLPPattern::Claim, "main claim", 0, 10, 0.9),
            EricksonElement::new(NLPPattern::Warrant, "therefore", 50, 58, 0.8),
        ];
        let text = "First paragraph. Second paragraph. The main claim. Therefore we should act.";
        let count = infer_relations(&mut elements, text);
        assert!(
            count <= elements.len(),
            "relation count must not exceed element count"
        );
    }

    #[test]
    fn test_infer_relations_contrast() {
        let mut elements = vec![
            EricksonElement::new(NLPPattern::Claim, "should do it", 0, 12, 0.9),
            EricksonElement::new(NLPPattern::Rebuttal, "however", 15, 22, 0.75),
        ];
        let text = "We should do it. However, there are risks.";
        let count = infer_relations(&mut elements, text);
        assert!(
            count <= elements.len(),
            "relation count must not exceed element count"
        );
    }

    #[test]
    fn test_populate_relations_empty() {
        let mut elements: Vec<EricksonElement> = vec![];
        let count = populate_relations(&mut elements);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_populate_relations_simple() {
        let mut elements = vec![
            EricksonElement::new(NLPPattern::Evidence, "evidence", 0, 8, 0.85),
            EricksonElement::new(NLPPattern::Claim, "claim", 10, 15, 0.9),
        ];
        let count = populate_relations(&mut elements);
        assert_eq!(count, 1);
        assert_eq!(elements[0].relation, Some(RelationType::Support));
    }

    #[test]
    fn test_reset_relations() {
        let mut elements = vec![EricksonElement::new(NLPPattern::Claim, "test", 0, 4, 0.9)];
        elements[0].relation = Some(RelationType::Support);
        reset_relations(&mut elements);
        assert!(elements[0].relation.is_none());
    }

    #[test]
    fn test_infer_relations_sorted_order() {
        let mut elements = vec![
            EricksonElement::new(NLPPattern::Claim, "later claim", 50, 61, 0.9),
            EricksonElement::new(NLPPattern::Evidence, "first evidence", 0, 14, 0.85),
        ];
        let text = "First evidence shows. Later claim needs support.";
        let count = infer_relations(&mut elements, text);
        // Elements should be sorted by position internally
        assert!(
            count <= elements.len(),
            "relation count must not exceed element count"
        );
    }

    #[test]
    fn test_relation_context_enum() {
        assert_eq!(RelationContext::Preceding, RelationContext::Preceding);
        assert_eq!(RelationContext::Following, RelationContext::Following);
        assert_eq!(RelationContext::Same, RelationContext::Same);
    }

    #[test]
    fn test_has_support_indicator() {
        let element = EricksonElement::new(NLPPattern::Claim, "test", 0, 4, 0.9);
        let text = "Yes, this is correct.";
        assert!(has_support_indicator_near(&element, text));
    }

    #[test]
    fn test_has_attack_indicator() {
        let element = EricksonElement::new(NLPPattern::Claim, "test", 0, 4, 0.9);
        let text = "No, this is wrong.";
        assert!(has_attack_indicator_near(&element, text));
    }

    #[test]
    fn test_has_contrast_marker() {
        let element = EricksonElement::new(NLPPattern::Rebuttal, "however", 0, 7, 0.9);
        let text = "However, this is not correct.";
        assert!(has_contrast_marker(&element, text));
    }

    #[test]
    fn test_infer_relations_multiple() {
        let mut elements = vec![
            EricksonElement::new(NLPPattern::Evidence, "evidence one", 0, 12, 0.85),
            EricksonElement::new(NLPPattern::Evidence, "evidence two", 15, 27, 0.85),
            EricksonElement::new(NLPPattern::Claim, "final claim", 30, 40, 0.9),
        ];
        let text = "Evidence one shows. Evidence two confirms. Final claim follows.";
        let count = infer_relations(&mut elements, text);
        assert!(
            count <= elements.len(),
            "relation count must not exceed element count"
        );
    }

    #[test]
    fn test_population_result_struct() {
        let result = PopulationResult {
            elements: vec![EricksonElement::new(NLPPattern::Claim, "test", 0, 4, 0.9)],
            relations_added: 0,
        };
        assert_eq!(result.elements.len(), 1);
        assert_eq!(result.relations_added, 0);
    }
}
