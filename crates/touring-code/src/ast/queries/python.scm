;; Python queries for symbol extraction
;; Matches: functions, async functions, classes, methods, decorated definitions, global assignments

;; Function definitions
(function_definition
  name: (identifier) @name
  parameters: (parameters) @params) @function

;; Class definitions
(class_definition
  name: (identifier) @name) @class

;; Decorated definitions (functions with decorators)
(decorated_definition
  (function_definition
    name: (identifier) @name
    parameters: (parameters) @params)) @decorated_function

;; Decorated class definitions
(decorated_definition
  (class_definition
    name: (identifier) @name)) @decorated_class

;; Method definitions (functions inside classes)
(class_definition
  body: (block
    (function_definition
      name: (identifier) @name
      parameters: (parameters) @params) @method))

;; Decorated methods inside classes
(class_definition
  body: (block
    (decorated_definition
      (function_definition
        name: (identifier) @name
        parameters: (parameters) @params)) @decorated_method))

;; Type alias statements (Python 3.12+: type Point = tuple[int, int])
;; The 'left' field contains a (type) node wrapping an identifier
(type_alias_statement
  left: (type (identifier) @name)) @type_alias

;; Top-level assignments (global constants/variables)
;; Only capture simple name = value patterns at module level
(module
  (expression_statement
    (assignment
      left: (identifier) @name)) @assignment)

