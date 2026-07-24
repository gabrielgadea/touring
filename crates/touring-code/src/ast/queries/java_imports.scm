;; Java import extraction queries
;;
;; import com.foo.Bar;  →  module = "com.foo.Bar", symbol = "Bar"
;; Java is file-based (one public type per file), so the fully-qualified name
;; both names the producer file (com/foo/Bar.java) and the imported symbol (Bar).
(import_declaration
  (scoped_identifier
    name: (identifier) @symbol) @module)
