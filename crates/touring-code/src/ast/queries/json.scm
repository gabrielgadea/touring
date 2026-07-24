;; JSON queries for symbol extraction
;; Matches: top-level object keys

;; Top-level keys in root object
(document
  (object
    (pair
      key: (string
        (string_content) @name)) @pair))
