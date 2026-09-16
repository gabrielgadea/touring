;; Python import extraction queries
;;
;; from module import symbol1, symbol2
(import_from_statement
  module_name: (dotted_name) @module
  name: (dotted_name (identifier) @symbol))

;; from module import symbol as alias
(import_from_statement
  module_name: (dotted_name) @module
  name: (aliased_import
    name: (dotted_name (identifier) @symbol)))

;; from .relative import symbol  (also `from . import symbol`)
;;
;; The WHOLE `relative_import` is captured, dots included: the leading dots live
;; in `import_prefix`, so capturing only `dotted_name` dropped them and
;; `from .formato import X` was probed from the SOURCE ROOT instead of the
;; package — 136 of the 231 residual false orphans measured in the analise
;; (16/09/2026). `from . import X` has no `dotted_name` at all and matched
;; nothing before.
(import_from_statement
  module_name: (relative_import) @module
  name: (dotted_name (identifier) @symbol))

;; from .relative import symbol as alias
(import_from_statement
  module_name: (relative_import) @module
  name: (aliased_import
    name: (dotted_name (identifier) @symbol)))

;; import module
(import_statement
  name: (dotted_name) @module)

;; import module as alias
(import_statement
  name: (aliased_import
    name: (dotted_name) @module))
