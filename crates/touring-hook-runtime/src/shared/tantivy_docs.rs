//! Search documents for ONE indexed file — the single builder shared by the
//! rebuild, `touring tantivy reindex` and the hook writers (2026-09-13).
//!
//! Until this module the search index was fed from three places that did not
//! agree: `tantivy reindex` copied symbol rows with no text, the post-edit hooks
//! upserted one `file` document per edit, and `index rebuild` — the command that
//! seals a generation — never wrote to it at all, so `search` answered from
//! whatever the last manual reindex left behind (measured: every companion root
//! walked by a rebuild was absent from `tantivy search`). Markdown adds the other
//! half: a heading symbol is searchable by its title only, so the text of a
//! memory, a rule or a skill never reached the index.
//!
//! [`docs_for_file`] turns a file's stored symbols into documents — every row as
//! `reindex` always did, plus, for markdown, the section text on each heading and
//! the document text on the file-level symbol (`ast::markdown_text`).
//! [`refresh_file`] replaces what the index holds for that file.

use crate::tantivy_index::{SymbolDoc, TantivyIndex, TantivyIndexError, extension_to_language};
use touring_code::ast::SymbolLocation;
use touring_code::ast::{doc_text, markdown_text};

/// The `crates/<name>/` component of a path, the facet the index groups by.
fn crate_name_of(file_path: &str) -> Option<String> {
    let parts: Vec<&str> = file_path.split('/').collect();
    parts
        .windows(2)
        .find(|w| w.first().copied() == Some("crates"))
        .and_then(|w| w.get(1).map(|s| (*s).to_owned()))
}

/// The words of a storage key, extension dropped: `crates/touring-ceg/src/
/// gateway/classify.rs` → `crates touring-ceg src gateway classify` (the field's
/// analyzer splits `-` and `_`). A file's name is often the best summary of what
/// it holds, and the path was invisible to every query until this field.
fn path_words(key: &str) -> String {
    let without_ext = key.rsplit_once('.').map_or(key, |(stem, _)| stem);
    without_ext.trim_start_matches('@').replace('/', " ")
}

/// Characters an index name keeps: an identifier or a title, never a page.
const NAME_MAX_CHARS: usize = 256;

/// One search document with the fields every builder below shares.
fn doc(
    key: &str,
    name: String,
    line: usize,
    text: Option<String>,
    language: &str,
    crate_name: Option<&String>,
) -> SymbolDoc {
    let symbol_name = if name.chars().count() > NAME_MAX_CHARS {
        name.chars().take(NAME_MAX_CHARS).collect()
    } else {
        name
    };
    SymbolDoc {
        symbol_name,
        file_path: key.to_string(),
        symbol_kind: "definition".to_owned(),
        module_path: Some(path_words(key)),
        docstring: text.filter(|t| !t.is_empty()),
        line_number: line as u64,
        language: language.to_string(),
        visibility: None,
        crate_name: crate_name.cloned(),
        blake3_hash: None,
        import_count: None,
        export_count: None,
        cognitive_score: None,
        functional_signature: None,
        community_id: None,
    }
}

/// The document of one stored row: a reference row keeps its kind and never
/// carries text (only definitions are described by a comment or a section).
fn sym_doc(
    key: &str,
    s: &SymbolLocation,
    text: Option<String>,
    language: &str,
    crate_name: Option<&String>,
) -> SymbolDoc {
    let mut d = doc(
        key,
        s.symbol_name.clone(),
        s.line,
        text.filter(|_| s.is_definition),
        language,
        crate_name,
    );
    if !s.is_definition {
        d.symbol_kind = "reference".to_owned();
    }
    d
}

