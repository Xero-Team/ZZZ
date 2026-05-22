(dir_sep) @punctuation.delimiter

(quoted_pattern
  "\"" @punctuation.special)

(range_notation) @string.special

(range_notation
  [
    "["
    "]"
  ] @punctuation.bracket)

(wildcard) @string.regexp

(range_negation) @operator

(character_class) @constant

(class_range
  "-" @operator)

[
  (ansi_c_escape)
  (escaped_char)
] @escape

(attribute
  (attr_name) @variable.parameter)

(attribute
  (builtin_attr) @variable.builtin)

[
  (attr_reset)
  (attr_unset)
  (attr_set)
] @operator

(boolean_value) @boolean

(string_value) @string

(macro_tag) @keyword

(macro_def
  macro_name: (_) @property)

((attr_name) @error
  (#match? @error "^builtin_"))

((attribute
  (attr_unset) @error
  (builtin_attr) @error @binary)
  (#eq? @binary "binary"))

((attribute
  (attr_reset) @error
  (builtin_attr) @error @binary)
  (#eq? @binary "binary"))

((attribute
  (builtin_attr) @error @binary
  [
    (attr_set)
    (boolean_value)
    (string_value)
  ] @error)
  (#eq? @binary "binary"))

[
  (pattern_negation)
  (redundant_escape)
  (trailing_slash)
  (ignored_value)
] @error

(comment) @comment
