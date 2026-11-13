//! The one door from a path or a language to a grammar and a bounded parse.
//!
//! Before this module the crate answered "which grammar reads this file?" in three
//! places (the `ast` extension match, the VGP table, the polyglot table) and built a
//! `tree_sitter::Parser` by hand in eighteen. Now there is one table of extensions
//! ([`EXTENSIONS`]), one parser factory ([`new_parser`]), one per-thread parser
//! cache, and one time budget ([`parse_budget`]): every parse of the crate goes
//! through [`Grammar::parse`], [`parse_str`] or [`parse_chunks`].
//!
//! # Interface
//!
//! - Identity: [`Grammar::of_extension`] / [`Grammar::of_path`] (a file's grammar,
//!   dialect-aware: `.ts` is [`Grammar::TypeScript`], `.tsx` is [`Grammar::Tsx`]),
//!   [`Grammar::of_lang`] / [`Grammar::lang`] (the bridge to [`Lang`]),
//!   [`Grammar::name`] / [`Grammar::from_name`] (the VL0 §2 `lang` string).
//! - Parse: [`Grammar::parse`] (a string), [`parse_str`] (a string, optionally
//!   against an old tree), [`parse_chunks`] (a chunked reader, the rope path).
//!   They answer `Result<Tree, `[`Refusal`]`>`: the reason there is no tree.
//!
//! # TypeScript is two grammars
//!
//! `<T>expr` is a cast in `.ts` and an unclosed JSX element in `.tsx`. A caller that
//! has a path decides it with [`Grammar::of_extension`]. A caller that only has an
//! [`ast::Lang`](Lang) cannot: `Lang::TypeScript` does not carry the dialect, so
//! [`Grammar::of_lang`] reads it as TSX, the superset that also reads a `.ts` file
//! without JSX. That is a limit of `Lang` (a `Lang::Tsx` variant would break the
//! exhaustive `match`es of the crates that consume it), stated here once.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::ControlFlow;
use std::path::Path;
use std::time::{Duration, Instant};

use tree_sitter::{Language, ParseOptions, ParseState, Parser, Point, Tree};

use crate::ast::languages::Lang;

/// A tree-sitter grammar the crate can parse with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub(crate) enum Grammar {
    /// Python (`.py`, `.pyi`).
    Python,
    /// Rust (`.rs`).
    Rust,
    /// TypeScript without JSX (`.ts`, `.mts`, `.cts`).
    TypeScript,
    /// TypeScript with JSX (`.tsx`).
    Tsx,
    /// JavaScript, JSX included (`.js`, `.jsx`, `.mjs`, `.cjs`).
    JavaScript,
    /// Shell (`.sh`, `.bash`, `.zsh`).
    Bash,
    /// HTML (`.html`, `.htm`).
    Html,
    /// CSS (`.css`; `.scss` is read by it too).
    Css,
    /// Markdown (`.md`; `.mdx` is read by it too).
    Markdown,
    /// JSON (`.json`; `.jsonc` is read by it too).
    Json,
    /// TOML (`.toml`).
    Toml,
    /// YAML (`.yaml`, `.yml`).
    Yaml,
    /// Go (`.go`), behind `more-languages` like [`Lang::Go`].
    #[cfg(feature = "more-languages")]
    Go,
    /// Java (`.java`), behind `more-languages` like [`Lang::Java`].
    #[cfg(feature = "more-languages")]
    Java,
}

/// How far an extension lookup reaches. The ordering is the reach: a wider lookup
/// sees every row a narrower one sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Reach {
    /// Only extensions that ARE the language (`.css` is CSS; `.scss` is not).
    Exact,
    /// Also the extensions of a dialect or superset the grammar happens to read
    /// (`.scss` as CSS, `.mdx` as Markdown, `.jsonc` as JSON).
    WithNeighbours,
}

