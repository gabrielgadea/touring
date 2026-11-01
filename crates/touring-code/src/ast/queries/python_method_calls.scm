;; Python attribute-access extraction (18/09/2026, analise).
;;
;; Python has no receiver type in the syntax: `no.tags_cli()` names a method
;; but not its class, and `no.todas_as_arestas` may be a property or a bound
;; method handed on. Every attribute name is captured, call or not; the storage
;; lookup keeps it honest (only `method` producers, only in modules the file
;; already imports from — `find_python_method_producers`).
;;
;; Captures:
;;   @method : `obj.name` and `obj.name(...)`

(attribute
  attribute: (identifier) @method)
