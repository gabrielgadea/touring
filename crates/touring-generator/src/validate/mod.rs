//! Post-render validation layers executed between `Rendered` and `Speculated`.
//!
//! Today: `pipeline` (7-layer `validate_plan`) + `boundary` (VGP L5) +
//! `polyglot` (ast-grep syntax check). The `pipeline` module formalizes the
//! implicit VGP gate into an explicit ordered pipeline (ESAA §7-layer + S6).

pub mod boundary;
pub mod pipeline;
pub mod polyglot;

pub use pipeline::{ValidationContext, ValidationLayer, ValidationReport, validate_plan};
