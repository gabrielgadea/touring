//! What a file refers to — the one place that decides it.
//!
//! The extractors beside this module (`imports`, `method_calls`, `python_uses`,
//! `module_paths`, `self_refs`) are pure: each reads a syntax tree and reports what
//! it sees. Which of what they see COUNTS as a reference is a different question, and
//! it used to be answered by whoever called them: the hook bridge subtracted the
//! re-exports from the imports, the hook runtime knew that a path-forwarding scan is
//! Rust-only, the rebuild repeated the language gates. [`FileReferences`] is the one
//! interface a consumer asks, and the extractors are private to the crate.
//!
//! The rules it keeps:
//!
//! - **Importing is not using** (decision of 18/09/2026). [`FileReferences::consumed_imports`]
//!   is [`FileReferences::imports`] minus what the file only forwards — a Rust re-export
//!   (`pub use`) it never names in its own code — or never touches — a Python name
//!   absent outside its import statements. Symbols are removed, never modules.
//! - **A re-export counts only if the file also uses it**: the same test serves the
//!   imports, the module paths and the type references.
//! - **A declaration is not a reference to itself**: the name a declaration introduces
//!   never counts as a use of it.
//! - **A language with no rule answers empty**, never an error: a consumer no longer
//!   needs to know that a pass is "Rust only".
//! - **A source that does not parse answers empty**: the extractors treat a missing
//!   tree as a blind spot, not a failure.
//!
//! Writing the answers to a database, and the order of those writes, stays with the
//! consumer: that is write policy, not a rule about references.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::imports::{ImportInfo, extract_imports, rust_names_used_outside_use, rust_reexports};
use super::method_calls::{
    extract_method_calls, extract_qualified_calls, extract_type_and_const_refs,
};
use super::module_paths::{
    ModuleDeclaration, rust_declared_module_names, rust_module_declarations, rust_module_paths,
};
use super::python_uses::{python_qualified_uses, python_unreferenced_imports};
use super::self_refs::self_referenced_names;
use crate::ast::languages::Lang;
use crate::ast::symbols::Symbol;

/// The references a file makes without importing them: called names, type and
/// constant positions, and qualified calls (`backup::run()` keeps `backup`).
///
/// The three travel together because every consumer records them together: they are
/// the INFERRED edges, guessed by name, as opposed to the imports, which are resolved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InferredReferences {
    /// Method and associated-function names called: `.m()`, `Type::f()`, bare `f()`.
    pub method_names: Vec<String>,
    /// Names in a type or constant position (`&ParsedTag`, `tags::LIMIT`), Rust only.
    pub type_refs: Vec<String>,
    /// `(qualifier, name)` of every qualified call.
    pub qualified_calls: Vec<(String, String)>,
}

/// The modules a Rust file names, and the ones it declares itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModuleReferences {
    /// Every module path the file names, longest prefixes included, deduplicated. A path
    /// only a re-export names is left out unless the file also uses what it forwards.
    pub paths: Vec<String>,
    /// The child modules the file declares in ALL three shapes (`mod x;`,
    /// `#[path = "…"] mod x;`, `mod x { … }`): the first segment of a bare path that
    /// names one of them is the file's own child, not a crate.
    pub declared: BTreeSet<String>,
}

/// One source file read as a language: the questions a wiring consumer asks about it.
///
/// A value is cheap (a borrow and a language tag); every method reads the source when
/// asked, so a consumer pays only for the questions it puts.
#[derive(Debug, Clone, Copy)]
pub struct FileReferences<'a> {
    source: &'a str,
    lang: Lang,
}

impl<'a> FileReferences<'a> {
    /// `source` read as `lang`.
    #[must_use]
    pub fn new(source: &'a str, lang: Lang) -> Self {
        Self { source, lang }
    }

    /// Every import, as written: a Rust `use` leaf, a Python `from … import`, a
    /// TypeScript `import`, and a Rust type alias to a path (`type X = a::B;`), which
    /// consumes its target the way an import does.
    #[must_use]
    pub fn imports(&self) -> Vec<ImportInfo> {
        extract_imports(self.source, self.lang)
    }

