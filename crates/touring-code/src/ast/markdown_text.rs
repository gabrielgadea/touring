//! Markdown as searchable text (2026-09-13, round 7 of the Graft analysis).
//!
//! The symbol index sees a markdown file only through its ATX headings
//! (`queries/markdown.scm`), so everything a memory, a rule or a skill SAYS was
//! invisible to every search — measured on the live workspace: 84 of this
//! project's 98 auto-memories carry no heading at all and produced zero symbols.
//! This module turns a markdown file into the two units the index stores:
//!
//! - one **document** per file (`markdown_document`): title from the frontmatter
//!   `name:`, else the first heading, else the caller's fallback (the file stem);
//!   text = the frontmatter `description:` followed by the body. It is also
//!   persisted as a symbol of kind `DOCUMENT_SYMBOL_KIND` at line
//!   [`DOCUMENT_LINE`] ([`with_document_symbol`]), so a file with no heading is
//!   still `indexed` and findable by its name.
//! - one **section** per ATX heading (`markdown_sections`): the heading's own
//!   line number (1-based, the line the heading symbol carries) and the text up
//!   to the next heading of any level.
//!
//! Headings inside fenced code blocks and inside the leading YAML frontmatter
//! are not headings — the same judgement tree-sitter-md makes. Text is
//! whitespace-collapsed and capped by characters (never split inside a UTF-8
//! sequence), so one huge file cannot inflate the search index.

use crate::ast::graph::SymbolLocation;

/// Kind of the file-level symbol a markdown file contributes.
const DOCUMENT_SYMBOL_KIND: &str = "document";

/// Line the document symbol is stored at. Headings start at line 1, so line 0
/// keeps the document distinct from a first heading of the same title (the
/// search index derives a document's id from name, path and line).
pub const DOCUMENT_LINE: usize = 0;

/// Character cap of one section's text.
const SECTION_TEXT_MAX_CHARS: usize = 4_000;
/// Characters a heading or document title keeps. A title rides on every chunk
/// under it, so its length multiplies by the chunk count; its words still reach
/// the index through the chunk bodies.
const TITLE_MAX_CHARS: usize = 200;

/// Character cap of the document text (description + body).
const DOCUMENT_TEXT_MAX_CHARS: usize = 8_000;

/// A searchable unit of a markdown file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownSection {
    /// Heading text (sections) or document title (documents).
    pub title: String,
    /// 1-based heading line for a section; [`DOCUMENT_LINE`] for the document.
    pub line: usize,
    /// Whitespace-collapsed text, capped by characters.
    pub text: String,
}

/// Splits the leading YAML frontmatter off `content`: `(frontmatter lines,
/// index of the first body line)`. No frontmatter → `([], 0)`.
fn split_frontmatter(lines: &[&str]) -> (Vec<String>, usize) {
    if lines.first().map(|l| l.trim()) != Some("---") {
        return (Vec::new(), 0);
    }
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            return (
                lines[1..i].iter().map(|l| (*l).to_string()).collect(),
                i + 1,
            );
        }
    }
    // An unclosed `---` is not frontmatter: the whole file is body.
    (Vec::new(), 0)
}

