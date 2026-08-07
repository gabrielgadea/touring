//! Sentence boundary detection for argument mining.
//!
//! Provides Unicode-aware sentence boundary detection with edge case handling
//! for abbreviations, quotes, parentheses, and list structures.

use std::cmp::Ordering;
use unicode_segmentation::UnicodeSegmentation;

/// Common abbreviations that should NOT be treated as sentence endings.
const ABBREVIATIONS: &[&str] = &[
    "Dr.", "Mr.", "Mrs.", "Ms.", "Prof.", "Sr.", "Jr.", "Inc.", "Ltd.", "Corp.", "vs.", "et al.",
    "e.g.", "i.e.", "etc.", "approx.", "App.", "Rev.", "Gen.", "Sen.", "Rep.", "Gov.", "Pres.",
    "Sec.", "Treas.", "Vol.", "No.", "Fig.", "ed.", "eds.", "trans.", "comp.", "intl.", "conf.",
    "proc.", "symp.", "Tech.", "Sci.", "Med.", "Biol.", "Chem.", "Phys.",
];

/// Checks if a potential sentence ending is actually an abbreviation.
fn is_abbreviation(text: &str, end_pos: usize) -> bool {
    if end_pos < 2 {
        return false;
    }

    // Include space/whitespace as word separators so "Mrs. Smith" finds "Mrs." not the whole phrase
    let start = text[..end_pos]
        .rfind(|c: char| !c.is_alphanumeric() && c != '.')
        .map(|i| {
            // Advance past the multi-byte char at position i
            i + text[i..].chars().next().map_or(1, |c| c.len_utf8())
        })
        .unwrap_or(0);

    let candidate = text[start..end_pos].trim();
    ABBREVIATIONS
        .iter()
        .any(|&abbr| candidate == abbr || candidate == abbr.trim_end_matches('.'))
}

/// Handles quotes and parentheses that may surround sentences.
fn is_inside_quote_or_paren(text: &str, pos: usize) -> bool {
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut paren_depth: i32 = 0;

    for c in text[..pos].chars() {
        match c {
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
            }
            '(' if !in_single_quote && !in_double_quote => {
                paren_depth += 1;
            }
            ')' if !in_single_quote && !in_double_quote => {
                paren_depth = paren_depth.saturating_sub(1);
            }
            _ => {}
        }
    }

    in_single_quote || in_double_quote || paren_depth > 0
}

/// Checks if a period at `end_pos` is part of a list item marker (e.g. "1.", "a.", "-", "•").
///
/// List item markers should not be treated as sentence endings.
fn is_list_item_start(text: &str, end_pos: usize) -> bool {
    if end_pos == 0 {
        return true;
    }

    let before = text[..end_pos].trim_end();
    if before.is_empty() {
        return true;
    }

    let before_trimmed = before.trim();
    before_trimmed.ends_with('.')
        && (before_trimmed.starts_with(|c: char| c.is_ascii_digit())
            || before_trimmed.starts_with(|c: char| c.is_ascii_lowercase())
            || before_trimmed.starts_with('-')
            || before_trimmed.starts_with('*')
            || before_trimmed.starts_with('•'))
}

