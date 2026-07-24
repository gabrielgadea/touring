;; typescript_fields.scm — Interface/class/enum member extraction for VGP v2
;;
;; Interface property signatures
(interface_declaration
  name: (type_identifier) @iface_name
  body: (interface_body
    (property_signature
      name: (property_identifier) @iface_prop_name)))

;; Interface method signatures
(interface_declaration
  name: (type_identifier) @iface_method_class
  body: (interface_body
    (method_signature
      name: (property_identifier) @iface_method_name)))

;; Class public field definitions
(class_declaration
  name: (type_identifier) @class_name
  body: (class_body
    (public_field_definition
      name: (property_identifier) @class_field_name)))

;; Class method definitions
(class_declaration
  name: (type_identifier) @class_method_class
  body: (class_body
    (method_definition
      name: (property_identifier) @class_method_name)))

;; Enum members with assignment (e.g., `Up = "UP"`)
;; In tree-sitter-tsx, enum members with `=` are `enum_assignment` nodes,
;; NOT `enum_member`.
(enum_declaration
  name: (identifier) @enum_name
  body: (enum_body
    (enum_assignment
      name: (property_identifier) @enum_member_name)))

;; Enum members without assignment (bare names like `Red` in `enum Color { Red, Green }`)
;; In tree-sitter-tsx, bare enum members are `property_identifier` nodes
;; directly inside `enum_body`.
(enum_declaration
  name: (identifier) @enum_bare_name
  body: (enum_body
    (property_identifier) @enum_bare_member_name))
