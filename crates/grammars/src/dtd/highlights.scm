;; Conditional sections

[ "INCLUDE" "IGNORE" ] @keyword

;; Delimiters & punctuation

[
  "<?" "?>"
  "<!" ">"
  "<![" "]]>"
] @punctuation.delimiter

[ "(" ")" "[" ] @punctuation.bracket

[ "\"" "'" ] @punctuation.delimiter

[ "," "|" "=" ] @operator

;; Misc

(Comment) @comment

(ERROR) @error
