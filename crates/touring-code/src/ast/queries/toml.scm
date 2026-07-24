;; TOML queries for symbol extraction
;; Matches: tables and top-level keys

;; Table headers [section]
(table
  (bare_key) @name) @table

;; Dotted table headers [section.subsection]
(table
  (dotted_key
    (bare_key) @name)) @dotted_table

;; Top-level key = value pairs
(pair
  (bare_key) @name) @pair
