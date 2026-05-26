[
  "&&"
  "||"
  "&"
  "|"
  "<"
  ">"
  ">>"
] @operator

(comment) @comment
(label_name) @label
(number) @number
(quoted_string) @string
(single_quoted_string) @string
(backquoted_string) @string.special
(variable_expansion) @variable
(delayed_variable) @variable
(for_variable) @variable
(builtin_command_name) @function.builtin
(keyword) @keyword
(switch) @parameter
(path) @string.special.path
(call_batch_text) @string.special.path
(call_label_target (label_name) @label)
(goto_label_target (label_name) @label)
(goto_eof_target) @constant.builtin