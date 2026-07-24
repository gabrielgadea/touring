;; HTML queries for symbol extraction
;; Matches: top-level elements with identifiers (id, class attributes)

;; Script tags (inline JavaScript)
(script_element
  (start_tag
    (tag_name) @name)) @script

;; Style tags (inline CSS)
(style_element
  (start_tag
    (tag_name) @name)) @style

;; Top-level elements
(element
  (start_tag
    (tag_name) @name)) @element
