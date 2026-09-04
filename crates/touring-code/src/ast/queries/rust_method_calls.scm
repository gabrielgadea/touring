;; Rust method-call / associated-function-call extraction queries.
;;
;; Captures call expressions that the import-based wiring map cannot see —
;; specifically `.method()` dispatch and `Type::assoc_fn()` syntax. These
;; account for ~43% of the orphan_count noise (audit 2026-05-11).
;;
;; Captures:
;;   @method     : `obj.method(...)`    (field_expression / method_call)
;;   @assoc_fn   : `Type::new(...)`, `Mod::fn(...)`  (scoped_identifier call)

;; Method calls: receiver.method(args)
(call_expression
  function: (field_expression
    field: (field_identifier) @method))

;; Associated function / type-qualified calls: Foo::new(), mod::fn()
(call_expression
  function: (scoped_identifier
    name: (identifier) @assoc_fn))

;; Generic method call: receiver.method::<T>(args)
(call_expression
  function: (generic_function
    function: (field_expression
      field: (field_identifier) @method)))

;; Generic associated call: Foo::<T>::new()
(call_expression
  function: (generic_function
    function: (scoped_identifier
      name: (identifier) @assoc_fn)))

;; W (2026-09-02): free-function calls — `apply_landlock(&p)`, `execute_in_sandbox(x)`.
;; A `use`-imported pub fn called bare is neither a method nor a scoped path, so
;; none of the patterns above saw its call sites; measured after a full rebuild:
;; 65 producers read as false orphans (36 with a cross-crate caller).
(call_expression
  function: (identifier) @free_fn)

;; Generic free call: parse::<T>(x)
(call_expression
  function: (generic_function
    function: (identifier) @free_fn))
