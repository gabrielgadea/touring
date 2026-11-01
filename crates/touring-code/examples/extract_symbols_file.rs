//! Runs the symbol extractor over files in sequence on one thread — the smallest
//! reproduction of a crash the rebuild hit inside tree-sitter, which a single file
//! did not reproduce (the thread-local parser carries state across files).
//!
//! ```text
//! cargo run -p touring-code --example extract_symbols_file -- <file> [iterations]
//! cargo run -p touring-code --example extract_symbols_file -- @<list of paths> [passes]
//! ```

use touring_code::ast::languages::Lang;
use touring_code::ast::symbols::extract_symbols;

fn main() {
    let mut args = std::env::args().skip(1);
    let target = args.next().expect("usage: <file>|@<list> [iterations]");
    let iterations: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(1);
    let paths: Vec<String> = match target.strip_prefix('@') {
        Some(list) => std::fs::read_to_string(list)
            .expect("readable list")
            .lines()
            .map(str::to_string)
            .collect(),
        None => vec![target],
    };
    for pass in 0..iterations {
        for (n, path) in paths.iter().enumerate() {
            let Ok(content) = std::fs::read_to_string(path) else {
                continue;
            };
            let Some(lang) = Lang::from_path(std::path::Path::new(path)) else {
                continue;
            };
            // Printed before the call and flushed: a crash leaves the file on screen.
            eprintln!("{pass} {n} {path}");
            let _ = extract_symbols(&content, lang);
        }
    }
    println!("done {} files x {iterations}", paths.len());
}
