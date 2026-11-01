//! The TypeScript adapter (VTS), and the engine its two siblings share.
//!
//! `.ts`/`.mts`/`.cts` parse with `LANGUAGE_TYPESCRIPT` (NOT `ast::Lang::TypeScript`, which is TSX
//! for everything and breaks the `<T>expr` cast); [`super::tsx::Tsx`] and
//! [`super::javascript::JavaScript`] run the same pipeline over their own grammar.
//!
//! The adapter MEASURES and `vgp::check` decides: each finding carries the [`Alvo`](crate::vgp::politica::Alvo) it is about —
//! a builtin member table (`Externo`), a symbol of the PROJECT (`Projeto`), or something the motor
//! could only partly see (`Incerto`) — and the severity policy does the rest.
//!
//! Layout (one concern per file, like `lang/python/`):
//!
//! | file | concern |
//! |---|---|
//! | `projeto` | disk + overlays, `tsconfig`/`jsconfig`, module resolution |
//! | `modulos` | what a module exports/declares, cached per check |
//! | `decls` | owned summaries of declarations and type expressions |
//! | `escopo` | lexical scope analysis: identifiers nothing binds |
//! | `globais` | runtime globals table + bounded search of the project's other sources |
//! | `tipos` | type expression → closed member set |
//! | `inferencia` | type of a receiver expression, only when provable |
//! | `analise` | the checks `import` · `nome` · `componente` · `atributo` · `kwarg` |
//! | `jsx` | the `prop` check |
//! | `tabelas` | GENERATED closed tables (globals, builtin members, React attributes) |
//!
//! Grammar field/node names come from `tree-sitter-typescript` 0.23.2 and `tree-sitter-javascript`
//! 0.25.0; module resolution follows the TypeScript handbook (Modules › Reference).

use super::{Adaptador, Verificacao};
use crate::vgp::CheckInput;

mod analise;
mod decls;
mod escopo;
mod globais;
mod inferencia;
mod jsdoc;
mod jsx;
mod modulos;
mod projeto;
mod tabelas;
mod tipos;
mod util;

#[cfg(test)]
mod testes;

/// The `typescript` adapter registered in `super::REGISTRO`.
pub struct TypeScript;

impl Adaptador for TypeScript {
    fn lang(&self) -> &'static str {
        "typescript"
    }

    fn verificar(&self, input: &CheckInput) -> Verificacao {
        analise::verificar("typescript", input)
    }
}

/// The shared pipeline, for the sibling adapters (`tsx`, `javascript`): same checks, own grammar.
pub(super) fn verificar_fonte(lang: &'static str, input: &CheckInput) -> Verificacao {
    analise::verificar(lang, input)
}