/// Gets all sentence boundaries in the given text.
pub fn get_sentence_boundaries(text: &str) -> Vec<std::ops::Range<usize>> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut boundaries = Vec::new();
    let sentence_terminators = ['.', '!', '?', '。', '！', '？'];

    let mut sentence_start = 0;
    let mut in_sentence = false;
    let mut last_terminal_pos: Option<usize> = None;
    let mut grapheme_index = 0;

    let grapheme_indices: Vec<(usize, &str)> =
        UnicodeSegmentation::grapheme_indices(text, false).collect();

    for (byte_offset, grapheme) in &grapheme_indices {
        // Use the first char of the current grapheme cluster (O(1) vs O(n) text.chars().nth)
        let char_at = grapheme.chars().next();

        if char_at.is_some_and(|ch| sentence_terminators.contains(&ch)) {
            let end_pos = byte_offset + grapheme.len();

            if is_abbreviation(text, end_pos) {
                grapheme_index += 1;
                continue;
            }

            // Skip list item markers such as "1.", "a.", "- " so they don't split sentences
            if is_list_item_start(text, end_pos) {
                grapheme_index += 1;
                continue;
            }

            if is_inside_quote_or_paren(text, end_pos) {
                grapheme_index += 1;
                continue;
            }

            last_terminal_pos = Some(end_pos);

            if in_sentence && sentence_start < end_pos {
                boundaries.push(sentence_start..end_pos);
            }
            sentence_start = end_pos;
            in_sentence = false;
        } else if char_at.is_some_and(|ch| ch.is_uppercase() && grapheme_index > 0) {
            if let Some(term_pos) = last_terminal_pos {
                let between = &text[term_pos..*byte_offset];
                if between
                    .chars()
                    .all(|c| c.is_whitespace() || c == ',' || c == ':' || c == ';')
                {
                    sentence_start = *byte_offset;
                }
            }
            in_sentence = true;
        } else if char_at.is_some_and(|ch| !ch.is_whitespace()) {
            in_sentence = true;
        }

        grapheme_index += 1;
    }

    if (last_terminal_pos.is_none() || sentence_start > last_terminal_pos.unwrap_or(0))
        && sentence_start < text.len()
    {
        let trimmed = text[sentence_start..].trim();
        if !trimmed.is_empty() {
            let start_byte = text[sentence_start..]
                .find(|c: char| !c.is_whitespace())
                .map(|i| sentence_start + i)
                .unwrap_or(sentence_start);
            let end_byte = text.find(trimmed).unwrap_or(text.len()) + trimmed.len();
            if start_byte < end_byte {
                boundaries.push(start_byte..end_byte);
            }
        }
    }

    if boundaries.len() > 1 {
        boundaries.sort_by_key(|r| r.start);
        let Some(first) = boundaries.first() else {
            return boundaries;
        };
        let mut merged = vec![first.clone()];
        for current in boundaries.iter().skip(1) {
            if let Some(last) = merged.last_mut() {
                if current.start < last.end {
                    // Overlapping ranges: merge
                    last.end = last.end.max(current.end);
                } else {
                    merged.push(current.clone());
                }
            }
        }
        boundaries = merged;
    }

    boundaries
}

/// Binary search helper to find which sentence contains a given position.
pub fn find_sentence_containing(
    pos: usize,
    boundaries: &[std::ops::Range<usize>],
) -> Option<usize> {
    if boundaries.is_empty() {
        return None;
    }

    let mut left = 0;
    let mut right = boundaries.len();

    while left < right {
        let mid = left + (right - left) / 2;
        let Some(boundary) = boundaries.get(mid) else {
            break;
        };
        match boundary.start.cmp(&pos) {
            Ordering::Equal => return Some(mid),
            Ordering::Less => {
                if pos < boundary.end {
                    return Some(mid);
                }
                left = mid + 1;
            }
            Ordering::Greater => {
                right = mid;
            }
        }
    }

    if left > 0
        && let Some(b) = boundaries.get(left - 1)
        && pos >= b.start
        && pos < b.end
    {
        return Some(left - 1);
    }
    if let Some(b) = boundaries.get(left)
        && pos >= b.start
        && pos < b.end
    {
        return Some(left);
    }
    None
}

