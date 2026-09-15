;; Rust use statement extraction queries
;;
;; use crate::module::Symbol;
(use_declaration
  argument: (scoped_identifier
    path: (scoped_identifier) @module
    name: (identifier) @symbol))

;; use crate::module;
(use_declaration
  argument: (scoped_identifier
    path: (identifier) @module
    name: (identifier) @symbol))

;; use module;
(use_declaration
  argument: (identifier) @module)

;; use crate::module::{Foo, Bar, baz};   — brace import, each leaf identifier
;; emitted as its own (@module, @symbol) pair so wiring captures every symbol.
(use_declaration
  argument: (scoped_use_list
    path: (scoped_identifier) @module
    list: (use_list
      (identifier) @symbol)))

;; use module::{Foo, Bar};               — single-segment path before the brace
(use_declaration
  argument: (scoped_use_list
    path: (identifier) @module
    list: (use_list
      (identifier) @symbol)))

;; use foo::{bar as baz};                — alias inside brace group; capture origin name
(use_declaration
  argument: (scoped_use_list
    path: (scoped_identifier) @module
    list: (use_list
      (use_as_clause
        path: (identifier) @symbol))))

(use_declaration
  argument: (scoped_use_list
    path: (identifier) @module
    list: (use_list
      (use_as_clause
        path: (identifier) @symbol))))

;; use crate::module::Symbol as Alias;   — capture the origin name
(use_declaration
  argument: (use_as_clause
    path: (scoped_identifier
      path: (_) @module
      name: (identifier) @symbol)))

;; use module::*;   — a glob names the module and no symbol.
;; `default_import` (a TypeScript node) stood here until 15/09/2026: it made
;; `Query::new` fail for Rust, so every Rust file fell back to the regex
;; extractor, which skips `pub use` lines (cross-audit R2).
(use_declaration
  argument: (use_wildcard
    (scoped_identifier) @module))

(use_declaration
  argument: (use_wildcard
    (identifier) @module))
