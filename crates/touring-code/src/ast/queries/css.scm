;; CSS queries for symbol extraction
;; Matches: selectors, @rules, keyframes

;; Class selectors
(rule_set
  (selectors
    (class_selector
      (class_name) @name)) @rule)

;; ID selectors
(rule_set
  (selectors
    (id_selector
      (id_name) @name)) @rule)

;; Tag selectors
(rule_set
  (selectors
    (tag_name) @name) @rule)

;; @media rules
(media_statement
  (keyword_query) @name) @media

;; @keyframes
(keyframes_statement
  (keyframes_name) @name) @keyframes
