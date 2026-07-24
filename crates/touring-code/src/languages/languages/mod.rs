//! Language-specific support definitions.
//!
//! Each module reflects the realistic support boundary for that language
//! as declared in the capability matrix.

/// Rust — Tier 1. Full AST, symbols, quality, wiring, cognitive.
pub mod rust {}
/// TypeScript — Tier 1. Full AST, symbols, quality, wiring, cognitive.
pub mod typescript {}
/// Python — Tier 2. AST, symbols, quality; partial wiring.
pub mod python {}
/// Go — Tier 2. AST, symbols, quality; partial wiring.
pub mod go_lang {}
/// C — Tier 2. AST, symbols, quality; partial wiring.
pub mod c_lang {}
/// Kotlin — Tier 3. AST, partial symbols.
pub mod kotlin {}
/// Swift — Tier 3. AST, partial symbols.
pub mod swift {}
/// Java — Tier 3. AST, partial symbols.
pub mod java {}
/// Ruby — Tier 4. Basic tokens only.
pub mod ruby {}
/// PHP — Tier 4. Basic tokens only.
pub mod php {}
