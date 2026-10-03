; Adapted for ZZZ from SeniorMars/tree-sitter-typst @ 1acd48c90d260bc9f8a99f903d3e2a8031bc7aae

; Comments and shebang
(shebang) @keyword.directive
(line_comment) @comment
(block_comment) @comment

; Markup
[
  (incomplete_let_binding)
  (incomplete_module_import)
  (incomplete_module_include)
  (incomplete_return_expression)
  (incomplete_set_rule)
  (incomplete_show_rule)
  (malformed_automatic_link)
  (malformed_embedded_code)
  (malformed_escape)
  (malformed_number)
] @error

(heading) @title
(bullet_list_item marker: (bullet_list_marker) @punctuation.list_marker)
(numbered_list_item marker: (numbered_list_marker) @punctuation.list_marker)
(term_list_item marker: (term_list_marker) @punctuation.list_marker)
(strong) @emphasis.strong
(emphasis) @emphasis
(smart_quote) @string.special.symbol
(shorthand) @string.special.symbol
(escape) @string.escape
(linebreak) @string.escape
(bracketed_text ["[" "]"] @punctuation.bracket)
(automatic_link) @link_uri
(label) @label
(reference) @link_text
(reference "@" @punctuation.special)
(reference target: (identifier) @label)

; Raw text and language tags
(raw
  (raw_delimiter) @punctuation.special)
