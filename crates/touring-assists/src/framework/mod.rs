//! Framework core types for assists.

pub mod assist;
pub mod assists;
pub mod catalog;
pub mod context;

pub use assist::{
    Assist, AssistCodeAction, AssistEdit, AssistGroup, AssistHandler, AssistId, AssistTarget,
    AssistTextChange, LazySourceChange,
};
pub use assists::Assists;
pub use catalog::AssistCatalog;
pub use context::AssistContext;
