;; rust_fields.scm — Field/variant/method extraction for VGP v2
;;
;; Struct fields
(struct_item
  name: (type_identifier) @struct_name
  body: (field_declaration_list
    (field_declaration
      name: (field_identifier) @field_name
      type: (_) @field_type)))

;; Enum variants
(enum_item
  name: (type_identifier) @enum_name
  body: (enum_variant_list
    (enum_variant
      name: (identifier) @variant_name)))

;; Impl methods
(impl_item
  type: (type_identifier) @impl_type
  body: (declaration_list
    (function_item
      name: (identifier) @method_name)))
