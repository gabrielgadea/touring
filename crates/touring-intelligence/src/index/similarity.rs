//! SIMD-accelerated file similarity via touring-simd.
//!
//! Computes file similarity vectors from symbol profiles (kind distribution,
//! complexity, import count) and uses cosine similarity for nearest-neighbor
//! file lookups. Useful for co-edit prediction and cache pre-warming.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use touring_code::ast::symbols::Symbol;
use touring_simd::quantization::ScalarQuantizer;
use touring_simd::{CosineComputer, CosineSimilarity};

/// Dimensionality of the file feature vector.
const FEATURE_DIM: usize = 8;

/// A file's feature vector derived from its symbol profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFeatureVector {
    /// Source file path.
    pub path: PathBuf,
    /// Normalized feature vector (length = FEATURE_DIM, f32 for SIMD compat).
    pub features: Vec<f32>,
}

/// Quantized version of FileFeatureVector for 4x memory reduction.
/// f32 → u8 quantization with 97-99% accuracy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizedFileFeatureVector {
    /// Source file path.
    pub path: PathBuf,
    /// Quantized features (u8, 4x smaller than f32).
    pub quantized_features: Vec<u8>,
    /// Quantizer used for this vector (for dequantization).
    pub quantizer: ScalarQuantizer,
    /// Original dimensionality (for dequantization).
    pub dim: usize,
}

impl QuantizedFileFeatureVector {
    /// Re-quantize back to `f32` for similarity computation.
    pub fn dequantize(&self) -> Vec<f32> {
        self.quantizer.dequantize(&self.quantized_features)
    }
}

impl FileFeatureVector {
    /// Quantize this feature vector for memory-efficient storage.
    pub fn quantize(&self) -> QuantizedFileFeatureVector {
        let quantizer = ScalarQuantizer::fit(&self.features);
        let quantized = quantizer.quantize(&self.features);
        let dim = self.features.len();
        QuantizedFileFeatureVector {
            path: self.path.clone(),
            quantized_features: quantized,
            quantizer,
            dim,
        }
    }
}

impl FileFeatureVector {
    /// Build a feature vector from a file's extracted symbols.
    ///
    /// Features (8 dimensions):
    /// 0. function_ratio — fraction of symbols that are functions
    /// 1. class_ratio — fraction that are classes/structs
    /// 2. avg_complexity — mean cyclomatic complexity (normalized 0..1)
    /// 3. import_density — placeholder (requires import extraction)
    /// 4. pub_ratio — public symbols / total_symbols
    /// 5. async_ratio — async symbols / total_symbols
    /// 6. line_density — average line span (normalized)
    /// 7. symbol_count_log — log2(symbol_count + 1) / 10 (capped at 1.0)
    #[allow(clippy::indexing_slicing)] // All indices 0..7 are within FEATURE_DIM=8
    pub fn from_symbols(path: &Path, symbols: &[Symbol]) -> Self {
        let total = symbols.len().max(1) as f32;
        let mut features = vec![0.0_f32; FEATURE_DIM];

        let functions = symbols
            .iter()
            .filter(|s| matches!(s.kind, touring_code::ast::symbols::SymbolKind::Function))
            .count() as f32;
        let classes = symbols
            .iter()
            .filter(|s| {
                matches!(
                    s.kind,
                    touring_code::ast::symbols::SymbolKind::Class
                        | touring_code::ast::symbols::SymbolKind::Struct
                )
            })
            .count() as f32;
        let pub_count = symbols.iter().filter(|s| s.is_public).count() as f32;
        let async_count = symbols.iter().filter(|s| s.is_async).count() as f32;
        let avg_complexity = if symbols.is_empty() {
            0.0
        } else {
            let sum: f32 = symbols
                .iter()
                .map(|s| s.complexity.unwrap_or(1) as f32)
                .sum();
            (sum / total).min(20.0) / 20.0
        };
        let line_density = if symbols.is_empty() {
            0.0
        } else {
            let sum: f32 = symbols.iter().map(|s| s.line as f32).sum();
            (sum / total / 500.0).min(1.0)
        };

        features[0] = functions / total;
        features[1] = classes / total;
        features[2] = avg_complexity;
        features[3] = 0.0; // import_density placeholder
        features[4] = pub_count / total;
        features[5] = async_count / total;
        features[6] = line_density;
        features[7] = (((symbols.len() + 1) as f32).log2() / 10.0).min(1.0);

        Self {
            path: path.to_path_buf(),
            features,
        }
    }
}