    /// The imports that are USES of what they name: [`Self::imports`] minus what the
    /// file only forwards or never touches.
    ///
    /// Rust: a re-export (`pub use`, `pub(crate) use`) is subtracted, unless the file
    /// also names the symbol in its own code (`pub use a::X;` then `[X, …]` in a
    /// table). Python: a `from x import Nome` whose `Nome` is never referenced outside
    /// the import statements (an `__all__` or `__init__.py` façade, a dead import) is
    /// subtracted. Any other language keeps every import.
    #[must_use]
    pub fn consumed_imports(&self) -> Vec<ImportInfo> {
        let imports = self.imports();
        match self.lang {
            Lang::Rust => {
                let used = rust_names_used_outside_use(self.source);
                let forwarded: HashSet<(String, String)> = rust_reexports(self.source)
                    .into_iter()
                    .flat_map(|import| {
                        let module = import.module_path;
                        import.symbols.into_iter().map(move |s| (module.clone(), s))
                    })
                    .filter(|(_, symbol)| !used.contains(symbol))
                    .collect();
                drop_symbols(imports, |module, symbol| {
                    forwarded.contains(&(module.to_string(), symbol.to_string()))
                })
            }
            Lang::Python => {
                let unreferenced = python_unreferenced_imports(self.source);
                drop_symbols(imports, |_, symbol| unreferenced.contains(symbol))
            }
            _ => imports,
        }
    }

    /// `module::symbol` of every leaf a Rust file re-exports (`pub use crate::a::Foo;`
    /// gives `crate::a::Foo`) — the full paths a path scan must not read as uses.
    /// Empty for any other language.
    #[must_use]
    pub fn forwarded_paths(&self) -> HashSet<String> {
        if self.lang != Lang::Rust {
            return HashSet::new();
        }
        rust_reexports(self.source)
            .into_iter()
            .flat_map(|import| {
                let module = import.module_path;
                import
                    .symbols
                    .into_iter()
                    .map(move |symbol| format!("{module}::{symbol}"))
            })
            .collect()
    }

    /// The references the file makes by name, with no import to resolve them.
    #[must_use]
    pub fn inferred(&self) -> InferredReferences {
        InferredReferences {
            method_names: extract_method_calls(self.source, self.lang),
            type_refs: extract_type_and_const_refs(self.source, self.lang),
            qualified_calls: extract_qualified_calls(self.source, self.lang),
        }
    }

    /// `(module_path, symbol)` for every `name.Symbol` whose `name` the file bound with
    /// an `import` or a `from … import` (`import modulo as gm` then `gm.Nome`).
    /// Empty unless the language is Python.
    #[must_use]
    pub fn python_qualified_uses(&self) -> Vec<(String, String)> {
        if self.lang != Lang::Python {
            return Vec::new();
        }
        python_qualified_uses(self.source)
    }

    /// The modules the file's paths name and the children it declares. Empty unless the
    /// language is Rust.
    #[must_use]
    pub fn module_references(&self) -> ModuleReferences {
        if self.lang != Lang::Rust {
            return ModuleReferences::default();
        }
        ModuleReferences {
            paths: rust_module_paths(self.source),
            declared: rust_declared_module_names(self.source),
        }
    }

    /// How each child module the file declares is found: by Cargo's layout, by a
    /// `#[path]` attribute, or inline. Empty unless the language is Rust.
    #[must_use]
    pub fn module_declarations(&self) -> BTreeMap<String, ModuleDeclaration> {
        if self.lang != Lang::Rust {
            return BTreeMap::new();
        }
        rust_module_declarations(self.source)
    }

    /// The public symbols among `symbols` that the file itself uses outside their own
    /// definition: the `internal_only` class. One rule for the rebuild and the edit
    /// path. Languages without a rule answer empty (Rust and Python have one).
    #[must_use]
    pub fn self_references(&self, symbols: &[Symbol]) -> BTreeSet<String> {
        self_referenced_names(self.source, symbols, self.lang)
    }
}

