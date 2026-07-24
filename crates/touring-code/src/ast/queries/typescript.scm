;; TypeScript queries for symbol extraction
;; Matches: functions, classes, interfaces, type aliases, enums, arrow functions, methods

;; Function declarations
(function_declaration
  name: (identifier) @name) @function

;; Exported function declarations
(export_statement
  (function_declaration
    name: (identifier) @name) @exported_function)

;; Class declarations
(class_declaration
  name: (type_identifier) @name) @class

;; Exported class declarations
(export_statement
  (class_declaration
    name: (type_identifier) @name) @exported_class)

;; Interface declarations
(interface_declaration
  name: (type_identifier) @name) @interface

;; Exported interface declarations
(export_statement
  (interface_declaration
    name: (type_identifier) @name) @exported_interface)

;; Type alias declarations
(type_alias_declaration
  name: (type_identifier) @name) @type_alias

;; Exported type aliases
(export_statement
  (type_alias_declaration
    name: (type_identifier) @name) @exported_type_alias)

;; Enum declarations
(enum_declaration
  name: (identifier) @name) @enum

;; Exported enum declarations
(export_statement
  (enum_declaration
    name: (identifier) @name) @exported_enum)

;; Namespace declarations (namespace Foo { ... })
(internal_module
  name: (identifier) @name) @namespace

;; Exported namespace declarations
(export_statement
  (internal_module
    name: (identifier) @name) @exported_namespace)

;; Method definitions in classes
(class_body
  (method_definition
    name: (property_identifier) @name) @method)

;; Arrow functions (top-level const declarations)
(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: (arrow_function)) @arrow_function)

;; Exported arrow functions
(export_statement
  (lexical_declaration
    (variable_declarator
      name: (identifier) @name
      value: (arrow_function)) @exported_arrow))
