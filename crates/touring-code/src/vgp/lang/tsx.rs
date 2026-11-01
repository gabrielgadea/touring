//! The TSX adapter (VTSX): the TypeScript pipeline over `LANGUAGE_TSX`, plus the JSX-only checks —
//! `componente` (`<Comp>` neither imported nor defined, found by the shared scope analysis) and
//! `prop` (an attribute the component's `Props`, or a React intrinsic element, does not have).
//!
//! The engine lives in `lang/typescript/` (see [`super::typescript`]); this file only registers the
//! grammar. TSX is a grammar of its own — `<T>expr` is an unclosed JSX element here — never
//! `ast::Lang::TypeScript`.

use super::typescript::verificar_fonte;
use super::{Adaptador, Verificacao};
use crate::vgp::CheckInput;

/// The `tsx` adapter registered in `super::REGISTRO`.
pub struct Tsx;

impl Adaptador for Tsx {
    fn lang(&self) -> &'static str {
        "tsx"
    }

    fn verificar(&self, input: &CheckInput) -> Verificacao {
        verificar_fonte("tsx", input)
    }
}
