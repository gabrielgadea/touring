//! Helpers compartilhados pelos testes de integração de `touring-hooks`.
//!
//! Cargo não compila `tests/common/` como um binário de teste próprio, então
//! este módulo é incluído via `mod common;` por cada suíte que precisa dele.
//! Cada suíte usa apenas parte da superfície — daí o `allow(dead_code)`, que
//! aqui significa "não usado POR ESTA suíte", não "morto".

#![allow(dead_code)]

pub mod private_daemon;
