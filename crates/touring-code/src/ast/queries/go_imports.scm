;; Go import extraction queries
;;
;; import "path/pkg"   (single or within an `import ( ... )` block)
;;   →  module = "\"path/pkg\"" (the string literal, quotes included)
;;
;; A Go import denotes a PACKAGE (a directory), not a single file, and carries
;; no symbol — usage is `pkg.Foo()`, wired via method-dispatch, not import
;; resolution. This query completes extraction (dependency listing); file-keyed
;; wiring for Go is deferred (see docs/2026-07-03-polyglot-parity-plan.md §6).
(import_spec
  path: (interpreted_string_literal) @module)
