# Strategy: shared

## Items to extract into crates/touring-server/src/tools/shared.rs:

  - (diagnose detected no concrete type refs; manual review)

## Steps:
1. Create `crates/touring-server/src/tools/shared.rs` with extracted items.
2. Update `tools/mod.rs` with `pub mod shared;`.
3. Replace `use super::project_tools::X` with `use super::shared::X` in file_tools.rs.
4. Replace `use super::file_tools::X` with `use super::shared::X` in project_tools.rs.
5. cargo check --workspace + touring wiring cycles --min-depth 2 (expect Cycle #1 GONE).
