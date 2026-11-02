//! The proofs of the code-mode gates.
//!
//! * `rows` — the table of event sequences → verdicts, driven through [`evaluate`] by
//!   `scenarios` (the DSL and the runner).
//! * `replay` — the recorded corpus: every response of the pre-refactor gates, byte for byte;
//!   the G7 × G3 × G9 state-space digest; the S-13 rubric; the latency benchmark.
//! * `modes` — the precedence of the presentation, as a table.
//! * `contract` — the declared windows against the ones the ledgers enforce, and the map of
//!   the old tests to the rows.
//! * `unit` / `arm_unit` — the pure parts and the A/B arm, moved verbatim.
//!
//! [`evaluate`]: super::evaluate

mod arm_unit;
mod modes;
mod contract;
mod replay;
mod rows;
mod scenarios;
mod support;
mod unit;
