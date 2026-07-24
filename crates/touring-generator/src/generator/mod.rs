//! Generator trait, kinds, and dynamic dispatch.

pub mod kinds;
pub mod trait_def;

pub use kinds::GeneratorKind;
pub use trait_def::{DynGenerator, ErasedGenerator, Generator};
