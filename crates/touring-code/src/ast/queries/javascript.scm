;; JavaScript queries for symbol extraction
;; Matches: functions, classes, arrow functions, methods, generator functions

;; Function declarations
(function_declaration
  name: (identifier) @name) @function

;; Exported function declarations (ES6 modules)
(export_statement
  (function_declaration
    name: (identifier) @name) @exported_function)

;; Class declarations
(class_declaration
  name: (identifier) @name) @class

;; Exported class declarations
(export_statement
  (class_declaration
    name: (identifier) @name) @exported_class)

;; Method definitions in classes
(class_body
  (method_definition
    name: (property_identifier) @name) @method)

;; Arrow functions in const declarations (top-level)
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

;; Regular const with function expression
(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: (function_expression)) @function_expr)

;; Generator function declarations
(generator_function_declaration
  name: (identifier) @name) @generator_function

;; Exported generator functions
(export_statement
  (generator_function_declaration
    name: (identifier) @name) @exported_generator)
