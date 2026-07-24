;; Go queries for symbol extraction

;; Top-level function declarations
(function_declaration name: (identifier) @name)

;; Method declarations (on types)
(method_declaration name: (field_identifier) @name)

;; Type declarations: struct, interface, type alias, etc.
(type_spec name: (type_identifier) @name)

;; Constant specifications
(const_spec name: (identifier) @name)
