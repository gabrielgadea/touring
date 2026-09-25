//! The `search_index` submodule: the `.claude/` document preprocessor as a
//! PyO3 class (Gabriel's «exporte todos os tipos CLK»; the analise's
//! `indexing/tests/test_pyo3_bindings.py` measures the contract — T1-T8).
//!
//! Wraps `touring_intelligence::ann::search_index::SearchPreprocessor`:
//! `SearchPreprocessor()` → `preprocess(path, content) -> PreprocessedDoc`,
//! plus the batch form the test contract calls (`preprocess_batch`).

use pyo3::prelude::*;
use touring_intelligence::ann::search_index::{
    PreprocessedDoc as RsDoc, SearchPreprocessor as RsPreprocessor,
};

/// A document preprocessed for Tantivy indexing (the Rust `PreprocessedDoc`).
#[pyclass(name = "PreprocessedDoc", frozen, get_all, from_py_object)]
#[derive(Debug, Clone)]
pub struct PreprocessedDoc {
    /// Source path of the document.
    pub path: String,
    /// File name of the document.
    pub name: String,
    /// Detected document type (e.g. `skill`, `hook`, `rule`).
    pub doc_type: String,
    /// Code-aware tokens extracted from the content.
    pub content_tokens: Vec<String>,
    /// Code symbols (fn/class/struct names) found in the content.
    pub symbols: Vec<String>,
    /// Trigger phrases extracted from the content.
    pub triggers: Vec<String>,
    /// `.claude/` dependency paths referenced by the content.
    pub dependencies: Vec<String>,
    /// Logical module the document belongs to.
    pub module: String,
}

impl From<RsDoc> for PreprocessedDoc {
    fn from(d: RsDoc) -> Self {
        Self {
            path: d.path,
            name: d.name,
            doc_type: d.doc_type,
            content_tokens: d.content_tokens,
            symbols: d.symbols,
            triggers: d.triggers,
            dependencies: d.dependencies,
            module: d.module,
        }
    }
}

#[pymethods]
impl PreprocessedDoc {
    fn __repr__(&self) -> String {
        format!(
            "PreprocessedDoc(path={:?}, doc_type={:?}, module={:?})",
            self.path, self.doc_type, self.module
        )
    }
}

/// The `.claude/` document preprocessor (code-aware tokenizer + extractor).
#[pyclass(name = "SearchPreprocessor")]
pub struct SearchPreprocessor {
    inner: RsPreprocessor,
}

#[pymethods]
impl SearchPreprocessor {
    #[new]
    fn new() -> Self {
        Self {
            inner: RsPreprocessor::new(),
        }
    }

    /// Preprocess one document into a `PreprocessedDoc`.
    fn preprocess(&self, path: &str, content: &str) -> PreprocessedDoc {
        self.inner.preprocess(path, content).into()
    }

    /// The batch form (the test contract's T6/T7): one doc per (path,
    /// content) pair, in order, equivalent to calling `preprocess` per item.
    fn preprocess_batch(&self, docs: Vec<(String, String)>) -> Vec<PreprocessedDoc> {
        docs.iter()
            .map(|(path, content)| self.preprocess(path, content))
            .collect()
    }
}

/// Register the `search_index` submodule under the package (the simd pattern).
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let submodule = PyModule::new(m.py(), "search_index")?;
    submodule.add_class::<SearchPreprocessor>()?;
    submodule.add_class::<PreprocessedDoc>()?;
    m.add_submodule(&submodule)?;
    Ok(())
}