(raw language: (raw_language) @label)
(raw content: (raw_content) @text.literal)
((raw
  (raw_delimiter) @_raw_delimiter
  content: (raw_content) @text.literal)
  (#match? @_raw_delimiter "^```"))

; Cross-mode punctuation.
(embedded_code "#" @punctuation.special)
(equation "$" @punctuation.special)

[
  "(" ")" "[" "]" "{" "}"
] @punctuation.bracket

[
  "," ":" ";"
] @punctuation.delimiter

"." @punctuation.delimiter
(content_block ["[" "]"] @punctuation.special)

; Code keywords and builtins
"let" @keyword.storage.type
["set" "show" "context"] @keyword
(if_expression "if" @keyword.control.conditional)
(else_clause "else" @keyword.control.conditional)
(for_loop ["for" "in"] @keyword.control.repeat)
(while_loop "while" @keyword.control.repeat)
(return_expression "return" @keyword.control.return)
["import" "include"] @keyword.control.import
"as" @keyword.operator

(break_expression) @keyword.control.repeat
(continue_expression) @keyword.control.repeat

(none) @constant.builtin
(auto) @constant.builtin
(boolean) @boolean
(integer) @number
(float) @number
(numeric) @number
(unit) @type.builtin
(string) @string
(module_import source: (string) @string.special.path)
(module_include source: (string) @string.special.path)
(string_escape) @string.escape
(discard_pattern) @variable.special

; Definitions and calls
(let_binding
  name: (identifier) @function
  parameters: (parameters))
(let_binding
  name: (identifier) @variable
  !parameters)
(let_binding "=" @operator)
(closure parameters: (identifier) @variable.parameter)
(closure parameters: (discard_pattern) @variable.special)
(parameters (identifier) @variable.parameter)
(parameters (discard_pattern) @variable.special)
(named_parameter name: (identifier) @variable.parameter)
(sink_parameter name: (identifier) @variable.parameter)
(sink_parameter name: (discard_pattern) @variable.special)
(destructuring_pattern (identifier) @variable)
(named_destructuring_item pattern: (identifier) @variable)
(destructuring_sink pattern: (identifier) @variable)
(for_loop pattern: (identifier) @variable)
(named_destructuring_item key: (identifier) @property)
(dictionary_entry key: (identifier) @property)
(named_argument name: (identifier) @variable.parameter)

; Identifier references. Keep this targeted to expression-value positions:
; import items, dictionary keys, parameters, and field names have their own
; captures and should not fall through to generic variable highlighting.
(embedded_code body: (identifier) @variable)
(arguments (identifier) @variable)
(array (identifier) @variable)
(code_block body: (identifier) @variable)
(parenthesized_expression expression: (identifier) @variable)
(let_binding value: (identifier) @variable)
(named_parameter default: (identifier) @variable)
(named_argument value: (identifier) @variable)
(spread value: (identifier) @variable)
(spread_argument value: (identifier) @variable)
(dictionary_entry value: (identifier) @variable)
(binary_expression left: (identifier) @variable)
(binary_expression right: (identifier) @variable)
(unary_expression operand: (identifier) @variable)
(logical_not_expression operand: (identifier) @variable)
(assignment_expression left: (identifier) @variable)
(assignment_expression right: (identifier) @variable)
(destructuring_assignment value: (identifier) @variable)
(if_expression condition: (identifier) @variable)
(for_loop iterable: (identifier) @variable)
(while_loop condition: (identifier) @variable)
(return_expression value: (identifier) @variable)
(contextual_expression body: (identifier) @variable)
(field_access object: (identifier) @variable)

(field_access field: (identifier) @property)
(function_call function: (identifier) @function)
(function_call function: (field_access field: (identifier) @function.method))
(set_rule target: (identifier) @function.builtin)
(set_rule target: (field_access field: (identifier) @function.builtin))
(show_rule selector: (identifier) @function.builtin)
(show_rule selector: (field_access field: (identifier) @function.builtin))
(module_import alias: (identifier) @namespace)
(import_item alias: (identifier) @namespace)
(import_path
  head: (identifier) @namespace)
(import_path
  tail: (identifier) @namespace)

; Operators
(unary_expression operator: ["+" "-" "−"] @operator)
(logical_not_expression operator: "not" @keyword.operator)
(assignment_expression operator: ["=" "+=" "-=" "−=" "*=" "/="] @operator)
(binary_expression
  operator: [
    "==" "!=" "<" "<=" ">" ">="
    "+" "-" "−" "*" "/"
  ] @operator)
(binary_expression
  operator: ["or" "and" "in"] @keyword.operator)
(not_in_operator) @keyword.operator
(destructuring_assignment operator: "=" @operator)
(closure "=>" @operator)
(spread ".." @operator)
(spread_argument ".." @operator)
(sink_parameter ".." @operator)
(destructuring_sink ".." @operator)
(wildcard_import "*" @operator)

; Math
(math_identifier) @constant
(math_letter) @variable
((math_text) @operator
  (#match? @operator "^[+=<>]$"))
((math_text) @string.special.symbol
  (#match? @string.special.symbol "^([^+=<>]|..+)$"))
(math_number) @number
(math_shorthand) @operator
(math_alignment_point) @operator
(math_primes) @operator
(math_field_access field: (math_identifier) @property)
(math_call function: (math_identifier) @function)
(math_call function: (math_field_access field: (math_identifier) @function.method))
(math_application function: (math_letter) @function)
(math_application function: (math_identifier) @function)
(math_application function: (math_field_access field: (math_identifier) @function.method))
(math_named_argument name: (identifier) @variable.parameter)
(math_spread_argument (math_spread_operator) @operator)
(math_fraction "/" @operator)
(math_root operator: _ @operator)
(math_factorial "!" @operator)
(math_delimiter) @punctuation.bracket
(math_group
  open: (math_delimiter) @punctuation.special
  close: (math_delimiter) @punctuation.special)

; Attachments are captured at their structural node instead of trying to match
; arbitrary descendants. This handles subscript, superscript, and grouped prime
; forms consistently.
(math_attachment base: (math_text) @variable)
(math_attachment base: (math_letter) @variable)
(math_attachment base: (math_identifier) @variable)
(math_attachment subscript: (math_text) @variable)
(math_attachment subscript: (math_letter) @variable)
(math_attachment subscript: (math_identifier) @variable)
(math_attachment superscript: (math_text) @variable)
(math_attachment superscript: (math_letter) @variable)
(math_attachment superscript: (math_identifier) @variable)
(math_attachment ["_" "^"] @operator)
(math_attachment primes: (math_primes) @operator)
