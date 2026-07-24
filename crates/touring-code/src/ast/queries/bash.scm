;; Bash queries for symbol extraction
;; Matches: function definitions, aliases, variable assignments

;; Function definitions: function name() { ... } or name() { ... }
(function_definition
  name: (word) @name) @function

;; Variable assignments at top level
(variable_assignment
  name: (variable_name) @name) @variable