/// Why a parse produced no tree. Callers that only need "a tree or not" read
/// `.ok()`; the ones that report map the variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The tree-sitter runtime rejected the grammar (an ABI mismatch with the
    /// grammar crate); carries the runtime's own message.
    Rejected(String),
    /// The parse outlived [`parse_budget`] and was halted.
    Halted,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected(why) => write!(
                f,
                "grammar rejected by the tree-sitter runtime (ABI mismatch): {why}"
            ),
            Self::Halted => f.write_str("tree-sitter parse outlived its budget and was halted"),
        }
    }
}

/// Every grammar. [`Grammar::from_name`] scans it; a test pins it to the enum, so a
/// variant added without a line here fails instead of going unnamed.
const ALL: &[Grammar] = &[
    Grammar::Python,
    Grammar::Rust,
    Grammar::TypeScript,
    Grammar::Tsx,
    Grammar::JavaScript,
    Grammar::Bash,
    Grammar::Html,
    Grammar::Css,
    Grammar::Markdown,
    Grammar::Json,
    Grammar::Toml,
    Grammar::Yaml,
    #[cfg(feature = "more-languages")]
    Grammar::Go,
    #[cfg(feature = "more-languages")]
    Grammar::Java,
];

/// THE extension table: extension (lowercase, no dot), its grammar, and the
/// [`Reach`] from which the row is visible. Add a row here and nowhere else.
const EXTENSIONS: &[(&str, Grammar, Reach)] = &[
    ("py", Grammar::Python, Reach::Exact),
    ("pyi", Grammar::Python, Reach::Exact),
    ("rs", Grammar::Rust, Reach::Exact),
    ("ts", Grammar::TypeScript, Reach::Exact),
    ("mts", Grammar::TypeScript, Reach::Exact),
    ("cts", Grammar::TypeScript, Reach::Exact),
    ("tsx", Grammar::Tsx, Reach::Exact),
    ("js", Grammar::JavaScript, Reach::Exact),
    ("jsx", Grammar::JavaScript, Reach::Exact),
    ("mjs", Grammar::JavaScript, Reach::Exact),
    ("cjs", Grammar::JavaScript, Reach::Exact),
    ("sh", Grammar::Bash, Reach::Exact),
    ("bash", Grammar::Bash, Reach::Exact),
    ("zsh", Grammar::Bash, Reach::Exact),
    ("html", Grammar::Html, Reach::Exact),
    ("htm", Grammar::Html, Reach::Exact),
    ("css", Grammar::Css, Reach::Exact),
    ("scss", Grammar::Css, Reach::WithNeighbours),
    ("md", Grammar::Markdown, Reach::Exact),
    ("mdx", Grammar::Markdown, Reach::WithNeighbours),
    ("json", Grammar::Json, Reach::Exact),
    ("jsonc", Grammar::Json, Reach::WithNeighbours),
    ("toml", Grammar::Toml, Reach::Exact),
    ("yaml", Grammar::Yaml, Reach::Exact),
    ("yml", Grammar::Yaml, Reach::Exact),
    #[cfg(feature = "more-languages")]
    ("go", Grammar::Go, Reach::Exact),
    #[cfg(feature = "more-languages")]
    ("java", Grammar::Java, Reach::Exact),
];

impl Grammar {
    /// The grammar that reads an [`ast::Lang`](Lang). Blind to the dialect:
    /// `Lang::TypeScript` reads TSX (see the module docs).
    pub(crate) fn of_lang(lang: Lang) -> Self {
        match lang {
            Lang::Python => Self::Python,
            Lang::Rust => Self::Rust,
            Lang::TypeScript => Self::Tsx,
            Lang::JavaScript => Self::JavaScript,
            Lang::Bash => Self::Bash,
            Lang::Html => Self::Html,
            Lang::Css => Self::Css,
            Lang::Markdown => Self::Markdown,
            Lang::Json => Self::Json,
            Lang::Toml => Self::Toml,
            Lang::Yaml => Self::Yaml,
            #[cfg(feature = "more-languages")]
            Lang::Go => Self::Go,
            #[cfg(feature = "more-languages")]
            Lang::Java => Self::Java,
        }
    }

