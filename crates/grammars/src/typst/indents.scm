(_
  "["
  "]" @end) @indent

(_
  "{"
  "}" @end) @indent

(_
  "("
  ")" @end) @indent

(_
  "$"
  "$" @end) @indent

((block_comment) @indent
  (#match? @indent "^/\\*"))

(list_body) @indent
