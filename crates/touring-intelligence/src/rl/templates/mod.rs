//! Self-improving context injection templates.
//!
//! Templates are versioned text patterns for context injection.
//! They evolve via UCB1 selection and mutation based on reward feedback.
//!
//! Unlike the `bandit` module (LinUCB for *what type* of context to inject),
//! this module manages the *template text itself* -- which sections to include,
//! in which order, and how to format them.

pub mod evolving;

pub use evolving::{ContextTemplate, TemplateLibrary};