    /// The [`ast::Lang`](Lang) of this grammar (`.ts` and `.tsx` are both
    /// `Lang::TypeScript`).
    pub(crate) fn lang(self) -> Lang {
        match self {
            Self::Python => Lang::Python,
            Self::Rust => Lang::Rust,
            Self::TypeScript | Self::Tsx => Lang::TypeScript,
            Self::JavaScript => Lang::JavaScript,
            Self::Bash => Lang::Bash,
            Self::Html => Lang::Html,
            Self::Css => Lang::Css,
            Self::Markdown => Lang::Markdown,
            Self::Json => Lang::Json,
            Self::Toml => Lang::Toml,
            Self::Yaml => Lang::Yaml,
            #[cfg(feature = "more-languages")]
            Self::Go => Lang::Go,
            #[cfg(feature = "more-languages")]
            Self::Java => Lang::Java,
        }
    }

    /// The grammar of a file extension, dialect-aware. `ext` is matched as written
    /// (lowercase, no dot): reading the case is the caller's policy, because the
    /// readers disagree on it (`Lang::from_path` matches the case as written, the
    /// VGP and polyglot readers lowercase first).
    pub(crate) fn of_extension(ext: &str, reach: Reach) -> Option<Self> {
        EXTENSIONS
            .iter()
            .find(|(known, _, from)| *known == ext && *from <= reach)
            .map(|(_, grammar, _)| *grammar)
    }

    /// [`Grammar::of_extension`] of the extension of `path`.
    pub(crate) fn of_path(path: &Path, reach: Reach) -> Option<Self> {
        Self::of_extension(path.extension()?.to_str()?, reach)
    }

    /// The canonical lowercase name (the `lang` string of VL0 §2: `python`,
    /// `rust`, `typescript`, `tsx`, `javascript`, `html`, `css`, …).
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::JavaScript => "javascript",
            Self::Bash => "bash",
            Self::Html => "html",
            Self::Css => "css",
            Self::Markdown => "markdown",
            Self::Json => "json",
            Self::Toml => "toml",
            Self::Yaml => "yaml",
            #[cfg(feature = "more-languages")]
            Self::Go => "go",
            #[cfg(feature = "more-languages")]
            Self::Java => "java",
        }
    }

    /// The inverse of [`Grammar::name`] (`None` for any other string, spelled
    /// exactly: `"Python"` and `"ts"` are not names). Derived from `name`, so the
    /// two can not disagree.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        ALL.iter().copied().find(|grammar| grammar.name() == name)
    }

    /// The tree-sitter language object. Queries compile against it too, so the
    /// grammar a query is written for and the grammar that parses can not drift.
    pub(crate) fn language(self) -> Language {
        match self {
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Self::Bash => tree_sitter_bash::LANGUAGE.into(),
            Self::Html => tree_sitter_html::LANGUAGE.into(),
            Self::Css => tree_sitter_css::LANGUAGE.into(),
            Self::Markdown => tree_sitter_md::LANGUAGE.into(),
            Self::Json => tree_sitter_json::LANGUAGE.into(),
            Self::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
            Self::Yaml => tree_sitter_yaml::LANGUAGE.into(),
            #[cfg(feature = "more-languages")]
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            #[cfg(feature = "more-languages")]
            Self::Java => tree_sitter_java::LANGUAGE.into(),
        }
    }

    /// Parse `source` with this grammar, bounded in time.
    ///
    /// Note a returned tree may still contain ERROR nodes: the grammars recover,
    /// and judging them is the caller's call.
    pub(crate) fn parse(self, source: &str) -> Result<Tree, Refusal> {
        parse_str(&self.language(), source, None)
    }
}

/// [`Grammar::parse`] for a bare [`Language`], optionally against an `old_tree`
/// already `edit()`ed to the new text (the incremental path).
pub(crate) fn parse_str(
    language: &Language,
    source: &str,
    old_tree: Option<&Tree>,
) -> Result<Tree, Refusal> {
    let bytes = source.as_bytes();
    parse_chunks(
        language,
        bytes.len(),
        &mut |offset: usize, _: Point| bytes.get(offset..).unwrap_or(&[]),
        old_tree,
    )
}