/// The value of a top-level `key:` in frontmatter lines, unquoted.
fn frontmatter_value(frontmatter: &[String], key: &str) -> Option<String> {
    frontmatter.iter().find_map(|line| {
        let rest = line.strip_prefix(key)?.strip_prefix(':')?;
        let value = rest.trim().trim_matches('"').trim_matches('\'').trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

/// The ATX heading text of `line`, when it is one (`#`..`######`, up to three
/// leading spaces, a space or end of line after the hashes).
fn atx_heading(line: &str) -> Option<String> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let hashes = rest.len() - rest.trim_start_matches('#').len();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let after = &rest[hashes..];
    if !(after.is_empty() || after.starts_with(' ') || after.starts_with('\t')) {
        return None;
    }
    Some(
        after
            .trim()
            .trim_end_matches('#')
            .trim()
            .chars()
            .take(TITLE_MAX_CHARS)
            .collect(),
    )
}

/// Opening or closing fence marker of `line` (```` ``` ```` or `~~~`).
fn fence_marker(line: &str) -> Option<&'static str> {
    let t = line.trim_start();
    if t.starts_with("```") {
        Some("```")
    } else if t.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

/// Collapses whitespace runs into one space and caps the result by characters.
fn collapse(parts: &[&str], max_chars: usize) -> String {
    let mut out = String::new();
    let mut count = 0usize;
    for word in parts.iter().flat_map(|p| p.split_whitespace()) {
        let needed = word.chars().count() + usize::from(!out.is_empty());
        if count + needed > max_chars {
            let room = max_chars.saturating_sub(count + usize::from(!out.is_empty()));
            if room > 0 {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.extend(word.chars().take(room));
            }
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
        count += needed;
    }
    out
}

/// One section per ATX heading outside frontmatter and fenced code, each with
/// its 1-based line and the text up to the next heading.
#[must_use]
fn markdown_sections(content: &str) -> Vec<MarkdownSection> {
    let lines: Vec<&str> = content.lines().collect();
    let (_, body_start) = split_frontmatter(&lines);
    let mut sections: Vec<(String, usize, Vec<&str>)> = Vec::new();
    let mut fence: Option<&'static str> = None;
    for (i, line) in lines.iter().enumerate().skip(body_start) {
        if let Some(marker) = fence_marker(line) {
            fence = match fence {
                Some(open) if open == marker => None,
                None => Some(marker),
                other => other,
            };
        } else if fence.is_none()
            && let Some(title) = atx_heading(line)
        {
            sections.push((title, i + 1, Vec::new()));
            continue;
        }
        if let Some((_, _, body)) = sections.last_mut() {
            body.push(line);
        }
    }
    sections
        .into_iter()
        .map(|(title, line, body)| MarkdownSection {
            text: collapse(&body, SECTION_TEXT_MAX_CHARS),
            title,
            line,
        })
        .collect()
}

/// The file as one searchable unit: title from frontmatter `name:`, else the
/// first heading, else `fallback_title`; text = frontmatter `description:`
/// followed by the body.
#[must_use]
fn markdown_document(content: &str, fallback_title: &str) -> MarkdownSection {
    let lines: Vec<&str> = content.lines().collect();
    let (frontmatter, body_start) = split_frontmatter(&lines);
    let title = frontmatter_value(&frontmatter, "name")
        .or_else(|| {
            markdown_sections(content)
                .into_iter()
                .next()
                .map(|s| s.title)
        })
        .filter(|t| !t.is_empty())
        .map(|t| t.chars().take(TITLE_MAX_CHARS).collect())
        .unwrap_or_else(|| fallback_title.to_string());
    let description = frontmatter_value(&frontmatter, "description").unwrap_or_default();
    let mut parts: Vec<&str> = vec![description.as_str()];
    parts.extend(lines.iter().skip(body_start).copied());
    MarkdownSection {
        title,
        line: DOCUMENT_LINE,
        text: collapse(&parts, DOCUMENT_TEXT_MAX_CHARS),
    }
}

/// The file-level text: the frontmatter `description:` followed by the first
/// `max_chars` characters of the body (whitespace-collapsed).
#[must_use]
pub fn document_text(content: &str, max_chars: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let (frontmatter, body_start) = split_frontmatter(&lines);
    let description = frontmatter_value(&frontmatter, "description").unwrap_or_default();
    let body: Vec<&str> = lines.iter().skip(body_start).copied().collect();
    let body_text = collapse(&body, max_chars);
    collapse(&[description.as_str(), body_text.as_str()], usize::MAX)
}

/// The frontmatter `description:` of `content`, when it has one.
#[must_use]
pub fn frontmatter_description(content: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    let (frontmatter, _) = split_frontmatter(&lines);
    frontmatter_value(&frontmatter, "description")
}

/// The whole body as consecutive chunks of at most `max_chars` characters, cut
/// at line boundaries (a single longer line is cut by characters). Each chunk
/// carries its first 1-based line and the title in effect there — the heading it
/// falls under, else the document title. Nothing of the body is left out, which
/// the capped document and section texts cannot promise for a long file.
#[must_use]
pub fn markdown_chunks(
    content: &str,
    fallback_title: &str,
    max_chars: usize,
) -> Vec<MarkdownSection> {
    let lines: Vec<&str> = content.lines().collect();
    let (_, body_start) = split_frontmatter(&lines);
    let mut title = markdown_document(content, fallback_title).title;
    let mut chunks: Vec<MarkdownSection> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut current_line = body_start + 1;
    let mut current_chars = 0usize;
    let mut fence: Option<&'static str> = None;
    let flush =
        |chunks: &mut Vec<MarkdownSection>, parts: &mut Vec<&str>, line: usize, title: &str| {
            let text = collapse(parts, usize::MAX);
            if !text.is_empty() {
                // A single line longer than the cap becomes several pieces at the same
                // line; the search index derives ids from name, path and line, so every
                // piece after the first carries its ordinal in the title.
                for (n, piece) in text
                    .chars()
                    .collect::<Vec<_>>()
                    .chunks(max_chars.max(1))
                    .enumerate()
                {
                    chunks.push(MarkdownSection {
                        title: if n == 0 {
                            title.to_string()
                        } else {
                            format!("{title} ({})", n + 1)
                        },
                        line,
                        text: piece.iter().collect(),
                    });
                }
            }
            parts.clear();
        };
    for (i, line) in lines.iter().enumerate().skip(body_start) {
        let heading = if let Some(marker) = fence_marker(line) {
            fence = match fence {
                Some(open) if open == marker => None,
                None => Some(marker),
                other => other,
            };
            None
        } else if fence.is_none() {
            atx_heading(line)
        } else {
            None
        };
        let chars = line
            .split_whitespace()
            .map(|w| w.chars().count() + 1)
            .sum::<usize>();
        if heading.is_some() || (current_chars + chars > max_chars && !current.is_empty()) {
            flush(&mut chunks, &mut current, current_line, &title);
            current_line = i + 1;
            current_chars = 0;
        }
        if let Some(h) = heading {
            title = h;
        }
        current.push(line);
        current_chars += chars;
    }
    flush(&mut chunks, &mut current, current_line, &title);
    chunks
}

/// Stems that name a ROLE, not a subject: `char_classes/mod.rs` is about char
/// classes, `academic-research-writer/SKILL.md` about that skill.
const GENERIC_STEMS: &[&str] = &["mod", "lib", "main", "index", "__init__", "README", "SKILL"];

/// The title of last resort for `file_path`: its stem, or — when the stem only
/// names a role (`mod.rs`, `lib.rs`, `__init__.py`, `SKILL.md`, …) — the name of
/// the directory holding it. Measured 13/09/2026: the module document of
/// `char_classes/mod.rs` was titled `mod` and never ranked for "char classes".
#[must_use]
pub fn fallback_title(file_path: &str) -> String {
    let path = std::path::Path::new(file_path);
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
    match stem {
        Some(s) if GENERIC_STEMS.contains(&s.as_str()) => path
            .parent()
            .and_then(|p| p.file_name())
            .map_or(s, |d| d.to_string_lossy().into_owned()),
        Some(s) => s,
        None => file_path.to_string(),
    }
}

/// `locations` plus the document symbol of a markdown `file_path`; any other
/// file is returned unchanged. Every writer of the symbol store calls this, so
/// the document row exists however the file reached the store.
#[must_use]
pub fn with_document_symbol(
    file_path: &str,
    content: &str,
    mut locations: Vec<SymbolLocation>,
) -> Vec<SymbolLocation> {
    let is_markdown = matches!(
        crate::ast::Lang::from_path(std::path::Path::new(file_path)),
        Some(crate::ast::Lang::Markdown)
    );
    if !is_markdown {
        return locations;
    }
    let document = markdown_document(content, &fallback_title(file_path));
    locations
        .retain(|l| l.line != DOCUMENT_LINE || l.kind.as_deref() != Some(DOCUMENT_SYMBOL_KIND));
    locations.insert(
        0,
        SymbolLocation::new(file_path, document.title, DOCUMENT_LINE, 0, true)
            .with_kind(Some(DOCUMENT_SYMBOL_KIND.to_string())),
    );
    locations
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEMORY: &str = "---\nname: exit-code-engolido\ndescription: \"`cmd | tail` faz $? ser do tail\"\nmetadata:\n  type: feedback\n---\n\nLi EXIT=0 de dois comandos que falharam.\n\n**Why:** o pipe engole o status.\n";

    /// A title rides on every chunk under it, so its length multiplies. Measured
    /// 13/09/2026: a PDF converted to markdown had an 841.812-character heading
    /// line; its ~1.400 pieces each carried it — 1,2 GB of text for a 7,7 MB file,
    /// and the daemon's rebuild jumped by 2,5 GB. The title is capped; its words
    /// still reach the index through the chunk bodies.
    #[test]
    fn a_giant_heading_is_capped_so_chunks_stay_linear_in_the_file() {
        let giant = format!(
            "# {}\n{}\n",
            "palavra ".repeat(20_000),
            "corpo ".repeat(2_000)
        );
        assert!(
            atx_heading(giant.lines().next().unwrap())
                .unwrap()
                .chars()
                .count()
                <= TITLE_MAX_CHARS
        );
        let chunks = markdown_chunks(&giant, "stem", 600);
        let text_bytes: usize = chunks.iter().map(|c| c.text.len() + c.title.len()).sum();
        assert!(
            text_bytes < 4 * giant.len(),
            "chunk text must grow with the file, not with title x pieces: {text_bytes} for {}",
            giant.len()
        );
        assert!(
            chunks
                .iter()
                .all(|c| c.title.chars().count() <= TITLE_MAX_CHARS + 8)
        );
        assert!(
            chunks.iter().any(|c| c.text.contains("palavra palavra")),
            "the heading words are still indexed through the body"
        );
    }

    #[test]
    fn the_document_text_is_the_description_then_the_capped_body() {
        let text = document_text(MEMORY, 10_000);
        assert!(
            text.starts_with("`cmd | tail` faz $? ser do tail Li EXIT=0"),
            "{text}"
        );
        assert!(text.ends_with("o pipe engole o status."), "{text}");
        assert!(!text.contains("type: feedback"), "{text}");
        let capped = document_text(MEMORY, 6);
        assert!(
            capped.ends_with("Li EXI"),
            "the cap applies to the body: {capped}"
        );
        assert_eq!(document_text("sem frontmatter", 100), "sem frontmatter");
    }

    #[test]
    fn a_memory_without_headings_is_one_document_named_by_its_frontmatter() {
        assert!(
            markdown_sections(MEMORY).is_empty(),
            "no heading, no section"
        );
        let doc = markdown_document(MEMORY, "fallback");
        assert_eq!(doc.title, "exit-code-engolido");
        assert_eq!(doc.line, DOCUMENT_LINE);
        assert!(
            doc.text.starts_with("`cmd | tail` faz $? ser do tail"),
            "{}",
            doc.text
        );
        assert!(
            doc.text
                .contains("Li EXIT=0 de dois comandos que falharam."),
            "{}",
            doc.text
        );
        assert!(
            !doc.text.contains("type: feedback"),
            "frontmatter keys never leak into the text"
        );
    }

    #[test]
    fn sections_run_to_the_next_heading_and_skip_fences_and_frontmatter() {
        let md = "---\ntitle: x\n# not a heading in frontmatter\n---\n# Top\nintro words\n## Child\n```bash\n# a shell comment, not a heading\nrm -i x\n```\nafter fence\n### Deep ###\nlast\n";
        let s = markdown_sections(md);
        let titles: Vec<(&str, usize)> = s.iter().map(|x| (x.title.as_str(), x.line)).collect();
        assert_eq!(titles, vec![("Top", 5), ("Child", 7), ("Deep", 13)]);
        assert_eq!(s[0].text, "intro words");
        assert!(
            s[1].text.contains("# a shell comment, not a heading"),
            "fenced lines are text: {}",
            s[1].text
        );
        assert!(s[1].text.ends_with("after fence"), "{}", s[1].text);
        assert_eq!(
            s[2].text, "last",
            "closing hashes are not part of the title"
        );
    }

    #[test]
    fn the_document_title_falls_back_to_the_first_heading_then_to_the_stem() {
        assert_eq!(
            markdown_document("# Título Real\ncorpo\n", "stem").title,
            "Título Real"
        );
        assert_eq!(markdown_document("só corpo\n", "stem").title, "stem");
        assert_eq!(fallback_title("dir/notes.md"), "notes");
        assert_eq!(
            fallback_title("crates/x/src/char_classes/mod.rs"),
            "char_classes"
        );
        assert_eq!(
            fallback_title("@companion/skills/academic-research-writer/SKILL.md"),
            "academic-research-writer"
        );
        assert_eq!(
            fallback_title("lib.rs"),
            "lib",
            "no parent directory: the stem stays"
        );
    }

    #[test]
    fn an_unclosed_frontmatter_is_body_and_not_a_heading_is_not_a_section() {
        let md = "---\nname: never-closed\n#hashtag without space\n    # indented four spaces\n";
        assert!(markdown_sections(md).is_empty());
        let doc = markdown_document(md, "stem");
        assert_eq!(doc.title, "stem", "unclosed frontmatter carries no name");
        assert!(doc.text.contains("name: never-closed"));
    }

    #[test]
    fn text_is_capped_by_characters_without_splitting_a_code_point() {
        // Words of four characters: the cap lands on a word boundary or inside a
        // word, never past it, and a partial word is cut by chars, not bytes.
        let body = "ação ".repeat(SECTION_TEXT_MAX_CHARS);
        let s = markdown_sections(&format!("# T\n{body}"));
        let n = s[0].text.chars().count();
        assert!(
            (SECTION_TEXT_MAX_CHARS - 5..=SECTION_TEXT_MAX_CHARS).contains(&n),
            "{n}"
        );
        let odd = "ação".repeat(SECTION_TEXT_MAX_CHARS); // one word longer than the cap
        let cut = markdown_sections(&format!("# T\n{odd}"));
        assert_eq!(
            cut[0].text.chars().count(),
            SECTION_TEXT_MAX_CHARS,
            "a single long word is cut at the cap"
        );
        assert!(
            odd.starts_with(&cut[0].text),
            "the cut is a prefix of the source — no code point split or mangled"
        );
        let doc = markdown_document(&body, "t");
        assert!(doc.text.chars().count() <= DOCUMENT_TEXT_MAX_CHARS);
    }

    #[test]
    fn chunks_cover_the_whole_body_under_the_heading_in_effect() {
        let body: String = (0..30)
            .map(|i| format!("linha {i} com palavras suficientes para encher\n"))
            .collect();
        let md = format!(
            "---\nname: longa\n---\npreâmbulo antes de tudo\n# Primeira\n{body}## Segunda\nfim da nota\n"
        );
        let chunks = markdown_chunks(&md, "stem", 200);
        let joined: String = chunks
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        for i in 0..30 {
            assert!(
                joined.contains(&format!("linha {i} com")),
                "line {i} is covered"
            );
        }
        assert_eq!(
            (chunks[0].title.as_str(), chunks[0].line),
            ("longa", 4),
            "the preamble belongs to the document"
        );
        assert_eq!(
            (chunks[1].title.as_str(), chunks[1].line),
            ("Primeira", 5),
            "a heading starts a chunk"
        );
        assert!(chunks.iter().all(|c| c.text.chars().count() <= 200));
        let last = chunks.last().expect("chunks");
        assert_eq!(
            (last.title.as_str(), last.text.as_str()),
            ("Segunda", "## Segunda fim da nota")
        );
        let ids: std::collections::HashSet<(String, usize)> =
            chunks.iter().map(|c| (c.title.clone(), c.line)).collect();
        assert_eq!(
            ids.len(),
            chunks.len(),
            "no two chunks share a (title, line) id"
        );
        let giant = format!("# T\n{}\n", "x".repeat(450));
        let pieces = markdown_chunks(&giant, "stem", 200);
        let ids: Vec<(&str, usize)> = pieces.iter().map(|c| (c.title.as_str(), c.line)).collect();
        assert_eq!(
            ids,
            vec![("T", 1), ("T", 2), ("T (2)", 2), ("T (3)", 2)],
            "the heading line, then one long line split into pieces with distinct ids"
        );
        assert_eq!(
            frontmatter_description("---\ndescription: \"d\"\n---\n").as_deref(),
            Some("d")
        );
    }

    #[test]
    fn only_markdown_gets_a_document_symbol_and_never_twice() {
        let heading = SymbolLocation::new("@companion/rules/x.md", "Top", 1, 0, true)
            .with_kind(Some("heading".to_string()));
        let once = with_document_symbol("@companion/rules/x.md", "# Top\nbody\n", vec![heading]);
        assert_eq!(once.len(), 2);
        assert_eq!(
            (
                once[0].symbol_name.as_str(),
                once[0].line,
                once[0].kind.as_deref()
            ),
            ("Top", DOCUMENT_LINE, Some(DOCUMENT_SYMBOL_KIND))
        );
        let twice = with_document_symbol("@companion/rules/x.md", "# Top\nbody\n", once);
        assert_eq!(
            twice.len(),
            2,
            "idempotent: re-applying replaces the document row"
        );
        let code = with_document_symbol("src/lib.rs", "pub fn f() {}", Vec::new());
        assert!(code.is_empty(), "code files are untouched");
    }
}