/// Index of file feature vectors for similarity queries.
#[derive(Debug)]
pub struct FileSimilarityIndex {
    entries: HashMap<PathBuf, FileFeatureVector>,
    computer: CosineComputer,
}

impl FileSimilarityIndex {
    /// Create a new empty similarity index.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            computer: CosineComputer::new(),
        }
    }

    /// Add or update a file's feature vector.
    pub fn upsert(&mut self, path: &Path, symbols: &[Symbol]) {
        let fv = FileFeatureVector::from_symbols(path, symbols);
        self.entries.insert(path.to_path_buf(), fv);
    }

    /// Remove a file from the index.
    pub fn remove(&mut self, path: &Path) {
        self.entries.remove(path);
    }

    /// Find the k most similar files to the given path.
    ///
    /// Returns `(path, similarity_score)` pairs sorted by descending similarity.
    pub fn find_similar(&self, path: &Path, k: usize) -> Vec<(PathBuf, f64)> {
        let query = match self.entries.get(path) {
            Some(fv) => &fv.features,
            None => return vec![],
        };

        let mut results: Vec<(PathBuf, f64)> = self
            .entries
            .iter()
            .filter(|(p, _)| p.as_path() != path)
            .map(|(p, fv)| {
                let sim = self.computer.cosine(query, &fv.features);
                (p.clone(), sim)
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(k);
        results
    }

    /// Number of files in the index.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for FileSimilarityIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use touring_code::ast::symbols::{Symbol, SymbolKind};

    fn make_symbol(name: &str, kind: SymbolKind) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            line: 1,
            end_line: 10,
            is_public: true,
            complexity: Some(3),
            ..Symbol::default()
        }
    }

    #[test]
    fn test_feature_vector_from_symbols() {
        let symbols = vec![
            make_symbol("foo", SymbolKind::Function),
            make_symbol("bar", SymbolKind::Function),
            make_symbol("Baz", SymbolKind::Class),
        ];
        let fv = FileFeatureVector::from_symbols(Path::new("test.py"), &symbols);
        assert_eq!(fv.features.len(), FEATURE_DIM);
        // 2/3 functions
        assert!((fv.features[0] - 2.0 / 3.0).abs() < 0.01);
        // 1/3 classes
        assert!((fv.features[1] - 1.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn test_similarity_index_find_similar() {
        let mut index = FileSimilarityIndex::new();

        let syms_a = vec![
            make_symbol("f1", SymbolKind::Function),
            make_symbol("f2", SymbolKind::Function),
        ];
        let syms_b = vec![
            make_symbol("f3", SymbolKind::Function),
            make_symbol("f4", SymbolKind::Function),
        ];
        let syms_c = vec![
            make_symbol("C1", SymbolKind::Class),
            make_symbol("C2", SymbolKind::Class),
        ];

        index.upsert(Path::new("a.py"), &syms_a);
        index.upsert(Path::new("b.py"), &syms_b);
        index.upsert(Path::new("c.py"), &syms_c);

        // a.py and b.py should be more similar (both are function-heavy)
        let results = index.find_similar(Path::new("a.py"), 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, Path::new("b.py"));
    }

    #[test]
    fn test_similarity_index_empty() {
        let index = FileSimilarityIndex::new();
        assert!(index.is_empty());
        assert_eq!(index.find_similar(Path::new("x.py"), 5).len(), 0);
    }

    #[test]
    fn test_upsert_and_remove() {
        let mut index = FileSimilarityIndex::new();
        let syms = vec![make_symbol("f", SymbolKind::Function)];
        index.upsert(Path::new("a.py"), &syms);
        assert_eq!(index.len(), 1);
        index.remove(Path::new("a.py"));
        assert!(index.is_empty());
    }
}