/// [`parse_str`] over a chunked reader (the rope path): `read(byte, point)` returns
/// the text from that offset on, `len_bytes` is the whole length (it sizes the budget).
pub(crate) fn parse_chunks<T: AsRef<[u8]>, F: FnMut(usize, Point) -> T>(
    language: &Language,
    len_bytes: usize,
    read: &mut F,
    old_tree: Option<&Tree>,
) -> Result<Tree, Refusal> {
    PARSERS.with(|cell| match cell.try_borrow_mut() {
        Ok(mut parsers) => {
            let parser = match parsers.entry(language.clone()) {
                std::collections::hash_map::Entry::Occupied(slot) => slot.into_mut(),
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(new_parser(language)?)
                }
            };
            run(parser, len_bytes, read, old_tree)
        }
        // Re-entered from inside a parse on this thread (the reader called back into
        // the door): the cached parser is mid-parse, so this one parses on a parser
        // of its own instead of panicking on the `RefCell`.
        Err(_) => run(&mut new_parser(language)?, len_bytes, read, old_tree),
    })
}

thread_local! {
    /// One parser per (thread, grammar): `Parser` is mutable and `!Send`, `Language`
    /// is `Hash + Eq` by the grammar's pointer. No lock, no contention.
    static PARSERS: RefCell<HashMap<Language, Parser>> = RefCell::new(HashMap::new());
}

/// THE parser factory: the only `Parser::new()` of the crate (a test guards it).
fn new_parser(language: &Language) -> Result<Parser, Refusal> {
    let mut parser = Parser::new();
    parser
        .set_language(language)
        .map_err(|e| Refusal::Rejected(e.to_string()))?;
    Ok(parser)
}

/// Wall-clock budget of one parse: 2 s, plus 2 s per MiB of source.
///
/// Tree-sitter parses megabytes per second, so a real file finishes far inside
/// it; the budget only ever cuts a parse that stopped making progress.
fn parse_budget(len_bytes: usize) -> Duration {
    #[cfg(test)]
    if let Some(budget) = budget_override() {
        return budget;
    }
    const BASE_MS: u64 = 2_000;
    const PER_MIB_MS: u64 = 2_000;
    Duration::from_millis(BASE_MS + PER_MIB_MS * (len_bytes as u64 / (1024 * 1024)))
}