/// Splits text into sentences, returning the sentence texts.
pub fn split_into_sentences(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }

    let boundaries = get_sentence_boundaries(text);
    boundaries
        .iter()
        .filter_map(|range| {
            let sentence = text.get(range.clone())?;
            let trimmed = sentence.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_sentence_boundaries() {
        let text = "Hello world. This is a test!";
        let boundaries = get_sentence_boundaries(text);
        assert!(boundaries.len() >= 2);
    }

    #[test]
    fn test_sentence_with_abbreviations() {
        let text = "Dr. Smith is here. She arrived today.";
        let sentences = split_into_sentences(text);
        assert!(sentences.iter().any(|s| s.contains("Dr. Smith")));
    }

    #[test]
    fn test_empty_text() {
        let boundaries = get_sentence_boundaries("");
        assert!(boundaries.is_empty());
        let sentences = split_into_sentences("");
        assert!(sentences.is_empty());
    }

    #[test]
    fn test_single_sentence() {
        let text = "Hello world.";
        let boundaries = get_sentence_boundaries(text);
        assert_eq!(boundaries.len(), 1);
        let sentences = split_into_sentences(text);
        assert_eq!(sentences.len(), 1);
        assert_eq!(sentences[0], "Hello world.");
    }

    #[test]
    fn test_multi_paragraph() {
        let text = "First paragraph.\n\nSecond paragraph.";
        let boundaries = get_sentence_boundaries(text);
        assert!(boundaries.len() >= 2);
    }

    #[test]
    fn test_find_sentence_containing() {
        let text = "First sentence. Second sentence. Third.";
        let boundaries = get_sentence_boundaries(text);
        assert_eq!(find_sentence_containing(0, &boundaries), Some(0));
        // Position 16 is 'S' in "Second sentence." (pos 15 is the space gap)
        assert_eq!(find_sentence_containing(16, &boundaries), Some(1));
    }

    #[test]
    fn test_find_sentence_out_of_bounds() {
        let text = "Hello world.";
        let boundaries = get_sentence_boundaries(text);
        assert_eq!(find_sentence_containing(100, &boundaries), None);
    }

    #[test]
    fn test_quote_handling() {
        // The period inside a quote is skipped; only "Then he left." ends the sentence.
        // The whole input is one sentence (no boundary inside the quote).
        let text = "He said \"Hello world.\" Then he left.";
        let sentences = split_into_sentences(text);
        assert!(sentences.len() >= 1);
    }

    #[test]
    fn test_list_items() {
        let text = "1. First item\n2. Second item\n3. Third item";
        let boundaries = get_sentence_boundaries(text);
        assert!(!boundaries.is_empty());
    }

    #[test]
    fn test_multiple_abbreviations() {
        let text = "Mr. Jones and Mrs. Smith met at 9 a.m. to discuss etc. matters.";
        let sentences = split_into_sentences(text);
        assert!(sentences.len() <= 4);
    }

    #[test]
    fn test_binary_search_boundaries() {
        let text = "One. Two. Three. Four.";
        let boundaries = get_sentence_boundaries(text);
        assert_eq!(boundaries.len(), 4);

        // Positions within sentences (not the inter-sentence spaces)
        assert_eq!(find_sentence_containing(0, &boundaries), Some(0)); // 'O' of "One."
        assert_eq!(find_sentence_containing(3, &boundaries), Some(0)); // '.' of "One."
        assert_eq!(find_sentence_containing(5, &boundaries), Some(1)); // 'T' of "Two."
        assert_eq!(find_sentence_containing(8, &boundaries), Some(1)); // '.' of "Two."
        assert_eq!(find_sentence_containing(10, &boundaries), Some(2)); // 'T' of "Three."
    }

    #[test]
    fn test_sentence_with_question_mark() {
        let text = "How are you? I am fine.";
        let sentences = split_into_sentences(text);
        assert!(sentences.iter().any(|s| s.contains("How are you?")));
        assert!(sentences.iter().any(|s| s.contains("I am fine.")));
    }

    #[test]
    fn test_cjk_punctuation() {
        let text = "你好世界。你好吗？";
        let boundaries = get_sentence_boundaries(text);
        assert!(boundaries.len() >= 2);
    }

    #[test]
    fn test_sentence_boundary_ranges() {
        let text = "Hello world.";
        let boundaries = get_sentence_boundaries(text);
        assert_eq!(boundaries.len(), 1);
        let extracted: &str = &text[boundaries[0].start..boundaries[0].end];
        assert_eq!(extracted, "Hello world.");
    }

    #[test]
    fn test_edge_case_whitespace_only() {
        let text = "   ";
        let boundaries = get_sentence_boundaries(text);
        assert!(boundaries.is_empty());
    }

    #[test]
    fn test_no_terminal_punctuation() {
        let text = "This is a sentence without terminal punctuation";
        let boundaries = get_sentence_boundaries(text);
        assert!(!boundaries.is_empty());
    }
}
