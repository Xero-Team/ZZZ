; Treat the first record as a header row when present.
(source_file
  . (record
      [
        (string)
        (non_escaped)
      ] @property))

[
  (string)
  (non_escaped)
  (text)
] @string

(escape_sequence) @string.escape

[
  (integer)
  (float)
  (hex)
] @number

(boolean) @boolean

[
  (null)
  (na)
] @constant.builtin

(delimiter) @punctuation.delimiter

(quote) @punctuation.bracket