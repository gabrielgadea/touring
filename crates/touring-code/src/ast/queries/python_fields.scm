;; python_fields.scm — Class fields, methods, enum members for VGP v2
;;
;; Class methods (function_definition inside class_definition)
(class_definition
  name: (identifier) @class_name
  body: (block
    (function_definition
      name: (identifier) @method_name)))

;; Dataclass-style field (typed assignment at class body level)
(class_definition
  name: (identifier) @dc_class_name
  body: (block
    (expression_statement
      (assignment
        left: (identifier) @dc_field_name
        type: (type) @dc_field_type))))

;; Simple class-level assignment (e.g., enum members: NAME = value)
(class_definition
  name: (identifier) @enum_class_name
  body: (block
    (expression_statement
      (assignment
        left: (identifier) @enum_member_name))))
