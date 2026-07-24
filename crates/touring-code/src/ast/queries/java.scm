;; Java queries for symbol extraction

;; Method declarations
(method_declaration name: (identifier) @name)

;; Class declarations
(class_declaration name: (identifier) @name)

;; Interface declarations
(interface_declaration name: (identifier) @name)

;; Constructor declarations
(constructor_declaration name: (identifier) @name)

;; Enum declarations
(enum_declaration name: (identifier) @name)

;; Record declarations (Java 16+)
(record_declaration name: (identifier) @name)
