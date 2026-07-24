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

;; from .relative import symbol
(import_from_statement
  module_name: (relative_import (dotted_name) @module)
  name: (dotted_name (identifier) @symbol))

;; from .relative import symbol as alias
(import_from_statement
  module_name: (relative_import (dotted_name) @module)
  name: (aliased_import
    name: (dotted_name (identifier) @symbol)))

;; import module
(import_statement
  name: (dotted_name) @module)

;; import module as alias
(import_statement
  name: (aliased_import
    name: (dotted_name) @module))