/// Parse through `parser`, halting when the parse outlives [`parse_budget`]
/// (cross-audit R2-25).
///
/// With the TypeScript grammar 0.23.2 on runtime 0.26.9, an 86-byte ASCII input
/// keeps `ts_parser_parse` in error recovery (`ts_language_next_state`) forever:
/// the fuzz suite hung and, under the full workspace run, crashed. The daemon
/// parses every indexed `.ts`, so one such file held the project actor for good.
/// A halted parser is reset, because it keeps its state to resume the SAME
/// document and would corrupt the next one.
fn run<T: AsRef<[u8]>, F: FnMut(usize, Point) -> T>(
    parser: &mut Parser,
    len_bytes: usize,
    read: &mut F,
    old_tree: Option<&Tree>,
) -> Result<Tree, Refusal> {
    let budget = parse_budget(len_bytes);
    let deadline = Instant::now() + budget;
    let mut halt = |_: &ParseState| {
        if Instant::now() >= deadline {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };
    let options = ParseOptions::new().progress_callback(&mut halt);
    match parser.parse_with_options(read, old_tree, Some(options)) {
        Some(tree) => Ok(tree),
        None => {
            parser.reset();
            tracing::warn!(
                len_bytes,
                budget_ms = budget.as_millis() as u64,
                "tree-sitter parse halted: it outlived its budget without completing"
            );
            Err(Refusal::Halted)
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Test-only: replaces [`parse_budget`] on this thread.
    static BUDGET_OVERRIDE: std::cell::Cell<Option<Duration>> =
        const { std::cell::Cell::new(None) };
}

/// Test-only: the budget [`with_budget`] installed on this thread, if any.
#[cfg(test)]
pub(crate) fn budget_override() -> Option<Duration> {
    BUDGET_OVERRIDE.with(std::cell::Cell::get)
}

/// Test-only: run `f` with every parse on this thread bounded by `budget`
/// (`Duration::ZERO` halts a parse at tree-sitter's first progress check).
#[cfg(test)]
pub(crate) fn with_budget<R>(budget: Duration, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<Duration>);
    impl Drop for Restore {
        fn drop(&mut self) {
            BUDGET_OVERRIDE.with(|cell| cell.set(self.0));
        }
    }
    let _restore = Restore(BUDGET_OVERRIDE.with(|cell| cell.replace(Some(budget))));
    f()
}

/// Test-only: how many parsers this thread's cache holds.
#[cfg(test)]
fn cached_parsers() -> usize {
    PARSERS.with(|cell| cell.borrow().len())
}

/// Test-only: the extensions of `grammar` that a lookup of `reach` sees (the readers
/// that keep their own policy on top of the table prove it against this).
#[cfg(test)]
pub(crate) fn extensions(grammar: Grammar, reach: Reach) -> Vec<&'static str> {
    EXTENSIONS
        .iter()
        .filter(|(_, g, from)| *g == grammar && *from <= reach)
        .map(|(ext, _, _)| *ext)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use strum::IntoEnumIterator;

    fn ext(ext: &str) -> Option<Grammar> {
        Grammar::of_extension(ext, Reach::Exact)
    }

    /// A sample every grammar reads without ERROR nodes.
    fn sample(grammar: Grammar) -> &'static str {
        match grammar {
            Grammar::Python => "x = 1\n",
            Grammar::Rust => "fn main() {}\n",
            Grammar::Html => "<p id=\"a\">oi</p>\n",
            Grammar::Css => ".a { color: red; }\n",
            Grammar::Bash => "echo hi\n",
            Grammar::Markdown => "# titulo\n\ntexto\n",
            Grammar::Json => "{\"a\": 1}\n",
            Grammar::Toml => "a = 1\n",
            Grammar::Yaml => "a: 1\n",
            _ => "const x = 1;\n",
        }
    }

    #[test]
    fn every_grammar_loads_in_the_runtime_and_parses_its_sample_clean() {
        for grammar in Grammar::iter() {
            let tree = grammar
                .parse(sample(grammar))
                .unwrap_or_else(|e| panic!("{}: {e}", grammar.name()));
            assert!(
                !tree.root_node().has_error(),
                "{} parses its sample clean",
                grammar.name()
            );
        }
    }

    #[test]
    fn a_dialect_is_decided_by_the_extension_and_ts_is_not_tsx() {
        assert_eq!(ext("ts"), Some(Grammar::TypeScript));
        assert_eq!(ext("mts"), Some(Grammar::TypeScript));
        assert_eq!(ext("cts"), Some(Grammar::TypeScript));
        assert_eq!(ext("tsx"), Some(Grammar::Tsx));
        // `<T>expr` is a cast in .ts and an unclosed JSX element in .tsx; JSX only exists in TSX.
        let cast = "const y = <number>x;\n";
        let jsx = "const el = <div className=\"a\" />;\n";
        let has_error = |g: Grammar, src: &str| g.parse(src).unwrap().root_node().has_error();
        assert!(!has_error(Grammar::TypeScript, cast));
        assert!(has_error(Grammar::Tsx, cast));
        assert!(!has_error(Grammar::Tsx, jsx));
        assert!(has_error(Grammar::TypeScript, jsx));
    }

    /// `Lang::TypeScript` does not carry the dialect: it reads TSX, the superset that
    /// reads a `.ts` file without JSX. The one place that limit is decided.
    #[test]
    fn a_lang_without_a_dialect_reads_tsx() {
        assert_eq!(Grammar::of_lang(Lang::TypeScript), Grammar::Tsx);
        assert_eq!(Grammar::TypeScript.lang(), Lang::TypeScript);
        assert_eq!(Grammar::Tsx.lang(), Lang::TypeScript);
        let plain_ts = "export function f(a: number): string { return String(a); }\n";
        assert!(
            !Grammar::of_lang(Lang::TypeScript)
                .parse(plain_ts)
                .unwrap()
                .root_node()
                .has_error()
        );
    }

    #[test]
    fn reach_separates_the_languages_own_extensions_from_the_neighbours() {
        for neighbour in ["scss", "mdx", "jsonc"] {
            assert_eq!(ext(neighbour), None, "{neighbour} is not exact");
            assert!(
                Grammar::of_extension(neighbour, Reach::WithNeighbours).is_some(),
                "{neighbour} is read by the neighbouring grammar"
            );
        }
        assert_eq!(
            Grammar::of_extension("scss", Reach::WithNeighbours),
            Some(Grammar::Css)
        );
        assert_eq!(
            Grammar::of_extension("mdx", Reach::WithNeighbours),
            Some(Grammar::Markdown)
        );
        assert_eq!(
            Grammar::of_extension("jsonc", Reach::WithNeighbours),
            Some(Grammar::Json)
        );
        // a wider reach never loses a row a narrower one has
        for (known, grammar, _) in EXTENSIONS {
            assert_eq!(
                Grammar::of_extension(known, Reach::WithNeighbours),
                Some(*grammar)
            );
        }
    }

    #[test]
    fn the_extension_is_matched_as_written_and_a_dotfile_has_none() {
        assert_eq!(ext("PY"), None, "the case is the caller's policy");
        let path = |p: &str| Grammar::of_path(&PathBuf::from(p), Reach::Exact);
        assert_eq!(path("src/lib.rs"), Some(Grammar::Rust));
        assert_eq!(path("a.mts"), Some(Grammar::TypeScript));
        assert_eq!(path(".py"), None, "a dotfile named `.py` has no extension");
        assert_eq!(path("Makefile"), None);
        assert_eq!(path(""), None);
    }

    #[test]
    fn the_table_has_no_duplicate_extension() {
        let mut seen = std::collections::HashSet::new();
        for (known, _, _) in EXTENSIONS {
            assert!(seen.insert(*known), "`{known}` has two rows");
            assert_eq!(*known, known.to_ascii_lowercase(), "`{known}` is lowercase");
        }
    }

    #[test]
    fn names_round_trip_and_the_lang_bridge_is_consistent() {
        assert_eq!(
            ALL.len(),
            Grammar::iter().count(),
            "ALL lists every variant once"
        );
        for grammar in Grammar::iter() {
            assert!(ALL.contains(&grammar), "{grammar:?} is missing from ALL");
            assert_eq!(Grammar::from_name(grammar.name()), Some(grammar));
            // every grammar is reachable by at least one extension
            assert!(
                EXTENSIONS.iter().any(|(_, g, _)| *g == grammar),
                "{} has no extension",
                grammar.name()
            );
            // Lang -> Grammar -> Lang is the identity; Grammar -> Lang -> Grammar is too,
            // except TypeScript, which Lang can only read as TSX.
            assert_eq!(Grammar::of_lang(grammar.lang()).lang(), grammar.lang());
            if grammar != Grammar::TypeScript {
                assert_eq!(Grammar::of_lang(grammar.lang()), grammar);
            }
        }
        for lang in <Lang as IntoEnumIterator>::iter() {
            assert_eq!(Grammar::of_lang(lang).lang(), lang);
            // `Lang::as_str` is the name, but for TypeScript (which reads TSX).
            let expected = if lang == Lang::TypeScript {
                "tsx"
            } else {
                lang.as_str()
            };
            assert_eq!(Grammar::of_lang(lang).name(), expected);
        }
        for wrong in ["", "Python", "ts", "py", "cobol", "c"] {
            assert_eq!(Grammar::from_name(wrong), None, "{wrong:?} is not a name");
        }
    }

    #[test]
    fn a_parse_that_outlives_its_budget_is_halted_and_the_next_parse_is_clean() {
        let big = "def f():\n    pass\n".repeat(3_000);
        with_budget(Duration::ZERO, || {
            assert_eq!(Grammar::Python.parse(&big).unwrap_err(), Refusal::Halted);
        });
        // The halted parser kept the state of `big`; the reset is what lets the next
        // document, on the same thread and the same cached parser, parse as itself.
        let small = Grammar::Python
            .parse("y = 2\n")
            .expect("the next parse runs");
        assert_eq!(
            small.root_node().to_sexp(),
            "(module (expression_statement (assignment left: (identifier) right: (integer))))"
        );
    }

    #[test]
    fn one_parser_per_grammar_per_thread_whoever_asks() {
        use crate::ast::parser::{IncrementalParser, ParserPool};
        // Run on a fresh thread so the count starts from nothing.
        std::thread::spawn(|| {
            assert_eq!(cached_parsers(), 0);
            Grammar::Python.parse("a = 1\n").unwrap();
            ParserPool::new().parse("b = 2\n", Lang::Python).unwrap();
            IncrementalParser::new()
                .parse_full("c = 3\n", Lang::Python)
                .unwrap();
            assert_eq!(cached_parsers(), 1, "three doors, one cached parser");
            Grammar::Rust.parse("fn f() {}\n").unwrap();
            assert_eq!(cached_parsers(), 2);
            // TypeScript and TSX are two grammars, so two parsers.
            Grammar::TypeScript.parse("let a = 1;\n").unwrap();
            Grammar::Tsx.parse("let a = 1;\n").unwrap();
            assert_eq!(cached_parsers(), 4);
        })
        .join()
        .expect("the thread did not panic");
    }

    #[test]
    fn a_reader_that_parses_again_does_not_panic_the_cache() {
        let text = "a = 1\n";
        let mut inner_ok = false;
        let mut read = |offset: usize, _: Point| -> Vec<u8> {
            // Re-entered from inside the outer parse, on the same thread.
            inner_ok = Grammar::Python.parse("z = 9\n").is_ok();
            text.as_bytes().get(offset..).unwrap_or(&[]).to_vec()
        };
        let tree = parse_chunks(&Grammar::Python.language(), text.len(), &mut read, None)
            .expect("the outer parse completes");
        assert!(inner_ok, "the nested parse completes too");
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn an_old_tree_is_reused_through_the_door() {
        let old = Grammar::Rust.parse("fn foo() {}").unwrap();
        let mut edited = old.clone();
        edited.edit(&tree_sitter::InputEdit {
            start_byte: 3,
            old_end_byte: 6,
            new_end_byte: 6,
            start_position: Point::new(0, 3),
            old_end_position: Point::new(0, 6),
            new_end_position: Point::new(0, 6),
        });
        let new = parse_str(&Grammar::Rust.language(), "fn bar() {}", Some(&edited)).unwrap();
        assert_eq!(new.root_node().to_sexp(), old.root_node().to_sexp());
        assert_eq!(new.root_node().end_byte(), 11);
    }

    /// The structural guard of C1 (rule "no two ways"): a `Parser` is built in ONE
    /// place and the grammar is set in ONE place. A second site is a second door.
    #[test]
    fn the_crate_builds_and_configures_a_parser_in_exactly_one_place() {
        let needle_new = ["Parser", "::new()"].concat();
        let needle_set = [".set_", "language("].concat();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut sites: Vec<String> = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("src is readable") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).expect("source is utf-8");
                for (n, line) in text.lines().enumerate() {
                    if line.trim_start().starts_with("//") {
                        continue;
                    }
                    let bare_new = line.match_indices(&needle_new).any(|(at, _)| {
                        !line[..at]
                            .chars()
                            .next_back()
                            .is_some_and(|c| c.is_alphanumeric() || c == '_')
                            || line[..at].ends_with("tree_sitter::")
                    });
                    if bare_new || line.contains(&needle_set) {
                        let rel = path.strip_prefix(&root).unwrap_or(&path).display();
                        sites.push(format!("{rel}:{}", n + 1));
                    }
                }
            }
        }
        assert_eq!(
            sites.len(),
            2,
            "one parser built and one grammar set, both in `new_parser` of ast/grammar.rs: {sites:?}"
        );
        assert!(
            sites.iter().all(|s| s.starts_with("ast/grammar.rs:")),
            "{sites:?}"
        );
    }
}
