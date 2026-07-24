//! CLI — Command-line interface for touring-definitions.
//!
//! Provides `touring definitions classify <file>` subcommand.

use super::classifier::SemanticClassifier;
use clap::{Parser, ValueHint};
use std::path::PathBuf;

/// Classify code symbols in a file.
#[derive(Debug, Parser)]
pub struct Classify {
    /// File to classify symbols from
    #[arg(value_hint = ValueHint::FilePath)]
    file: PathBuf,

    /// Language hint (rust, python, typescript, go, java)
    #[arg(short, long, default_value = "rust")]
    language: String,
}

impl Classify {
    /// Run the classify command.
    pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let classifier = SemanticClassifier::new();
        // REGRA #0 potencializar: validate the user-supplied --language
        // against the engine's supported set so we fail fast with a clear
        // diagnostic instead of silently producing low-confidence results.
        let supported = classifier.supported_languages();
        if !self.language.is_empty() && !supported.contains(&self.language.as_str()) {
            eprintln!(
                "warning: language '{}' has no built-in overrides (supported: {})",
                self.language,
                supported.join(", ")
            );
        }
        let source = std::fs::read_to_string(&self.file)?;
        let path = self.file.to_string_lossy();

        // Simple symbol extraction from source (placeholder)
        let symbols = extract_symbols(&source);

        for (name, _line) in symbols {
            let result = classifier.classify_with_path(&name, &path);
            println!(
                "{}: {} (confidence: {:.2}, stage: {})",
                name, result.primary, result.confidence, result.stage
            );
        }

        Ok(())
    }
}

/// Extract symbol names from source (simplified placeholder).
fn extract_symbols(source: &str) -> Vec<(String, usize)> {
    let mut symbols = Vec::new();
    for (i, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        // Skip comments and empty lines
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        // Extract potential symbol names (simplified)
        if let Some(name) = extract_identifier(trimmed) {
            symbols.push((name, i + 1));
        }
    }
    symbols
}

/// Extract identifier from line (simplified).
fn extract_identifier(line: &str) -> Option<String> {
    // Match function definitions, struct definitions, etc.
    let patterns = [
        "fn ",
        "def ",
        "function ",
        "class ",
        "struct ",
        "enum ",
        "trait ",
        "impl ",
        "type ",
        "const ",
        "static ",
        "macro_rules!",
    ];
    for pat in &patterns {
        if let Some(pos) = line.find(pat) {
            let rest = &line[pos + pat.len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_identifier() {
        assert_eq!(
            extract_identifier("fn my_function()"),
            Some("my_function".to_string())
        );
        assert_eq!(
            extract_identifier("struct MyStruct"),
            Some("MyStruct".to_string())
        );
        assert_eq!(extract_identifier("// comment"), None);
    }
}