/// Search documents for the symbols stored under `key`. `content` is the file's
/// text when the caller has it. Every document carries the file-path words
/// (`module_path`). A markdown file is indexed as consecutive chunks that cover
/// its whole body (`TOURING_TANTIVY_CHUNK_CHARS`, default 600 characters); its
/// heading symbols keep their names, and the document symbol also carries the
/// frontmatter `description` followed by the body
/// (`TOURING_TANTIVY_MD_DOC_TEXT_CHARS`, default 20 000 characters). The body thus
/// counts twice on purpose: a chunk answers a localized phrase, the whole-file
/// document answers words spread across the file. A code definition
/// carries its doc comment, and the file one module document (`//!`, module
/// docstring). Without `content` the documents carry names and paths only.
///
/// Measured on 13/09/2026 with `examples/search_eval.rs` (55 questions, train and
/// held-out): chunks beat capped document + section texts, and doc comments took
/// code questions from 6 to 9 of 10.
#[must_use]
pub fn docs_for_file(
    key: &str,
    symbols: &[SymbolLocation],
    content: Option<&str>,
) -> Vec<SymbolDoc> {
    use touring_hooks_shared::feature_flags as ff;
    let language = extension_to_language(key);
    let crate_name = crate_name_of(key);
    let crate_ref = crate_name.as_ref();
    let Some(text) = content else {
        return symbols
            .iter()
            .map(|s| sym_doc(key, s, None, &language, crate_ref))
            .collect();
    };
    if language == "markdown" {
        let fallback = markdown_text::fallback_title(key);
        let doc_chars = ff::tantivy_markdown_document_chars();
        let document_text = if doc_chars == 0 {
            markdown_text::frontmatter_description(text)
        } else {
            Some(markdown_text::document_text(text, doc_chars)).filter(|t| !t.is_empty())
        };
        let mut docs: Vec<SymbolDoc> = symbols
            .iter()
            .map(|s| {
                let description = (s.line == markdown_text::DOCUMENT_LINE)
                    .then(|| document_text.clone())
                    .flatten();
                sym_doc(key, s, description, &language, crate_ref)
            })
            .collect();
        docs.extend(
            markdown_text::markdown_chunks(text, &fallback, ff::tantivy_chunk_chars())
                .into_iter()
                .map(|c| doc(key, c.title, c.line, Some(c.text), &language, crate_ref)),
        );
        return docs;
    }
    let mut docs: Vec<SymbolDoc> = symbols
        .iter()
        .map(|s| {
            sym_doc(
                key,
                s,
                doc_text::item_doc_comment(text, &language, s.line),
                &language,
                crate_ref,
            )
        })
        .collect();
    if let Some(module) = doc_text::module_doc_comment(text, &language) {
        docs.push(doc(
            key,
            markdown_text::fallback_title(key),
            0,
            Some(module),
            &language,
            crate_ref,
        ));
    }
    docs
}