/// `imports` with every `(module, symbol)` that `drop` accepts removed. A module is
/// never removed: an import left with no symbol records nothing downstream.
fn drop_symbols(imports: Vec<ImportInfo>, drop: impl Fn(&str, &str) -> bool) -> Vec<ImportInfo> {
    imports
        .into_iter()
        .map(|import| {
            let symbols = import
                .symbols
                .into_iter()
                .filter(|symbol| !drop(&import.module_path, symbol))
                .collect();
            ImportInfo {
                module_path: import.module_path,
                symbols,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbols_of(imports: &[ImportInfo], module: &str) -> Vec<String> {
        imports
            .iter()
            .filter(|import| import.module_path == module)
            .flat_map(|import| import.symbols.clone())
            .collect()
    }

    /// A language with no rule answers empty on every pass that is not its own: the
    /// consumer needs no `if lang == Rust` in front of the call.
    #[test]
    fn a_pass_that_is_not_the_languages_answers_empty() {
        let prose = "pub use a::X;\nuse crate::b::Y;\nmod c;\n";
        for lang in [Lang::Markdown, Lang::Toml, Lang::Bash] {
            let refs = FileReferences::new(prose, lang);
            assert!(refs.forwarded_paths().is_empty(), "{lang:?}");
            assert!(refs.python_qualified_uses().is_empty(), "{lang:?}");
            assert_eq!(refs.module_references(), ModuleReferences::default());
            assert!(refs.module_declarations().is_empty(), "{lang:?}");
            assert!(refs.self_references(&[]).is_empty(), "{lang:?}");
        }
        let python = FileReferences::new("import m as x\nx.Nome\n", Lang::Python);
        assert!(python.forwarded_paths().is_empty(), "Rust only");
        assert!(python.module_declarations().is_empty(), "Rust only");
        assert_eq!(
            python.python_qualified_uses(),
            [("m".into(), "Nome".into())]
        );
    }

    /// Only a Rust or Python file has a rule for what it forwards: every other
    /// language keeps all of its imports.
    #[test]
    fn a_language_without_a_forwarding_rule_keeps_every_import() {
        let source = "import { a } from './x';\nexport { z } from './z';\n";
        let refs = FileReferences::new(source, Lang::TypeScript);
        let all = refs.imports();
        assert!(!all.is_empty(), "positive control: the file has imports");
        assert_eq!(
            all.iter().map(|i| &i.module_path).collect::<Vec<_>>(),
            refs.consumed_imports()
                .iter()
                .map(|i| &i.module_path)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            all.iter().map(|i| &i.symbols).collect::<Vec<_>>(),
            refs.consumed_imports()
                .iter()
                .map(|i| &i.symbols)
                .collect::<Vec<_>>()
        );
    }

    /// The module of a forwarded import survives with no symbols: downstream, an import
    /// left with nothing records nothing, but the fact that the file imports it stays.
    #[test]
    fn a_forwarded_import_loses_its_symbols_and_keeps_its_module() {
        let refs = FileReferences::new(
            "pub use alpha::Gone;\nuse beta::Kept;\nfn f(_: Kept) {}\n",
            Lang::Rust,
        );
        let consumed = refs.consumed_imports();
        assert!(
            consumed
                .iter()
                .any(|import| import.module_path == "alpha" && import.symbols.is_empty()),
            "the forwarded import keeps its module, with no symbol: {consumed:?}"
        );
        assert_eq!(symbols_of(&consumed, "beta"), ["Kept"]);
        assert_eq!(symbols_of(&refs.imports(), "alpha"), ["Gone"]);
        assert_eq!(
            refs.forwarded_paths().into_iter().collect::<Vec<_>>(),
            ["alpha::Gone"]
        );
    }

    /// Source that says nothing answers nothing, on every pass.
    #[test]
    fn an_empty_source_has_no_references() {
        for lang in [Lang::Rust, Lang::Python, Lang::TypeScript] {
            let refs = FileReferences::new("", lang);
            assert!(refs.imports().is_empty(), "{lang:?}");
            assert!(refs.consumed_imports().is_empty(), "{lang:?}");
            assert_eq!(refs.inferred(), InferredReferences::default());
            assert_eq!(refs.module_references(), ModuleReferences::default());
        }
    }
}
