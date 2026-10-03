; Adapted for ZZZ from SeniorMars/tree-sitter-typst @ 1acd48c90d260bc9f8a99f903d3e2a8031bc7aae

(let_binding
  parameters: (parameters)
  value: (_) @function.inside) @function.around

(closure
  body: (_) @function.inside) @function.around

(section
  body: (_) @class.inside) @class.around

(content_block
  body: (_) @class.inside) @class.around

(code_block
  body: (_) @class.inside) @class.around

[
  (line_comment)
  (block_comment)
] @comment.inside

(line_comment)+ @comment.around
(block_comment) @comment.around