/// Replaces every document the index holds for `key` with `docs`. The deletion
/// and the additions land together at the caller's next commit (tantivy applies
/// a delete only to documents added before it, so the new ones survive).
///
/// # Errors
///
/// The first index write that fails; documents before it stay pending.
pub fn refresh_file(
    idx: &TantivyIndex,
    key: &str,
    docs: &[SymbolDoc],
) -> Result<usize, TantivyIndexError> {
    // One file is one unit for a reader: the batch commit may land before its
    // delete or after its last addition, never in between. `upsert_symbol`
    // committed at the 500th write, so a rebuild's `memory recall` served during
    // a yield could see a file with half its documents (cross-audit 14/09/2026, A10).
    idx.delete_by_file(key)?;
    for doc in docs {
        idx.stage_symbol(doc)?;
    }
    let _ = idx.commit_if_due();
    Ok(docs.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(file: &str, name: &str, line: usize, def: bool, kind: &str) -> SymbolLocation {
        SymbolLocation::new(file, name.to_string(), line, 0, def).with_kind(Some(kind.to_string()))
    }

    /// An index name is an identifier or a title, never a page: a stored symbol
    /// whose name is a whole converted table is cut to a searchable prefix.
    /// Cross-audit 14/09/2026 (A10): a file larger than the batch is published
    /// whole. The batch commit used to fire at the 500th document, leaving the
    /// file's delete and 500 of its 600 documents visible and the rest pending.
    #[test]
    fn a_file_larger_than_the_batch_is_published_whole() {
        let dir = tempfile::tempdir().expect("dir");
        let idx = TantivyIndex::open_or_create(dir.path()).expect("index");
        let key = "src/big.rs";
        let locations: Vec<SymbolLocation> = (0..600)
            .map(|i| loc(key, &format!("item_{i}"), i + 1, true, "function"))
            .collect();
        let docs = docs_for_file(key, &locations, None);
        assert_eq!(docs.len(), 600);
        refresh_file(&idx, key, &docs).expect("refresh");
        assert_eq!(
            idx.stats().total_docs,
            600,
            "the commit after the file publishes every document of it"
        );
    }

    #[test]
    fn a_giant_symbol_name_is_cut_to_a_searchable_prefix() {
        let key = "src/huge.rs";
        let name = "n".repeat(10_000);
        let docs = docs_for_file(key, &[loc(key, &name, 1, true, "function")], None);
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].symbol_name.chars().count(), NAME_MAX_CHARS);
    }

    #[test]
    fn markdown_definitions_carry_their_text_and_everything_else_carries_none() {
        let key = "@companion/memory/exit-code.md";
        let content = "---\nname: exit-code\ndescription: o pipe engole o status\n---\nLi EXIT=0 falso.\n# Seção\ncorpo da seção\n";
        let symbols = markdown_text::with_document_symbol(
            key,
            content,
            // Line 6: four frontmatter lines, one body line, then the heading.
            vec![
                loc(key, "Seção", 6, true, "heading"),
                loc(key, "call", 7, false, "call"),
            ],
        );
        let docs = docs_for_file(key, &symbols, Some(content));
        // Symbols keep names; the document carries description + body; chunks the body.
        let document = docs.iter().find(|d| d.line_number == 0).expect("document");
        assert_eq!(document.symbol_name, "exit-code");
        let document_text = document.docstring.as_deref().expect("document text");
        assert!(
            document_text.starts_with("o pipe engole o status Li EXIT=0 falso."),
            "{document_text}"
        );
        assert!(document_text.contains("corpo da seção"), "{document_text}");
        let heading = docs
            .iter()
            .find(|d| d.line_number == 6 && d.symbol_kind == "definition" && d.docstring.is_none());
        assert!(
            heading.is_some(),
            "the heading symbol is a names-only document: {docs:?}"
        );
        let chunks: Vec<&SymbolDoc> = docs
            .iter()
            .filter(|d| d.line_number != 0 && d.docstring.is_some())
            .collect();
        let body: String = chunks
            .iter()
            .filter_map(|d| d.docstring.as_deref())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(body.contains("Li EXIT=0 falso."), "{body}");
        assert!(body.contains("corpo da seção"), "{body}");
        let reference = docs
            .iter()
            .find(|d| d.symbol_kind == "reference")
            .expect("reference");
        assert_eq!(
            reference.docstring, None,
            "a reference row never carries text"
        );
        assert!(
            docs.iter()
                .all(|d| d.file_path == key && d.language == "markdown")
        );
    }

    #[test]
    fn code_files_and_missing_content_keep_the_names_only_shape() {
        let rs = docs_for_file(
            "crates/touring-x/src/lib.rs",
            &[loc("crates/touring-x/src/lib.rs", "f", 1, true, "function")],
            Some("pub fn f() {}"),
        );
        assert_eq!(rs.len(), 1, "no module doc, no module document");
        assert_eq!(rs[0].docstring, None, "no doc comment, no text");
        assert_eq!(rs[0].crate_name.as_deref(), Some("touring-x"));
        let documented = "//! Fuses rankings.\n/// Adds one.\npub fn f() {}\n";
        let rs = docs_for_file(
            "crates/touring-x/src/fuse.rs",
            &[loc(
                "crates/touring-x/src/fuse.rs",
                "f",
                3,
                true,
                "function",
            )],
            Some(documented),
        );
        assert_eq!(
            rs[0].docstring.as_deref(),
            Some("Adds one."),
            "the item's doc comment is its text"
        );
        let module = rs
            .iter()
            .find(|d| d.line_number == 0)
            .expect("module document");
        assert_eq!(
            (module.symbol_name.as_str(), module.docstring.as_deref()),
            ("fuse", Some("Fuses rankings."))
        );
        let md = docs_for_file(
            "docs/a.md",
            &[loc("docs/a.md", "A", 1, true, "heading")],
            None,
        );
        assert_eq!(
            md[0].docstring, None,
            "no content, no text — never a stale guess"
        );
    }
}
