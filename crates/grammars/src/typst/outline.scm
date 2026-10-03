(section
  heading: (heading
    marker: (heading_marker) @context
    body: (heading_body) @name)) @item

(let_binding
  "let" @context
  name: (identifier) @name
  parameters: (parameters
    "(" @context
    ")" @context)) @item

(let_binding
  "let" @context
  name: (identifier) @name
  !parameters) @item
