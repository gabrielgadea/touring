//! The JavaScript adapter (VJS): the TypeScript pipeline over `tree-sitter-javascript` (JSX
//! included, so `.jsx` and JSX in `.js` parse).
//!
//! Plain JavaScript declares no types, so the member and key checks only fire on what a file proves
//! about itself — a `new Date()`, a class whose `this.x = …` assignments define its members, a
//! literal; `require`/`import` of project modules is resolved exactly as in TypeScript, and a
//! classic script (no `import`/`export`) is searched for globals in its neighbouring sources.
//! The engine lives in `lang/typescript/` (see [`super::typescript`]).

use super::typescript::verificar_fonte;
use super::{Adaptador, Verificacao};
use crate::vgp::CheckInput;

/// The `javascript` adapter registered in `super::REGISTRO`.
pub struct JavaScript;

impl Adaptador for JavaScript {
    fn lang(&self) -> &'static str {
        "javascript"
    }

    fn verificar(&self, input: &CheckInput) -> Verificacao {
        verificar_fonte("javascript", input)
    }
}
