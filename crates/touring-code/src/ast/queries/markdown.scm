;; Markdown queries for symbol extraction
;; Matches: headings (ATX style)

;; ATX headings: # Heading
(atx_heading
  (inline) @name) @heading
