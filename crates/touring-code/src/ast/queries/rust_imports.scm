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

;; use module::* (glob import with pub)
(use_declaration
  argument: (visibility_modifier
    (_)?
    pattern: (default_import
      name: (identifier) @default_import)))

;; use module::* (glob import without pub — direct default_import child)
(use_declaration
  (default_import
    name: (identifier) @default_import))
