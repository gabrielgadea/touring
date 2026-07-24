//! Bridge between touring-ast and touring-learning RL subsystem.
//!
//! Provides `compute_rl_state` which extracts AST features from a file and
//! hashes them into a state identifier for use with QTable and LinUCB.
//!
//! # Python API
//!
//! ```python
//! from claude_learning_kernel import compute_rl_state
//!
//! state_id = compute_rl_state(
//!     file_path="/path/to/file.py",
//!     cila_level=3,
//!     session_turn=25,
//!     recent_errors=2,
//! )
//! # Use state_id with process_reward() and get_best_action()
//! ```

use std::path::Path;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use touring_code::ast::{Lang, extract_symbols_from_file};
use touring_intelligence::rl::rl::djb2_hash;

// ═══════════════════════════════════════════════════════════════════════════
// State computation
// ═══════════════════════════════════════════════════════════════════════════

/// Map a file extension to the LinUCB file_type index.
///
/// Matches the encoding in linucb.rs feature vector:
///   0=python, 1=rust, 2=typescript, 3=other
fn file_type_index(path: &Path) -> u8 {
    match Lang::from_path(path) {
        Some(Lang::Python) => 0,
        Some(Lang::Rust) => 1,
        Some(Lang::TypeScript) => 2,
        _ => 3,
    }
}

/// Compute a deterministic state ID from file-level AST features.
///
/// The state encodes:
/// - File type (language detection via touring-ast)
/// - Number of symbols (proxy for file complexity/size)
/// - CILA level of the current task
/// - Session turn bucket (early/mid/late)
/// - Recent error count bucket
///
/// These are combined via djb2_hash into a u64 state identifier.
fn compute_state(
    file_path: &str,
    cila_level: u8,
    session_turn: u32,
    recent_errors: u32,
) -> Result<u64, String> {
    let path = Path::new(file_path);
    let ft = file_type_index(path);

    // Extract symbol count as a complexity proxy.
    // On failure (unsupported language, I/O error), default to 0 symbols
    // so the function remains usable for any file path.
    let symbol_count = extract_symbols_from_file(path)
        .map(|syms| syms.len())
        .unwrap_or(0);

    // Bucket symbol count: 0=empty, 1=small(<10), 2=medium(<50), 3=large(>=50)
    let size_bucket: u8 = if symbol_count == 0 {
        0
    } else if symbol_count < 10 {
        1
    } else if symbol_count < 50 {
        2
    } else {
        3
    };

    // Bucket session turn: 0=early(<10), 1=mid(<50), 2=late(>=50)
    let turn_bucket: u8 = if session_turn < 10 {
        0
    } else if session_turn < 50 {
        1
    } else {
        2
    };

    // Bucket errors: 0=none, 1=some(>=1)
    let error_bucket: u8 = if recent_errors == 0 { 0 } else { 1 };

    // Build a deterministic string and hash it.
    // Format: "ft:{ft}|sz:{size_bucket}|cila:{cila_level}|turn:{turn_bucket}|err:{error_bucket}"
    let state_str =
        format!("ft:{ft}|sz:{size_bucket}|cila:{cila_level}|turn:{turn_bucket}|err:{error_bucket}");

    Ok(djb2_hash(&state_str))
}

// ═══════════════════════════════════════════════════════════════════════════
// Python-exposed function
// ═══════════════════════════════════════════════════════════════════════════

/// Compute an RL state ID from a file path and contextual parameters.
///
/// Extracts AST features from the file (language, symbol count) and combines
/// them with CILA level, session turn, and error count into a deterministic
/// state hash suitable for QTable and get_best_action().
///
/// Args:
///     file_path: Path to the source file to analyze.
///     cila_level: CILA complexity level (0-6).
///     session_turn: Current turn number in the session.
///     recent_errors: Number of recent errors in this session.
///
/// Returns:
///     State ID (int, u64) for use with process_reward() and get_best_action().
#[pyfunction]
pub fn compute_rl_state(
    py: Python<'_>,
    file_path: String,
    cila_level: u8,
    session_turn: u32,
    recent_errors: u32,
) -> PyResult<u64> {
    let result = py.detach(|| compute_state(&file_path, cila_level, session_turn, recent_errors));

    result.map_err(|e| PyRuntimeError::new_err(format!("Failed to compute RL state: {e}")))
}

// ═══════════════════════════════════════════════════════════════════════════
// Module registration
// ═══════════════════════════════════════════════════════════════════════════

/// Register AST-RL bridge functions into the parent Python module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(compute_rl_state, m)?)?;
    Ok(())
}
