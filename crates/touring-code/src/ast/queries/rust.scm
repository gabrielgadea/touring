;; Rust queries for symbol extraction
;; Matches: functions, structs, enums, traits, impls, consts, statics, type aliases, macros, modules

;; Function items
(function_item
  name: (identifier) @name) @function

;; Struct items
(struct_item
  name: (type_identifier) @name) @struct

;; Enum items
(enum_item
  name: (type_identifier) @name) @enum

;; Trait items
(trait_item
  name: (type_identifier) @name) @trait

;; Impl items — capture the type being implemented
(impl_item
  type: (type_identifier) @name) @impl

;; Const items
(const_item
  name: (identifier) @name) @const

;; Static items
(static_item
  name: (identifier) @name) @static

;; Type aliases
(type_item
  name: (type_identifier) @name) @type_alias

;; Macro definitions (macro_rules!)
(macro_definition
  name: (identifier) @name) @macro

;; Module declarations
(mod_item
  name: (identifier) @name) @module

;; Method definitions inside impl blocks
(impl_item
  body: (declaration_list
    (function_item
      name: (identifier) @name) @method))

;; Trait method signatures (without body)
(trait_item
  body: (declaration_list
    (function_signature_item
      name: (identifier) @name) @trait_method))

;; Trait method with default body
(trait_item
  body: (declaration_list
    (function_item
      name: (identifier) @name) @trait_method_default))
