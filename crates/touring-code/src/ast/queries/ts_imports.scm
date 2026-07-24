;; TypeScript/JavaScript import extraction queries
;;
;; import { A, B } from 'module'
(import_statement
  source: (string (string_fragment) @module))

;; import X from 'module'
(import_statement
  (import_clause
    (identifier) @default_import)
  source: (string (string_fragment) @module))

;; import { A, B } from 'module'
(import_statement
  (import_clause
    (named_imports
      (import_specifier
        name: (identifier) @symbol)))
  source: (string (string_fragment) @module))
